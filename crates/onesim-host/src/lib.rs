//! Session manager and external API for OneSim (docs/protocol.md section A).
//!
//! Test hook: `create_session` skips the Swift build and spawns `app_path` directly when
//! `"prebuilt": true` is passed or `app_path` is an executable file that is not `*.swift`.
//! `onesim-fake-guest` (a bin of this crate) is such an executable.

mod events;
mod renderer;
mod rpc;
mod session;
mod ws;

use base64::Engine;
use serde::Serialize;
use serde_json::{json, Value};
use onesim_protocol::*;
use onesim_render::rn_input::{Out, RnCtx};
use onesim_render::{build_scene, scroll_anchor_offset, Fonts, Node, Scene, SceneParams};
use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, Write};
use std::time::{Duration, Instant};

pub use events::EventLog;
pub use rpc::Server;
pub use session::{device, onesim_home, Device, State, DEVICES};
pub use ws::serve_ws;
use session::Session;

fn err(code: ErrorCode, msg: impl Into<String>) -> ApiError {
    ApiError::new(code, msg)
}

fn check_device_platform(dev: &Device, platform: &str) -> Result<(), ApiError> {
    if dev.android != (platform == "android") {
        return Err(err(ErrorCode::Unsupported, format!("device {:?} does not support platform {platform:?}", dev.name)));
    }
    Ok(())
}

/// React Native: how long the guest must be quiet before a snapshot, and the most a snapshot waits.
const QUIET: Duration = Duration::from_millis(150);
const SETTLE_MAX: Duration = Duration::from_millis(2500);
/// React Native: when trees keep arriving but the layout no longer changes (an endless animation, e.g. a
/// rotating logo), a snapshot stops waiting after this long; startup waits `STARTUP_MAX` (splash, first layout).
const ANIM_MAX: Duration = Duration::from_millis(500);
const STARTUP_MAX: Duration = Duration::from_millis(2500);

pub struct Host {
    sessions: HashMap<String, Session>,
    next_id: u64,
    fonts: Option<Fonts>,
    renderer: renderer::RendererSlot,
    greeted: bool,
}

fn offset_changed(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() > 0.0 || (a.1 - b.1).abs() > 0.0
}

/// `index` is a descendant of `ancestor` by walking `Node::parent`.
fn is_descendant(nodes: &[Node], mut index: usize, ancestor: usize) -> bool {
    for _ in 0..nodes.len() {
        let Some(parent) = nodes.get(index).and_then(|n| n.parent) else { return false };
        if parent == ancestor {
            return true;
        }
        index = parent;
    }
    false
}

/// Scroll offset for `scroll_to_id`, or `None` when the scroll or the anchored descendant is missing.
fn scroll_to_offset(scene: &Scene, scroll_id: &str, target: &str, anchor: Option<[f64; 2]>) -> Option<(f64, f64)> {
    let scroll_idx = scene.nodes.iter().position(|n| n.id == scroll_id && n.scroll.is_some())?;
    let target_idx = (0..scene.nodes.len()).find(|&i| scene.nodes[i].anchor.as_deref() == Some(target) && is_descendant(&scene.nodes, i, scroll_idx))?;
    let sb = scene.nodes[scroll_idx].bounds;
    let info = scene.nodes[scroll_idx].scroll?;
    let tb = scene.nodes[target_idx].bounds;
    let a = anchor.unwrap_or([0.0, 0.0]);
    let x = scroll_anchor_offset(tb.x - sb.x + info.offset.0, sb.w, tb.w, a[0], info.max.0);
    let y = scroll_anchor_offset(tb.y - sb.y + info.offset.1, sb.h, tb.h, a[1], info.max.1);
    Some((x, y))
}

/// Anchored id whose content position on the scroll's main axis is the greatest still `<= offset + 1`.
fn current_anchor(scene: &Scene, scroll_id: &str) -> Option<String> {
    let idx = scene.nodes.iter().position(|n| n.id == scroll_id && n.scroll.is_some())?;
    let (vertical, origin, offset) = {
        let n = &scene.nodes[idx];
        let info = n.scroll?;
        if info.vertical { (true, n.bounds.y, info.offset.1) } else { (false, n.bounds.x, info.offset.0) }
    };
    let mut best: Option<(f64, String)> = None;
    for i in 0..scene.nodes.len() {
        let Some(anchor) = scene.nodes[i].anchor.clone() else { continue };
        if !is_descendant(&scene.nodes, i, idx) {
            continue;
        }
        let main = if vertical { scene.nodes[i].bounds.y } else { scene.nodes[i].bounds.x };
        let content = main - origin + offset;
        if content <= offset + 1.0 && best.as_ref().is_none_or(|(b, _)| content > *b) {
            best = Some((content, anchor));
        }
    }
    best.map(|(_, id)| id)
}

fn needs_geometry(n: &Node) -> bool {
    // The guest's first frame is the viewport at the origin. A matching size at a
    // different origin still needs one correction so `frame(in: .global)` is the screen frame.
    n.proposal.is_some_and(|[w, h]| {
        (n.bounds.w - w).abs() > 0.5 || (n.bounds.h - h).abs() > 0.5 || n.bounds.x.abs() > 0.5 || n.bounds.y.abs() > 0.5
    })
}

#[cfg(test)]
fn nodes_needing_geometry(scene: &Scene) -> Vec<&str> {
    scene.nodes.iter().filter(|n| needs_geometry(n)).map(|n| n.id.as_str()).collect()
}

