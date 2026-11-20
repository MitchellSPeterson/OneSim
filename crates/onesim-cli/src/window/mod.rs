//! `onesim window` / `onesim expo --window`: a native window on a live session (docs/rendering.md, "Window").
//!
//! The host owns the frame scheduler ([`scheduler`]): rendering is on demand, so a static screen costs nothing.
//! Input goes through the same `Host` calls the headless steps use; only presentation differs ([`present`]).

mod gesture;
mod present;
mod scheduler;
mod script;
mod stats;

use std::sync::Arc;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use onesim_host::Host;
use onesim_protocol::{Appearance, KeyboardEvent, PointerEvent, PointerKind};
use onesim_render::{BackendPref, DragPhase, RenderError};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Window, WindowId};

use gesture::{Gesture, Out};
use script::Step;
use present::Presenter;
use scheduler::{Poll, Reason, Scheduler};
use stats::{Frame, Stats};

/// Points scrolled per mouse-wheel line (trackpads report pixels and are used as they come).
const LINE_POINTS: f64 = 24.0;
/// A guest that fails this many frames in a row on the GPU swap chain moves to the CPU presenter.
const SURFACE_RETRIES: u32 = 3;
/// How often a hidden window is probed for becoming visible again (the OS does not always say).
const HIDDEN_POLL: Duration = Duration::from_millis(250);

pub struct Config {
    pub pref: BackendPref,
    /// React Native: the guest owns gesture physics and commits asynchronously.
    pub rn: bool,
    pub title: String,
    pub stats: bool,
    /// Start in dark appearance (the session was created with it).
    pub dark: bool,
}

enum UserEvent {
    Tree(u64),
    Stopped(String),
}

pub fn run(mut host: Host, id: String, cfg: Config) -> Result<()> {
    let status = host.status(&id)?;
    let vp = (status["viewport"]["width"].as_f64().unwrap_or(402.0), status["viewport"]["height"].as_f64().unwrap_or(874.0));
    if cfg.rn {
        host.set_settle(&id, false); // live: never wait for the guest after the first layout
    }
    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    forward_events(&mut host, &id, event_loop.create_proxy())?;
    let mut app = App::new(host, id, cfg, vp);
    app.dark = app.cfg.dark;
    if let Some(src) = std::env::var_os("SIMCORE_WINDOW_SCRIPT") {
        app.script = script::parse(&src.to_string_lossy()).map_err(|e| anyhow!("SIMCORE_WINDOW_SCRIPT: {e}"))?.into();
    }
    let result = event_loop.run_app(&mut app);
    app.host.shutdown();
    result?;
    app.fatal.map_or(Ok(()), Err)
}

/// Wake the event loop when the guest commits a tree or stops.
fn forward_events(host: &mut Host, id: &str, proxy: EventLoopProxy<UserEvent>) -> Result<()> {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    host.events(id)?.subscribe(u64::MAX, tx, Some(vec!["render.updated".into(), "runtime.stopped".into()]), None);
    std::thread::spawn(move || {
        for line in rx {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
            let p = &v["params"];
            let ev = match p["name"].as_str() {
                Some("render.updated") => UserEvent::Tree(p["data"]["treeVersion"].as_u64().unwrap_or(u64::MAX)),
                Some("runtime.stopped") => UserEvent::Stopped(p["data"]["reason"].as_str().unwrap_or("stopped").to_string()),
                _ => continue,
            };
            if proxy.send_event(ev).is_err() {
                break;
            }
        }
    });
    Ok(())
}

struct App {
    host: Host,
    id: String,
    cfg: Config,
    /// Device viewport in points; the window shows exactly this (zoomed to fit the monitor).
    vp: (f64, f64),
    window: Option<Arc<Window>>,
    presenter: Option<Presenter>,
    /// Why the GPU is not in use, when it was wanted.
    fallback: Option<String>,
    surface_failures: u32,
    sched: Scheduler,
    stats: Stats,
    next_stats: Instant,
    started: Instant,
    gesture: Gesture,
    mods: ModifiersState,
    left_down: bool,
    /// Cursor in physical pixels.
    cursor: PhysicalPosition<f64>,
    /// Wheel deltas (points) and the finger drag position since the last frame: applied once per draw.
    pending_scroll: Option<(f64, f64)>,
    pending_drag: Option<(f64, f64)>,
    /// Time spent in host calls (guest round trips) since the last frame.
    guest: Duration,
    last_tick: Instant,
    occluded: bool,
    /// The OS reports the window hidden: next time to try again (a slow poll, no frames meanwhile).
    hidden: Option<Instant>,
    drew: bool,
    script: VecDeque<Step>,
    script_wait: Option<Instant>,
    dark: bool,
    stopped: bool,
    fatal: Option<anyhow::Error>,
}

