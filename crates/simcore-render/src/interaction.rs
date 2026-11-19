//! Transport-agnostic interaction state machine (docs/protocol.md section D/E).
//!
//! Owns scroll offsets, text focus, software-keyboard shift and the press target. Feed it pointer
//! and keyboard events together with the scene they hit; it returns what the transport must do:
//! the native host sends the effect over the guest wire, the browser calls the Swift callbacks.
use crate::{autocap, ChromeParams, Role, Scene, SheetPos};
use simcore_protocol::{KeyboardEvent, PointerEvent, PointerKind};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Nothing changed.
    None,
    /// Deliver a button/toggle/link action to the guest.
    Action { id: u64, version: u64 },
    /// Deliver new text for a field to the guest.
    TextInput { field: u64, version: u64, text: String },
    /// Only interaction state changed (scroll, focus, shift): re-layout, no guest call.
    Redraw,
}

#[derive(Debug, Default)]
pub struct Interaction {
    /// Scroll offsets by node id; survive across trees.
    pub scroll: HashMap<String, (f64, f64)>,
    /// Detent and drag position of presented sheets by layer id; survive across trees while shown.
    pub sheets: HashMap<String, SheetPos>,
    /// Node id of the focused text field.
    pub focus: Option<String>,
    /// Software keyboard shift state. Key taps, focus and `keyboard` text/backspace update it (auto-capitalization).
    pub shift: bool,
    /// Current tree version; the transport sets it whenever a tree arrives. Stamped on effects.
    pub version: u64,
    /// Node id hit by the last `pointer` (the scroller for scroll events).
    pub hit: Option<String>,
    /// Whether the last `pointer` was an action (control fired or keyboard key tapped).
    pub acted: bool,
    pressed: Option<String>,
    /// Finger currently scrolling (or holding) a scroller.
    drag: Option<Drag>,
    /// Finger down inside a sheet, before it is clear whether it drags the sheet or scrolls its content.
    pending: Option<SheetDrag>,
    sheet_drag: Option<SheetDrag>,
    /// Scrollers in flight: node id -> (velocity along the axis in pt/s, spring target). Without a
    /// target it is momentum (and a bounce past an edge); with one, a spring to that offset.
    motion: HashMap<String, (f64, Option<f64>)>,
}

#[derive(Debug)]
struct Drag {
    id: String,
    /// Unrubbered offset along the axis: where the content would be without edge resistance.
    raw: f64,
    /// Last finger position along the axis (points).
    last: f64,
    /// Recent (time ms, finger position) samples for the release velocity.
    samples: Vec<(f64, f64)>,
}

/// A finger gesture on a sheet card.
#[derive(Debug, Clone)]
struct SheetDrag {
    id: String,
    tops: Vec<f64>,
    detent: usize,
    top0: f64,
    dismiss: Option<u64>,
    start: (f64, f64),
    last: f64,
    samples: Vec<(f64, f64)>,
}

/// Dragging down past this fraction of the way from the smallest detent to the screen bottom dismisses.
const SHEET_DISMISS_FRAC: f64 = 0.25;
/// A release projects the sheet this far ahead along the finger's velocity (s).
const SHEET_PROJECT: f64 = 0.15;

/// Drag gesture phases (`Interaction::drag`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragPhase {
    Begin,
    Move,
    End,
}

// UIScrollView feel. ponytail: constants fitted by eye to iOS, the knobs to retune against recordings.
/// Edge resistance coefficient (UIScrollView's rubber band).
const RUBBER: f64 = 0.55;
/// Momentum decay rate, 1/s: `UIScrollView.DecelerationRate.normal` is 0.998 per ms.
const DECEL: f64 = 2.002;
/// Bounce spring angular frequency, rad/s (critically damped: stiffness 100, damping 20).
const SPRING: f64 = 10.0;
/// Below this speed (pt/s) motion stops.
const MIN_V: f64 = 10.0;
/// Release velocity uses the finger samples of this window (ms); an older last sample means the
/// finger rested before lifting, so no fling.
const VELOCITY_WINDOW: f64 = 100.0;

