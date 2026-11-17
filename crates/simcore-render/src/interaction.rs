//! Transport-agnostic interaction state machine (docs/protocol.md section D/E).
//!
//! Owns scroll offsets, text focus, software-keyboard shift and the press target. Feed it pointer
//! and keyboard events together with the scene they hit; it returns what the transport must do:
//! the native host sends the effect over the guest wire, the browser calls the Swift callbacks.
use crate::{autocap, ChromeParams, Role, Scene};
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
    /// Node id of the focused text field.
    pub focus: Option<String>,
    /// Software keyboard shift state. Key taps and focus update it; `keyboard` does not.
    pub shift: bool,
    /// Current tree version; the transport sets it whenever a tree arrives. Stamped on effects.
    pub version: u64,
    /// Node id hit by the last `pointer` (the scroller for scroll events).
    pub hit: Option<String>,
    /// Whether the last `pointer` was an action (control fired or keyboard key tapped).
    pub acted: bool,
    pressed: Option<String>,
}

impl Interaction {
    /// Chrome parameters for the next layout (pair with `focus.as_deref()`).
    pub fn chrome(&self, island: Option<(f64, f64, f64, f64)>) -> ChromeParams {
        ChromeParams { keyboard: self.focus.is_some(), shift: self.shift, island }
    }

    /// `click` = down + up. A hit on a text field focuses it; any other click unfocuses. `scroll`
    /// hits the scroller and never reaches the guest.
    pub fn pointer(&mut self, scene: &Scene, ev: PointerEvent) -> Effect {
        self.hit = None;
        self.acted = false;
        if ev.kind == PointerKind::Scroll {
            let Some(i) = scene.scroll_target(ev.x, ev.y) else { return Effect::None };
            let n = &scene.nodes[i];
            let info = n.scroll.expect("scroll_target has scroll");
            let (ox, oy) = info.offset;
            let new = ((ox + ev.dx).clamp(0.0, info.max.0), (oy + ev.dy).clamp(0.0, info.max.1));
            self.hit = Some(n.id.clone());
            self.scroll.insert(n.id.clone(), new);
            return if new == info.offset { Effect::None } else { Effect::Redraw };
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
        .ok_or_else(|| format!("no button, toggle, text field or key labelled {spec:?}"))?,
    };
    Ok(center(n))
}