impl App {
    fn new(host: Host, id: String, cfg: Config, vp: (f64, f64)) -> Self {
        let now = Instant::now();
        Self {
            host,
            id,
            cfg,
            vp,
            window: None,
            presenter: None,
            fallback: None,
            surface_failures: 0,
            sched: Scheduler::new(Duration::from_micros(16_667)),
            stats: Stats::default(),
            next_stats: now + Duration::from_secs(1),
            started: now,
            gesture: Gesture::default(),
            mods: ModifiersState::default(),
            left_down: false,
            cursor: PhysicalPosition::new(0.0, 0.0),
            pending_scroll: None,
            pending_drag: None,
            guest: Duration::ZERO,
            last_tick: now,
            occluded: false,
            hidden: None,
            drew: false,
            script: VecDeque::new(),
            script_wait: None,
            dark: false,
            stopped: false,
            fatal: None,
        }
    }

    /// Render scale: device pixels per point, from the window's actual size.
    fn scale(&self) -> f64 {
        self.window.as_ref().map_or(1.0, |w| f64::from(w.inner_size().width.max(1)) / self.vp.0)
    }

    fn points(&self) -> (f64, f64) {
        let s = self.scale();
        (self.cursor.x / s, self.cursor.y / s)
    }

    fn now_ms(&self) -> f64 {
        self.started.elapsed().as_secs_f64() * 1000.0
    }

    fn title(&self) -> String {
        let Some(p) = &self.presenter else { return self.cfg.title.clone() };
        match &self.fallback {
            Some(r) => format!("{} - cpu (fallback: {r})", self.cfg.title),
            None => format!("{} - {} ({})", self.cfg.title, p.name(), p.adapter()),
        }
    }

    fn report_backend(&self) {
        if let Some(p) = &self.presenter {
            match &self.fallback {
                Some(r) => eprintln!("renderer: cpu (fallback: {r}); {}", p.describe()),
                None => eprintln!("renderer: {} ({})", p.name(), p.describe()),
            }
        }
        if let Some(w) = &self.window {
            w.set_title(&self.title());
        }
    }

    /// Replace the presenter (dropping the old one first: two swap chains cannot share a window).
    fn install(&mut self, want_gpu: bool, reason: Option<String>) {
        let Some(win) = self.window.clone() else { return };
        self.presenter = None;
        self.surface_failures = 0;
        let made = if want_gpu { Presenter::gpu(&win) } else { Presenter::cpu(&win) };
        match made {
            Ok(p) => {
                self.fallback = reason;
                self.presenter = Some(p);
            }
            Err(e) if want_gpu => {
                let why = format!("GPU renderer unavailable: {e}");
                log::warn!("{why}");
                match Presenter::cpu(&win) {
                    Ok(p) => {
                        self.fallback = Some(reason.map_or(why.clone(), |r| format!("{r}; {why}")));
                        self.presenter = Some(p);
                    }
                    Err(e) => self.fail(anyhow!("no presenter: {why}; CPU presenter: {e}")),
                }
            }
            Err(e) => self.fail(anyhow!("CPU presenter: {e}")),
        }
        self.report_backend();
        self.sched.invalidate(Reason::Renderer);
    }

    fn fail(&mut self, e: anyhow::Error) {
        self.fatal = Some(e);
    }

    /// Cmd+R / F5: GPU to CPU and back, same session and scene.
    fn toggle_renderer(&mut self) {
        let on_gpu = self.presenter.as_ref().is_some_and(|p| p.name() == "gpu");
        eprintln!("switching renderer to {}", if on_gpu { "cpu" } else { "gpu" });
        self.install(!on_gpu, None);
    }