/// Displayed overshoot for `x` points dragged past an edge of a `d`-point viewport.
pub fn rubber(x: f64, d: f64) -> f64 {
    (1.0 - 1.0 / (x * RUBBER / d + 1.0)) * d
}

/// Offset as shown for a raw (unresisted) offset within `[0, max]`.
fn shown(raw: f64, max: f64, d: f64) -> f64 {
    if raw < 0.0 {
        -rubber(-raw, d)
    } else if raw > max {
        max + rubber(raw - max, d)
    } else {
        raw
    }
}

/// Inverse of `shown`: picking up content mid-bounce keeps it under the finger.
fn unshown(o: f64, max: f64, d: f64) -> f64 {
    let un = |f: f64| f * d / (RUBBER * (d - f).max(1e-6));
    if o < 0.0 {
        -un(-o)
    } else if o > max {
        max + un(o - max)
    } else {
        o
    }
}

/// Advance one scroller by `dt` seconds: exponential momentum inside `[0, max]`, a critically
/// damped spring back to the edge outside it, or a spring to `to` when given. Returns the new
/// (offset, velocity); velocity 0 = at rest.
pub fn step(mut o: f64, mut v: f64, max: f64, dt: f64, to: Option<f64>) -> (f64, f64) {
    // 1 ms sub-steps: exact enough, and momentum hands over to the spring the moment it crosses an edge.
    let h = 0.001;
    for _ in 0..(dt / h).round().max(1.0) as usize {
        let edge = to.unwrap_or(o.clamp(0.0, max));
        if to.is_none() && o == edge {
            v *= (-DECEL * h).exp();
            if v.abs() < MIN_V {
                return (o, 0.0);
            }
        } else {
            let x = o - edge;
            if x.abs() < 0.1 && v.abs() < MIN_V {
                return (edge, 0.0);
            }
            v += (-SPRING * SPRING * x - 2.0 * SPRING * v) * h;
        }
        o += v * h;
    }
    (o, v)
}

impl Interaction {
    /// A finger is on a scroller or one is still moving: layout should show overscroll
    /// (`SceneParams::overscroll`).
    pub fn live(&self) -> bool {
        self.drag.is_some() || !self.motion.is_empty()
    }

    /// Chrome parameters for the next layout (pair with `focus.as_deref()`).
    pub fn chrome(&self) -> ChromeParams {
        ChromeParams { keyboard: self.focus.is_some(), shift: self.shift }
    }

