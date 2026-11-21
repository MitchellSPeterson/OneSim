//! Transport-agnostic interaction state machine (docs/protocol.md section D/E).
//!
//! Owns scroll offsets, text focus, software-keyboard shift and the press target. Feed it pointer
//! and keyboard events together with the scene they hit; it returns what the transport must do:
//! the native host sends the effect over the guest wire, the browser calls the Swift callbacks.
use crate::{autocap, chrome::Keypad, ChromeParams, Role, Scene, SheetPos};
use onesim_protocol::{Axis, GestureTarget, GestureTouch, KeyboardEvent, PointerEvent, PointerKind, TouchPhase};
use std::collections::HashMap;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Nothing changed.
    None,
    /// Deliver a button/toggle/link action to the guest.
    Action { id: u64, version: u64 },
    /// Deliver new text for a field to the guest.
    TextInput { field: u64, version: u64, text: String },
    /// Deliver touches to SwiftUI gesture views, in order (protocol section L). Not version-gated.
    Gesture(Vec<GestureTouch>),
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
    slider_drag: Option<String>,
    /// Finger currently scrolling (or holding) a scroller.
    drag: Option<Drag>,
    /// Finger down inside a sheet, before it is clear whether it drags the sheet or scrolls its content.
    pending: Option<SheetDrag>,
    sheet_drag: Option<SheetDrag>,
    /// Scrollers in flight: node id -> (velocity along the axis in pt/s, spring target). Without a
    /// target it is momentum (and a bounce past an edge); with one, a spring to that offset.
    motion: HashMap<String, (f64, Option<f64>)>,
    /// Fingers on SwiftUI gesture views (section L).
    touch: Option<Touch>,
    /// Time (ms) of the last synthesized touch (`pointer`), so whole gestures never overlap in time.
    clock: f64,
    focus_tree: Option<u64>,
    keypad: Option<Keypad>,
    phone_symbols: bool,
}

/// A touch sequence owned by SwiftUI gesture views.
#[derive(Debug)]
struct Touch {
    targets: Vec<GestureTarget>,
    /// Whether any target follows a moving finger; if none does, a scroller under it takes a moving touch.
    pans: bool,
    start: (f64, f64),
    points: Vec<[f64; 2]>,
    time: f64,
    /// Touch time last reported by `hold`.
    held: f64,
    /// First caller timestamp seen and the touch time it maps to: a touch begun by `pointer` (wall clock) and
    /// continued by `drag` (the transport's clock) keeps one time line.
    base: Option<(f64, f64)>,
}

impl Touch {
    /// Move the touch to time `t` on the caller's clock (never backwards).
    fn at(&mut self, t: f64) {
        let (t0, time0) = *self.base.get_or_insert((t, self.time));
        self.time = self.time.max(time0 + (t - t0));
    }
}

/// A finger must move this far (pt) before a scroller takes a touch from a tap-only gesture view (UIScrollView's slop).
pub(crate) const PAN_SLOP: f64 = 10.0;
/// Synthesized whole gestures (`drag`, `pinch`, `rotate`) move in this many steps, one per frame.
const GESTURE_STEPS: u32 = 8;
const FRAME_MS: f64 = 16.0;
/// A synthesized long press reports the held finger this often (ms).
const HOLD_MS: f64 = 100.0;
/// A resting live finger is reported at most this often (ms).
const LIVE_HOLD_MS: f64 = 50.0;
/// Synthesized two-finger gestures start with the fingers this far apart (pt), side by side.
const FINGER_GAP: f64 = 100.0;

/// Wall-clock milliseconds. ponytail: 0 on wasm32-unknown-unknown (no clock); only the GPU presenter builds there and it sends no input.
pub(crate) fn now_ms() -> f64 {
    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    return 0.0;
    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64() * 1000.0)
}

