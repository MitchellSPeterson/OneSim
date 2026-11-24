//! React Native `LayoutAnimation`: the guest puts the configuration of the commit it applies to on the root
//! (`__simcoreLayoutAnimation`, protocol section H) and the host's [`crate::Animator`] plays it, as Fabric's
//! LayoutAnimationKeyFrameManager does on iOS: `update` moves and resizes views that stay, `create` / `delete` bring
//! inserted views in and removed views out.

use crate::scene::NodeAnim;
use onesim_protocol::{AnimationCurve, AnimationSpec, ContentTransitionKind, TransitionEffect, TransitionSpec};
use serde_json::Value;

/// Points of Fabric's spring interpolation sampled over the duration.
const SPRING_SAMPLES: usize = 120;

/// Fabric's `springInterpolation` (LayoutAnimationKeyFrameManager): an overshooting decay, `damping` in (0, 1].
fn spring(x: f64, damping: f64) -> f64 {
    1.0 + 2f64.powf(-10.0 * x) * ((x - damping / 4.0) * std::f64::consts::TAU / damping).sin()
}

fn curve(anim: &Value) -> AnimationCurve {
    match anim.get("type").and_then(Value::as_str) {
        Some("linear") => AnimationCurve::Linear,
        Some("easeIn") => AnimationCurve::EaseIn,
        Some("easeOut") => AnimationCurve::EaseOut,
        Some("spring") => {
            let d = anim.get("springDamping").and_then(Value::as_f64).filter(|d| *d > 0.0).unwrap_or(0.5).min(1.0);
            let mut s: Vec<f64> = (0..=SPRING_SAMPLES).map(|i| spring(i as f64 / SPRING_SAMPLES as f64, d)).collect();
            *s.last_mut().expect("samples") = 1.0;
            AnimationCurve::Samples(s)
        }
        // UIKit's keyboard curve (UIViewAnimationCurve 7), as KeyboardAvoidingView configures it.
        Some("keyboard") => AnimationCurve::Bezier([0.38, 0.7, 0.125, 1.0]),
        // easeInEaseOut, and anything unknown.
        _ => AnimationCurve::EaseInOut,
    }
}

/// One of `create` / `update` / `delete`: its curve, duration and delay (ms on the wire; the default duration is
/// the configuration's own).
fn spec(anim: &Value, duration: f64, token: u64) -> AnimationSpec {
    let ms = |k: &str| anim.get(k).and_then(Value::as_f64).filter(|v| v.is_finite() && *v >= 0.0);
    AnimationSpec {
        curve: curve(anim),
        duration: ms("duration").unwrap_or(duration) / 1000.0,
        delay: ms("delay").unwrap_or(0.0) / 1000.0,
        speed: 1.0,
        repeat: None,
        autoreverses: false,
        token,
    }
}

/// The state a created view starts from / a deleted view ends at.
fn effect(anim: Option<&Value>) -> TransitionEffect {
    let Some(a) = anim else { return TransitionEffect::default() };
    match a.get("property").and_then(Value::as_str) {
        Some("scaleXY") => TransitionEffect { scale: Some(0.0), ..Default::default() },
        Some("scaleX") => TransitionEffect { scale_x: Some(0.0), ..Default::default() },
        Some("scaleY") => TransitionEffect { scale_y: Some(0.0), ..Default::default() },
        // `opacity`, and Fabric's default for a create / delete without one.
        _ => TransitionEffect { opacity: Some(0.0), ..Default::default() },
    }
}