    /// `click` = down + up. A hit on a text field focuses it; any other click unfocuses. `scroll`
    /// hits the scroller and never reaches the guest.
    pub fn pointer(&mut self, scene: &Scene, ev: PointerEvent) -> Effect {
        self.forget_gone(scene);
        self.hit = None;
        self.acted = false;
        if let Some(e) = self.slide(scene, &ev) {
            return e;
        }
        if ev.kind == PointerKind::Drag {
            // One whole gesture; the end time is long after the last move, so there is no fling.
            let (ex, ey) = (ev.x + ev.dx, ev.y + ev.dy);
            self.drag(scene, DragPhase::Begin, ev.x, ev.y, 0.0);
            self.drag(scene, DragPhase::Move, ex, ey, 16.0);
            let e = self.drag(scene, DragPhase::End, ex, ey, 1000.0);
            self.pressed = None;
            return if matches!(e, Effect::Action { .. }) { self.acted = true; e } else if self.hit.is_some() { Effect::Redraw } else { Effect::None };
        }
        if ev.kind == PointerKind::Scroll {
            let Some(i) = scene.scroll_target(ev.x, ev.y) else { return Effect::None };
            let n = &scene.nodes[i];
            let info = n.scroll.expect("scroll_target has scroll");
            let (ox, oy) = info.offset;
            let new = ((ox + ev.dx).clamp(0.0, info.max.0), (oy + ev.dy).clamp(0.0, info.max.1));
            self.hit = Some(n.id.clone());
            self.motion.remove(&n.id);
            self.scroll.insert(n.id.clone(), new);
            return if new == info.offset { Effect::None } else { Effect::Redraw };
        }
        // A touch on moving content stops it (and is not a tap); lifting releases it.
        if ev.kind == PointerKind::Down {
            if let Some(n) = scene.scroll_target(ev.x, ev.y).map(|i| &scene.nodes[i]) {
                if self.motion.remove(&n.id).is_some() {
                    self.catch(n, ev.x, ev.y);
                    return Effect::Redraw;
                }
            }
        }
        if ev.kind == PointerKind::Up && self.drag.take().is_some() {
            return Effect::Redraw;
        }
        let hit = scene.hit_test(ev.x, ev.y).map(|i| &scene.nodes[i]);
        self.hit = hit.map(|n| n.id.clone());
        let on_keyboard = scene.nodes.iter().any(|n| n.kind == "keyboard" && n.bounds.contains(ev.x, ev.y));
        let key = hit.filter(|n| n.role == Role::Key).and_then(|n| n.label.clone());
        let shifted = scene.nodes.iter().any(|n| n.kind == "keyboard" && n.value.as_deref() == Some("shift"));
        let released = match ev.kind {
            PointerKind::Down => {
                self.pressed = self.hit.clone();
                false
            }
            PointerKind::Up => self.pressed.take().is_some_and(|p| Some(&p) == self.hit.as_ref()),
            _ => true,
        };
        let fire = if released { hit.and_then(|n| n.action) } else { None };
        if let Some(label) = key.filter(|_| released) {
            self.acted = true;
            return self.press_key(&label, shifted, scene);
        }
        let before = (self.focus.clone(), self.shift);
        if ev.kind != PointerKind::Down && !on_keyboard {
            let new = hit.filter(|n| n.field.is_some()).map(|n| n.id.clone());
            if new != self.focus {
                self.shift = hit.and_then(|n| n.value.as_deref()).is_some_and(autocap);
            }
            self.focus = new;
        }
        if let Some(id) = fire {
            self.acted = true;
            return Effect::Action { id, version: self.version };
        }
        if before != (self.focus.clone(), self.shift) {
            Effect::Redraw
        } else {
            Effect::None
        }
    }

    /// A tap (click / up) or a drag starting on a slider sets its value where the finger ends.
    fn slide(&mut self, scene: &Scene, ev: &PointerEvent) -> Option<Effect> {
        if !matches!(ev.kind, PointerKind::Click | PointerKind::Up | PointerKind::Drag) {
            return None;
        }
        let n = scene.hit_test(ev.x, ev.y).map(|i| &scene.nodes[i]).filter(|n| n.role == Role::Slider)?;
        let ((lo, hi, thumb), field) = (n.range?, n.field?);
        let end = if ev.kind == PointerKind::Drag { ev.x + ev.dx } else { ev.x };
        let frac = ((end - n.bounds.x - thumb / 2.0) / (n.bounds.w - thumb).max(1.0)).clamp(0.0, 1.0);
        self.pressed = None;
        self.hit = Some(n.id.clone());
        self.acted = true;
        Some(Effect::TextInput { field, version: self.version, text: format!("{}", lo + frac * (hi - lo)) })
    }