fn geometry_changes(scene: &Scene, safe_top: f64, safe_bottom: f64, sent: &BTreeMap<String, GeoBox>) -> BTreeMap<String, GeoBox> {
    let mut frames = BTreeMap::new();
    for n in &scene.nodes {
        if n.proposal.is_none() {
            continue;
        }
        let geo = GeoBox { width: n.bounds.w, height: n.bounds.h, x: n.bounds.x, y: n.bounds.y, safe_top, safe_bottom, safe_leading: 0.0, safe_trailing: 0.0 };
        if let Some(prev) = sent.get(&n.id) {
            // A reader that returns to the origin still differs from the frame already sent.
            if prev != &geo {
                frames.insert(n.id.clone(), geo);
            }
            continue;
        }
        if needs_geometry(n) {
            frames.insert(n.id.clone(), geo);
        }
    }
    frames
}

/// First `.scrollPosition` scroll whose anchored target differs from the last writeback.
fn next_scroll_position(scene: &Scene, sent: &BTreeMap<String, Option<String>>) -> Option<(String, Option<String>)> {
    scene.nodes.iter().find_map(|n| {
        if n.value.as_deref() != Some("position") || n.scroll.is_none() {
            return None;
        }
        let target = current_anchor(scene, &n.id);
        match sent.get(&n.id) {
            Some(prev) if prev == &target => None,
            _ => Some((n.id.clone(), target)),
        }
    })
}

impl Default for Host {
    fn default() -> Self {
        Self::new()
    }
}

pub const COMMANDS: &[&str] = &[
    "hello",
    "get_capabilities",
    "create_session",
    "start_session",
    "stop_session",
    "reload_session",
    "get_session_status",
    "set_viewport",
    "set_appearance",
    "send_pointer_event",
    "send_keyboard_event",
    "capture_screenshot",
    "inspect_ui_tree",
    "get_errors",
    "shutdown",
];

impl Host {
    pub fn new() -> Self {
        Self { sessions: HashMap::new(), next_id: 1, fonts: None, renderer: renderer::RendererSlot::new(), greeted: false }
    }

    /// Choose the screenshot backend (default: `SIMCORE_RENDERER`, where `auto` means CPU headless).
    /// Here `Auto` tries the GPU and falls back to CPU, recording why in [`Host::renderer_diagnostics`].
    pub fn set_renderer(&mut self, pref: onesim_render::BackendPref) {
        self.renderer.set(pref);
    }

    /// Active backend, adapter, limitations, fallback reason and last-frame timings. Errors on an invalid
    /// `SIMCORE_RENDERER` or an unavailable explicit `gpu`.
    pub fn renderer_diagnostics(&mut self) -> Result<onesim_render::Diagnostics, ApiError> {
        self.renderer.diagnostics().map_err(|e| err(ErrorCode::RuntimeError, e))
    }

    /// React Native live mode for the calls that follow (`Session::settle`); restore `true` afterwards.
    pub fn set_settle(&mut self, id: &str, on: bool) {
        if let Some(s) = self.sessions.get_mut(id) {
            s.settle = on;
        }
    }

    pub(crate) fn get(&mut self, id: &str) -> Result<&mut Session, ApiError> {
        self.sessions.get_mut(id).ok_or_else(|| err(ErrorCode::NotFound, format!("unknown session {id}")))
    }

    pub fn create_session(&mut self, p: CreateSessionParams) -> Result<String, ApiError> {
        let id = format!("s{}", self.next_id);
        self.next_id += 1;
        self.create_session_as(id, p)
    }

    /// `create_session` with a caller-chosen id (the v2 server owns the id space).
    pub(crate) fn create_session_as(&mut self, id: String, mut p: CreateSessionParams) -> Result<String, ApiError> {
        if p.framework == "expo" {
            p.framework = "react-native".into();
        }
        if matches!(p.framework.as_str(), "jetpack-compose" | "android") {
            p.framework = "compose".into();
        }
        if !matches!(p.framework.as_str(), "swiftui" | "react-native" | "compose") {
            return Err(err(
                ErrorCode::Unsupported,
                format!("framework {:?} (\"swiftui\", \"react-native\", \"compose\")", p.framework),
            ));
        }
        // Compose runs on Android only; `platform` defaults to "ios" on the wire, so it is implied here.
        if p.framework == "compose" {
            p.platform = "android".into();
        }
        match (p.framework.as_str(), p.platform.as_str()) {
            (_, "ios") | ("compose" | "react-native", "android") => {}
            _ => return Err(err(ErrorCode::Unsupported, format!("platform {:?} for {}", p.platform, p.framework))),
        }
        if p.platform == "android" {
            p.device.get_or_insert_with(|| onesim_render::DEFAULT_ANDROID_DEVICE.into());
        }
        if matches!(p.framework.as_str(), "swiftui" | "compose") && !std::path::Path::new(&p.app_path).exists() {
            return Err(err(ErrorCode::BadRequest, format!("app_path {:?} does not exist", p.app_path)));
        }
        if let Some(d) = &p.device {
            if let Some(dev) = device(d) {
                check_device_platform(dev, &p.platform)?;
            } else {
                let known: Vec<&str> = DEVICES.iter().map(|d| d.name).collect();
                return Err(err(ErrorCode::Unsupported, format!("device {d:?} (known: {})", known.join(", "))));
            }
        }
        let log = EventLog::new(id.clone());
        self.sessions.insert(id.clone(), Session::new(id.clone(), p, log));
        Ok(id)
    }

    /// Blocks until the first tree arrives (or the build/launch fails).
    pub fn start_session(&mut self, id: &str) -> Result<(), ApiError> {
        let s = self.get(id)?;
        log::info!("starting session {id}");
        s.start()?;
        if self.get(id)?.is_rn() {
            // First layout pass: FlatList and friends render the rest once they know their frames.
            let _ = self.scene_within(id, STARTUP_MAX);
        }
        Ok(())
    }

    /// The session's event log (v2 API).
    pub fn events(&mut self, id: &str) -> Result<std::sync::Arc<EventLog>, ApiError> {
        Ok(self.get(id)?.log())
    }