#[derive(Debug)]
struct Drag {
    id: String,
    /// Decide the scroll axis on the first move when a pager shares the touch with a vertical scroller.
    pager_start: Option<(f64, f64)>,
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
    let decay = (-DECEL * h).exp();
    for _ in 0..(dt / h).round().max(1.0) as usize {
        let edge = to.unwrap_or(o.clamp(0.0, max));
        if to.is_none() && o == edge {
            v *= decay;
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

/// Paging rest offsets: `0, page, 2*page, ...` up to `max`, plus `max` when it is not already a stop.
pub fn page_stops(page: f64, max: f64) -> Vec<f64> {
    if page <= 0.0 {
        return if max > 0.0 { vec![0.0, max] } else { vec![0.0] };
    }
    let mut stops = Vec::new();
    let mut x = 0.0;
    let mut guard = 0;
    while x <= max && guard < 100_000 {
        stops.push(x);
        x += page;
        guard += 1;
    }
    if stops.last().is_none_or(|s| (max - *s).abs() > 0.5) {
        stops.push(max);
    }
    stops
}

/// Closest stop. Ties go to the lower stop.
pub fn nearest_stop(offset: f64, stops: &[f64]) -> f64 {
    let Some(&first) = stops.first() else { return offset };
    let mut best = first;
    let mut best_d = (offset - best).abs();
    for &s in stops.iter().skip(1) {
        let d = (offset - s).abs();
        if d < best_d || ((d - best_d).abs() <= 1e-9 && s < best) {
            best = s;
            best_d = d;
        }
    }
    best
}

/// Offset that puts `content` (a target's position in the scrolled content) at the same anchor as the viewport.
pub fn scroll_anchor_offset(content: f64, viewport: f64, target: f64, anchor: f64, max: f64) -> f64 {
    (content - (viewport - target) * anchor).clamp(0.0, max)
}

/// Rest stops for a paging or view-aligned scroller. Empty when it has neither.
fn scroll_rest_stops(scene: &Scene, index: usize) -> Vec<f64> {
    let n = &scene.nodes[index];
    let Some(info) = n.scroll else { return vec![] };
    let (_, max, ..) = axis(n, info.offset, 0.0, 0.0);
    if let Some(page) = info.page {
        return page_stops(page, max);
    }
    if !info.aligned {
        return vec![];
    }
    let mut stops = vec![0.0];
    let base = if info.vertical { n.bounds.y } else { n.bounds.x };
    let off = if info.vertical { info.offset.1 } else { info.offset.0 };
    for (j, t) in scene.nodes.iter().enumerate() {
        if j == index || !t.scroll_target || !under(scene, j, index) {
            continue;
        }
        let main = if info.vertical { t.bounds.y } else { t.bounds.x };
        stops.push((main - base + off).clamp(0.0, max));
    }
    stops
}

fn under(scene: &Scene, mut i: usize, ancestor: usize) -> bool {
    while let Some(p) = scene.nodes[i].parent {
        if p == ancestor {
            return true;
        }
        i = p;
    }
    false
}

impl Interaction {
    /// Whether the scroller `id` is still moving on its own (momentum or a bounce).
    pub fn moving(&self, id: &str) -> bool {
        self.motion.contains_key(id)
    }

    pub fn has_drag_target(&self) -> bool {
        self.slider_drag.is_some() || self.drag.is_some()
    }
    /// A finger is on a scroller or one is still moving: layout should show overscroll
    /// (`SceneParams::overscroll`).
    pub fn live(&self) -> bool {
        self.drag.is_some() || !self.motion.is_empty()
    }

    /// Chrome parameters for the next layout (pair with `focus.as_deref()`).
    pub fn chrome(&self) -> ChromeParams {
        ChromeParams { keyboard: self.focus.is_some(), shift: self.shift, keypad: self.keypad.is_some(), phone_symbols: self.phone_symbols }
    }

    /// Apply guest focus requests once per tree. Unbound inputs keep host-owned focus.
    pub fn sync_text_focus(&mut self, scene: &Scene, version: u64) -> bool {
        if self.focus_tree == Some(version) {
            return false;
        }
        self.focus_tree = Some(version);
        let inputs = || scene.nodes.iter().filter(|n| n.field.is_some() && !n.disabled && n.kind == "text_field");
        let requested = || inputs().filter(|n| n.input.as_ref().and_then(|i| i.focused) == Some(true));
        let next = requested().find(|n| self.focus.as_ref() == Some(&n.id))
            .or_else(|| requested().next())
            .or_else(|| inputs().find(|n| self.focus.as_ref() == Some(&n.id) && n.input.as_ref().and_then(|i| i.focused).is_none()));
        self.set_text_focus(next)
    }

    fn set_text_focus(&mut self, node: Option<&crate::Node>) -> bool {
        let focus = node.map(|n| n.id.clone());
        let keypad = node.and_then(|n| Keypad::for_input(n.input.as_deref()?));
        if self.focus == focus && self.keypad == keypad {
            return false;
        }
        self.focus = focus;
        self.keypad = keypad;
        self.phone_symbols = false;
        self.shift = node.is_some_and(|n| input_shift(n, n.value.as_deref().unwrap_or_default()));
        true
    }

    /// `click` = down + up. A hit on a text field focuses it; any other click unfocuses. `scroll`
    /// hits the scroller and never reaches the guest.
    pub fn pointer(&mut self, scene: &Scene, ev: PointerEvent) -> Effect {
        self.forget_gone(scene);
        self.hit = None;
        self.acted = false;
        if let Some(e) = self.gesture_pointer(scene, &ev) {
            return e;
        }
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
            let Some(i) = scene.scroll_target_where(ev.x, ev.y, |n| n.pager.is_none()) else { return Effect::None };
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
        if ev.kind == PointerKind::Up {
            if let Some(d) = self.drag.take() {
                if scene.nodes.iter().any(|n| n.id == d.id && n.pager.is_some()) {
                    self.scroll.remove(&d.id);
                }
                return Effect::Redraw;
            }
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
        let previous = self.focused_field(scene);
        let mut focus_action = None;
        if ev.kind != PointerKind::Down && !on_keyboard {
            let next = hit.filter(|n| n.field.is_some() && !n.disabled);
            // A control can move or clear a bound focus explicitly in its action.
            let retain = fire.is_some() && previous.is_some_and(|n| n.input.as_ref().is_some_and(|i| i.focused.is_some()));
            if !retain && self.set_text_focus(next) {
                focus_action = next.and_then(|n| n.input.as_ref()?.focus_action)
                    .or_else(|| previous.and_then(|n| n.input.as_ref()?.blur_action));
            }
        }
        if let Some(id) = fire.or(focus_action) {
            self.acted = fire.is_some();
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
        self.slide_to(n, if ev.kind == PointerKind::Drag { ev.x + ev.dx } else { ev.x })
    }

    fn slide_to(&mut self, n: &crate::Node, x: f64) -> Option<Effect> {
        let ((lo, hi, thumb), field) = (n.range?, n.field?);
        let frac = ((x - n.bounds.x - thumb / 2.0) / (n.bounds.w - thumb).max(1.0)).clamp(0.0, 1.0);
        self.pressed = None;
        self.hit = Some(n.id.clone());
        self.acted = true;
        Some(Effect::TextInput { field, version: self.version, text: format!("{}", lo + frac * (hi - lo)) })
    }

    /// Drop scroll state of scrollers that left the screen. Node ids are positional, so the next
    /// screen pushed at the same depth reuses them; like the guest's `@State`, a popped screen's
    /// offsets must not carry over (iOS starts every pushed view at the top).
    fn forget_gone(&mut self, scene: &Scene) {
        let has_scroll = !self.scroll.is_empty() || !self.motion.is_empty() || self.drag.is_some();
        let has_sheet = !self.sheets.is_empty() || self.pending.is_some() || self.sheet_drag.is_some();
        if !has_scroll && !has_sheet {
            return;
        }
        if has_scroll {
            let live: std::collections::HashSet<&str> = scene.nodes.iter().filter(|n| n.scroll.is_some()).map(|n| n.id.as_str()).collect();
            self.scroll.retain(|id, _| live.contains(id.as_str()));
            self.motion.retain(|id, _| live.contains(id.as_str()));
            if self.drag.as_ref().is_some_and(|d| !live.contains(d.id.as_str())) {
                self.drag = None;
            }
        }
        if has_sheet {
            let sheets: std::collections::HashSet<&str> = scene.nodes.iter().filter(|n| n.sheet.is_some()).map(|n| n.id.as_str()).collect();
            self.sheets.retain(|id, _| sheets.contains(id.as_str()));
            if self.pending.as_ref().or(self.sheet_drag.as_ref()).is_some_and(|d| !sheets.contains(d.id.as_str())) {
                self.pending = None;
                self.sheet_drag = None;
            }
        }
    }

    fn catch(&mut self, n: &crate::Node, x: f64, y: f64) {
        let info = n.scroll.expect("scroller");
        let (o, max, d, f) = axis(n, info.offset, x, y);
        self.drag = Some(Drag { id: n.id.clone(), pager_start: None, raw: unshown(o, max, d), last: f, samples: vec![] });
    }

    /// A finger drag (touch scrolling). `Begin` at the touch-down point, `Move` as it moves, `End`
    /// when it lifts; `t` is the event time in ms. The content follows the finger, rubber-banding
    /// past the edges; on release it carries on with the finger's velocity (see `tick`).
    pub fn drag(&mut self, scene: &Scene, phase: DragPhase, x: f64, y: f64, t: f64) -> Effect {
        self.forget_gone(scene);
        if let Some(e) = self.touch_drag(scene, phase, x, y, t) {
            return e;
        }
        if phase == DragPhase::Begin {
            self.slider_drag = scene
                .hit_test(x, y)
                .map(|i| &scene.nodes[i])
                .filter(|n| n.role == Role::Slider && n.range.is_some() && n.field.is_some())
                .map(|n| n.id.clone());
            if self.slider_drag.is_some() {
                self.drag = None;
                self.pending = None;
                self.sheet_drag = None;
                return Effect::None;
            }
        } else if let Some(id) = &self.slider_drag {
            let target = scene.nodes.iter().find(|n| n.id == *id && n.role == Role::Slider);
            if phase == DragPhase::End {
                self.slider_drag = None;
            }
            return target.and_then(|n| self.slide_to(n, x)).unwrap_or(Effect::None);
        }
        if let Some(e) = self.sheet_gesture(scene, phase, x, y, t) {
            return e;
        }
        match phase {
            DragPhase::Begin => self.scroll_begin(scene, x, y, t),
            DragPhase::Move => {
                if let Some((sx, sy)) = self.drag.as_mut().and_then(|d| d.pager_start.take()) {
                    if (x - sx).hypot(y - sy) < PAN_SLOP {
                        self.drag.as_mut().expect("drag exists").pager_start = Some((sx, sy));
                        return Effect::None;
                    }
                    let vertical = (y - sy).abs() > (x - sx).abs();
                    let target = scene.scroll_target_where(sx, sy, |n| n.scroll.is_some_and(|s| s.vertical == vertical));
                    let start = self.drag.take().and_then(|d| d.samples.first().copied()).map_or(t, |(time, _)| time);
                    if let Some(i) = target {
                        self.catch(&scene.nodes[i], sx, sy);
                        let d = self.drag.as_mut().expect("caught");
                        d.samples.push((start, d.last));
                    }
                }
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
                let id = d.id;
                if let Some((idx, n)) = scene.nodes.iter().enumerate().find(|(_, n)| n.id == id) {
                    let info = n.scroll.expect("scroller");
                    if let Some(pager) = &n.pager {
                        let moved = self.scroll.remove(&id).is_some();
                        let idle = pager.selected as f64 * n.bounds.w;
                        let projected = d.raw + d.last - x + v * 0.18 - idle;
                        let next = if projected > n.bounds.w / 2.0 { pager.selected + 1 }
                            else if projected < -n.bounds.w / 2.0 { pager.selected.saturating_sub(1) } else { pager.selected };
                        if let Some(action) = pager.actions.get(next).copied().flatten().filter(|_| next != pager.selected) {
                            self.acted = true;
                            return Effect::Action { id: action, version: self.version };
                        }
                        return if moved { Effect::Redraw } else { Effect::None };
                    }
                    let (o, max, ..) = axis(n, info.offset, x, y);
                    let in_title = info.snap.is_some_and(|s| info.vertical && o > 0.0 && o < s);
                    let outside = !(0.0..=max).contains(&o);
                    let stops = scroll_rest_stops(scene, idx);
                    let off_stop = !stops.is_empty() && stops.iter().all(|s| (o - *s).abs() > 0.5);
                    // A coast keeps today's velocity. At rest, page / view stops win unless a large
                    // title is mid-collapse or the only thing to settle is an edge.
                    if v.abs() >= MIN_V || in_title || (outside && !off_stop) {
                        self.motion.insert(id, (v, None));
                    } else if off_stop {
                        self.motion.insert(id, (0.0, Some(nearest_stop(o, &stops))));
                    } else if outside {
                        self.motion.insert(id, (v, None));
                    }
                }
                Effect::None
            }
        }
    }

    /// The gesture views a finger landing at (x, y) belongs to, innermost first, or `None` when there are none or a
    /// control (button, field, slider, key) above the innermost one takes the touch, as a `Button` inside a view with
    /// gestures does on iOS. Sets `hit` to the innermost gesture view.
    fn touch_at(&mut self, scene: &Scene, x: f64, y: f64, time: f64) -> Option<Touch> {
        let targets = scene.gesture_targets(x, y);
        let &inner = targets.first()?;
        if scene.hit_test(x, y).is_some_and(|c| c > inner) {
            return None;
        }
        self.hit = Some(scene.nodes[inner].id.clone());
        let nodes = targets.iter().map(|&i| &scene.nodes[i]);
        Some(Touch {
            pans: nodes.clone().any(|n| n.gesture.as_ref().is_some_and(|g| g.pans)),
            targets: nodes
                .map(|n| GestureTarget { key: n.gesture.as_ref().map(|g| g.key.clone()).unwrap_or_default(), frame: [n.bounds.x, n.bounds.y, n.bounds.w, n.bounds.h] })
                .collect(),
            start: (x, y),
            points: vec![[x, y]],
            time,
            held: time,
            base: Some((time, time)),
        })
    }

    fn touch_event(t: &Touch, phase: TouchPhase) -> GestureTouch {
        GestureTouch { phase, time: t.time, points: t.points.clone(), targets: t.targets.clone() }
    }

    /// Live fingers on gesture views. Returns `None` when the touch is not (or no longer) the gesture views'.
    fn touch_drag(&mut self, scene: &Scene, phase: DragPhase, x: f64, y: f64, t: f64) -> Option<Effect> {
        match phase {
            DragPhase::Begin => {
                // A touch already down here (a `pointer` down, then the finger moved): it carries on.
                if let Some(touch) = self.touch.as_mut().filter(|tc| tc.start == (x, y)) {
                    touch.base = None;
                    touch.at(t);
                    return Some(Effect::None);
                }
                self.touch = None;
                let touch = self.touch_at(scene, x, y, t)?;
                self.pressed = None;
                self.drag = None;
                self.pending = None;
                self.sheet_drag = None;
                self.slider_drag = None;
                let e = Self::touch_event(&touch, TouchPhase::Start);
                self.touch = Some(touch);
                Some(Effect::Gesture(vec![e]))
            }
            DragPhase::Move => {
                let touch = self.touch.as_mut()?;
                let (sx, sy) = touch.start;
                if !touch.pans && scene.scroll_target(sx, sy).is_some() && (x - sx).hypot(y - sy) > PAN_SLOP {
                    // The scroller takes over from where the finger came down; the gesture views see a cancel.
                    let mut touch = self.touch.take()?;
                    touch.at(t);
                    self.scroll_begin(scene, sx, sy, t);
                    self.drag(scene, DragPhase::Move, x, y, t);
                    return Some(Effect::Gesture(vec![Self::touch_event(&touch, TouchPhase::Cancel)]));
                }
                touch.points = vec![[x, y]];
                touch.at(t);
                Some(Effect::Gesture(vec![Self::touch_event(touch, TouchPhase::Move)]))
            }
            DragPhase::End => {
                let mut touch = self.touch.take()?;
                touch.points = vec![[x, y]];
                touch.at(t);
                Some(Effect::Gesture(vec![Self::touch_event(&touch, TouchPhase::End)]))
            }
        }
    }

    /// While a finger rests on a gesture view, report it again `dt` seconds later (no movement), so a long press can
    /// fire before the finger lifts. Live transports call this every frame; `Effect::None` without a finger.
    pub fn hold(&mut self, dt: f64) -> Effect {
        let Some(touch) = self.touch.as_mut() else { return Effect::None };
        touch.time += dt * 1000.0;
        touch.base = None; // the next caller timestamp continues from here
        // Every frame would re-evaluate the guest for nothing; a long press needs far less.
        if touch.time - touch.held < LIVE_HOLD_MS {
            return Effect::None;
        }
        touch.held = touch.time;
        Effect::Gesture(vec![Self::touch_event(touch, TouchPhase::Move)])
    }

    /// Whether a finger is down on a gesture view (call `hold` every frame meanwhile).
    pub fn holding(&self) -> bool {
        self.touch.is_some()
    }

    /// Whole pointer gestures on gesture views. Times are wall-clock ms, never before the previous gesture's end.
    fn gesture_pointer(&mut self, scene: &Scene, ev: &PointerEvent) -> Option<Effect> {
        let t0 = now_ms().max(self.clock);
        let (x, y) = (ev.x, ev.y);
        let mut out = vec![];
        match ev.kind {
            PointerKind::Down => return self.touch_drag(scene, DragPhase::Begin, x, y, t0),
            PointerKind::Up => {
                let touch = self.touch.as_mut()?;
                touch.base = Some((t0, t0)); // wall clock again, as at the down
                let e = self.touch_drag(scene, DragPhase::End, x, y, t0);
                self.acted = true;
                return e;
            }
            PointerKind::Click => {
                let mut touch = self.touch_at(scene, x, y, t0)?;
                out.push(Self::touch_event(&touch, TouchPhase::Start));
                touch.time += FRAME_MS * 3.0;
                out.push(Self::touch_event(&touch, TouchPhase::End));
            }
            PointerKind::LongPress => {
                let mut touch = self.touch_at(scene, x, y, t0)?;
                let end = t0 + ev.duration.unwrap_or(1.0) * 1000.0;
                out.push(Self::touch_event(&touch, TouchPhase::Start));
                while touch.time + HOLD_MS < end {
                    touch.time += HOLD_MS;
                    out.push(Self::touch_event(&touch, TouchPhase::Move));
                }
                touch.time = end;
                out.push(Self::touch_event(&touch, TouchPhase::End));
            }
            PointerKind::Pinch | PointerKind::Rotate => {
                let mut touch = self.touch_at(scene, x, y, t0)?;
                let (scale, deg) = (ev.scale.unwrap_or(1.0), ev.degrees.unwrap_or(0.0));
                for i in 0..=GESTURE_STEPS {
                    let f = f64::from(i) / f64::from(GESTURE_STEPS);
                    let (half, a) = (FINGER_GAP / 2.0 * (1.0 + (scale - 1.0) * f), (deg * f).to_radians());
                    let (dx, dy) = (half * a.cos(), half * a.sin());
                    touch.points = vec![[x - dx, y - dy], [x + dx, y + dy]];
                    touch.time = t0 + FRAME_MS * f64::from(i);
                    out.push(Self::touch_event(&touch, if i == 0 { TouchPhase::Start } else { TouchPhase::Move }));
                }
                touch.time += FRAME_MS;
                out.push(Self::touch_event(&touch, TouchPhase::End));
            }
            PointerKind::Drag => {
                let Some(Effect::Gesture(start)) = self.touch_drag(scene, DragPhase::Begin, x, y, t0) else { return None };
                out.extend(start);
                let mut t = t0;
                for i in 1..=GESTURE_STEPS {
                    let f = f64::from(i) / f64::from(GESTURE_STEPS);
                    t += FRAME_MS;
                    if let Effect::Gesture(e) = self.drag(scene, DragPhase::Move, x + ev.dx * f, y + ev.dy * f, t) {
                        out.extend(e);
                    }
                }
                t += FRAME_MS;
                // Handed to a scroller: release it like a resting finger (no fling). ponytail: a whole `drag` never flings.
                let e = self.drag(scene, DragPhase::End, x + ev.dx, y + ev.dy, if self.touch.is_some() { t } else { t + 1000.0 });
                match e {
                    Effect::Gesture(e) => out.extend(e),
                    // This synthesized touch was handed to a pager. The guest has not seen its start.
                    Effect::Action { .. } => return Some(e),
                    _ => {}
                }
            }
            PointerKind::Scroll => return None,
        }
        self.clock = out.last().map_or(t0, |e| e.time);
        self.acted = true;
        Some(Effect::Gesture(out))
    }

    fn scroll_begin(&mut self, scene: &Scene, x: f64, y: f64, t: f64) -> Effect {
        let Some(i) = scene.scroll_target(x, y) else { return Effect::None };
        self.begin_scroll(scene, i, x, y, t)
    }

    /// A finger at (x, y) starts dragging the scroller `scene.nodes[i]` (an outer one when the drag's direction is not
    /// the innermost's). Follow with `drag` moves and an end.
    pub fn begin_scroll(&mut self, scene: &Scene, i: usize, x: f64, y: f64, t: f64) -> Effect {
        self.forget_gone(scene);
        let n = &scene.nodes[i];
        self.pressed = None;
        self.motion.remove(&n.id);
        if self.drag.as_ref().is_none_or(|d| d.id != n.id) {
            self.catch(n, x, y);
        }
        let d = self.drag.as_mut().expect("caught");
        d.pager_start = scene.scroll_target_where(x, y, |n| n.pager.is_some()).map(|_| (x, y));
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
                    let dx = (x - p.start.0).abs();
                    if dx > dy.abs().max(PAN_SLOP) && scene.scroll_target_where(p.start.0, p.start.1, |n| n.pager.is_some()).is_some() {
                        self.scroll_begin(scene, p.start.0, p.start.1, t);
                        return None;
                    }
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
        if self.motion.is_empty() && self.scroll.is_empty() {
            return false;
        }
        self.forget_gone(scene);
        let held = self.drag.as_ref().map(|d| d.id.clone());
        let mut moving = false;
        for (i, n) in scene.nodes.iter().enumerate() {
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
            // That wins over paging / viewAligned while the offset is inside the collapse band.
            let snap = info.snap.filter(|&s| info.vertical && to.is_none() && o > 0.0 && o < s);
            if let (0.0, Some(s)) = (v, snap) {
                self.motion.insert(n.id.clone(), (0.0, Some(if o < s / 2.0 { 0.0 } else { s })));
                moving = true;
            } else if v == 0.0 && to.is_none() {
                let stops = scroll_rest_stops(scene, i);
                if !stops.is_empty() && stops.iter().all(|s| (o - *s).abs() > 0.5) {
                    self.motion.insert(n.id.clone(), (0.0, Some(nearest_stop(o, &stops))));
                    moving = true;
                } else {
                    self.motion.remove(&n.id);
                }
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

    /// Dismiss the keyboard and clear a SwiftUI focus binding, if present.
    pub fn blur_text(&mut self, scene: &Scene) -> Effect {
        let action = self.focused_field(scene).and_then(|n| n.input.as_ref()?.blur_action);
        self.set_text_focus(None);
        action.map_or(Effect::Redraw, |id| Effect::Action { id, version: self.version })
    }

    fn focused_field<'a>(&self, scene: &'a Scene) -> Option<&'a crate::Node> {
        self.focus.as_ref().and_then(|f| scene.nodes.iter().find(|n| &n.id == f && n.field.is_some() && !n.disabled))
    }

    /// A tap on a software-keyboard key (`label` as exposed by the inspection tree).
    fn press_key(&mut self, label: &str, shifted: bool, scene: &Scene) -> Effect {
        let Some(node) = self.focused_field(scene) else { return Effect::None };
        match label {
            "shift" => {
                self.shift = !shifted;
                Effect::Redraw
            }
            "+*#" | "123" if self.keypad == Some(Keypad::Phone) => {
                self.phone_symbols = !self.phone_symbols;
                Effect::Redraw
            }
            "return" => self.enter(node),
            "delete" => self.backspace(node),
            "space" => self.append(node, " "),
            "pause" if self.phone_symbols => self.append(node, ","),
            "wait" if self.phone_symbols => self.append(node, ";"),
            l if l.chars().count() == 1 => {
                let text = if shifted { l.to_uppercase() } else { l.to_string() };
                self.append(node, &text)
            }
            _ => Effect::None,
        }
    }

    fn enter(&mut self, node: &crate::Node) -> Effect {
        if let Some(input) = &node.input {
            if let Some(id) = input.submit_action {
                return Effect::Action { id, version: self.version };
            }
            if input.editor || input.axis == Some(Axis::Vertical) {
                return self.append(node, "\n");
            }
            if input.focused.is_some() {
                return Effect::None;
            }
        }
        self.set_text_focus(None);
        Effect::Redraw
    }

    fn append(&mut self, node: &crate::Node, suffix: &str) -> Effect {
        let mut text = node.value.clone().unwrap_or_default();
        text.push_str(suffix);
        self.edit(node, text)
    }

    fn backspace(&mut self, node: &crate::Node) -> Effect {
        let mut text = node.value.clone().unwrap_or_default();
        if let Some((start, _)) = text.grapheme_indices(true).next_back() {
            text.truncate(start);
        }
        self.edit(node, text)
    }

    fn edit(&mut self, node: &crate::Node, text: String) -> Effect {
        if node.input.as_ref().is_some_and(|input| input.editor || input.axis == Some(Axis::Vertical)) {
            self.scroll.remove(&node.id);
            self.motion.remove(&node.id);
        }
        self.shift = input_shift(node, &text);
        Effect::TextInput { field: node.field.expect("focused field"), version: self.version, text }
    }

    /// Keyboard input for the focused text field.
    pub fn keyboard(&mut self, scene: &Scene, ev: KeyboardEvent) -> Result<Effect, &'static str> {
        self.forget_gone(scene);
        let node = self.focused_field(scene).ok_or("no focused text field (click a text field first)")?;
        match ev {
            KeyboardEvent::Text { text } => Ok(self.append(node, &text)),
            KeyboardEvent::Key { key } if key == "backspace" => Ok(self.backspace(node)),
            KeyboardEvent::Key { .. } => Ok(self.enter(node)),
        }
    }

}

fn input_shift(node: &crate::Node, text: &str) -> bool {
    match node.input.as_ref().and_then(|i| i.autocapitalization.as_deref()) {
        Some("never") => false,
        Some("characters") => true,
        Some("words") => text.chars().next_back().is_none_or(char::is_whitespace),
        _ if node.secure => false,
        _ => autocap(text),
    }
}

/// Point (points) to click for a label in the inspection tree. Syntax: `Label` (button / toggle
/// switch / text field placeholder / keyboard key), `Label#increment` or `Label#decrement`
/// (stepper whose label is `Label`).
pub fn resolve_label(tree: &onesim_protocol::UiNode, spec: &str) -> Result<(f64, f64), String> {
    use onesim_protocol::UiNode;
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
        // Any other view by its text (views with gestures have no control role).
        .or_else(|| find(tree, &|n| n.role.as_deref() == Some("text") && n.label.as_deref() == Some(spec)))
        .ok_or_else(|| format!("no button, toggle, text field, key or text labelled {spec:?}"))?,
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
    fn swiftui_input_return_distinguishes_submit_from_newline() {
        let mut node = crate::Node::new("entry", "text_field");
        node.field = Some(1);
        node.value = Some("👩🏾‍💻".into());
        node.input = Some(Box::new(onesim_protocol::TextInputOptions {
            focused: Some(true),
            submit_action: Some(2),
            ..Default::default()
        }));
        let mut scene = Scene {
            size: (402.0, 874.0),
            appearance: onesim_protocol::Appearance::Light,
            ios: onesim_protocol::IosVersion::Ios27,
            nodes: vec![node],
            unsupported_symbols: vec![],
            notes: vec![],
        };
        let mut inter = Interaction { focus: Some("entry".into()), version: 3, ..Default::default() };
        let enter = KeyboardEvent::Key { key: "enter".into() };
        assert_eq!(inter.keyboard(&scene, enter.clone()), Ok(Effect::Action { id: 2, version: 3 }));
        assert_eq!(inter.focus.as_deref(), Some("entry"));
        let input = scene.nodes[0].input.as_mut().unwrap();
        input.editor = true;
        input.submit_action = None;
        assert_eq!(inter.keyboard(&scene, enter), Ok(Effect::TextInput { field: 1, version: 3, text: "👩🏾‍💻\n".into() }));
        let delete = KeyboardEvent::Key { key: "backspace".into() };
        assert_eq!(inter.keyboard(&scene, delete), Ok(Effect::TextInput { field: 1, version: 3, text: String::new() }));

        let mut second = scene.nodes[0].clone();
        second.id = "second".into();
        second.input.as_mut().unwrap().blur_action = Some(4);
        scene.nodes.push(second);
        inter.focus = None;
        assert!(inter.sync_text_focus(&scene, 4));
        assert_eq!(inter.focus.as_deref(), Some("entry"));
        inter.focus = Some("second".into());
        assert!(!inter.sync_text_focus(&scene, 5));
        assert_eq!(inter.focus.as_deref(), Some("second"));
        assert_eq!(inter.blur_text(&scene), Effect::Action { id: 4, version: 3 });
        assert!(inter.focus.is_none());
        scene.nodes.iter_mut().for_each(|n| n.input.as_mut().unwrap().focused = Some(false));
        assert!(!inter.sync_text_focus(&scene, 6));
        assert!(inter.focus.is_none());
    }

    #[test]
    fn editing_a_scrolled_vertical_field_brings_the_caret_back_into_view() {
        let device = crate::device(crate::DEFAULT_DEVICE).unwrap();
        let mut input: onesim_protocol::ViewNode = serde_json::from_str(
            r#"{"kind":"text_field","id":"entry","field":1,"axis":"vertical",
                "text":"First\nSecond\nThird","modifiers":[{"lineLimit":{"max":2}}]}"#,
        ).unwrap();
        let mut fonts = crate::Fonts::with_fallback(true);
        let mut interaction = Interaction { focus: Some("entry".into()), ..Default::default() };
        let mut layout = |root: &onesim_protocol::ViewNode, state: &Interaction| {
            crate::build_scene(root, &crate::SceneParams {
                viewport: (120.0, 200.0),
                device,
                scale: 1.0,
                appearance: onesim_protocol::Appearance::Light,
                dynamic_type: Default::default(),
                ios: Default::default(),
                scroll: &state.scroll,
                sheets: &state.sheets,
                focus: state.focus.as_deref(),
                overscroll: false,
                status_bar: false,
                chrome: ChromeParams::default(),
            }, &mut fonts)
        };
        let caret_visible = |scene: &Scene| {
            let field = scene.nodes.iter().find(|n| n.id == "entry").unwrap().bounds;
            let caret = scene.nodes.iter().find(|n| n.id == "entry#caret").unwrap().bounds;
            field.contains(caret.x, caret.y)
        };
        let initial = layout(&input, &interaction);
        let field = initial.nodes.iter().find(|n| n.id == "entry").unwrap().bounds;
        let up = PointerEvent::moved(PointerKind::Scroll, field.x + 5.0, field.y + 5.0, 0.0, -100.0);
        assert_eq!(interaction.pointer(&initial, up), Effect::Redraw);
        let scrolled = layout(&input, &interaction);
        assert!(!caret_visible(&scrolled));

        let effect = interaction.keyboard(&scrolled, KeyboardEvent::Text { text: " line".into() }).unwrap();
        let Effect::TextInput { text, .. } = effect else { panic!("expected text update, got {effect:?}") };
        let onesim_protocol::ViewKind::TextField { text: value, .. } = &mut input.kind else { unreachable!() };
        *value = text;
        assert!(caret_visible(&layout(&input, &interaction)));
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
            n.scroll = Some(ScrollInfo { vertical: true, offset: (0.0, 0.0), max: (0.0, 500.0), snap: None, content: None, page: None, aligned: false });
            let nodes = if with_scroller { vec![n] } else { vec![] };
            Scene { size: (400.0, 800.0), appearance: onesim_protocol::Appearance::Light,
                ios: Default::default(), nodes, unsupported_symbols: vec![], notes: vec![] }
        };
        let mut it = Interaction::default();
        let ev = |kind, dy| PointerEvent::moved(kind, 100.0, 100.0, 0.0, dy);
        assert_eq!(it.pointer(&screen(true), ev(PointerKind::Scroll, 120.0)), Effect::Redraw);
        assert_eq!(it.scroll.get("0/s1"), Some(&(0.0, 120.0)));
        // Back to a screen without it (the pop), then a new screen reusing the id starts at the top.
        it.pointer(&screen(false), ev(PointerKind::Click, 0.0));
        assert!(it.scroll.is_empty() && !it.live());
    }

    #[test]
    fn slider_tracks_the_finger_even_outside_its_bounds() {
        let mut n = crate::Node::new("slider", "slider");
        n.bounds = crate::Rect { x: 10.0, y: 10.0, w: 100.0, h: 20.0 };
        n.role = Role::Slider;
        n.range = Some((0.0, 10.0, 20.0));
        n.field = Some(4);
        let scene = Scene { size: (120.0, 100.0), appearance: onesim_protocol::Appearance::Light, ios: Default::default(), nodes: vec![n], unsupported_symbols: vec![], notes: vec![] };
        let mut it = Interaction { version: 3, ..Default::default() };
        assert_eq!(it.drag(&scene, DragPhase::Begin, 40.0, 20.0, 0.0), Effect::None);
        assert_eq!(it.drag(&scene, DragPhase::Move, 60.0, 50.0, 16.0), Effect::TextInput { field: 4, version: 3, text: "5".into() });
        it.version = 4;
        assert_eq!(it.drag(&scene, DragPhase::End, 130.0, 50.0, 32.0), Effect::TextInput { field: 4, version: 4, text: "10".into() });
        assert!(it.slider_drag.is_none() && it.scroll.is_empty());
    }

    #[test]
    fn large_title_never_rests_half_collapsed() {
        use crate::scene::{Node, Rect, ScrollInfo};
        let scene = |off: f64| {
            let mut n = Node::new("l", "list");
            n.bounds = Rect { x: 0.0, y: 0.0, w: 400.0, h: 800.0 };
            n.scroll = Some(ScrollInfo { vertical: true, offset: (0.0, off), max: (0.0, 500.0), snap: Some(52.0), content: None, page: None, aligned: false });
            Scene { size: (400.0, 800.0), appearance: onesim_protocol::Appearance::Light,
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

    #[test]
    fn page_stops_include_the_max_offset() {
        assert_eq!(page_stops(100.0, 250.0), vec![0.0, 100.0, 200.0, 250.0]);
        assert_eq!(page_stops(100.0, 200.0), vec![0.0, 100.0, 200.0]);
    }

    #[test]
    fn nearest_stop_prefers_the_lower_on_a_tie() {
        let stops = [0.0, 100.0, 200.0, 250.0];
        assert_eq!(nearest_stop(140.0, &stops), 100.0);
        assert_eq!(nearest_stop(160.0, &stops), 200.0);
        assert_eq!(nearest_stop(150.0, &stops), 100.0);
    }

    #[test]
    fn scroll_anchor_offset_aligns_and_clamps() {
        assert_eq!(scroll_anchor_offset(500.0, 200.0, 40.0, 0.0, 800.0), 500.0);
        assert_eq!(scroll_anchor_offset(500.0, 200.0, 40.0, 0.5, 800.0), 420.0);
        assert_eq!(scroll_anchor_offset(900.0, 100.0, 20.0, 0.0, 800.0), 800.0);
    }

    fn paging_scene(off: f64, snap: Option<f64>) -> Scene {
        use crate::scene::{Node, Rect, ScrollInfo};
        let mut n = Node::new("s", "scroll");
        n.bounds = Rect { x: 0.0, y: 0.0, w: 100.0, h: 100.0 };
        n.scroll = Some(ScrollInfo { vertical: true, offset: (0.0, off), max: (0.0, 300.0), snap, content: None, page: Some(100.0), aligned: false });
        Scene { size: (100.0, 100.0), appearance: onesim_protocol::Appearance::Light, ios: Default::default(), nodes: vec![n], unsupported_symbols: vec![], notes: vec![] }
    }

    #[test]
    fn view_aligned_stops_clamp_to_the_max_offset() {
        use crate::scene::{Node, Rect, ScrollInfo};
        let mut scroll = Node::new("s", "scroll");
        scroll.bounds = Rect { x: 0.0, y: 0.0, w: 370.0, h: 72.0 };
        scroll.scroll = Some(ScrollInfo {
            vertical: false,
            offset: (500.0, 0.0),
            max: (590.0, 0.0),
            snap: None,
            content: None,
            page: None,
            aligned: true,
        });
        let page = |id, content: f64| {
            let mut n = Node::new(id, "page");
            n.bounds = Rect { x: content - 500.0, y: 0.0, w: 320.0, h: 72.0 };
            n.parent = Some(0);
            n.scroll_target = true;
            n
        };
        let scene = Scene {
            size: (370.0, 72.0),
            appearance: onesim_protocol::Appearance::Light,
            ios: Default::default(),
            nodes: vec![scroll, page("a", 0.0), page("b", 320.0), page("c", 640.0)],
            unsupported_symbols: vec![],
            notes: vec![],
        };
        let stops = scroll_rest_stops(&scene, 0);
        assert!(stops.iter().all(|s| *s <= 590.0), "stops {stops:?} pass the max offset");
        assert_eq!(nearest_stop(560.0, &stops), 590.0);
    }

    #[test]
    fn parked_paging_offset_does_not_snap() {
        let scene = paging_scene(40.0, None);
        let mut it = Interaction::default();
        assert!(!it.tick(&scene, 1.0));
        assert_eq!(it.scroll.get("s").copied().unwrap_or((0.0, 40.0)).1, 40.0);
    }

    #[test]
    fn paging_drag_at_rest_springs_to_the_nearest_stop() {
        let scene = |off: f64| paging_scene(off, None);
        let mut it = Interaction::default();
        it.drag(&scene(40.0), DragPhase::Begin, 10.0, 10.0, 0.0);
        it.drag(&scene(40.0), DragPhase::End, 10.0, 10.0, 0.0);
        let mut off = 40.0;
        for _ in 0..4 {
            if !it.tick(&scene(off), 1.0) {
                break;
            }
            off = it.scroll["s"].1;
        }
        assert!((off - 0.0).abs() < 1e-6, "rests at {off}");
    }

    #[test]
    fn title_snap_beats_paging_in_the_collapse_band() {
        let scene = |off: f64| paging_scene(off, Some(52.0));
        for (start, want) in [(20.0, 0.0), (40.0, 52.0)] {
            let mut it = Interaction::default();
            it.drag(&scene(0.0), DragPhase::Begin, 50.0, 80.0, 0.0);
            it.drag(&scene(0.0), DragPhase::Move, 50.0, 80.0 - start, 500.0);
            it.drag(&scene(start), DragPhase::End, 50.0, 80.0 - start, 900.0);
            let mut off = start;
            for _ in 0..120 {
                if !it.tick(&scene(off), 1.0 / 60.0) {
                    break;
                }
                off = it.scroll["s"].1;
            }
            assert!((off - want).abs() < 1e-9, "released at {start}: rests at {off}, want {want}");
        }
    }
}
