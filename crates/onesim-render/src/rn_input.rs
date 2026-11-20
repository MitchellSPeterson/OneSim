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

/// Per-session React Native input state (the touch in progress, the frames the guest already has).
#[derive(Default)]
pub struct RnInput {
    touch: Option<u64>,
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
    /// Pointer input. Scrolls become `scroll` events, the software keyboard and native controls are the
    /// interaction state machine's, native chrome (stack back pill, tab bar items) sends its own message, and
    /// everything else is a touch (a tap on a text input focuses it). Returns `(hit node id, acted, messages)`.
    pub fn pointer(&mut self, inter: &mut Interaction, cx: &RnCtx, ev: PointerEvent) -> (Option<String>, bool, Vec<Out>) {
        let scene = cx.scene;
        if ev.kind == PointerKind::Scroll {
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
                if n.scroll.is_some_and(|i| i.vertical && i.offset.1 <= 0.0) && ev.dy < 0.0 {
                    out.extend([(ScrollPhase::Begin, (0.0, 0.0)), (ScrollPhase::Scroll, (0.0, ev.dy)), (ScrollPhase::End, (0.0, 0.0))].into_iter().filter_map(|(p, o)| scroll_message(n, p, o)));
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
        let hosted_scroll = ev.kind == PointerKind::Drag && scene.scroll_target(ev.x, ev.y).is_some_and(|i| scene.nodes[i].tag.is_none());
        if hosted_scroll || hit.is_some_and(|n| n.action.is_some() || n.role == Role::Slider) {
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

    /// One phase of a live finger drag, delivered to the touched Fabric view (React Native supplies its own gesture
    /// behavior). `None`: the drag belongs to a native control (a slider, a scroller React Native does not own), so
    /// the caller runs `Interaction::drag` instead.
    pub fn drag(&mut self, inter: &Interaction, scene: &Scene, phase: DragPhase, x: f64, y: f64, t_ms: f64) -> Option<Vec<Out>> {
        let native = if phase == DragPhase::Begin {
            scene.hit_test(x, y).is_some_and(|i| scene.nodes[i].role == Role::Slider) || scene.scroll_target(x, y).is_some_and(|i| scene.nodes[i].tag.is_none())
        } else {
            inter.has_drag_target()
        };
        if native {
            return None;
        }
        let (phase, tag) = match phase {
            DragPhase::Begin => {
                self.touch = scene.hit_test(x, y).and_then(|i| scene.nodes[i].tag);
                (TouchPhase::Start, self.touch)
            }
            DragPhase::Move => (TouchPhase::Move, self.touch),
            DragPhase::End => (TouchPhase::End, self.touch.take()),
        };
        let n = tag.and_then(|tag| scene.nodes.iter().find(|n| n.tag == Some(tag)));
        Some(n.map(|n| Out::Send(touch_message(phase, n, x, y, t_ms), Wait::None)).into_iter().collect())
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
        let (root, inter, mut rn) = (tree(), Interaction::default(), RnInput::default());
        let sc = scene(&root, &inter);
        let (x, y) = center(&sc, "3");
        let phase = |o: Option<Vec<Out>>| match &sent(&o.expect("React Native drag"))[..] {
            [(HostMessage::Touch { phase, tag: 3, .. }, Wait::None)] => *phase,
            m => panic!("{m:?}"),
        };
        assert_eq!(phase(rn.drag(&inter, &sc, DragPhase::Begin, x, y, 0.0)), TouchPhase::Start);
        assert_eq!(phase(rn.drag(&inter, &sc, DragPhase::Move, x, 600.0, 16.0)), TouchPhase::Move);
        assert_eq!(phase(rn.drag(&inter, &sc, DragPhase::End, x, 600.0, 32.0)), TouchPhase::End);
        assert_eq!(rn.drag(&inter, &sc, DragPhase::Move, x, 600.0, 48.0), Some(vec![]), "no touch in progress");
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