    pub fn stop_session(&mut self, id: &str) -> Result<(), ApiError> {
        self.get(id)?.stop();
        Ok(())
    }

    pub fn state(&mut self, id: &str) -> Result<State, ApiError> {
        Ok(self.get(id)?.state())
    }

    pub fn status(&mut self, id: &str) -> Result<Value, ApiError> {
        let s = self.get(id)?;
        Ok(json!({
            "session_id": s.id, "state": s.state().as_str(), "framework": s.params.framework,
            "platform": s.params.platform, "app_path": s.params.app_path, "viewport": s.viewport,
            "device": s.device.name, "os_version": s.os_version(), "safe_area": {"top": s.device.safe_top, "bottom": s.device.safe_bottom},
            "focus": s.inter.focus, "island": s.device.island.map(|(x, y, w, h)| json!({"x": x, "y": y, "width": w, "height": h})),
            "scale": s.scale, "appearance": s.appearance, "tree_version": s.tree().map(|t| t.0),
        }))
    }

    pub fn set_viewport(&mut self, id: &str, width: f64, height: f64) -> Result<(), ApiError> {
        if !(width > 0.0 && height > 0.0) {
            return Err(err(ErrorCode::BadRequest, "width and height must be positive"));
        }
        let s = self.get(id)?;
        s.viewport = Viewport { width, height };
        Self::push_env(s)
    }

    pub fn set_appearance(&mut self, id: &str, mode: Appearance) -> Result<(), ApiError> {
        let s = self.get(id)?;
        s.appearance = mode;
        Self::push_env(s)
    }

    fn push_env(s: &mut Session) -> Result<(), ApiError> {
        if s.state() == State::Running {
            let m = HostMessage::Environment {
                appearance: s.appearance,
                viewport: s.viewport,
                dynamic_type_size: (s.dynamic_type != DynamicTypeSize::Large).then_some(s.dynamic_type),
            };
            s.send_and_wait_tree(&m)?;
        }
        Ok(())
    }

    /// Scene of the current tree. React Native sessions first settle: the guest's trees, layout
    /// frames, focus requests and `scroll_to` replies are exchanged until nothing changes.
    /// Running SwiftUI sessions settle `scroll_to_id`, geometry and `scroll_position` (4 rounds).
    fn scene(&mut self, id: &str) -> Result<(Scene, u64, f64), ApiError> {
        self.scene_within(id, ANIM_MAX)
    }

    /// `scene`, where an animation that keeps trees coming (layout unchanged) is waited on for `anim` at most.
    /// Paused sessions skip settling. SwiftUI uses a 4-round settle and ignores `anim`.
    fn scene_within(&mut self, id: &str, anim: Duration) -> Result<(Scene, u64, f64), ApiError> {
        // A paused guest cannot answer: serve the last frame as is.
        if self.get(id)?.state() == State::Paused {
            return self.build(id);
        }
        if !self.get(id)?.is_rn() {
            return if self.get(id)?.is_swiftui() { self.swiftui_settle(id) } else { self.build(id) };
        }
        let settle = self.get(id)?.settle;
        let (start, deadline) = (Instant::now(), Instant::now() + SETTLE_MAX.max(anim));
        loop {
            if settle {
                self.get(id)?.wait_quiet(QUIET, anim);
            }
            let generation = self.get(id)?.generation();
            let out = self.build(id)?;
            let acted = self.rn_apply_requests(id, &out.0);
            let s = self.get(id)?;
            let sent = s.sync_layout(&out.0);
            let animating = !acted && !sent && start.elapsed() >= anim;
            let done = !acted && !sent && s.generation() == generation;
            // Live: no waiting, but a host-side change (`scroll_to`, focus) still needs one rebuild,
            // since the guest may never send a tree for it.
            let live_done = !settle && !acted;
            if done || animating || live_done || Instant::now() > deadline {
                return Ok(out);
            }
        }
    }

    /// Apply the guest's `focus_request` / `scroll_to` messages and drop focus on a vanished field.
    /// Returns whether the interaction state changed (the scene must be rebuilt).
    fn rn_apply_requests(&mut self, id: &str, scene: &Scene) -> bool {
        let Some(s) = self.sessions.get_mut(id) else { return false };
        let Some((_, root)) = s.tree() else { return false };
        let cx = RnCtx { scene, root: &root, keyboard: s.keyboard_height() };
        let requests = s.take_requests();
        let (changed, out) = s.rn.apply_requests(&mut s.inter, &cx, requests);
        let _ = s.send_out(out);
        changed
    }

    /// Running SwiftUI: at most 4 builds. `scroll_to_id` rebuilds without waiting. Geometry and
    /// `scroll_position` wait for a tree only when `settle` is set; a silent guest stops the loop.
    fn swiftui_settle(&mut self, id: &str) -> Result<(Scene, u64, f64), ApiError> {
        let mut scene = self.build(id)?;
        let mut round = 0;
        loop {
            let rebuild = self.swiftui_round(id, &scene.0)?;
            round += 1;
            if !rebuild || round == 4 {
                return Ok(scene);
            }
            scene = self.build(id)?;
        }
    }

    /// `true` when this round changed host scroll state or the guest answered a correction with a tree.
    fn swiftui_round(&mut self, id: &str, scene: &Scene) -> Result<bool, ApiError> {
        if self.apply_scroll_ids(id, scene) {
            return Ok(true);
        }
        if let Some(again) = self.dispatch_geometry(id, scene)? {
            return Ok(again);
        }
        let frames = scene.pending_shape_frames();
        if !frames.is_empty() {
            return self.get(id)?.send_and_wait_tree(&HostMessage::Layout { frames });
        }
        let next = {
            let sent = &self.get(id)?.sent_positions;
            next_scroll_position(scene, sent)
        };
        if let Some((scroll, target)) = next {
            return self.dispatch_scroll_position(id, scroll, target);
        }
        Ok(false)
    }