    /// The GPU presenter failed for good: keep the session, draw on the CPU, remember why.
    fn fall_back(&mut self, reason: String) {
        eprintln!("GPU presenter failed: {reason}; falling back to the CPU presenter");
        let Some(win) = self.window.clone() else { return };
        self.presenter = None;
        match Presenter::cpu(&win) {
            Ok(p) => {
                self.fallback = Some(reason);
                self.presenter = Some(p);
                self.report_backend();
                self.sched.invalidate(Reason::Renderer);
            }
            Err(e) => self.fail(anyhow!("GPU failed ({reason}) and the CPU presenter failed: {e}")),
        }
    }

    // --- input -----------------------------------------------------------------------------------------------

    fn timed<R>(&mut self, f: impl FnOnce(&mut Host, &str) -> R) -> R {
        let t = Instant::now();
        let r = f(&mut self.host, &self.id);
        self.guest += t.elapsed();
        r
    }

    fn pointer(&mut self, ev: PointerEvent) {
        if let Err(e) = self.timed(|h, id| h.pointer(id, ev)) {
            log::debug!("pointer {ev:?}: {e}");
        }
        self.sched.invalidate(Reason::Input);
        if !self.cfg.rn && self.host.is_animating(&self.id) {
            self.sched.set_animating(true);
            self.last_tick = Instant::now();
        }
    }

    fn drag(&mut self, phase: DragPhase, p: (f64, f64)) {
        let t = self.now_ms();
        self.drag_at(phase, p, t);
    }

    fn drag_at(&mut self, phase: DragPhase, p: (f64, f64), t: f64) {
        match self.timed(|h, id| h.drag(id, phase, p.0, p.1, t)) {
            Ok(_) => {
                self.sched.invalidate(Reason::Input);
                if phase == DragPhase::End && !self.cfg.rn {
                    self.sched.set_animating(true); // momentum / bounce: the first tick says whether there is any
                    self.last_tick = Instant::now();
                }
            }
            Err(e) => log::debug!("drag: {e}"),
        }
    }

    /// Apply input that was only recorded (wheel, finger moves): once per frame, not per OS event.
    fn flush_input(&mut self) {
        if let Some(p) = self.pending_drag.take() {
            self.drag(DragPhase::Move, p);
        }
        if let Some((dx, dy)) = self.pending_scroll.take() {
            let (x, y) = self.points();
            self.pointer(PointerEvent::moved(PointerKind::Scroll, x, y, dx, dy));
        }
    }

    fn mouse(&mut self, state: ElementState) {
        let p = self.points();
        match state {
            ElementState::Pressed => {
                self.left_down = true;
                self.gesture.press(p);
                if !self.cfg.rn {
                    // SwiftUI: the finger is down now (gesture views see it, a long press can fire while held).
                    self.pointer(PointerEvent::new(PointerKind::Down, p.0, p.1));
                    self.sched.set_animating(true);
                    self.last_tick = Instant::now();
                }
            }
            ElementState::Released => {
                self.left_down = false;
                self.flush_input();
                match self.gesture.release(p) {
                    Out::Click(x, y) if !self.cfg.rn => self.pointer(PointerEvent::new(PointerKind::Up, x, y)),
                    Out::Click(x, y) => self.pointer(PointerEvent::new(PointerKind::Click, x, y)),
                    Out::DragEnd(x, y) => self.drag(DragPhase::End, (x, y)),
                    _ => {}
                }
            }
        }
    }

    fn cursor_moved(&mut self, pos: PhysicalPosition<f64>) {
        self.cursor = pos;
        if !self.left_down {
            return;
        }
        let p = self.points();
        match self.gesture.moved(p) {
            Out::DragStart { from, to } => {
                self.drag(DragPhase::Begin, from);
                self.pending_drag = Some(to);
                self.sched.invalidate(Reason::Input);
            }
            Out::DragMove(x, y) => {
                self.pending_drag = Some((x, y));
                self.sched.invalidate(Reason::Input);
            }
            _ => {}
        }
    }

