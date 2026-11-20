//! React Native input, shared by the native host (`onesim-host`) and the browser (`onesim-web`): pointer, drag,
//! keyboard and the guest's `focus_request` / `scroll_to` become the host messages of docs/protocol.md section H.
//! Nothing here sends: the native host delivers each [`Out`] (waiting as its [`Wait`] says), the browser posts them
//! to the guest Worker and never waits (later trees simply arrive).
use crate::{autocap, DragPhase, Effect, Interaction, Node, Role, Scene};
use onesim_protocol::{GuestMessage, HostMessage, PointerEvent, PointerKind, ScrollPhase, TouchPhase, ViewKind, ViewNode};
use serde_json::Value;
use std::collections::BTreeMap;

/// What the native host does after sending a message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wait {
    /// Send and carry on.
    None,
    /// Wait for the guest's trees to stop (`Session::send_and_settle`).
    Settle,
}

#[derive(Debug, PartialEq)]
pub enum Out {
    Send(HostMessage, Wait),
    /// An `Interaction` effect for a native control hosted in React Native (button, slider, keyboard key).
    Effect(Effect),
}

/// What the input functions read: the scene being touched, the tree it came from, and the software keyboard's
/// height while shown (`look::keyboard_height`).
pub struct RnCtx<'a> {
    pub scene: &'a Scene,
    pub root: &'a ViewNode,
    pub keyboard: f64,
}

/// Who a live drag belongs to.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum DragOwner {
    #[default]
    None,
    /// The touched Fabric view (React Native's touch handling).
    Js,
    /// `Interaction::drag`: a slider, or a scroller React Native does not own.
    Native,
    /// On a React Native scroller, the scroll not decided: JS has the touch. (x, y): where it began; `last`: the latest
    /// finger point (x, y, t). `asked`: the finger passed the slop along a scroller's axis at (x, y, t) and the guest
    /// was sent `touch_ask` number `seq`; its `touch_answer` (or `ANSWER_TIMEOUT_MS` of silence) decides.
    Pending { x: f64, y: f64, last: (f64, f64, f64), asked: Option<Asked> },
    /// The React Native scroller took the pan; the host scrolls it.
    Scroll,
}

/// A `touch_ask` waiting for its answer: its number and the finger point (x, y, t) where the slop was passed.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Asked {
    seq: u64,
    x: f64,
    y: f64,
    t: f64,
}

/// Finger time (ms) a scroll waits for the guest's `touch_answer` before taking the pan anyway (a hung guest).
const ANSWER_TIMEOUT_MS: f64 = 250.0;

/// Per-session React Native input state (the touch in progress, the live drag and its scroller, the fling still
/// moving, the frames the guest already has).
#[derive(Default)]
pub struct RnInput {
    touch: Option<u64>,
    drag: DragOwner,
    scroller: Option<String>,
    fling: Option<String>,
    /// The JS responder blocking native ones (`js_responder`).
    blocker: Option<u64>,
    /// The last `touch_ask` number.
    asks: u64,
    sent_frames: BTreeMap<String, [f64; 4]>,
}

fn now_ms() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64() * 1000.0)
}

fn touch_message(phase: TouchPhase, n: &Node, x: f64, y: f64, timestamp: f64) -> HostMessage {
    HostMessage::Touch { phase, tag: n.tag.unwrap_or(0), x, y, lx: x - n.bounds.x, ly: y - n.bounds.y, timestamp }
}

/// A `scroll` event for the scroller `n` at `offset` (none if `n` is not a React Native scroller).
fn scroll_message(n: &Node, phase: ScrollPhase, offset: (f64, f64)) -> Option<Out> {
    let info = n.scroll?;
    let tag = n.id.parse::<u64>().ok()?;
    let (content_width, content_height) =
        info.content.unwrap_or(if info.vertical { (n.bounds.w, n.bounds.h + info.max.1) } else { (n.bounds.w + info.max.0, n.bounds.h) });
    let m = HostMessage::Scroll { tag, phase, x: offset.0, y: offset.1, content_width, content_height, width: n.bounds.w, height: n.bounds.h };
    Some(Out::Send(m, Wait::None))
}

/// Whether the view with tag `blocker` contains the scroller `i` (RCTScrollView checks its superviews).
fn blocked_by(scene: &Scene, i: usize, blocker: u64) -> bool {
    let mut p = scene.nodes[i].parent;
    while let Some(j) = p {
        if scene.nodes[j].tag == Some(blocker) {
            return true;
        }
        p = scene.nodes[j].parent;
    }
    false
}

/// Whether return keeps `id` focused (multiline inputs and `submitBehavior` "submit"/"newline").
fn submit_keeps_focus(n: &ViewNode, id: &str) -> bool {
    match &n.kind {
        ViewKind::Rn { component, props, .. } if n.id == id => match props.get("submitBehavior").and_then(Value::as_str) {
            Some("submit" | "newline") => true,
            Some("blurAndSubmit") => false,
            _ => component == "RCTMultilineTextInputView"
                || (component == "AndroidTextInput" && props.get("multiline").and_then(Value::as_bool) == Some(true)),
        },
        ViewKind::Rn { children, .. } => children.iter().any(|c| submit_keeps_focus(c, id)),
        ViewKind::Presentation { content, .. } => submit_keeps_focus(content, id),
        _ => false,
    }
}

/// The messages that tell the guest the focused text input changed from `before` to `inter.focus`.
pub fn focus_change(inter: &Interaction, before: Option<String>, keyboard: f64) -> Vec<Out> {
    let tag = |f: &Option<String>| f.as_ref().and_then(|f| f.parse::<u64>().ok());
    let after = &inter.focus;
    if before == *after {
        return vec![];
    }
    let mut out = vec![];
    if let Some(tag) = tag(&before) {
        out.push(Out::Send(HostMessage::Blur { tag }, Wait::None));
    }
    if let Some(tag) = tag(after) {
        out.push(Out::Send(HostMessage::Focus { tag }, Wait::None));
    }
    // The keyboard stays up while focus moves between fields.
    if before.is_some() != after.is_some() {
        let height = if after.is_some() { keyboard } else { 0.0 };
        out.push(Out::Send(HostMessage::Keyboard { height }, Wait::None));
    }
    out
}