    /// Apply queued `scroll_to_id` requests. Missing targets are dropped. Returns whether any offset changed.
    fn apply_scroll_ids(&mut self, id: &str, scene: &Scene) -> bool {
        let Some(s) = self.sessions.get_mut(id) else { return false };
        let mut changed = false;
        for r in s.take_requests() {
            let GuestMessage::ScrollToId { scroll, target, anchor } = r else { continue };
            let Some(off) = scroll_to_offset(scene, &scroll, &target, anchor) else { continue };
            let prev = scene.nodes.iter().find(|n| n.id == scroll && n.scroll.is_some()).and_then(|n| n.scroll).map(|info| info.offset);
            if prev.is_none_or(|p| offset_changed(p, off)) {
                changed = true;
            }
            s.inter.scroll.insert(scroll, off);
        }
        changed
    }

    /// `Some(again)` when a geometry correction was sent. `again` is true only if a new tree arrived.
    fn dispatch_geometry(&mut self, id: &str, scene: &Scene) -> Result<Option<bool>, ApiError> {
        let changed = {
            let s = self.get(id)?;
            geometry_changes(scene, s.device.safe_top, s.device.safe_bottom, &s.sent_geometry)
        };
        if changed.is_empty() {
            return Ok(None);
        }
        let settle = self.get(id)?.settle;
        let msg = HostMessage::Geometry { frames: changed.clone() };
        let arrived = if settle { self.get(id)?.send_and_wait_tree(&msg)? } else { self.get(id)?.send(&msg)?; false };
        self.get(id)?.sent_geometry.extend(changed);
        Ok(Some(settle && arrived))
    }

    /// Send one `scroll_position`. Stores the target only after the send succeeds.
    fn dispatch_scroll_position(&mut self, id: &str, scroll: String, target: Option<String>) -> Result<bool, ApiError> {
        let settle = self.get(id)?.settle;
        let msg = HostMessage::ScrollPosition { scroll: scroll.clone(), target: target.clone() };
        let s = self.get(id)?;
        let arrived = if settle { s.send_and_wait_tree(&msg)? } else { s.send(&msg)?; false };
        s.sent_positions.insert(scroll, target);
        Ok(settle && arrived)
    }

    /// The tree a React Native input is interpreted against (`RnCtx::root`).
    fn rn_root(s: &Session) -> Result<ViewNode, ApiError> {
        s.tree().map(|t| t.1).ok_or_else(|| err(ErrorCode::InvalidState, "no tree"))
    }

    fn build(&mut self, id: &str) -> Result<(Scene, u64, f64), ApiError> {
        let s = self.sessions.get_mut(id).ok_or_else(|| err(ErrorCode::NotFound, format!("unknown session {id}")))?;
        let (version, root) = s
            .tree()
            .ok_or_else(|| err(ErrorCode::InvalidState, format!("session {id} is {} and has no tree", s.state().as_str())))?;
        let gen = s.generation();
        let fonts = self.fonts.get_or_insert_with(Fonts::new);
        let params = SceneParams {
            viewport: (s.viewport.width, s.viewport.height),
            device: &s.device,
            scale: s.scale,
            appearance: s.appearance,
            dynamic_type: s.dynamic_type,
            ios: s.ios,
            scroll: &s.inter.scroll,
            sheets: &s.inter.sheets,
            focus: s.inter.focus.as_deref(),
            overscroll: s.inter.live(),
            status_bar: true,
            chrome: s.inter.chrome(),
        };
        let mut scene = build_scene(&root, &params, fonts);
        if s.is_swiftui() {
            s.sync_schedule();
            if gen != s.last_anim_gen {
                s.anim.update(&scene);
                s.last_anim_gen = gen;
            }
            if s.anim.is_animating() {
                scene = s.anim.render_scene(&scene);
            }
        }
        for name in &scene.unsupported_symbols {
            s.report_once("host", format!("unsupported_symbol: no system symbol {name:?} on this host; drew a placeholder"));
        }
        for note in &scene.notes {
            s.report_once("host", note.clone());
        }
        Ok((scene, version, s.scale))
    }

    /// Keep the SwiftUI guest's monotonic clock aligned with host `clock_ms` before an input
    /// starts new timers, animators, or completion callbacks.
    fn sync_guest_clock(&mut self, id: &str) -> Result<(), ApiError> {
        let needs_sync = {
            let s = self.get(id)?;
            s.is_swiftui() && s.state() == State::Running && (s.clock_ms - s.guest_clock_ms).abs() > 1e-6
        };
        if needs_sync {
            let tree_arrived = self.get(id)?.send_tick_and_wait()?;
            if tree_arrived {
                let _ = self.scene(id)?;
            }
        }
        Ok(())
    }

    /// Returns `(hit node id, action dispatched)`. `click` = down + up. A hit on a text field
    /// focuses it (action = false); any other click unfocuses. `scroll` hits the scroller and
    /// never reaches the guest. The state machine is `onesim_render::Interaction`.
    pub fn pointer(&mut self, id: &str, ev: PointerEvent) -> Result<(Option<String>, bool), ApiError> {
        self.get(id)?.require_running()?;
        self.sync_guest_clock(id)?;
        let (scene, version, _) = self.scene(id)?;
        if self.get(id)?.is_rn() {
            if ev.kind == PointerKind::Drag && scene.scroll_target(ev.x, ev.y).is_some_and(|i| scene.nodes[i].tag.is_some()) {
                return self.rn_whole_drag(id, &scene, ev);
            }
            return self.rn_pointer(id, &scene, version, ev);
        }
        let s = self.get(id)?;
        s.inter.version = version;
        let effect = s.inter.pointer(&scene, ev);
        let out = (s.inter.hit.clone(), s.inter.acted);
        s.send_effect(effect)?;
        Ok(out)
    }