    fn wheel(&mut self, delta: MouseScrollDelta) {
        let s = self.scale();
        // winit: positive y = scrolled up (content moves down); the host's positive dy reveals later content.
        let (dx, dy) = match delta {
            MouseScrollDelta::LineDelta(x, y) => (-f64::from(x) * LINE_POINTS, -f64::from(y) * LINE_POINTS),
            MouseScrollDelta::PixelDelta(p) => (-p.x / s, -p.y / s),
        };
        let (ax, ay) = self.pending_scroll.unwrap_or_default();
        self.pending_scroll = Some((ax + dx, ay + dy));
        self.sched.invalidate(Reason::Input);
    }

    fn key(&mut self, event: &winit::event::KeyEvent) {
        if event.state != ElementState::Pressed {
            return;
        }
        let cmd = self.mods.super_key();
        match &event.logical_key {
            Key::Named(NamedKey::F5) => return self.toggle_renderer(),
            Key::Character(c) if cmd && c.eq_ignore_ascii_case("r") => return self.toggle_renderer(),
            Key::Character(c) if cmd && c.eq_ignore_ascii_case("d") => return self.toggle_appearance(),
            Key::Character(c) if cmd && c.eq_ignore_ascii_case("s") => {
                if let Err(e) = self.snapshot(std::path::Path::new("onesim-window.png")) {
                    eprintln!("snapshot: {e:#}");
                }
                return;
            }
            _ => {}
        }
        if cmd || self.mods.control_key() {
            return;
        }
        let ev = match &event.logical_key {
            Key::Named(NamedKey::Backspace) => KeyboardEvent::Key { key: "backspace".into() },
            Key::Named(NamedKey::Enter) => KeyboardEvent::Key { key: "enter".into() },
            _ => match event.text.as_deref().filter(|t| !t.is_empty() && !t.chars().any(char::is_control)) {
                Some(t) => KeyboardEvent::Text { text: t.to_string() },
                None => return,
            },
        };
        self.key_event(ev);
    }

    fn key_event(&mut self, ev: KeyboardEvent) {
        match self.timed(|h, id| h.keyboard(id, ev)) {
            Ok(()) => {
                self.sched.invalidate(Reason::Input);
            }
            Err(e) => log::debug!("keyboard: {e}"), // no focused field
        }
    }

    fn toggle_appearance(&mut self) {
        self.dark = !self.dark;
        let mode = if self.dark { Appearance::Dark } else { Appearance::Light };
        if let Err(e) = self.timed(|h, id| h.set_appearance(id, mode)) {
            eprintln!("appearance: {e}");
        }
        self.sched.invalidate(Reason::Appearance);
    }

    // --- snapshots and scripted input -------------------------------------------------------------------------

    /// Write a PNG of the window as presented.
    fn snapshot(&mut self, path: &std::path::Path) -> Result<()> {
        let size = self.window.as_ref().ok_or_else(|| anyhow!("no window"))?.inner_size();
        let scale = f64::from(size.width.max(1)) / self.vp.0;
        let presenter = self.presenter.as_mut().ok_or_else(|| anyhow!("no presenter"))?;
        let frame = self.host.with_frame(&self.id, |scene, fonts, _| presenter.snapshot(scene, fonts, scale))?.map_err(anyhow::Error::msg)?;
        std::fs::write(path, frame.encode_png().map_err(anyhow::Error::msg)?)?;
        eprintln!("wrote {} ({}x{}, {})", path.display(), frame.width, frame.height, self.presenter.as_ref().map_or("none", Presenter::name));
        Ok(())
    }