    /// Drop scroll state of scrollers that left the screen. Node ids are positional, so the next
    /// screen pushed at the same depth reuses them; like the guest's `@State`, a popped screen's
    /// offsets must not carry over (iOS starts every pushed view at the top).
    fn forget_gone(&mut self, scene: &Scene) {
        let live: std::collections::HashSet<&str> = scene.nodes.iter().filter(|n| n.scroll.is_some()).map(|n| n.id.as_str()).collect();
        self.scroll.retain(|id, _| live.contains(id.as_str()));
        self.motion.retain(|id, _| live.contains(id.as_str()));
        let sheets: std::collections::HashSet<&str> = scene.nodes.iter().filter(|n| n.sheet.is_some()).map(|n| n.id.as_str()).collect();
        self.sheets.retain(|id, _| sheets.contains(id.as_str()));
        if self.pending.as_ref().or(self.sheet_drag.as_ref()).is_some_and(|d| !sheets.contains(d.id.as_str())) {
            self.pending = None;
            self.sheet_drag = None;
        }
        if self.drag.as_ref().is_some_and(|d| !live.contains(d.id.as_str())) {
            self.drag = None;
        }
    }

    fn catch(&mut self, n: &crate::Node, x: f64, y: f64) {
        let info = n.scroll.expect("scroller");
        let (o, max, d, f) = axis(n, info.offset, x, y);
        self.drag = Some(Drag { id: n.id.clone(), raw: unshown(o, max, d), last: f, samples: vec![] });
    }

    /// A finger drag (touch scrolling). `Begin` at the touch-down point, `Move` as it moves, `End`
    /// when it lifts; `t` is the event time in ms. The content follows the finger, rubber-banding
    /// past the edges; on release it carries on with the finger's velocity (see `tick`).
    pub fn drag(&mut self, scene: &Scene, phase: DragPhase, x: f64, y: f64, t: f64) -> Effect {
        self.forget_gone(scene);
        if let Some(e) = self.sheet_gesture(scene, phase, x, y, t) {
            return e;
        }
        match phase {
            DragPhase::Begin => self.scroll_begin(scene, x, y, t),
            DragPhase::Move => {
                let Some(d) = self.drag.as_mut() else { return Effect::None };
                let Some(n) = scene.nodes.iter().find(|n| n.id == d.id) else { return Effect::None };
                let info = n.scroll.expect("scroller");
                let (_, max, dim, f) = axis(n, info.offset, x, y);
                d.raw += d.last - f;
                d.last = f;
                d.samples.push((t, f));
                d.samples.retain(|s| t - s.0 <= VELOCITY_WINDOW);
                let o = shown(d.raw, max, dim);
                let new = if info.vertical { (info.offset.0, o) } else { (o, info.offset.1) };
                self.hit = Some(d.id.clone());
                self.scroll.insert(d.id.clone(), new);
                if new == info.offset { Effect::None } else { Effect::Redraw }
            }
            DragPhase::End => {
                let Some(d) = self.drag.take() else { return Effect::None };
                let fresh = d.samples.last().is_some_and(|s| t - s.0 <= VELOCITY_WINDOW);
                let v = match (d.samples.first(), fresh) {
                    (Some(a), true) if t > a.0 => -(d.last - a.1) / ((t - a.0) / 1000.0),
                    _ => 0.0,
                };
                let settle = scene.nodes.iter().find(|n| n.id == d.id).is_some_and(|n| {
                    let info = n.scroll.expect("scroller");
                    let (o, max, ..) = axis(n, info.offset, x, y);
                    !(0.0..=max).contains(&o) || info.snap.is_some_and(|s| o > 0.0 && o < s)
                });
                // Released past an edge or mid-collapse: settle on the next tick even at zero velocity.
                if v.abs() >= MIN_V || settle {
                    self.motion.insert(d.id, (v, None));
                }
                Effect::None
            }
        }
    }

    fn scroll_begin(&mut self, scene: &Scene, x: f64, y: f64, t: f64) -> Effect {
        let Some(n) = scene.scroll_target(x, y).map(|i| &scene.nodes[i]) else { return Effect::None };
        self.pressed = None;
        self.motion.remove(&n.id);
        if self.drag.as_ref().is_none_or(|d| d.id != n.id) {
            self.catch(n, x, y);
        }
        let d = self.drag.as_mut().expect("caught");
        d.samples = vec![(t, d.last)];
        Effect::None
    }