    /// A whole `drag` on a React Native scroll view, run as live phases (each settles first): JS gets the touch and
    /// a move inside the slop, so a PanResponder around the list can claim it, before the scroll view takes it at the
    /// slop. Lifted at rest (no fling); the content lags the finger by the slop, as on iOS.
    fn rn_whole_drag(&mut self, id: &str, scene: &Scene, ev: PointerEvent) -> Result<(Option<String>, bool), ApiError> {
        use onesim_render::DragPhase;
        let hit = scene.hit_test(ev.x, ev.y).map(|i| scene.nodes[i].id.clone());
        let (x, y, len) = (ev.x, ev.y, ev.dx.hypot(ev.dy));
        let mut t = self.get(id)?.clock_ms;
        self.drag(id, DragPhase::Begin, x, y, t)?;
        // Inside the slop, just past it, then on to the end in 16 ms steps.
        let mut stops = vec![5.0, 11.0];
        stops.extend((1..=8).map(|i| len * f64::from(i) / 8.0));
        for d in stops.into_iter().filter(|d| *d <= len) {
            t += 16.0;
            let f = d / len;
            self.drag(id, DragPhase::Move, x + ev.dx * f, y + ev.dy * f, t)?;
        }
        self.drag(id, DragPhase::End, x + ev.dx, y + ev.dy, t + 1000.0)?;
        let _ = self.scene(id)?;
        Ok((hit, false))
    }

    /// React Native pointer input (`onesim_render::rn_input`), then settle and push the new layout.
    fn rn_pointer(&mut self, id: &str, scene: &Scene, version: u64, ev: PointerEvent) -> Result<(Option<String>, bool), ApiError> {
        let s = self.get(id)?;
        s.inter.version = version;
        let root = Self::rn_root(s)?;
        let cx = RnCtx { scene, root: &root, keyboard: s.keyboard_height() };
        let (hit, acted, out) = s.rn.pointer(&mut s.inter, &cx, ev);
        s.send_out(out)?;
        let _ = self.scene(id);
        Ok((hit, acted))
    }

    /// Live viewers (native window): lay out the current tree and hand the scene, the shared fonts and the tree
    /// version to `f`. Pair React Native sessions with `set_settle(id, false)` so this never waits for the guest.
    pub fn with_frame<R>(&mut self, id: &str, f: impl FnOnce(&Scene, &mut Fonts, u64) -> R) -> Result<R, ApiError> {
        let (scene, version, _) = self.scene(id)?;
        let fonts = self.fonts.get_or_insert_with(Fonts::new);
        Ok(f(&scene, fonts, version))
    }

    /// Whether the session has active SwiftUI animations, a scheduled guest tick, or a held gesture.
    pub fn is_animating(&mut self, id: &str) -> bool {
        let Some(s) = self.sessions.get_mut(id) else { return false };
        s.sync_schedule();
        s.anim.is_animating() || s.guest_due_ms.is_some() || s.inter.holding()
    }

    /// Advance scroll momentum, bounces, SwiftUI animations, and scheduled guest ticks by `dt` seconds.
    /// Returns whether anything is still moving (call again next frame); false means the scene is at rest.
    /// A finger resting on a SwiftUI gesture view is reported again each tick (long presses fire while held), and keeps
    /// the scene live.
    pub fn tick(&mut self, id: &str, dt: f64) -> Result<bool, ApiError> {
        let (scene, ..) = self.scene(id)?;
        let s = self.get(id)?;
        let dt = dt.max(0.0);
        s.clock_ms += dt * 1000.0;
        s.sync_schedule();
        let mut guest_ticked = false;
        if s.is_swiftui() && s.state() == State::Running && s.guest_due_ms.is_some_and(|due| s.clock_ms + 1e-6 >= due) {
            s.guest_due_ms = None;
            guest_ticked = s.send_tick_and_wait()?;
        }
        let anim_moving = s.anim.tick(dt);
        let scroll_moving = s.inter.tick(&scene, dt);
        if s.is_rn() {
            let out = s.rn.scrolled(&s.inter, &scene);
            s.send_out(out)?;
        }
        let holding = s.inter.holding();
        if holding {
            let held = s.inter.hold(dt);
            s.send_effect(held)?;
        }
        if guest_ticked {
            // Ingest any new tree the guest submitted on tick into `Animator`.
            let _ = self.scene(id)?;
        }
        let s = self.get(id)?;
        s.sync_schedule();
        Ok(anim_moving || scroll_moving || holding || s.anim.is_animating() || s.guest_due_ms.is_some())
    }

    /// Advance the session's animation clock deterministically by `ms` milliseconds in frame steps
    /// (headless CLI `--wait`). Ensures any preceding tree is ingested before stepping.
    pub fn wait(&mut self, id: &str, ms: f64) -> Result<(), ApiError> {
        let _ = self.scene(id)?;
        let mut left = ms.max(0.0);
        const STEP_MS: f64 = 16.0;
        while left > 1e-6 {
            let s = self.get(id)?;
            s.sync_schedule();
            // Step to the next guest due time or 16ms frame, whichever is smaller.
            let mut step = left.min(STEP_MS);
            if let Some(due) = s.guest_due_ms {
                let until = (due - s.clock_ms).max(0.0);
                if until > 1e-6 && until < step {
                    step = until;
                }
            }
            left -= step;
            let active = self.tick(id, step / 1000.0)?;
            if !active && left > 0.0 {
                // Still advance the clock if nothing is scheduled, in case a later step checks time.
                self.get(id)?.clock_ms += left;
                break;
            }
        }
        let _ = self.scene(id)?;
        Ok(())
    }