    /// Run the next scripted step once the window is quiet. Returns when to wake for a pending `wait`.
    fn run_script(&mut self, el: &ActiveEventLoop, now: Instant) -> Option<Instant> {
        while self.drew && !self.sched.wants_frame() && self.presenter.is_some() {
            if let Some(t) = self.script_wait {
                if now < t {
                    return Some(t);
                }
                self.script_wait = None;
            }
            let step = self.script.pop_front()?;
            eprintln!("script: {step:?}");
            match step {
                Step::Wait(d) => self.script_wait = Some(now + d),
                Step::Click(x, y) => self.pointer(PointerEvent::new(PointerKind::Click, x, y)),
                Step::Scroll(x, y, dx, dy) => self.pointer(PointerEvent::moved(PointerKind::Scroll, x, y, dx, dy)),
                Step::Drag(x, y, dx, dy) => {
                    let t0 = self.now_ms();
                    self.drag_at(DragPhase::Begin, (x, y), t0);
                    for i in 1..=6 {
                        let f = f64::from(i) / 6.0;
                        self.drag_at(DragPhase::Move, (x + dx * f, y + dy * f), t0 + 16.0 * f64::from(i));
                    }
                    self.drag_at(DragPhase::End, (x + dx, y + dy), t0 + 16.0 * 7.0);
                }
                Step::DragPhase(phase, x, y) => self.drag(phase, (x, y)),
                Step::Type(t) => self.key_event(KeyboardEvent::Text { text: t }),
                Step::Key(k) => self.key_event(KeyboardEvent::Key { key: k }),
                Step::Toggle => self.toggle_renderer(),
                Step::Dark => self.toggle_appearance(),
                Step::Snap(p) => {
                    if let Err(e) = self.snapshot(&p) {
                        eprintln!("snapshot {}: {e:#}", p.display());
                    }
                }
                Step::Quit => el.exit(),
            }
        }
        None
    }

    // --- drawing -----------------------------------------------------------------------------------------------

    fn redraw(&mut self) {
        let Some(win) = self.window.clone() else { return };
        let started = Instant::now();
        if self.hidden.is_some_and(|until| started < until) {
            return;
        }
        let os_initiated = !self.sched.wants_frame();
        self.flush_input();
        if self.sched.animating() {
            let dt = started.saturating_duration_since(self.last_tick).as_secs_f64().min(0.1);
            self.last_tick = started;
            match self.timed(|h, id| h.tick(id, dt)) {
                Ok(moving) => self.sched.set_animating(moving),
                Err(e) => {
                    log::debug!("tick: {e}");
                    self.sched.set_animating(false);
                }
            }
        }
        let size = win.inner_size();
        let scale = f64::from(size.width.max(1)) / self.vp.0;
        let Some(presenter) = self.presenter.as_mut() else { return };
        let call = Instant::now();
        let out = self.host.with_frame(&self.id, |scene, fonts, version| {
            let laid_out = call.elapsed();
            (laid_out, version, presenter.draw(scene, fonts, scale, (size.width, size.height)))
        });
        let (scene_t, version, result) = match out {
            Ok(v) => v,
            Err(e) => {
                // No tree (the guest died, or has not drawn yet): nothing to show; wait for the next one.
                log::debug!("frame: {e}");
                self.sched.drawn(Instant::now(), 0);
                return;
            }
        };
        match result {
            Ok(d) => {
                self.surface_failures = 0;
                self.drew = true;
                if d.shown {
                    self.hidden = None;
                    self.sched.drawn(Instant::now(), version);
                    if !self.cfg.rn && !self.sched.animating() && self.host.is_animating(&self.id) {
                        self.sched.set_animating(true);
                        self.last_tick = Instant::now();
                    }
                    let guest = std::mem::take(&mut self.guest);
                    self.stats.frame(Frame { guest, scene: scene_t, list: d.list, exec: d.exec, present: d.present }, os_initiated);
                } else {
                    // Nothing was shown. Hidden window (minimized, covered, screen locked): poll slowly, never
                    // per frame. A transient acquire timeout: try again at the next interval.
                    self.stats.skip();
                    self.sched.drawn(Instant::now(), 0);
                    if d.occluded {
                        self.hidden = Some(Instant::now() + HIDDEN_POLL);
                    } else {
                        self.sched.invalidate(Reason::Renderer);
                    }
                }
            }
            Err(RenderError::SurfaceLost(m)) => {
                self.surface_failures += 1;
                if self.surface_failures >= SURFACE_RETRIES {
                    self.fall_back(format!("surface lost: {m}"));
                } else {
                    self.sched.invalidate(Reason::Renderer);
                }
            }
            Err(e) if presenter_is_gpu(&self.presenter) => self.fall_back(e.to_string()),
            Err(e) => {
                self.sched.drawn(Instant::now(), version);
                eprintln!("render error: {e}");
            }
        }
    }
}

/// `flow`, woken no later than `t`.
fn earlier(flow: ControlFlow, t: Instant) -> ControlFlow {
    match flow {
        ControlFlow::WaitUntil(f) => ControlFlow::WaitUntil(f.min(t)),
        _ => ControlFlow::WaitUntil(t),
    }
}