/// Return pressed while `before` was focused (the interaction state already dropped the focus): `submit`, then
/// the field's submit behavior.
fn submit(inter: &mut Interaction, root: &ViewNode, before: &str) -> Vec<Out> {
    let Ok(tag) = before.parse::<u64>() else { return vec![] };
    let mut out = vec![Out::Send(HostMessage::Submit { tag }, Wait::None)];
    if submit_keeps_focus(root, before) {
        inter.focus = Some(before.to_string());
    } else {
        out.push(Out::Send(HostMessage::Blur { tag }, Wait::None));
        out.push(Out::Send(HostMessage::Keyboard { height: 0.0 }, Wait::None));
    }
    out
}

/// The guest messages an interaction effect stands for (the browser's half of `Session::send_effect`).
pub fn effect_messages(e: Effect) -> Vec<HostMessage> {
    match e {
        Effect::Action { id, version } => vec![HostMessage::Action { id, version }],
        Effect::TextInput { field, version, text } => vec![HostMessage::TextInput { field, version, text }],
        Effect::Gesture(touches) => touches.into_iter().map(HostMessage::Gesture).collect(),
        Effect::None | Effect::Redraw => vec![],
    }
}

impl RnInput {
    /// Pointer input. Scrolls and drags on a scroller (a React Native ScrollView is a native UIScrollView: the host
    /// moves it) become `scroll` events, the software keyboard and native controls are the
    /// interaction state machine's, native chrome (stack back pill, tab bar items) sends its own message, and
    /// everything else is a touch (a tap on a text input focuses it). Returns `(hit node id, acted, messages)`.
    pub fn pointer(&mut self, inter: &mut Interaction, cx: &RnCtx, ev: PointerEvent) -> (Option<String>, bool, Vec<Out>) {
        let scene = cx.scene;
        if ev.kind == PointerKind::Scroll || (ev.kind == PointerKind::Drag && scene.scroll_target(ev.x, ev.y).is_some()) {
            // How far the content is pulled down: a wheel's negative dy, a finger moving down.
            let pull = if ev.kind == PointerKind::Drag { -ev.dy } else { ev.dy };
            let effect = inter.pointer(scene, ev);
            let hit = inter.hit.clone();
            let n = scene.nodes.iter().find(|n| Some(&n.id) == hit.as_ref());
            let mut out = vec![];
            if effect == Effect::Redraw {
                let new = hit.as_ref().and_then(|h| inter.scroll.get(h)).copied();
                if let (Some(n), Some(new)) = (n, new) {
                    let old = n.scroll.map_or(new, |i| i.offset);
                    out.extend([(ScrollPhase::Begin, old), (ScrollPhase::Scroll, new), (ScrollPhase::End, new)].into_iter().filter_map(|(p, o)| scroll_message(n, p, o)));
                }
            } else if let Some(n) = n {
                // Pulled down past the top: iOS reports the rubber-band overscroll as a negative offset (a RefreshControl
                // fires on it); the content springs back to 0 on release.
                // ponytail: no rubber-band drawing, the overscroll is only reported.
                if n.scroll.is_some_and(|i| i.vertical && i.offset.1 <= 0.0) && pull < 0.0 {
                    out.extend([(ScrollPhase::Begin, (0.0, 0.0)), (ScrollPhase::Scroll, (0.0, pull)), (ScrollPhase::End, (0.0, 0.0))].into_iter().filter_map(|(p, o)| scroll_message(n, p, o)));
                }
            }
            return (hit, false, out);
        }
        let on_keyboard = scene.nodes.iter().any(|n| n.kind == "keyboard" && n.bounds.contains(ev.x, ev.y));
        let hit = scene.hit_test(ev.x, ev.y).map(|i| &scene.nodes[i]);
        // Native chrome: a stack header's back pill (`<stack>#back`) and tab bar items (`<tabs>#tab-<key>`).
        if let Some(n) = hit {
            let (tag, key) = n.id.split_once('#').unwrap_or((n.id.as_str(), ""));
            let msg = match (tag.parse::<u64>(), key) {
                (Ok(tag), "back") => Some(HostMessage::NativeBack { tag }),
                (Ok(tag), k) if k.starts_with("tab-") && !k.contains('#') => Some(HostMessage::SelectTab { tag, key: k[4..].to_string() }),
                _ => None,
            };
            if let Some(msg) = msg {
                let acted = ev.kind != PointerKind::Down;
                return (Some(n.id.clone()), acted, if acted { vec![Out::Send(msg, Wait::Settle)] } else { vec![] });
            }
        }
        // Native actions and sliders hosted inside React Native use the same interaction as standalone views.
        if hit.is_some_and(|n| n.action.is_some() || n.role == Role::Slider) {
            let focus = inter.focus.clone();
            let effect = inter.pointer(scene, ev);
            inter.focus = focus;
            return (inter.hit.clone(), inter.acted, vec![Out::Effect(effect)]);
        }
        if on_keyboard {
            let before = inter.focus.clone();
            let returned = ev.kind != PointerKind::Down && hit.is_some_and(|n| n.role == Role::Key && n.label.as_deref() == Some("return"));
            let effect = inter.pointer(scene, ev);
            let (hit, acted) = (inter.hit.clone(), inter.acted);
            let mut out = vec![Out::Effect(effect)];
            match before {
                Some(b) if returned => out.extend(submit(inter, cx.root, &b)),
                b => out.extend(focus_change(inter, b, cx.keyboard)),
            }
            return (hit, acted, out);
        }
        self.touch(inter, cx, ev)
    }