    /// One phase of a live finger drag (`t_ms` is the event time): the content follows the finger and carries on
    /// with its release velocity (see [`Host::tick`]); sheets follow too. Returns whether the scene changed.
    /// React Native receives each phase on the same Fabric target and supplies its own gesture behavior.
    pub fn drag(&mut self, id: &str, phase: onesim_render::DragPhase, x: f64, y: f64, t_ms: f64) -> Result<bool, ApiError> {
        self.get(id)?.require_running()?;
        self.sync_guest_clock(id)?;
        let (scene, version, _) = self.scene(id)?;
        let s = self.get(id)?;
        s.inter.version = version;
        // React Native: touches go to the touched Fabric view, scrolls to the host; later trees arrive through
        // render.updated, so the live caller never waits for a commit.
        if s.is_rn() {
            let root = Self::rn_root(s)?;
            let cx = RnCtx { scene: &scene, root: &root, keyboard: s.keyboard_height() };
            let out = s.rn.drag(&mut s.inter, &cx, phase, x, y, t_ms);
            let changed = out.iter().any(|o| matches!(o, Out::Effect(e) if *e != onesim_render::Effect::None));
            s.send_out(out)?;
            return Ok(changed);
        }
        let effect = s.inter.drag(&scene, phase, x, y, t_ms);
        let changed = effect != onesim_render::Effect::None;
        s.send_effect(effect)?;
        Ok(changed)
    }

    /// Keyboard input for the focused text field. The host computes the new text and sends it
    /// to the guest as `text_input`.
    pub fn keyboard(&mut self, id: &str, ev: KeyboardEvent) -> Result<(), ApiError> {
        self.get(id)?.require_running()?;
        self.sync_guest_clock(id)?;
        if let KeyboardEvent::Key { key } = &ev {
            if !matches!(key.as_str(), "backspace" | "enter") {
                return Err(err(ErrorCode::Unsupported, format!("unknown key {key:?} (backspace, enter)")));
            }
        }
        let (scene, version, _) = self.scene(id)?;
        let s = self.get(id)?;
        s.inter.version = version;
        if s.is_rn() {
            let root = Self::rn_root(s)?;
            let cx = RnCtx { scene: &scene, root: &root, keyboard: s.keyboard_height() };
            let out = s.rn.keyboard(&mut s.inter, &cx, ev).map_err(|m| err(ErrorCode::InvalidState, m))?;
            s.send_out(out)?;
            self.scene(id)?;
            return Ok(());
        }
        let effect = s.inter.keyboard(&scene, ev).map_err(|m| err(ErrorCode::InvalidState, m))?;
        s.send_effect(effect)
    }

    /// PNG bytes plus pixel size.
    pub fn screenshot(&mut self, id: &str) -> Result<(u32, u32, Vec<u8>), ApiError> {
        self.screenshot_sized(id, None, None, None).map(|(w, h, png, _)| (w, h, png))
    }

    /// Screenshot at a different resolution: `scale` = device pixels per point, or a target `width` /
    /// `height` in pixels (aspect kept; both = the largest size that fits). Also returns the tree version.
    pub fn screenshot_sized(
        &mut self,
        id: &str,
        scale: Option<f64>,
        width: Option<u32>,
        height: Option<u32>,
    ) -> Result<(u32, u32, Vec<u8>, u64), ApiError> {
        let (scene, version, dev_scale) = self.scene(id)?;
        let fit = |px: u32, pt: f64| f64::from(px) / pt;
        let scale = match (width, height) {
            (Some(w), Some(h)) => fit(w, scene.size.0).min(fit(h, scene.size.1)),
            (Some(w), None) => fit(w, scene.size.0),
            (None, Some(h)) => fit(h, scene.size.1),
            (None, None) => scale.unwrap_or(dev_scale),
        };
        let px = |v: f64| (v * scale).round().max(1.0) as u32;
        if !scale.is_finite() || scale <= 0.0 || px(scene.size.0).max(px(scene.size.1)) > 8192 {
            return Err(err(ErrorCode::BadRequest, "screenshot size must be between 1 and 8192 pixels"));
        }
        let fonts = self.fonts.get_or_insert_with(Fonts::new);
        let png = self
            .renderer
            .capture(&scene, fonts, scale)
            .and_then(|f| f.encode_png())
            .map_err(|e| err(ErrorCode::RuntimeError, e))?;
        Ok((px(scene.size.0), px(scene.size.1), png, version))
    }

    /// Switch the device profile: viewport, scale and chrome follow; the guest gets the new dimensions.
    pub fn set_device(&mut self, id: &str, name: &str) -> Result<(), ApiError> {
        let dev = *device(name).ok_or_else(|| err(ErrorCode::Unsupported, format!("device {name:?}")))?;
        let s = self.get(id)?;
        check_device_platform(&dev, &s.params.platform)?;
        s.device = dev;
        s.viewport = dev.viewport;
        s.scale = dev.scale;
        s.params.device = Some(dev.name.to_string());
        Self::push_env(s)
    }

    /// Hide the software keyboard by blurring the focused field.
    pub fn hide_keyboard(&mut self, id: &str) -> Result<(), ApiError> {
        let s = self.get(id)?;
        s.require_running()?;
        let before = s.inter.focus.take();
        if s.is_rn() {
            s.sync_focus(before)?;
            self.scene(id)?;
        }
        Ok(())
    }

    /// React Native native-module inventory reported by the guest (empty until it has booted).
    pub fn modules(&mut self, id: &str) -> Result<Value, ApiError> {
        let s = self.get(id)?;
        if !s.is_rn() {
            return Err(err(ErrorCode::Unsupported, "native modules exist only in react-native sessions"));
        }
        Ok(s.modules().unwrap_or_else(|| json!([])))
    }