fn presenter_is_gpu(p: &Option<Presenter>) -> bool {
    p.as_ref().is_some_and(|p| p.name() == "gpu")
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        // The window shows the device viewport 1:1 in points, shrunk to fit a small monitor.
        let zoom = el
            .primary_monitor()
            .map(|m| {
                let s = m.size().to_logical::<f64>(m.scale_factor());
                ((s.height - 140.0) / self.vp.1).clamp(0.25, 1.0)
            })
            .unwrap_or(1.0);
        let attrs = Window::default_attributes()
            .with_title(self.cfg.title.clone())
            .with_inner_size(LogicalSize::new((self.vp.0 * zoom).round(), (self.vp.1 * zoom).round()))
            .with_resizable(false);
        let win = match el.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                self.fail(anyhow!("create window: {e}"));
                return el.exit();
            }
        };
        if let Some(hz) = win.current_monitor().and_then(|m| m.refresh_rate_millihertz()).filter(|h| *h > 0) {
            self.sched.set_interval(Duration::from_secs_f64(1000.0 / f64::from(hz)));
        }
        self.window = Some(win);
        self.install(self.cfg.pref != BackendPref::Cpu, None);
        if self.cfg.pref == BackendPref::Gpu {
            // An explicit `gpu` is the GPU or a clear error.
            if let Some(r) = self.fallback.take() {
                self.fail(anyhow!("{r}"));
                el.exit();
            }
        }
    }

    fn new_events(&mut self, _: &ActiveEventLoop, c: winit::event::StartCause) {
        let _ = c;
        self.stats.wake();
    }

    fn user_event(&mut self, _: &ActiveEventLoop, ev: UserEvent) {
        match ev {
            UserEvent::Tree(v) => {
                self.sched.invalidate(Reason::Tree(v));
            }
            UserEvent::Stopped(reason) => {
                if !std::mem::replace(&mut self.stopped, true) {
                    eprintln!("session stopped ({reason}); the window shows the last frame");
                }
            }
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, ev: WindowEvent) {
        match ev {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::Resized(s) => {
                if let Some(p) = &mut self.presenter {
                    p.resize(s.width, s.height);
                }
                self.sched.invalidate(Reason::Resize);
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(hz) = self.window.as_ref().and_then(|w| w.current_monitor()).and_then(|m| m.refresh_rate_millihertz()).filter(|h| *h > 0) {
                    self.sched.set_interval(Duration::from_secs_f64(1000.0 / f64::from(hz)));
                }
                self.sched.invalidate(Reason::Resize);
            }
            WindowEvent::Occluded(o) => {
                self.occluded = o;
                if !o {
                    self.hidden = None;
                    self.sched.invalidate(Reason::Resize);
                }
            }
            WindowEvent::ModifiersChanged(m) => self.mods = m.state(),
            WindowEvent::CursorMoved { position, .. } => self.cursor_moved(position),
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => self.mouse(state),
            WindowEvent::MouseWheel { delta, .. } => self.wheel(delta),
            WindowEvent::KeyboardInput { event, .. } => self.key(&event),
            _ => {}
        }
        if self.fatal.is_some() {
            el.exit();
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        let now = Instant::now();
        let mut flow = ControlFlow::Wait;
        if let Some(t) = self.hidden {
            if now >= t {
                self.hidden = None;
                self.sched.invalidate(Reason::Renderer);
            } else {
                flow = ControlFlow::WaitUntil(t);
            }
        }
        if let Some(t) = self.run_script(el, now) {
            flow = earlier(flow, t);
        }
        if !self.occluded && self.hidden.is_none() && self.presenter.is_some() {
            match self.sched.poll(now) {
                Poll::Draw => {
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                }
                Poll::WaitUntil(t) => flow = earlier(flow, t),
                Poll::Idle => {}
            }
        }
        if self.cfg.stats {
            if now >= self.next_stats {
                let backend = self.presenter.as_ref().map_or("none", Presenter::name);
                eprintln!("{}", self.stats.report(backend));
                self.next_stats = now + Duration::from_secs(1);
            }
            flow = earlier(flow, self.next_stats);
        }
        el.set_control_flow(flow);
    }
}