    /// Sheet swipes. A finger down inside the topmost sheet card is held back until it moves: the
    /// sheet follows it unless the sheet is at its largest detent over content that can still
    /// scroll that way, in which case the content scrolls (UIKit's rule). Returns `None` when the
    /// gesture is the scroll engine's.
    fn sheet_gesture(&mut self, scene: &Scene, phase: DragPhase, x: f64, y: f64, t: f64) -> Option<Effect> {
        match phase {
            DragPhase::Begin => {
                self.pending = None;
                self.sheet_drag = None;
                let scrim = scene.nodes.iter().rposition(|n| n.kind == "scrim");
                let i = scene.nodes.iter().rposition(|n| n.sheet.is_some() && n.bounds.contains(x, y))?;
                if scrim.is_some_and(|s| s > i) {
                    return None; // a layer above (alert, another sheet) owns the touch
                }
                let (n, info) = (&scene.nodes[i], scene.nodes[i].sheet.as_ref()?);
                self.pressed = None;
                self.pending = Some(SheetDrag {
                    id: n.id.clone(),
                    tops: info.tops.clone(),
                    detent: info.detent,
                    top0: info.top,
                    dismiss: info.dismiss,
                    start: (x, y),
                    last: y,
                    samples: vec![(t, y)],
                });
                Some(Effect::None)
            }
            DragPhase::Move => {
                if let Some(p) = self.pending.take() {
                    let dy = y - p.start.1;
                    if dy.abs() < 2.0 {
                        self.pending = Some(p);
                        return Some(Effect::None);
                    }
                    let at_largest = p.detent + 1 >= p.tops.len();
                    let content = scene.scroll_target(p.start.0, p.start.1).map(|i| &scene.nodes[i]);
                    let scrolls = at_largest && content.and_then(|n| n.scroll).is_some_and(|s| dy < 0.0 || s.offset.1 > 0.0);
                    if scrolls {
                        // Hand the touch to the scroller, from where it started.
                        self.sheet_drag = None;
                        self.scroll_begin(scene, p.start.0, p.start.1, t);
                        return None;
                    }
                    self.sheet_drag = Some(p);
                }
                let d = self.sheet_drag.as_mut()?;
                d.last = y;
                d.samples.push((t, y));
                d.samples.retain(|s| t - s.0 <= VELOCITY_WINDOW);
                // Not above the largest detent, not below the screen's bottom edge.
                let top = (d.top0 + (y - d.start.1)).clamp(*d.tops.last().unwrap_or(&0.0), scene.size.1);
                self.hit = Some(d.id.clone());
                let pos = self.sheets.entry(d.id.clone()).or_default();
                let changed = pos.top != Some(top);
                *pos = SheetPos { detent: d.detent, top: Some(top) };
                Some(if changed { Effect::Redraw } else { Effect::None })
            }
            DragPhase::End => {
                if self.pending.take().is_some() {
                    return Some(Effect::None);
                }
                let d = self.sheet_drag.take()?;
                self.hit = Some(d.id.clone());
                let fresh = d.samples.last().is_some_and(|s| t - s.0 <= VELOCITY_WINDOW);
                let v = match (d.samples.first(), fresh) {
                    (Some(a), true) if t > a.0 => (d.last - a.1) / ((t - a.0) / 1000.0),
                    _ => 0.0,
                };
                let top = (d.top0 + (d.last - d.start.1)).clamp(*d.tops.last().unwrap_or(&0.0), scene.size.1) + v * SHEET_PROJECT;
                // Past the dismiss line: gone. Otherwise the nearest detent.
                let low = d.tops.first().copied().unwrap_or(0.0);
                let line = low + SHEET_DISMISS_FRAC * (scene.size.1 - low);
                if top > line {
                    if let Some(id) = d.dismiss {
                        self.sheets.insert(d.id, SheetPos { detent: d.detent, top: None });
                        return Some(Effect::Action { id, version: self.version });
                    }
                }
                let near = (0..d.tops.len()).min_by(|&a, &b| (d.tops[a] - top).abs().total_cmp(&(d.tops[b] - top).abs())).unwrap_or(0);
                self.sheets.insert(d.id, SheetPos { detent: near, top: None });
                Some(Effect::Redraw)
            }
        }
    }