    pub fn simulate(&mut self, id: &str, module: String, state: Value) -> Result<(), ApiError> {
        let s = self.get(id)?;
        if !s.is_rn() {
            return Err(err(ErrorCode::Unsupported, "native modules exist only in react-native sessions"));
        }
        s.require_running()?;
        s.send(&HostMessage::Simulate { module, state })
    }

    pub fn inspect(&mut self, id: &str) -> Result<(u64, UiNode), ApiError> {
        let (scene, version, _) = self.scene(id)?;
        Ok((version, scene.ui_tree()))
    }

    pub fn errors(&mut self, id: &str) -> Result<Vec<SessionError>, ApiError> {
        Ok(self.get(id)?.errors())
    }

    pub fn shutdown(&mut self) {
        self.sessions.values_mut().for_each(Session::stop);
    }

    /// Dispatch one JSON request. The bool is true after `shutdown`.
    pub fn handle(&mut self, req: Request) -> (Response, bool) {
        let shutdown = req.method == "shutdown" && (self.greeted);
        let r = self.dispatch(&req.method, req.params);
        if shutdown && r.is_ok() {
            self.shutdown();
        }
        (Response::from_result(req.id, r), shutdown)
    }

    fn dispatch(&mut self, method: &str, params: Value) -> Result<Value, ApiError> {
        fn p<T: serde::de::DeserializeOwned>(v: Value) -> Result<T, ApiError> {
            serde_json::from_value(v).map_err(|e| err(ErrorCode::BadRequest, format!("bad params: {e}")))
        }
        fn ok<T: Serialize>(v: T) -> Result<Value, ApiError> {
            Ok(serde_json::to_value(v).expect("serialize"))
        }
        if method == "hello" {
            let h: HelloParams = p(params)?;
            if h.protocol_version != PROTOCOL_VERSION {
                return Err(err(
                    ErrorCode::ProtocolVersion,
                    format!("server speaks protocol {PROTOCOL_VERSION}, client sent {}", h.protocol_version),
                ));
            }
            self.greeted = true;
            return ok(json!({"protocol_version": PROTOCOL_VERSION, "server": "onesim", "version": env!("CARGO_PKG_VERSION")}));
        }
        if !self.greeted {
            return Err(err(ErrorCode::BadRequest, "first request must be hello"));
        }
        match method {
            "get_capabilities" => ok(json!({
                "frameworks": ["swiftui", "react-native", "compose"], "platforms": ["ios", "android"], "commands": COMMANDS, "headless": true,
                "devices": DEVICES.iter().map(|d| d.name).collect::<Vec<_>>(),
                "pointer_events": ["down", "up", "click", "scroll"],
                "keyboard": {"events": ["text", "key"], "keys": ["backspace", "enter"]},
                "views": ["text", "vstack", "hstack", "zstack", "spacer", "button", "color", "rectangle", "shape", "divider",
                          "scroll", "list", "navigation", "toggle", "stepper", "text_field", "picker", "slider", "progress", "image", "label", "empty"],
                "symbols": onesim_render::symbols_capability(),
                "fonts": onesim_render::fonts_capability(),
            })),
            "create_session" => ok(json!({"session_id": self.create_session(p(params)?)?})),
            "start_session" => {
                let a: SessionIdParams = p(params)?;
                self.start_session(&a.session_id)?;
                ok(json!({"state": self.state(&a.session_id)?.as_str()}))
            }
            "stop_session" => {
                let a: SessionIdParams = p(params)?;
                self.stop_session(&a.session_id)?;
                ok(json!({"state": "stopped"}))
            }
            "reload_session" => {
                let a: SessionIdParams = p(params)?;
                self.get(&a.session_id)?.reload()?;
                ok(json!({"state": self.state(&a.session_id)?.as_str()}))
            }
            "get_session_status" => self.status(&p::<SessionIdParams>(params)?.session_id),
            "set_viewport" => {
                let a: SetViewportParams = p(params)?;
                self.set_viewport(&a.session_id, a.width, a.height)?;
                ok(json!({}))
            }
            "set_appearance" => {
                let a: SetAppearanceParams = p(params)?;
                self.set_appearance(&a.session_id, a.mode)?;
                ok(json!({}))
            }
            "send_pointer_event" => {
                let a: PointerParams = p(params)?;
                let (hit, action) = self.pointer(&a.session_id, a.event)?;
                ok(json!({"hit": hit, "action": action}))
            }
            "send_keyboard_event" => {
                let a: KeyboardParams = p(params)?;
                self.keyboard(&a.session_id, a.event)?;
                ok(json!({}))
            }
            "capture_screenshot" => {
                let a: ScreenshotParams = p(params)?;
                let (width, height, png) = self.screenshot(&a.session_id)?;
                match a.path {
                    Some(path) => {
                        std::fs::write(&path, png).map_err(|e| err(ErrorCode::RuntimeError, format!("write {path}: {e}")))?;
                        ok(json!({"width": width, "height": height, "path": path}))
                    }
                    None => ok(json!({"width": width, "height": height,
                        "png_base64": base64::engine::general_purpose::STANDARD.encode(png)})),
                }
            }
            "inspect_ui_tree" => {
                let (tree_version, root) = self.inspect(&p::<SessionIdParams>(params)?.session_id)?;
                ok(json!({"tree_version": tree_version, "root": root}))
            }
            "get_errors" => ok(json!({"errors": self.errors(&p::<SessionIdParams>(params)?.session_id)?})),
            "shutdown" => ok(json!({})),
            m => Err(err(ErrorCode::BadRequest, format!("unknown method {m:?}"))),
        }
    }