/// The animation every view of the commit carries, from the root's `__simcoreLayoutAnimation`
/// (`{duration, create?, update?, delete?, token}`); `None` when the commit is not animated.
pub(crate) fn node_anim(root_props: &Value) -> Option<NodeAnim> {
    let cfg = root_props.get("__simcoreLayoutAnimation")?;
    let token = cfg.get("token").and_then(Value::as_u64).filter(|t| *t > 0)?;
    let duration = cfg.get("duration").and_then(Value::as_f64).filter(|d| d.is_finite() && *d >= 0.0).unwrap_or(0.0);
    let part = |k: &str| cfg.get(k).filter(|v| v.is_object());
    let (create, delete) = (part("create"), part("delete"));
    let transition = TransitionSpec { insertion: effect(create), removal: effect(delete), animation: create.map(|a| spec(a, duration, token)) };
    Some(NodeAnim {
        animation: part("update").map(|a| spec(a, duration, token)),
        transition: Some(transition),
        matched: None,
        content_transition: ContentTransitionKind::Identity,
        layout: true,
        removal: delete.map(|a| spec(a, duration, token)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn no_configuration_no_animation() {
        assert!(node_anim(&json!({})).is_none());
        assert!(node_anim(&json!({"__simcoreLayoutAnimation": {"duration": 300}})).is_none(), "no token");
    }

    #[test]
    fn preset_ease_in_ease_out() {
        // LayoutAnimation.Presets.easeInEaseOut
        let a = node_anim(&json!({"__simcoreLayoutAnimation": {"duration": 300, "token": 4,
            "create": {"type": "easeInEaseOut", "property": "opacity"}, "update": {"type": "easeInEaseOut"},
            "delete": {"type": "easeInEaseOut", "property": "opacity"}}}))
        .unwrap();
        let u = a.animation.unwrap();
        assert_eq!((u.curve, u.duration, u.delay, u.token), (AnimationCurve::EaseInOut, 0.3, 0.0, 4));
        let t = a.transition.unwrap();
        assert_eq!(t.insertion.opacity, Some(0.0));
        assert_eq!(t.removal.opacity, Some(0.0));
        assert!(a.layout);
    }

    #[test]
    fn spring_samples_overshoot_and_settle() {
        // LayoutAnimation.Presets.spring: update spring with damping 0.4, create/delete linear opacity.
        let a = node_anim(&json!({"__simcoreLayoutAnimation": {"duration": 700, "token": 1,
            "create": {"type": "linear", "property": "scaleXY"}, "update": {"type": "spring", "springDamping": 0.4}}}))
        .unwrap();
        let u = a.animation.unwrap();
        let AnimationCurve::Samples(s) = &u.curve else { panic!("spring is sampled") };
        assert_eq!(s[0], spring(0.0, 0.4));
        assert!(s.iter().any(|v| *v > 1.05), "overshoots");
        assert_eq!(*s.last().unwrap(), 1.0);
        assert_eq!(u.duration, 0.7);
        let t = a.transition.unwrap();
        assert_eq!(t.insertion.scale, Some(0.0));
        assert_eq!(t.removal, TransitionEffect::default(), "no delete: removed views go at once");
        assert_eq!(t.animation.unwrap().curve, AnimationCurve::Linear);
        assert!(a.removal.is_none());
    }

    #[test]
    fn own_durations_delays_and_axes() {
        let a = node_anim(&json!({"__simcoreLayoutAnimation": {"duration": 500, "token": 2,
            "create": {"type": "easeOut", "property": "scaleX", "duration": 200, "delay": 100},
            "delete": {"type": "keyboard", "property": "scaleY"}}}))
        .unwrap();
        assert!(a.animation.is_none(), "no update: frames jump");
        let t = a.transition.unwrap();
        assert_eq!(t.insertion.scale_x, Some(0.0));
        assert_eq!(t.removal.scale_y, Some(0.0));
        let c = t.animation.unwrap();
        assert_eq!((c.duration, c.delay, c.curve), (0.2, 0.1, AnimationCurve::EaseOut));
        let d = a.removal.unwrap();
        assert_eq!((d.duration, d.delay), (0.5, 0.0), "delete's own timing for removals");
        assert!(matches!(d.curve, AnimationCurve::Bezier(_)));
    }

    #[test]
    fn bad_numbers_fall_back() {
        let a = node_anim(&json!({"__simcoreLayoutAnimation": {"duration": -3, "token": 1, "update": {"type": "nope", "duration": "x"}}})).unwrap();
        let u = a.animation.unwrap();
        assert_eq!((u.curve, u.duration), (AnimationCurve::EaseInOut, 0.0));
    }
}