    /// Advance scroll momentum and bounces by `dt` seconds. Returns true while anything is moving
    /// (keep calling every frame), after updating `scroll`: re-layout when it returns true.
    pub fn tick(&mut self, scene: &Scene, dt: f64) -> bool {
        self.forget_gone(scene);
        let held = self.drag.as_ref().map(|d| d.id.clone());
        let mut moving = false;
        for n in &scene.nodes {
            let Some(info) = n.scroll else { continue };
            if held.as_ref() == Some(&n.id) {
                continue;
            }
            let (o, max, ..) = axis(n, info.offset, 0.0, 0.0);
            let (o, v, to) = match self.motion.get(&n.id) {
                Some(&(v, to)) => {
                    let (o, v) = step(o, v, max, dt, to);
                    (o, v, to)
                }
                None if (0.0..=max).contains(&o) => continue,
                // Stale overscroll (the content shrank under it): settle at once, like iOS.
                None => (o.clamp(0.0, max), 0.0, None),
            };
            // Never at rest mid-collapse of a large title: spring to expanded or collapsed, whichever is nearer.
            let snap = info.snap.filter(|&s| info.vertical && to.is_none() && o > 0.0 && o < s);
            if let (0.0, Some(s)) = (v, snap) {
                self.motion.insert(n.id.clone(), (0.0, Some(if o < s / 2.0 { 0.0 } else { s })));
                moving = true;
            } else if v == 0.0 {
                self.motion.remove(&n.id);
            } else {
                self.motion.insert(n.id.clone(), (v, to));
                moving = true;
            }
            let new = if info.vertical { (info.offset.0, o) } else { (o, info.offset.1) };
            self.scroll.insert(n.id.clone(), new);
            moving |= new != info.offset;
        }
        moving
    }

    fn focused_field(&self, scene: &Scene) -> Option<(u64, String)> {
        let n = self.focus.as_ref().and_then(|f| scene.nodes.iter().find(|n| &n.id == f && n.field.is_some()))?;
        Some((n.field.expect("filtered"), n.value.clone().unwrap_or_default()))
    }

    /// A tap on a software-keyboard key (`label` as exposed by the inspection tree).
    fn press_key(&mut self, label: &str, shifted: bool, scene: &Scene) -> Effect {
        let Some((field, mut text)) = self.focused_field(scene) else { return Effect::None };
        match label {
            "shift" => {
                self.shift = !shifted;
                return Effect::Redraw;
            }
            "return" => {
                self.focus = None;
                return Effect::Redraw;
            }
            "delete" => {
                text.pop();
            }
            "space" => text.push(' '),
            // ponytail: no number/emoji/dictation layers; those keys are inert.
            l if l.chars().count() == 1 => text.push_str(&if shifted { l.to_uppercase() } else { l.to_string() }),
            _ => return Effect::None,
        }
        // Typing a letter drops shift; a space after a sentence end or an empty field raises it.
        self.shift = if label.chars().count() == 1 { false } else { autocap(&text) };
        Effect::TextInput { field, version: self.version, text }
    }