    /// JSON-lines server loop: one request per line in, one response per line out.
    pub fn serve(&mut self, input: impl BufRead, mut out: impl Write) -> std::io::Result<()> {
        for line in input.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let (resp, done) = match serde_json::from_str::<Request>(&line) {
                Ok(req) => self.handle(req),
                Err(e) => (Response::from_result(0, Err(err(ErrorCode::BadRequest, format!("invalid request: {e}")))), false),
            };
            writeln!(out, "{}", serde_json::to_string(&resp).expect("serialize"))?;
            out.flush()?;
            if done {
                break;
            }
        }
        self.shutdown();
        Ok(())
    }
}

#[cfg(test)]
mod settle_tests {
    use super::{current_anchor, geometry_changes, nodes_needing_geometry, scroll_to_offset};
    use onesim_protocol::{Appearance, GeoBox, IosVersion};
    use onesim_render::{Node, Rect, Scene, ScrollInfo};
    use std::collections::BTreeMap;

    fn scene(nodes: Vec<Node>) -> Scene {
        Scene { size: (200.0, 400.0), appearance: Appearance::Light, ios: IosVersion::default(), nodes, unsupported_symbols: vec![], notes: vec![] }
    }

    fn scroll_node(offset_y: f64) -> Node {
        let mut n = Node::new("s", "scroll");
        n.bounds = Rect { x: 0.0, y: 10.0, w: 0.0, h: 100.0 };
        n.scroll = Some(ScrollInfo { vertical: true, offset: (0.0, offset_y), max: (0.0, 500.0), snap: None, content: None, page: None, aligned: false });
        n
    }

    /// Content position is `bounds.y - 10 + offset_y`.
    fn anchored(id: &str, anchor: &str, parent: usize, content_y: f64, offset_y: f64) -> Node {
        let mut n = Node::new(id, "row");
        n.parent = Some(parent);
        n.anchor = Some(anchor.into());
        n.bounds.y = content_y - offset_y + 10.0;
        n.bounds.h = 20.0;
        n
    }

    #[test]
    fn scroll_to_id_offset_aligns_anchor() {
        let mut mid = Node::new("mid", "box");
        mid.parent = Some(0);
        let mut row = Node::new("row", "row");
        row.parent = Some(1);
        row.anchor = Some("there".into());
        row.bounds.y = 210.0;
        row.bounds.h = 20.0;
        let scene = scene(vec![scroll_node(40.0), mid, row]);
        assert_eq!(scroll_to_offset(&scene, "s", "there", None), Some((0.0, 240.0)));
        assert_eq!(scroll_to_offset(&scene, "s", "there", Some([0.0, 0.5])), Some((0.0, 200.0)));
    }

    #[test]
    fn scroll_position_anchor_is_greatest_at_or_above_the_top() {
        let mut mid = Node::new("mid", "box");
        mid.parent = Some(0);
        let two = scene(vec![scroll_node(100.0), mid.clone(), anchored("row0", "0", 1, 0.0, 100.0), anchored("row180", "180", 0, 180.0, 100.0)]);
        assert_eq!(current_anchor(&two, "s").as_deref(), Some("0"));
        let three = scene(vec![
            scroll_node(100.0),
            mid,
            anchored("row0", "0", 1, 0.0, 100.0),
            anchored("row180", "180", 0, 180.0, 100.0),
            anchored("row150", "150", 0, 150.0, 100.0),
        ]);
        assert_eq!(current_anchor(&three, "s").as_deref(), Some("0"));
        assert_ne!(current_anchor(&three, "s").as_deref(), Some("150"));
        let greatest = scene(vec![
            scroll_node(200.0),
            anchored("low", "low", 0, 40.0, 200.0),
            anchored("high", "high", 0, 180.0, 200.0),
        ]);
        assert_eq!(current_anchor(&greatest, "s").as_deref(), Some("high"));
        let none = scene(vec![scroll_node(0.0), anchored("row50", "50", 0, 50.0, 0.0)]);
        assert_eq!(current_anchor(&none, "s"), None);
    }

    #[test]
    fn geometry_correction_when_bounds_differ_from_proposal() {
        let mut needs = Node::new("needs", "geometry");
        needs.proposal = Some([10.0, 10.0]);
        needs.bounds = Rect { x: 1.0, y: 2.0, w: 120.0, h: 80.0 };
        let mut close = Node::new("close", "geometry");
        close.proposal = Some([120.0, 80.0]);
        close.bounds = Rect { x: 0.0, y: 0.0, w: 120.5, h: 80.5 };
        let mut open = Node::new("open", "geometry");
        open.proposal = None;
        open.bounds = Rect { x: 0.0, y: 0.0, w: 120.0, h: 80.0 };
        let mut placed = Node::new("placed", "geometry");
        placed.proposal = Some([40.0, 20.0]);
        placed.bounds = Rect { x: 4.0, y: 8.0, w: 40.0, h: 20.0 };
        assert_eq!(nodes_needing_geometry(&scene(vec![needs, close, open, placed.clone()])), vec!["needs", "placed"]);
        let placed_scene = scene(vec![placed]);
        let first = geometry_changes(&placed_scene, 62.0, 34.0, &BTreeMap::new());
        assert_eq!((first["placed"].x, first["placed"].y), (4.0, 8.0));
        assert!(geometry_changes(&placed_scene, 62.0, 34.0, &first).is_empty());
        let mut home = Node::new("home", "geometry");
        home.proposal = Some([40.0, 20.0]);
        home.bounds = Rect { x: 0.0, y: 0.0, w: 40.0, h: 20.0 };
        let mut sent = BTreeMap::new();
        sent.insert(
            "home".to_string(),
            GeoBox { width: 40.0, height: 20.0, x: 4.0, y: 8.0, safe_top: 62.0, safe_bottom: 34.0, safe_leading: 0.0, safe_trailing: 0.0 },
        );
        let back = geometry_changes(&scene(vec![home]), 62.0, 34.0, &sent);
        assert_eq!((back["home"].x, back["home"].y), (0.0, 0.0));
    }
}