    /// Touches: down = `start` on the deepest tag, up = `end` on the same tag, click = both, drag = `start`,
    /// `move`s along (dx, dy) (one per 16 ms of timestamps), `end`, all on the start's target.
    fn touch(&mut self, inter: &mut Interaction, cx: &RnCtx, ev: PointerEvent) -> (Option<String>, bool, Vec<Out>) {
        let scene = cx.scene;
        let hit = scene.hit_test(ev.x, ev.y).map(|i| &scene.nodes[i]);
        let now = now_ms();
        let send = |phase, n: &Node, x, y, t, wait| Out::Send(touch_message(phase, n, x, y, t), wait);
        let mut target = hit;
        let mut out = vec![];
        match ev.kind {
            PointerKind::Drag => {
                if let Some(n) = hit.filter(|n| n.tag.is_some()) {
                    const STEPS: u32 = 8;
                    out.push(send(TouchPhase::Start, n, ev.x, ev.y, now, Wait::None));
                    for i in 1..=STEPS {
                        let f = f64::from(i) / f64::from(STEPS);
                        out.push(send(TouchPhase::Move, n, ev.x + ev.dx * f, ev.y + ev.dy * f, now + 16.0 * f64::from(i), Wait::None));
                    }
                    out.push(send(TouchPhase::End, n, ev.x + ev.dx, ev.y + ev.dy, now + 16.0 * f64::from(STEPS + 1), Wait::Settle));
                }
            }
            PointerKind::Down => {
                self.touch = hit.and_then(|n| n.tag);
                if let Some(n) = hit.filter(|n| n.tag.is_some()) {
                    out.push(send(TouchPhase::Start, n, ev.x, ev.y, now, Wait::None));
                }
            }
            PointerKind::Up => {
                target = self.touch.take().and_then(|t| scene.nodes.iter().find(|n| n.tag == Some(t))).or(hit);
                if let Some(n) = target.filter(|n| n.tag.is_some()) {
                    out.push(send(TouchPhase::End, n, ev.x, ev.y, now, Wait::Settle));
                }
            }
            _ => {
                if let Some(n) = hit.filter(|n| n.tag.is_some()) {
                    out.push(send(TouchPhase::Start, n, ev.x, ev.y, now, Wait::None));
                    out.push(send(TouchPhase::End, n, ev.x, ev.y, now, Wait::Settle));
                }
            }
        }
        // A tap on a text input focuses it (RN never blurs on a tap elsewhere; `Keyboard.dismiss` does).
        if let Some(field) = hit.filter(|n| ev.kind != PointerKind::Down && n.field.is_some()) {
            let before = inter.focus.clone();
            if before.as_deref() != Some(field.id.as_str()) {
                inter.shift = field.value.as_deref().is_some_and(autocap);
                inter.focus = Some(field.id.clone());
                out.extend(focus_change(inter, before, cx.keyboard));
            }
        }
        (target.map(|n| n.id.clone()), ev.kind != PointerKind::Down && target.is_some(), out)
    }

    /// One phase of a live finger drag. A slider, or a scroller React Native does not own, is native
    /// (`Interaction::drag`). Elsewhere the touched Fabric view gets the touch (React Native supplies its own gesture
    /// behavior). On a React Native scroller (a native UIScrollView) the touch goes to JS until the finger moves
    /// `PAN_SLOP` along the axis of a scroller under it (the innermost one that scrolls that way); then the guest is
    /// asked (`touch_ask`) and its answer decides (see `answer`): a captured touch, or a blocking JS responder around
    /// the scroller, keeps it in JS; otherwise the scroller takes the pan, the JS touch is cancelled and the host
    /// scrolls, reporting `scroll` events.
    pub fn drag(&mut self, inter: &mut Interaction, cx: &RnCtx, phase: DragPhase, x: f64, y: f64, t_ms: f64) -> Vec<Out> {
        let scene = cx.scene;
        let touch = |phase, tag: Option<u64>| {
            let n = tag.and_then(|tag| scene.nodes.iter().find(|n| n.tag == Some(tag)));
            n.map(|n| Out::Send(touch_message(phase, n, x, y, t_ms), Wait::None))
        };
        let mut out = vec![];
        match (phase, self.drag) {
            (DragPhase::Begin, _) => {
                let hit = scene.hit_test(x, y);
                self.drag = match scene.scroll_target(x, y) {
                    _ if hit.is_some_and(|i| scene.nodes[i].role == Role::Slider) => DragOwner::Native,
                    Some(i) if scene.nodes[i].tag.is_some() => DragOwner::Pending { x, y, last: (x, y, t_ms), asked: None },
                    Some(_) => DragOwner::Native,
                    None => DragOwner::Js,
                };
                self.scroller = None;
                if self.drag == DragOwner::Native {
                    out.push(Out::Effect(inter.drag(scene, phase, x, y, t_ms)));
                } else {
                    // Finger-down (a pointer `down`) may have started this touch already.
                    let tag = hit.and_then(|i| scene.nodes[i].tag);
                    if self.touch != tag {
                        self.touch = tag;
                        out.extend(touch(TouchPhase::Start, self.touch));
                    }
                }
            }
            (DragPhase::Move, DragOwner::Pending { x: sx, y: sy, asked, .. }) => {
                let mut asked = asked;
                out.extend(touch(TouchPhase::Move, self.touch));
                if asked.is_none() {
                    let (dx, dy) = (x - sx, y - sy);
                    let vertical = dy.abs() > dx.abs();
                    let along = if vertical { dy.abs() } else { dx.abs() };
                    let target = scene.scroll_target_where(sx, sy, |n| n.tag.is_some() && n.scroll.is_some_and(|s| s.vertical == vertical));
                    if let Some(i) = target.filter(|_| along >= crate::interaction::PAN_SLOP) {
                        self.asks += 1;
                        self.scroller = Some(scene.nodes[i].id.clone());
                        asked = Some(Asked { seq: self.asks, x, y, t: t_ms });
                        out.push(Out::Send(HostMessage::TouchAsk { ask: self.asks }, Wait::None));
                    }
                }
                self.drag = DragOwner::Pending { x: sx, y: sy, last: (x, y, t_ms), asked };
                if asked.is_some_and(|a| t_ms - a.t > ANSWER_TIMEOUT_MS) {
                    out.extend(self.decide(inter, scene, false));
                }
            }
            (DragPhase::Move, DragOwner::Js) => out.extend(touch(TouchPhase::Move, self.touch)),
            (DragPhase::Move, DragOwner::Native | DragOwner::Scroll) => {
                out.push(Out::Effect(inter.drag(scene, phase, x, y, t_ms)));
                out.extend(self.scrolled(inter, scene));
            }
            (DragPhase::End, DragOwner::Native) => out.push(Out::Effect(inter.drag(scene, phase, x, y, t_ms))),
            (DragPhase::End, DragOwner::Scroll) => {
                out.push(Out::Effect(inter.drag(scene, phase, x, y, t_ms)));
                out.extend(self.scrolled(inter, scene));
                if let Some(n) = self.scroller.take().and_then(|id| scene.nodes.iter().find(|n| n.id == id)) {
                    let off = inter.scroll.get(&n.id).copied().or(n.scroll.map(|i| i.offset)).unwrap_or_default();
                    // A fling or bounce carries on: its end is reported by `scrolled` once it stops.
                    let fling = inter.moving(&n.id);
                    out.extend(scroll_message(n, if fling { ScrollPhase::Release } else { ScrollPhase::End }, off));
                    if fling {
                        self.fling = Some(n.id.clone());
                    }
                }
            }
            // Lifted before the scroll was decided: the touch stays JS's (a tap, or a drag the scroll never took).
            (DragPhase::End, DragOwner::Js | DragOwner::Pending { .. }) => out.extend(touch(TouchPhase::End, self.touch.take())),
            (_, DragOwner::None) => {}
        }
        if phase == DragPhase::End {
            self.drag = DragOwner::None;
            self.scroller = None;
        }
        out
    }