    /// Keyboard input for the focused text field: text is appended, `backspace` pops, any other
    /// key (`enter`) unfocuses. Err when no field is focused.
    pub fn keyboard(&mut self, scene: &Scene, ev: KeyboardEvent) -> Result<Effect, &'static str> {
        self.forget_gone(scene);
        let (field, mut text) = self.focused_field(scene).ok_or("no focused text field (click a text field first)")?;
        match ev {
            KeyboardEvent::Text { text: t } => text.push_str(&t),
            KeyboardEvent::Key { key } if key == "backspace" => {
                text.pop();
            }
            KeyboardEvent::Key { .. } => {
                self.focus = None;
                return Ok(Effect::Redraw);
            }
        }
        // Text arrives through the software keyboard, so its shift follows like for key taps.
        self.shift = autocap(&text);
        Ok(Effect::TextInput { field, version: self.version, text })
    }
}

/// Point (points) to click for a label in the inspection tree. Syntax: `Label` (button / toggle
/// switch / text field placeholder / keyboard key), `Label#increment` or `Label#decrement`
/// (stepper whose label is `Label`).
pub fn resolve_label(tree: &simcore_protocol::UiNode, spec: &str) -> Result<(f64, f64), String> {
    use simcore_protocol::UiNode;
    fn find<'a>(n: &'a UiNode, pred: &dyn Fn(&UiNode) -> bool) -> Option<&'a UiNode> {
        if pred(n) {
            return Some(n);
        }
        n.children.iter().find_map(|c| find(c, pred))
    }
    let center = |n: &UiNode| (n.frame.x + n.frame.width / 2.0, n.frame.y + n.frame.height / 2.0);
    if let Some((label, part)) = spec.rsplit_once('#') {
        if matches!(part, "increment" | "decrement") {
            let st = find(tree, &|n| n.role.as_deref() == Some("stepper") && n.label.as_deref() == Some(label))
                .ok_or_else(|| format!("no stepper labelled {label:?}"))?;
            let b = find(st, &|n| n.id.ends_with(&format!("#{part}"))).ok_or("stepper half not found")?;
            return Ok(center(b));
        }
    }
    // Software keyboard keys (`h`, `space`, `delete`, `shift`, `return`, `123`) win when it is up.
    let key = find(tree, &|n| n.role.as_deref() == Some("key") && n.label.as_deref().is_some_and(|l| l.eq_ignore_ascii_case(spec)));
    let n = match key {
        Some(k) => k,
        None => find(tree, &|n| {
            matches!(n.role.as_deref(), Some("button" | "toggle" | "textfield")) && n.label.as_deref() == Some(spec)
        })
        // A native stack header's back pill is labelled with the previous title; `Back` always finds it.
        .or_else(|| find(tree, &|n| spec.eq_ignore_ascii_case("back") && n.id.ends_with("#back")))
        .ok_or_else(|| format!("no button, toggle, text field or key labelled {spec:?}"))?,
    };
    Ok(center(n))
}

