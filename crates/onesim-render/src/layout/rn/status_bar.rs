use crate::device::StatusBar;
use crate::layout::swiftui::rn_status_bar;
use crate::scene::Node;
use crate::{Fonts, SceneParams};
use onesim_protocol::{IosVersion, RnStatusBarFrame};
use serde_json::Value;

pub(super) fn draw(nodes: &mut Vec<Node>, params: &SceneParams, fonts: &mut Fonts, props: &Value) {
    if !params.status_bar {
        return;
    }
    let mut device = *params.device;
    let vertical = device.safe_right > 0.0 && params.ios >= IosVersion::Ios27
        && props.get("__simcoreStatusBarVerticalDisabled").and_then(Value::as_bool) != Some(true);
    if !vertical && device.safe_right > 0.0 {
        // Duo's horizontal bar shares the wide viewport and uses its 24pt top band.
        device.safe_right = 0.0;
        device.ios27 = None;
        device.status_bar = StatusBar { time: (56.0, 12.0), icons: (device.viewport.width - 45.0, -20.2),
            font: 17.0, icon_scale: 1.0 };
    }
    let frame = props.get("__simcoreStatusBarFrame").and_then(|v| serde_json::from_value::<RnStatusBarFrame>(v.clone()).ok());
    let (opacity, slide) = if vertical {
        (1.0, 0.0)
    } else if let Some(f) = frame {
        (f.opacity.clamp(0.0, 1.0), f.slide.clamp(-1.0, 0.0))
    } else if props.get("__simcoreStatusBarHidden").and_then(Value::as_bool) == Some(true) {
        return;
    } else {
        (1.0, 0.0)
    };
    if opacity == 0.0 {
        return;
    }
    let (light, automatic) = if let Some(f) = frame {
        (f.light.clamp(0.0, 1.0), f.automatic.clamp(0.0, 1.0))
    } else {
        match props.get("__simcoreStatusBarStyle").and_then(Value::as_str) {
            Some("light-content") => (1.0, 0.0),
            Some("dark-content") => (0.0, 0.0),
            _ => (0.0, 1.0),
        }
    };
    let light = if automatic > 0.0 && dark_background(nodes, &device, params) { (light + automatic).min(1.0) } else { light };
    rn_status_bar(nodes, params, fonts, &device, opacity, slide * device.safe_top, light);
}

/// UIKit's default style responds to the content under the bar. Sample painted fills under the clock.
fn dark_background(nodes: &[Node], device: &crate::Device, params: &SceneParams) -> bool {
    let (mut x, y) = device.status_bar_for(params.ios).time;
    if device.safe_right > 0.0 {
        x += params.viewport.0 - device.viewport.width;
    } else if device.home_button {
        x += (params.viewport.0 - device.viewport.width) / 2.0;
    }
    let mut luminance = 0.0;
    for n in nodes {
        let Some(c) = n.fill.filter(|_| n.bounds.contains(x, y)) else { continue };
        let alpha = c.a * n.opacity;
        let shade = c.r * 0.2126 + c.g * 0.7152 + c.b * 0.0722;
        luminance += (shade - luminance) * alpha;
    }
    luminance < 0.46
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{build_scene, device, Color};
    use onesim_protocol::{Appearance, ViewNode};
    use serde_json::json;

    fn scene(device_name: &str, props: Value, enable: bool) -> crate::Scene {
        let device = device(device_name).unwrap();
        let tree: ViewNode = serde_json::from_value(json!({
            "kind": "rn", "component": "RootView", "id": "1", "props": props
        })).unwrap();
        let params = SceneParams {
            viewport: (device.viewport.width, device.viewport.height), device, scale: device.scale,
            appearance: Appearance::Light, dynamic_type: Default::default(), ios: IosVersion::Ios27,
            scroll: &Default::default(), focus: None, overscroll: false, status_bar: enable,
            chrome: Default::default(), sheets: &Default::default(),
        };
        build_scene(&tree, &params, &mut Fonts::new())
    }

    fn clock(scene: &crate::Scene) -> Option<&Node> {
        scene.nodes.iter().find(|n| n.id == "statusbar#time")
    }

    #[test]
    fn status_bar_style_overrides_content_and_default_follows_background() {
        for (background, style, expected) in [
            (0xff202020_u32, "default", 1.0), (0xfff0f0f0, "default", 0.0),
            (0xff202020, "dark-content", 0.0), (0xfff0f0f0, "light-content", 1.0),
            (0xff202020, "invalid", 1.0),
        ] {
            let s = scene("iphone-18-pro", json!({ "backgroundColor": background, "__simcoreStatusBarStyle": style }), true);
            assert_eq!(clock(&s).unwrap().text.as_ref().unwrap().color, Color::rgba(expected, expected, expected, 1.0), "{background:x} {style}");
        }
    }

    #[test]
    fn hidden_bar_keeps_the_island_and_keeps_android_navigation_independent() {
        let hidden = json!({ "__simcoreStatusBarHidden": true });
        for s in [scene("iphone-18-pro", hidden.clone(), true), scene("iphone-18-pro", json!({}), false)] {
            assert!(clock(&s).is_none());
            assert!(s.nodes.iter().any(|n| n.id == "island"));
        }
        let android = scene("pixel-10a", hidden.clone(), true);
        assert!(clock(&android).is_none());
        assert!(android.nodes.iter().any(|n| n.id == "navigation_handle"));
        assert!(clock(&scene("iphone-duo", hidden, true)).is_some());
        let horizontal = scene("iphone-duo", json!({ "__simcoreStatusBarVerticalDisabled": true }), true);
        assert!(clock(&horizontal).unwrap().bounds.center().0 < 100.0);
    }

    #[test]
    fn transition_frames_move_and_fade_glyphs_without_moving_the_island() {
        let initial = scene("iphone-18-pro", json!({}), true);
        let moving = scene("iphone-18-pro", json!({ "__simcoreStatusBarHidden": true,
            "__simcoreStatusBarFrame": { "opacity": 0.5, "slide": -0.25, "light": 0.6, "automatic": 0.3 }
        }), true);
        let (a, b) = (clock(&initial).unwrap(), clock(&moving).unwrap());
        assert!((b.bounds.y - (a.bounds.y - 62.0 / 4.0)).abs() < 1e-5);
        assert_eq!(b.opacity, 0.5);
        assert_eq!(b.text.as_ref().unwrap().color, Color::rgba(0.6, 0.6, 0.6, 1.0));
        assert_eq!(initial.nodes.last().unwrap().bounds, moving.nodes.last().unwrap().bounds);
        let malformed = scene("iphone-18-pro", json!({ "__simcoreStatusBarHidden": true,
            "__simcoreStatusBarFrame": { "opacity": "invalid" }
        }), true);
        assert!(clock(&malformed).is_none());
    }
}