    /// Settle a pending drag (the guest's answer, or none in time): JS keeps the touch when a recognizer `captured`
    /// it, a blocking JS responder contains the scroller (RCTScrollView's `_shouldDisableScrollInteraction`) or the
    /// scroller is gone; otherwise the scroller takes the pan from where the finger passed the slop (the content lags
    /// the finger by the slop, as UIScrollView's does) to the latest finger point.
    fn decide(&mut self, inter: &mut Interaction, scene: &Scene, captured: bool) -> Vec<Out> {
        let DragOwner::Pending { last: (lx, ly, lt), asked: Some(a), .. } = self.drag else { return vec![] };
        let i = self.scroller.as_ref().and_then(|id| scene.nodes.iter().position(|n| &n.id == id));
        let Some(i) = i.filter(|&i| !captured && !self.blocker.is_some_and(|b| blocked_by(scene, i, b))) else {
            self.drag = DragOwner::Js;
            return vec![];
        };
        self.drag = DragOwner::Scroll;
        let mut out = vec![];
        let n = self.touch.take().and_then(|t| scene.nodes.iter().find(|n| n.tag == Some(t)));
        out.extend(n.map(|n| Out::Send(touch_message(TouchPhase::Cancel, n, lx, ly, lt), Wait::None)));
        // The innermost scroller goes through `drag` (a sheet's drag arbitration included); an outer one directly.
        out.push(Out::Effect(if scene.scroll_target(a.x, a.y) == Some(i) {
            inter.drag(scene, DragPhase::Begin, a.x, a.y, a.t)
        } else {
            inter.begin_scroll(scene, i, a.x, a.y, a.t)
        }));
        let n = &scene.nodes[i];
        out.extend(scroll_message(n, ScrollPhase::Begin, n.scroll.map_or((0.0, 0.0), |s| s.offset)));
        out.push(Out::Effect(inter.drag(scene, DragPhase::Move, lx, ly, lt)));
        out.extend(self.scrolled(inter, scene));
        out
    }

    /// `scroll` events for the React Native scrollers the host moved since `scene` was built (by a drag or
    /// `Interaction::tick`), which window lists and fire RefreshControl, and `momentum_end` once a fling stopped.
    pub fn scrolled(&mut self, inter: &Interaction, scene: &Scene) -> Vec<Out> {
        let mut out = vec![];
        for n in scene.nodes.iter().filter(|n| n.tag.is_some()) {
            if let (Some(info), Some(&new)) = (n.scroll, inter.scroll.get(&n.id)) {
                if new != info.offset {
                    out.extend(scroll_message(n, ScrollPhase::Scroll, new));
                }
            }
        }
        if let Some(id) = self.fling.take_if(|id| !inter.moving(id)) {
            let n = scene.nodes.iter().find(|n| n.id == id);
            out.extend(n.and_then(|n| scroll_message(n, ScrollPhase::MomentumEnd, inter.scroll.get(&id).copied().unwrap_or_default())));
        }
        out
    }