/// A scroller along its axis: (offset, max offset, viewport length, finger coordinate).
fn axis(n: &crate::Node, off: (f64, f64), x: f64, y: f64) -> (f64, f64, f64, f64) {
    let info = n.scroll.expect("scroller");
    if info.vertical {
        (off.1, info.max.1, n.bounds.h, y)
    } else {
        (off.0, info.max.0, n.bounds.w, x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settle(mut o: f64, mut v: f64, max: f64) -> (f64, f64, f64) {
        let mut peak = o;
        for frame in 0..600 {
            (o, v) = step(o, v, max, 1.0 / 60.0, None);
            peak = peak.max(o);
            if v == 0.0 {
                return (o, peak, frame as f64 / 60.0);
            }
        }
        panic!("still moving after 10 s: o={o} v={v}");
    }

    #[test]
    fn rubber_band_resists_and_round_trips() {
        assert_eq!(rubber(0.0, 800.0), 0.0);
        assert!((rubber(100.0, 800.0) - 51.46).abs() < 0.01, "{}", rubber(100.0, 800.0));
        assert!(rubber(1e9, 800.0) < 800.0, "never past the viewport");
        for o in [-120.0, -3.0, 0.0, 500.0, 1000.0, 1090.0] {
            assert!((shown(unshown(o, 1000.0, 800.0), 1000.0, 800.0) - o).abs() < 1e-6);
        }
    }

    #[test]
    fn flick_decelerates_like_uiscrollview() {
        // v0 / ln(1/0.998 per ms): a 1000 pt/s flick travels ~500 pt.
        let (o, _, t) = settle(0.0, 1000.0, 1e6);
        assert!((o - 495.0).abs() < 10.0, "travelled {o}");
        assert!((1.5..2.5).contains(&t), "took {t}s");
    }

    #[test]
    fn flick_into_edge_bounces_back() {
        let (o, peak, t) = settle(900.0, 3000.0, 1000.0);
        assert_eq!(o, 1000.0, "rests at the edge");
        assert!((1050.0..1300.0).contains(&peak), "overshoot peak {peak}");
        assert!(t < 1.5, "settles in {t}s");
    }

    #[test]
    fn released_overscroll_springs_back() {
        let (o, _, t) = settle(-80.0, 0.0, 1000.0);
        assert_eq!(o, 0.0);
        assert!((0.3..0.9).contains(&t), "bounce back took {t}s");
    }

    #[test]
    fn popped_screen_offsets_do_not_carry_over() {
        use crate::scene::{Node, Rect, ScrollInfo};
        let screen = |with_scroller: bool| {
            let mut n = Node::new("0/s1", "scroll");
            n.bounds = Rect { x: 0.0, y: 0.0, w: 400.0, h: 800.0 };
            n.scroll = Some(ScrollInfo { vertical: true, offset: (0.0, 0.0), max: (0.0, 500.0), snap: None, content: None });
            let nodes = if with_scroller { vec![n] } else { vec![] };
            Scene { size: (400.0, 800.0), appearance: simcore_protocol::Appearance::Light,
                ios: Default::default(), nodes, unsupported_symbols: vec![], notes: vec![] }
        };
        let mut it = Interaction::default();
        let ev = |kind, dy| PointerEvent { kind, x: 100.0, y: 100.0, dx: 0.0, dy };
        assert_eq!(it.pointer(&screen(true), ev(PointerKind::Scroll, 120.0)), Effect::Redraw);
        assert_eq!(it.scroll.get("0/s1"), Some(&(0.0, 120.0)));
        // Back to a screen without it (the pop), then a new screen reusing the id starts at the top.
        it.pointer(&screen(false), ev(PointerKind::Click, 0.0));
        assert!(it.scroll.is_empty() && !it.live());
    }

    #[test]
    fn large_title_never_rests_half_collapsed() {
        use crate::scene::{Node, Rect, ScrollInfo};
        let scene = |off: f64| {
            let mut n = Node::new("l", "list");
            n.bounds = Rect { x: 0.0, y: 0.0, w: 400.0, h: 800.0 };
            n.scroll = Some(ScrollInfo { vertical: true, offset: (0.0, off), max: (0.0, 500.0), snap: Some(52.0), content: None });
            Scene { size: (400.0, 800.0), appearance: simcore_protocol::Appearance::Light,
                ios: Default::default(), nodes: vec![n], unsupported_symbols: vec![], notes: vec![] }
        };
        for (start, want) in [(20.0, 0.0), (40.0, 52.0)] {
            let mut it = Interaction::default();
            // A slow drag to `start`, released without a fling.
            it.drag(&scene(0.0), DragPhase::Begin, 200.0, 400.0, 0.0);
            it.drag(&scene(0.0), DragPhase::Move, 200.0, 400.0 - start, 500.0);
            it.drag(&scene(start), DragPhase::End, 200.0, 400.0 - start, 900.0);
            let mut off = start;
            for _ in 0..120 {
                if !it.tick(&scene(off), 1.0 / 60.0) {
                    break;
                }
                off = it.scroll["l"].1;
            }
            assert!((off - want).abs() < 1e-9, "released at {start}: rests at {off}, want {want}");
            assert!(!it.live());
        }
    }
}