    /// Keyboard input for the focused text input. `enter` dropping the focus is a return press; the field decides
    /// whether to blur. Errors when no field is focused.
    pub fn keyboard(&mut self, inter: &mut Interaction, cx: &RnCtx, ev: onesim_protocol::KeyboardEvent) -> Result<Vec<Out>, &'static str> {
        let before = inter.focus.clone();
        let effect = inter.keyboard(cx.scene, ev)?;
        if let Effect::TextInput { text, .. } = &effect {
            inter.shift = autocap(text); // typed text drops (or raises) shift like the real keyboard
        }
        let mut out = vec![Out::Effect(effect)];
        match before {
            Some(b) if inter.focus.is_none() => out.extend(submit(inter, cx.root, &b)),
            b => out.extend(focus_change(inter, b, cx.keyboard)),
        }
        Ok(out)
    }

    /// Apply the guest's `focus_request` / `scroll_to` messages and drop focus on a vanished field. Returns
    /// whether the interaction state changed (the scene must be rebuilt) and the messages to send.
    pub fn apply_requests(&mut self, inter: &mut Interaction, cx: &RnCtx, requests: Vec<GuestMessage>) -> (bool, Vec<Out>) {
        let scene = cx.scene;
        let before = inter.focus.clone();
        let field = |tag: &str| scene.nodes.iter().find(|n| n.id == tag && n.field.is_some());
        if before.as_deref().is_some_and(|f| field(f).is_none()) {
            inter.focus = None;
        }
        let mut changed = inter.focus != before;
        let mut out = vec![];
        for r in requests {
            match r {
                GuestMessage::FocusRequest { tag } => {
                    let new = tag.map(|t| t.to_string()).and_then(|t| field(&t).map(|n| (t, n.value.clone().unwrap_or_default())));
                    let old = inter.focus.clone();
                    if old.as_ref() != new.as_ref().map(|n| &n.0) {
                        inter.shift = new.as_ref().is_some_and(|n| autocap(&n.1));
                        inter.focus = new.map(|n| n.0);
                        out.extend(focus_change(inter, old, cx.keyboard));
                    }
                }
                GuestMessage::JsResponder { tag } => {
                    self.blocker = tag;
                    continue;
                }
                GuestMessage::TouchAnswer { ask, captured } => {
                    if !matches!(self.drag, DragOwner::Pending { asked: Some(a), .. } if a.seq == ask) {
                        continue; // a drag that already ended or was decided
                    }
                    out.extend(self.decide(inter, scene, captured));
                }
                GuestMessage::ScrollTo { tag, x, y } => {
                    let Some(n) = scene.nodes.iter().find(|n| n.id == tag.to_string() && n.scroll.is_some()) else { continue };
                    let max = n.scroll.map_or((0.0, 0.0), |i| i.max);
                    let off = (x.resolve(max.0).clamp(0.0, max.0), y.resolve(max.1).clamp(0.0, max.1));
                    inter.scroll.insert(n.id.clone(), off);
                    out.extend(scroll_message(n, ScrollPhase::End, off));
                }
                _ => {}
            }
            changed = true;
        }
        (changed, out)
    }

    /// A `layout` message with the frames that changed since the last one (none if nothing did).
    pub fn layout(&mut self, scene: &Scene) -> Option<HostMessage> {
        let frames: BTreeMap<String, [f64; 4]> = scene
            .nodes
            .iter()
            .filter(|n| n.tag.is_some() || n.kind == "expo_ui_content")
            .map(|n| (n.id.clone(), [n.bounds.x, n.bounds.y, n.bounds.w, n.bounds.h]))
            .collect();
        let changed: BTreeMap<_, _> = frames.iter().filter(|(k, v)| self.sent_frames.get(*k) != Some(*v)).map(|(k, v)| (k.clone(), *v)).collect();
        if changed.is_empty() {
            return None;
        }
        self.sent_frames = frames;
        Some(HostMessage::Layout { frames: changed })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{build_scene, device, Fonts, SceneParams, DEFAULT_DEVICE};
    use onesim_protocol::{Appearance, KeyboardEvent};
    use serde_json::json;

    fn tree() -> ViewNode {
        let v = |id: &str, comp: &str, props: Value, kids: Vec<Value>| json!({"kind":"rn","id":id,"component":comp,"props":props,"children":kids});
        serde_json::from_value(v("1", "RootView", json!({}), vec![v("2", "RCTView", json!({"flex":1,"paddingTop":100}), vec![
            v("3", "RCTView", json!({"height":60,"backgroundColor":0xFF0080FFu64}), vec![]),
            v("4", "RCTSinglelineTextInputView", json!({"height":40,"placeholder":"Name","__simcoreText":""}), vec![]),
            v("5", "RCTMultilineTextInputView", json!({"height":80,"placeholder":"Notes","__simcoreText":""}), vec![]),
        ])]))
        .unwrap()
    }

    fn scene(root: &ViewNode, inter: &Interaction) -> Scene {
        let params = SceneParams {
            viewport: (402.0, 874.0),
            device: device(DEFAULT_DEVICE).unwrap(),
            scale: 3.0,
            appearance: Appearance::Light,
            dynamic_type: Default::default(),
            ios: Default::default(),
            scroll: &inter.scroll,
            sheets: &inter.sheets,
            focus: inter.focus.as_deref(),
            overscroll: false,
            status_bar: true,
            chrome: inter.chrome(),
        };
        build_scene(root, &params, &mut Fonts::new())
    }

    fn center(s: &Scene, id: &str) -> (f64, f64) {
        s.nodes.iter().find(|n| n.id == id).unwrap().bounds.center()
    }

    /// The messages only, with timestamps zeroed (they come from the clock).
    fn sent(out: &[Out]) -> Vec<(HostMessage, Wait)> {
        out.iter()
            .filter_map(|o| match o {
                Out::Send(HostMessage::Touch { phase, tag, x, y, lx, ly, .. }, w) => {
                    Some((HostMessage::Touch { phase: *phase, tag: *tag, x: *x, y: *y, lx: *lx, ly: *ly, timestamp: 0.0 }, *w))
                }
                Out::Send(m, w) => Some((m.clone(), *w)),
                Out::Effect(_) => None,
            })
            .collect()
    }

    #[test]
    fn tap_is_start_then_end_on_the_deepest_tag() {
        let (root, mut inter, mut rn) = (tree(), Interaction::default(), RnInput::default());
        let sc = scene(&root, &inter);
        let (x, y) = center(&sc, "3");
        let b = sc.nodes.iter().find(|n| n.id == "3").unwrap().bounds;
        let cx = RnCtx { scene: &sc, root: &root, keyboard: 300.0 };
        let (hit, acted, out) = rn.pointer(&mut inter, &cx, PointerEvent::new(PointerKind::Click, x, y));
        assert_eq!((hit.as_deref(), acted), (Some("3"), true));
        let touch = |phase| HostMessage::Touch { phase, tag: 3, x, y, lx: x - b.x, ly: y - b.y, timestamp: 0.0 };
        assert_eq!(sent(&out), vec![(touch(TouchPhase::Start), Wait::None), (touch(TouchPhase::End), Wait::Settle)]);
        // Down on the view, up elsewhere: the end still goes to the view the touch started on.
        rn.pointer(&mut inter, &cx, PointerEvent::new(PointerKind::Down, x, y));
        let (hit, _, out) = rn.pointer(&mut inter, &cx, PointerEvent::new(PointerKind::Up, 1.0, 860.0));
        assert_eq!(hit.as_deref(), Some("3"));
        assert!(matches!(sent(&out)[..], [(HostMessage::Touch { phase: TouchPhase::End, tag: 3, .. }, Wait::Settle)]));
    }

    #[test]
    fn layout_sends_only_changed_frames() {
        let (root, inter, mut rn) = (tree(), Interaction::default(), RnInput::default());
        let sc = scene(&root, &inter);
        let Some(HostMessage::Layout { frames }) = rn.layout(&sc) else { panic!("first layout sends every tagged frame") };
        assert_eq!(frames.get("3").map(|f| f[3]), Some(60.0));
        assert!(frames.contains_key("1") && frames.contains_key("4"));
        assert_eq!(rn.layout(&sc), None, "nothing changed");
    }

    #[test]
    fn tapping_an_input_focuses_it_and_return_submits() {
        let (root, mut inter, mut rn) = (tree(), Interaction::default(), RnInput::default());
        let sc = scene(&root, &inter);
        let (x, y) = center(&sc, "4");
        let (_, _, out) = rn.pointer(&mut inter, &RnCtx { scene: &sc, root: &root, keyboard: 300.0 }, PointerEvent::new(PointerKind::Click, x, y));
        assert_eq!(inter.focus.as_deref(), Some("4"));
        assert_eq!(sent(&out)[2..], [(HostMessage::Focus { tag: 4 }, Wait::None), (HostMessage::Keyboard { height: 300.0 }, Wait::None)]);
        // Single line: return submits, blurs and hides the keyboard.
        let sc = scene(&root, &inter);
        let cx = RnCtx { scene: &sc, root: &root, keyboard: 300.0 };
        let out = rn.keyboard(&mut inter, &cx, KeyboardEvent::Key { key: "enter".into() }).unwrap();
        assert_eq!(inter.focus, None);
        assert_eq!(sent(&out), vec![
            (HostMessage::Submit { tag: 4 }, Wait::None), (HostMessage::Blur { tag: 4 }, Wait::None), (HostMessage::Keyboard { height: 0.0 }, Wait::None)]);
        // Multiline: return submits and the field keeps focus.
        let (x, y) = center(&sc, "5");
        rn.pointer(&mut inter, &cx, PointerEvent::new(PointerKind::Click, x, y));
        let sc = scene(&root, &inter);
        let out = rn.keyboard(&mut inter, &RnCtx { scene: &sc, root: &root, keyboard: 300.0 }, KeyboardEvent::Key { key: "enter".into() }).unwrap();
        assert_eq!(inter.focus.as_deref(), Some("5"));
        assert!(!sent(&out).iter().any(|(m, _)| matches!(m, HostMessage::Blur { .. })), "{out:?}");
        // Typed text reaches the guest as the interaction's text effect.
        let out = rn.keyboard(&mut inter, &RnCtx { scene: &sc, root: &root, keyboard: 300.0 }, KeyboardEvent::Text { text: "hi".into() }).unwrap();
        assert!(matches!(&out[0], Out::Effect(Effect::TextInput { field: 5, text, .. }) if text == "hi"), "{out:?}");
    }

    #[test]
    fn drag_phases_go_to_the_view_the_drag_began_on() {
        let (root, mut inter, mut rn) = (tree(), Interaction::default(), RnInput::default());
        let sc = scene(&root, &inter);
        let cx = RnCtx { scene: &sc, root: &root, keyboard: 0.0 };
        let (x, y) = center(&sc, "3");
        let mut phase = |p, y, t| match &sent(&rn.drag(&mut inter, &cx, p, x, y, t))[..] {
            [(HostMessage::Touch { phase, tag: 3, .. }, Wait::None)] => Some(*phase),
            [] => None,
            m => panic!("{m:?}"),
        };
        assert_eq!(phase(DragPhase::Begin, y, 0.0), Some(TouchPhase::Start));
        assert_eq!(phase(DragPhase::Move, 600.0, 16.0), Some(TouchPhase::Move));
        assert_eq!(phase(DragPhase::End, 600.0, 32.0), Some(TouchPhase::End));
        assert_eq!(phase(DragPhase::Move, 600.0, 48.0), None, "no touch in progress");
    }

    /// A vertical list (tag 2) of 30 rows (10..40); row 12 carries `row_props`. `carousel`: row 11 is a horizontal
    /// scroll view (tag 50) of wide items instead.
    fn list(row_props: Value, carousel: bool) -> ViewNode {
        let v = |id: &str, comp: &str, props: Value, kids: Vec<Value>| json!({"kind":"rn","id":id,"component":comp,"props":props,"children":kids});
        let rows = (10..40)
            .map(|i| match i {
                11 if carousel => v("50", "RCTScrollView", json!({"height":100,"horizontal":true}), vec![
                    v("51", "RCTScrollContentView", json!({"width":1500,"flexDirection":"row"}), (52..57).map(|k| v(&k.to_string(), "RCTView", json!({"width":300,"height":100}), vec![])).collect()),
                ]),
                12 => v("12", "RCTView", row_props.clone(), vec![]),
                i => v(&i.to_string(), "RCTView", json!({"height":44}), vec![]),
            })
            .collect();
        serde_json::from_value(v("1", "RootView", json!({}), vec![
            v("2", "RCTScrollView", json!({"flex":1}), vec![v("3", "RCTScrollContentView", json!({}), rows)]),
        ]))
        .unwrap()
    }

    /// How the simulated guest answers `touch_ask`.
    #[derive(Clone, Copy)]
    enum Guest {
        /// After dispatching: `responder` as the blocking JS responder (`js_responder`), then the answer.
        Answers { captured: bool, responder: Option<u64> },
        Silent,
        /// Answers with another ask's number.
        Stale,
    }
    const PLAIN: Guest = Guest::Answers { captured: false, responder: None };

    /// Finger down on view `on`, a live drag by (dx, dy) in 10 moves `step` ms apart, lifted `lift` ms after the last,
    /// the guest answering each `touch_ask` per `guest`, then ticks until everything stops. Returns every message sent
    /// and the final scene.
    fn drag_on(root: &ViewNode, on: &str, (dx, dy): (f64, f64), step: f64, lift: f64, guest: Guest) -> (Vec<HostMessage>, Scene) {
        let (mut inter, mut rn) = (Interaction::default(), RnInput::default());
        let mut sc = scene(root, &inter);
        let (x, y) = center(&sc, on);
        let mut msgs: Vec<HostMessage> = vec![];
        let out = rn.pointer(&mut inter, &RnCtx { scene: &sc, root, keyboard: 0.0 }, PointerEvent::new(PointerKind::Down, x, y)).2;
        msgs.extend(sent(&out).into_iter().map(|(m, _)| m));
        let moves = 10;
        for i in 0..=moves + 1 {
            let (phase, t) = match i {
                0 => (DragPhase::Begin, 0.0),
                i if i <= moves => (DragPhase::Move, step * i as f64),
                _ => (DragPhase::End, step * moves as f64 + lift),
            };
            let f = i.min(moves) as f64 / moves as f64;
            let out = rn.drag(&mut inter, &RnCtx { scene: &sc, root, keyboard: 0.0 }, phase, x + dx * f, y + dy * f, t);
            let out = sent(&out).into_iter().map(|(m, _)| m).collect::<Vec<_>>();
            let ask = out.iter().find_map(|m| if let HostMessage::TouchAsk { ask } = m { Some(*ask) } else { None });
            msgs.extend(out);
            sc = scene(root, &inter);
            let reply = match (ask, guest) {
                (Some(ask), Guest::Answers { captured, responder }) => {
                    vec![GuestMessage::JsResponder { tag: responder }, GuestMessage::TouchAnswer { ask, captured }]
                }
                (Some(ask), Guest::Stale) => vec![GuestMessage::TouchAnswer { ask: ask + 1, captured: false }],
                _ => vec![],
            };
            if !reply.is_empty() {
                let (_, out) = rn.apply_requests(&mut inter, &RnCtx { scene: &sc, root, keyboard: 0.0 }, reply);
                msgs.extend(sent(&out).into_iter().map(|(m, _)| m));
                sc = scene(root, &inter);
            }
        }
        // A late answer for the finished drag changes nothing.
        let (_, out) = rn.apply_requests(&mut inter, &RnCtx { scene: &sc, root, keyboard: 0.0 }, vec![GuestMessage::TouchAnswer { ask: rn.asks, captured: false }]);
        assert!(out.is_empty(), "{out:?}");
        for _ in 0..600 {
            let moving = inter.tick(&sc, 1.0 / 60.0);
            let out = rn.scrolled(&inter, &sc);
            msgs.extend(sent(&out).into_iter().map(|(m, _)| m));
            sc = scene(root, &inter);
            if !moving && rn.fling.is_none() {
                break;
            }
        }
        (msgs, sc)
    }

    fn offset(sc: &Scene, id: &str) -> (f64, f64) {
        sc.nodes.iter().find(|n| n.id == id).unwrap().scroll.unwrap().offset
    }

    fn scroll_phases(msgs: &[HostMessage], tag: u64) -> Vec<(ScrollPhase, f64, f64)> {
        msgs.iter().filter_map(|m| if let HostMessage::Scroll { tag: t, phase, x, y, .. } = m { (*t == tag).then_some((*phase, *x, *y)) } else { None }).collect()
    }

    fn touches(msgs: &[HostMessage], tag: u64) -> Vec<TouchPhase> {
        msgs.iter().filter_map(|m| if let HostMessage::Touch { phase, tag: t, .. } = m { (*t == tag).then_some(*phase) } else { None }).collect()
    }

    fn asks(msgs: &[HostMessage]) -> usize {
        msgs.iter().filter(|m| matches!(m, HostMessage::TouchAsk { .. })).count()
    }

    /// JS kept the whole touch: start, moves, end, and the list did not move.
    fn stayed_in_js(msgs: &[HostMessage], sc: &Scene) {
        assert_eq!(offset(sc, "2"), (0.0, 0.0));
        assert!(scroll_phases(msgs, 2).is_empty(), "{msgs:?}");
        let t = touches(msgs, 12);
        assert_eq!((t.first(), t.last()), (Some(&TouchPhase::Start), Some(&TouchPhase::End)), "{t:?}");
        assert!(t.iter().filter(|p| **p == TouchPhase::Start).count() == 1, "finger-down's start is not repeated: {t:?}");
    }

    #[test]
    fn a_finger_drag_scrolls_a_scroll_view_and_reports_it() {
        let root = list(json!({"height":44}), false);
        // Lifted at rest: JS has the touch until the guest answers the ask, then it is cancelled and begin … end.
        let (msgs, sc) = drag_on(&root, "12", (0.0, -200.0), 16.0, 500.0, PLAIN);
        assert_eq!(asks(&msgs), 1);
        let t = touches(&msgs, 12);
        assert_eq!((t.first(), t.last()), (Some(&TouchPhase::Start), Some(&TouchPhase::Cancel)), "{t:?}");
        let phases: Vec<_> = scroll_phases(&msgs, 2).into_iter().map(|p| p.0).collect();
        assert_eq!((phases.first(), phases.last()), (Some(&ScrollPhase::Begin), Some(&ScrollPhase::End)), "{phases:?}");
        assert!(phases.contains(&ScrollPhase::Scroll) && !phases.contains(&ScrollPhase::Release));
        let off = offset(&sc, "2").1;
        assert_eq!(off, 180.0, "follows from where the finger passed the slop (the first 20 pt move)");
        // A fling: release, the momentum's scrolls, then momentum_end once it stops, further along.
        let (msgs, sc) = drag_on(&root, "12", (0.0, -200.0), 16.0, 0.0, PLAIN);
        let phases: Vec<_> = scroll_phases(&msgs, 2).into_iter().map(|p| p.0).collect();
        let release = phases.iter().position(|p| *p == ScrollPhase::Release).expect("release");
        assert_eq!(phases.last(), Some(&ScrollPhase::MomentumEnd), "{phases:?}");
        assert!(phases[release + 1..phases.len() - 1].iter().all(|p| *p == ScrollPhase::Scroll) && phases.len() > release + 2);
        assert!(offset(&sc, "2").1 > off);
    }

    #[test]
    fn a_drag_inside_the_slop_stays_a_js_touch() {
        let (msgs, sc) = drag_on(&list(json!({"height":44}), false), "12", (0.0, -9.0), 16.0, 0.0, PLAIN);
        assert_eq!(asks(&msgs), 0);
        stayed_in_js(&msgs, &sc);
    }

    #[test]
    fn a_captured_touch_stays_in_js() {
        // A gesture recognizer (a pan, or a long press already held) activated before the answer.
        let (msgs, sc) = drag_on(&list(json!({"height":44}), false), "12", (0.0, -200.0), 16.0, 500.0, Guest::Answers { captured: true, responder: None });
        assert_eq!(asks(&msgs), 1);
        stayed_in_js(&msgs, &sc);
    }

    #[test]
    fn a_blocking_js_responder_around_the_list_stops_it_scrolling() {
        let root = list(json!({"height":44}), false);
        // A PanResponder on the root (around the scroll view) keeps the drag in JS.
        let (msgs, sc) = drag_on(&root, "12", (0.0, -200.0), 16.0, 500.0, Guest::Answers { captured: false, responder: Some(1) });
        stayed_in_js(&msgs, &sc);
        // One inside the list (a pressable row) does not: the scroll view still takes the pan.
        let (_, sc) = drag_on(&root, "12", (0.0, -200.0), 16.0, 500.0, Guest::Answers { captured: false, responder: Some(12) });
        assert_eq!(offset(&sc, "2").1, 180.0);
    }

    #[test]
    fn a_drag_across_the_lists_axis_stays_in_js() {
        // A horizontal swipe on a vertical list (a swipeable row): no scroller that way, so nothing is asked.
        let (msgs, sc) = drag_on(&list(json!({"height":44}), false), "12", (-200.0, 30.0), 16.0, 500.0, PLAIN);
        assert_eq!(asks(&msgs), 0);
        stayed_in_js(&msgs, &sc);
    }

    #[test]
    fn nested_scroll_views_take_the_drag_their_way() {
        let root = list(json!({"height":44}), true);
        // Vertical on the carousel: the list around it scrolls.
        let (msgs, sc) = drag_on(&root, "52", (0.0, -200.0), 16.0, 500.0, PLAIN);
        assert_eq!((offset(&sc, "2").1, offset(&sc, "50").0), (180.0, 0.0));
        assert!(scroll_phases(&msgs, 50).is_empty());
        assert_eq!(touches(&msgs, 52).last(), Some(&TouchPhase::Cancel));
        // Horizontal on the carousel: the carousel scrolls.
        let (msgs, sc) = drag_on(&root, "52", (-200.0, 0.0), 16.0, 500.0, PLAIN);
        assert_eq!((offset(&sc, "2").1, offset(&sc, "50").0), (0.0, 180.0));
        assert!(scroll_phases(&msgs, 2).is_empty());
        assert_eq!(scroll_phases(&msgs, 50).last().map(|p| (p.0, p.1)), Some((ScrollPhase::End, 180.0)));
    }

    #[test]
    fn a_silent_guest_loses_the_drag_after_the_answer_timeout() {
        // Moves 40 ms apart: asked at 40 ms, still unanswered past 290 ms (the 8th move), the scroll takes the pan.
        let (msgs, sc) = drag_on(&list(json!({"height":44}), false), "12", (0.0, -200.0), 40.0, 500.0, Guest::Silent);
        assert_eq!(asks(&msgs), 1);
        assert_eq!(touches(&msgs, 12).last(), Some(&TouchPhase::Cancel));
        assert_eq!(scroll_phases(&msgs, 2).first().map(|p| p.0), Some(ScrollPhase::Begin));
        assert_eq!(offset(&sc, "2").1, 180.0, "from where the slop was passed");
        // Within the timeout a silent guest keeps the touch (moves 16 ms apart, lifted 144 ms after the ask).
        let (msgs, sc) = drag_on(&list(json!({"height":44}), false), "12", (0.0, -200.0), 16.0, 0.0, Guest::Silent);
        stayed_in_js(&msgs, &sc);
    }

    #[test]
    fn a_stale_answer_is_ignored_and_a_lift_before_the_answer_ends_the_js_touch() {
        let (msgs, sc) = drag_on(&list(json!({"height":44}), false), "12", (0.0, -200.0), 16.0, 0.0, Guest::Stale);
        assert_eq!(asks(&msgs), 1);
        stayed_in_js(&msgs, &sc);
    }

    #[test]
    fn pulling_down_at_the_top_reports_the_overscroll() {
        // RefreshControl fires on a negative offset; the content springs back to 0.
        let (msgs, sc) = drag_on(&list(json!({"height":44}), false), "12", (0.0, 250.0), 16.0, 500.0, PLAIN);
        let ys: Vec<f64> = scroll_phases(&msgs, 2).into_iter().map(|p| p.2).collect();
        assert!(ys.iter().any(|y| *y < -50.0), "{ys:?}");
        assert_eq!(offset(&sc, "2").1, 0.0);
    }

    #[test]
    fn focus_request_from_the_guest_focuses_and_shows_the_keyboard() {
        let (root, mut inter, mut rn) = (tree(), Interaction::default(), RnInput::default());
        let sc = scene(&root, &inter);
        let (changed, out) = rn.apply_requests(&mut inter, &RnCtx { scene: &sc, root: &root, keyboard: 300.0 }, vec![GuestMessage::FocusRequest { tag: Some(4) }]);
        assert!(changed);
        assert_eq!(inter.focus.as_deref(), Some("4"));
        assert_eq!(sent(&out), vec![(HostMessage::Focus { tag: 4 }, Wait::None), (HostMessage::Keyboard { height: 300.0 }, Wait::None)]);
    }
}
