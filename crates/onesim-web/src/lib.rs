//! In-process twin of `onesim-runtime` for the browser (docs/web.md).
//!
//! Same C ABI (docs/protocol.md section C), but `submit_tree` updates an in-process scene
//! instead of writing JSON lines, and `run` returns at once: the browser is the event loop.
//! JS drives it through the `simcore_web_*` exports; input goes through
//! `onesim_render::Interaction` (the same state machine as the native host) and its effects
//! call the Swift callbacks directly.

mod abi;
mod render_out;
mod rn;
mod state;
mod web_exports;

pub use abi::*;
pub use render_out::*;
pub use rn::*;
pub use state::{ActionCb, EnvCb, GestureCb, LayoutCb, TextCb, WakeCb};
pub use web_exports::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{with, with_frame, STATE};
    use onesim_protocol::IosVersion;
    use std::cell::RefCell;
    use std::ffi::{c_char, c_void};

    const TREE: &str = r#"{"kind":"vstack","id":"0","spacing":16.0,"alignment":"center","children":[
        {"kind":"text","id":"0.0","text":"Count: 0"},
        {"kind":"button","id":"0.1","action":1,"label":{"kind":"text","id":"0.1.l","text":"Increment"}}]}"#;

    thread_local!(static HITS: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) });
    unsafe extern "C" fn on_action(_: *mut c_void, id: u64) {
        HITS.with(|h| h.borrow_mut().push(id));
    }

    #[test]
    fn submit_renders_and_click_dispatches() {
        simcore_runtime_set_callbacks(std::ptr::null_mut(), Some(on_action), None);
        assert_eq!(unsafe { simcore_submit_tree(TREE.as_ptr(), TREE.len(), 1) }, 0);
        assert_eq!(unsafe { simcore_submit_tree(TREE.as_ptr(), TREE.len(), 1) }, 1, "version must increase");
        let version = simcore_web_frame_version();
        assert_eq!(simcore_web_inspect(), 0, "layout is already available");
        assert!(with(|s| s.fb.is_empty()), "inspection does not rasterize");
        assert_eq!((simcore_web_fb_width(), simcore_web_fb_height()), (1206, 2622));
        assert_eq!(simcore_web_fb_len(), 1206 * 2622 * 4);
        assert_eq!(simcore_web_frame_version(), version, "reading pixels does not invalidate the frame");
        assert_eq!(simcore_web_pointer(2, 1.0, 1.0), 0, "miss");
        let (x, y) = STATE.with(|s| {
            let s = s.borrow();
            let b = s.scene.as_ref().unwrap().nodes.iter().find(|n| n.action == Some(1)).unwrap().bounds;
            (b.x + b.w / 2.0, b.y + b.h / 2.0)
        });
        assert_eq!(simcore_web_pointer(2, x, y), 1);
        assert_eq!(HITS.with(|h| h.borrow().clone()), vec![1]);
    }

    #[test]
    fn display_list_mode() {
        use onesim_render::display::DisplayList;
        let dl = || {
            let b = unsafe { std::slice::from_raw_parts(simcore_web_dl_ptr(), simcore_web_dl_len()) };
            DisplayList::decode(b).expect("decodes")
        };
        simcore_runtime_set_callbacks(std::ptr::null_mut(), Some(on_action), None);
        assert_eq!(unsafe { simcore_submit_tree(TREE.as_ptr(), TREE.len(), 1) }, 0);
        let cpu = with_frame(|s| s.fb.clone());
        assert!(!cpu.is_empty());
        assert_eq!(simcore_web_set_output(2), 1);
        assert_eq!(simcore_web_set_output(1), 0);
        assert_eq!(simcore_web_fb_len(), 0, "no pixels in display-list mode");
        let first = dl();
        assert_eq!((first.width, first.height), (1206, 2622));
        assert!(!first.ops.is_empty() && !first.added.is_empty(), "text sprites are sent");
        // A redraw (scale change back and forth) reuses the cached resources.
        simcore_web_set_scale(3.0);
        let again = dl();
        assert!(again.added.is_empty(), "resources are not re-sent: {}", again.added.len());
        // A size query can build a list without consuming it. Its resource changes must survive.
        simcore_web_set_scale(2.0);
        assert!(simcore_web_dl_len() > 0);
        simcore_web_set_scale(2.0); // the same frame again: nothing new by itself
        assert!(!dl().added.is_empty(), "the unread frame's resources are carried over");
        simcore_web_set_scale(3.0);
        let _ = dl();
        // Reset (new consumer): everything used is sent again.
        assert_eq!(simcore_web_set_output(1), 0);
        let reset = dl();
        assert_eq!(reset.added.len(), first.added.len());
        // Back to CPU: byte-identical pixels to before, and the list is gone.
        assert_eq!(simcore_web_set_output(0), 0);
        assert_eq!(simcore_web_dl_len(), 0);
        assert!(with(|s| s.fb == cpu), "CPU output unchanged");
    }

    /// Fixtures for the browser presenter test (`web/test/presenter.mjs`): for a few showcase trees, the same frame as
    /// a display list (`NAME.dl`) and as CPU pixels (`NAME.rgba`), plus `manifest.json`. Only runs when
    /// `SIMCORE_WEB_FIXTURE_DIR` is set (the test script does that).
    #[test]
    fn write_browser_fixtures() {
        let Ok(dir) = std::env::var("SIMCORE_WEB_FIXTURE_DIR") else { return };
        std::fs::create_dir_all(&dir).unwrap();
        const SHOWCASE: [(&str, &str); 2] = [
            ("home", include_str!("../../onesim-render/tests/data/home.json")),
            ("controls", include_str!("../../onesim-render/tests/data/controls.json")),
        ];
        let tabs = r#"[{"title":"Home","systemImage":"house","action":1},{"title":"Counter","action":2},{"title":"Settings","systemImage":"gearshape","badge":"3","action":3}]"#;
        let mut trees: Vec<(String, String)> = SHOWCASE.iter().map(|(n, t)| (n.to_string(), t.to_string())).collect();
        trees.push(("home-tabbar".into(), format!(r#"{{"kind":"tabview","id":"tabs","selected":0,"tabs":{tabs},"content":{}}}"#, SHOWCASE[0].1)));
        trees.push(("home-compact".into(), SHOWCASE[0].1.into()));
        let mut manifest = vec![];
        for (version, (name, tree)) in trees.iter().enumerate() {
            if name == "home-compact" {
                simcore_web_set_viewport(320.0, 568.0);
                simcore_web_set_scale(2.0);
            }
            let v = version as u64 + 1;
            for out in [0, 1] {
                with(|s| s.version = Some(v - 1));
                assert_eq!(unsafe { simcore_submit_tree(tree.as_ptr(), tree.len(), v) }, 0, "{name}");
                // Switching resets the builder and redraws: the list carries every resource it uses.
                assert_eq!(simcore_web_set_output(out), 0);
                let (ptr, len) = if out == 1 { (simcore_web_dl_ptr(), simcore_web_dl_len()) } else { (simcore_web_fb_ptr(), simcore_web_fb_len()) };
                let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
                std::fs::write(format!("{dir}/{name}.{}", if out == 1 { "dl" } else { "rgba" }), bytes).unwrap();
            }
            manifest.push(serde_json::json!({"name": name, "width": simcore_web_fb_width(), "height": simcore_web_fb_height()}));
        }
        std::fs::write(format!("{dir}/manifest.json"), serde_json::to_vec(&manifest).unwrap()).unwrap();
    }

    #[test]
    fn find_inspect_and_scroll() {
        simcore_runtime_set_callbacks(std::ptr::null_mut(), Some(on_action), None);
        assert_eq!(unsafe { simcore_submit_tree(TREE.as_ptr(), TREE.len(), 1) }, 0);
        let buf = |t: &str| unsafe { std::slice::from_raw_parts_mut(simcore_web_buf(t.len()), t.len()) }.copy_from_slice(t.as_bytes());
        buf("Increment");
        assert_eq!(simcore_web_find(9), 0);
        let (x, y) = with(|s| (f64::from_le_bytes(s.buf[..8].try_into().unwrap()), f64::from_le_bytes(s.buf[8..16].try_into().unwrap())));
        assert_eq!(simcore_web_pointer(2, x, y), 1);
        buf("Nope");
        assert_eq!(simcore_web_find(4), 1);
        assert_eq!(simcore_web_inspect(), 0);
        assert!(with(|s| String::from_utf8_lossy(&s.buf).contains("Increment")));
        assert_eq!(simcore_web_scroll(200.0, 400.0, 0.0, 50.0), 0, "nothing scrollable");
        buf("hi");
        assert_eq!(simcore_web_key(0, 2), -1, "no focus");
    }

    #[test]
    fn react_native_mode_relays_trees_and_touches() {
        let buf = |t: &str| unsafe { std::slice::from_raw_parts_mut(simcore_web_buf(t.len()), t.len()) }.copy_from_slice(t.as_bytes());
        let outbox = || {
            let n = simcore_web_rn_outbox();
            let v: Vec<serde_json::Value> = with(|s| serde_json::from_slice(&s.buf).unwrap());
            assert_eq!(v.len() as i32, n);
            v
        };
        assert_eq!(simcore_web_rn_init(402.0, 874.0, 1.0, 0, 0), 0);
        let tree = r#"{"type":"tree","version":7,"root":{"kind":"rn","id":"1","component":"RootView","props":{},"children":[
            {"kind":"rn","id":"2","component":"RCTView","props":{"flex":1,"paddingTop":100},"children":[
              {"kind":"rn","id":"3","component":"RCTView","props":{"height":60,"backgroundColor":4278222079},"children":[]}]}]}}"#;
        let v0 = simcore_web_frame_version();
        buf(tree);
        assert_eq!(simcore_web_rn_receive(tree.len()), 1, "a tree draws");
        assert_eq!(simcore_web_frame_version(), v0.wrapping_add(1), "a single RN tree should only lay out once");
        assert_eq!(simcore_web_fb_width(), 402);
        let first = outbox();
        assert_eq!(first.len(), 1);
        assert_eq!((first[0]["type"].as_str(), first[0]["frames"]["3"][3].as_f64()), (Some("layout"), Some(60.0)));
        assert!(outbox().is_empty(), "drained");
        // A tap on the tagged view is a touch start + end for the guest, not a Swift callback.
        assert_eq!(simcore_web_pointer(2, 200.0, 130.0), 1);
        let touches: Vec<_> = outbox().iter().map(|m| (m["type"].clone(), m["phase"].clone(), m["tag"].clone())).collect();
        assert_eq!(touches, vec![("touch".into(), "start".into(), 3.into()), ("touch".into(), "end".into(), 3.into())]);
        // The same tree version again is stale; a bad message is reported.
        buf(tree);
        assert_eq!(simcore_web_rn_receive(tree.len()), -1);
        buf("{}");
        assert_eq!(simcore_web_rn_receive(2), -1);
        assert!(with(|s| String::from_utf8_lossy(&s.buf).contains("bad guest message")));
        // A restarted guest (hello) gets every frame again with its first tree.
        buf(r#"{"type":"hello","protocol":1}"#);
        assert_eq!(simcore_web_rn_receive(29), 0);
        let again = tree.replace(r#""version":7"#, r#""version":8"#);
        buf(&again);
        assert_eq!(simcore_web_rn_receive(again.len()), 1);
        assert_eq!(outbox()[0]["frames"]["3"][3].as_f64(), Some(60.0), "layout resent after a restart");
        // Environment changes go to the guest.
        assert_eq!(simcore_web_set_dark(1), 0);
        assert_eq!(outbox()[0]["type"], "environment");
    }

    #[test]
    fn changing_ios_redraws_the_loaded_scene() {
        assert_eq!(simcore_web_set_ios(0), 0);
        assert_eq!(unsafe { simcore_submit_tree(TREE.as_ptr(), TREE.len(), 1) }, 0);
        let rendered_ios = || with(|s| s.scene.as_ref().unwrap().ios);
        assert_eq!(rendered_ios(), IosVersion::Ios26);
        let before = with_frame(|s| s.fb.clone());
        let ptr = simcore_web_fb_ptr();

        assert_eq!(simcore_web_set_ios(1), 0);
        assert_eq!(rendered_ios(), IosVersion::Ios27);
        assert!(with_frame(|s| s.fb != before), "framebuffer should redraw with the new iOS look");
        assert_eq!(simcore_web_fb_ptr(), ptr, "same-size redraw reuses the framebuffer");
    }

    #[test]
    fn scratch_buffer_submit_and_android_device_metadata() {
        assert_eq!(simcore_web_devices(), 0);
        let devs: serde_json::Value = with(|s| serde_json::from_slice(&s.buf).unwrap());
        let pixel = devs.as_array().unwrap().iter().find(|d| d["name"] == "pixel-10a").unwrap();
        assert_eq!(pixel["android"], true);
        assert!(pixel["safeTop"].as_f64().unwrap() > 50.0);
        assert!(pixel["safeBottom"].as_f64().unwrap() > 20.0);

        let put = |t: &str| {
            let p = simcore_web_buf(t.len());
            unsafe { std::slice::from_raw_parts_mut(p, t.len()) }.copy_from_slice(t.as_bytes());
            t.len()
        };
        let name_len = put("pixel-10a");
        assert_eq!(simcore_web_set_device(name_len), 0);
        let tree = r#"{"kind":"compose","id":"root","component":"Box","props":{},"children":[{"kind":"compose","id":"root/0","component":"Text","props":{"text":"Hello Compose"}}]}"#;
        let len = put(tree);
        assert_eq!(simcore_web_submit_tree(len, 1.0), 0);
        assert_eq!(simcore_web_inspect(), 0);
        assert!(with(|s| String::from_utf8_lossy(&s.buf).contains("Hello Compose")));

        // Vertical scroll on a Compose scroll container updates the framebuffer.
        let scroll_tree = r#"{"kind":"compose","id":"root","component":"Column","props":{"modifier":[{"fillMaxWidth":1.0},{"fillMaxHeight":1.0},{"scroll":true}]},"children":[
            {"kind":"compose","id":"root/0","component":"Box","props":{"modifier":[{"fillMaxWidth":1.0},{"height":600},{"background":{"color":[1,0,0,1]}}]}},
            {"kind":"compose","id":"root/1","component":"Box","props":{"modifier":[{"fillMaxWidth":1.0},{"height":600},{"background":{"color":[0,0,1,1]}}]}}]}"#;
        let slen = put(scroll_tree);
        assert_eq!(simcore_web_submit_tree(slen, 2.0), 0);
        let before = with_frame(|s| s.fb.clone());
        assert_eq!(simcore_web_scroll(200.0, 400.0, 0.0, 200.0), 1);
        assert!(with_frame(|s| s.fb != before), "compose scroll should redraw scrolled pixels");
    }

    #[test]
    fn scrolling_reports_visual_effect_geometry_without_a_guest_submit() {
        thread_local! { static FRAMES: RefCell<Vec<f64>> = const { RefCell::new(Vec::new()) }; }
        unsafe extern "C" fn capture(_: *mut c_void, line: *const c_char) {
            let bytes = unsafe { std::ffi::CStr::from_ptr(line) }.to_bytes();
            let value: serde_json::Value = serde_json::from_slice(bytes).unwrap();
            if let Some(y) = value["frames"]["effect"][1].as_f64() {
                FRAMES.with(|frames| frames.borrow_mut().push(y));
            }
        }

        let _ = simcore_web_init(200.0, 300.0, 1.0, 0);
        simcore_runtime_set_layout_callback(Some(capture));
        let tree = r#"{"kind":"scroll","id":"scroll","child":{
            "kind":"visualEffect","id":"effect","frame":[0,0,200,600],
            "child":{"kind":"rectangle","id":"rect","modifiers":[{"frame":{"height":600}}]}}}"#;
        assert_eq!(unsafe { simcore_submit_tree(tree.as_ptr(), tree.len(), 1) }, 0);
        let initial = FRAMES.with(|frames| frames.borrow().last().copied().unwrap());
        assert_eq!(simcore_web_scroll(100.0, 150.0, 0.0, 30.0), 1);
        let scrolled = FRAMES.with(|frames| frames.borrow().last().copied().unwrap());
        assert_eq!(initial - scrolled, 30.0);
    }

    #[test]
    fn scene_phase_steps_reach_the_guest() {
        thread_local! { static LINES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) }; }
        unsafe extern "C" fn capture(_: *mut c_void, line: *const c_char) {
            let s = unsafe { std::ffi::CStr::from_ptr(line) }.to_str().unwrap().to_string();
            LINES.with(|l| l.borrow_mut().push(s));
        }
        let take = || LINES.with(|l| std::mem::take(&mut *l.borrow_mut()));
        simcore_runtime_set_layout_callback(Some(capture));
        assert_eq!(simcore_web_set_scene_phase(2), 0);
        assert_eq!(take(), [r#"{"type":"scene_phase","phase":"inactive"}"#, r#"{"type":"scene_phase","phase":"background"}"#]);
        assert_eq!(simcore_web_scene_phase(), 2);
        assert_eq!(simcore_web_set_scene_phase(2), 0);
        assert!(take().is_empty(), "the same phase sends nothing");
        assert_eq!(simcore_web_set_scene_phase(0), 0);
        assert_eq!(take(), [r#"{"type":"scene_phase","phase":"inactive"}"#, r#"{"type":"scene_phase","phase":"active"}"#]);
        assert_eq!(simcore_web_set_scene_phase(1), 0);
        assert_eq!(take(), [r#"{"type":"scene_phase","phase":"inactive"}"#]);
        assert_eq!(simcore_web_set_scene_phase(3), -1);
        assert_eq!(simcore_web_set_scene_phase(-1), -1);
        assert!(take().is_empty());
        assert_eq!(simcore_web_scene_phase(), 1);
    }

    #[test]
    fn leaving_the_foreground_cancels_a_held_touch() {
        thread_local! { static PHASES: RefCell<Vec<i32>> = const { RefCell::new(Vec::new()) }; }
        unsafe extern "C" fn on_gesture(_: *mut c_void, phase: i32, _: f64, _: *const f64, _: usize, _: *const *const c_char, _: *const f64, _: usize) {
            PHASES.with(|p| p.borrow_mut().push(phase));
        }
        let _ = simcore_web_init(200.0, 300.0, 1.0, 0);
        simcore_runtime_set_gesture_callback(Some(on_gesture));
        let tree = r#"{"kind":"rectangle","id":"r","modifiers":[{"frame":{"width":200.0,"height":300.0}},{"gesture":{"key":"r#g0","pans":false}}]}"#;
        assert_eq!(unsafe { simcore_submit_tree(tree.as_ptr(), tree.len(), 1) }, 0);
        simcore_web_drag(0, 100.0, 150.0, 0.0);
        assert_eq!(PHASES.with(|p| p.borrow().clone()), vec![0], "the finger went down on the gesture view");
        assert_eq!(simcore_web_set_scene_phase(2), 0);
        assert_eq!(PHASES.with(|p| p.borrow().clone()), vec![0, 3], "the touch is cancelled");
        assert_eq!(simcore_web_set_scene_phase(0), 0);
        simcore_web_drag(2, 100.0, 150.0, 50.0);
        assert_eq!(PHASES.with(|p| p.borrow().clone()), vec![0, 3], "no touch left to end");
    }

    #[test]
    fn web_tick_redraws_final_settling_frame() {
        let _ = simcore_web_init(200.0, 200.0, 1.0, 0);
        let t1 = r#"{"kind":"rectangle","id":"r","modifiers":[{"frame":{"width":40.0,"height":40.0}}]}"#;
        let t2 = r#"{"kind":"rectangle","id":"r","modifiers":[{"frame":{"width":160.0,"height":40.0}},{"animation":{"curve":"linear","duration":0.2,"token":1}}]}"#;
        assert_eq!(unsafe { simcore_submit_tree(t1.as_ptr(), t1.len(), 1) }, 0);
        assert_eq!(unsafe { simcore_submit_tree(t2.as_ptr(), t2.len(), 2) }, 0);
        // Mid-flight at 100ms: moving, returns 1.
        assert_eq!(simcore_web_tick(100.0), 1);
        assert!(with(|s| s.fb.is_empty()), "ticking does not rasterize");
        let fb_mid = with_frame(|s| s.fb.clone());
        let mid_version = simcore_web_frame_version();
        // Final settling tick at +100ms (total 200ms): animation finishes, MUST redraw settled frame and return 1.
        assert_eq!(simcore_web_tick(100.0), 1);
        let end_version = simcore_web_frame_version();
        assert_ne!(mid_version, end_version);
        assert!(with(|s| s.fb == fb_mid), "settling rasterizes only when requested");
        let fb_end = with_frame(|s| s.fb.clone());
        assert!(fb_end != fb_mid, "final settling tick must redraw framebuffer to settled state");
        // Next tick at rest: nothing moves, returns 0.
        assert_eq!(simcore_web_tick(16.0), 0);
        assert_eq!(simcore_web_frame_version(), end_version);
    }

    #[test]
    fn lazy_tree_json_and_coalesced_drag() {
        let _ = simcore_web_init(200.0, 300.0, 1.0, 0);
        let tree = r#"{"kind":"scroll","id":"scroll","child":{"kind":"rectangle","id":"rect","modifiers":[{"frame":{"height":800}}]}}"#;
        assert_eq!(unsafe { simcore_submit_tree(tree.as_ptr(), tree.len(), 1) }, 0);
        assert!(with(|s| s.tree_json.is_empty()), "tree_json is lazy until queried");
        let len = simcore_web_tree_len();
        let ptr = simcore_web_tree_ptr();
        assert!(len > 0);
        let text = std::str::from_utf8(unsafe { std::slice::from_raw_parts(ptr, len) }).unwrap();
        assert!(text.contains("\"scroll\""));

        // Coalesced drag moves (phase 3) update drag state without rebuilding the scene.
        assert_eq!(simcore_web_drag(0, 100.0, 200.0, 0.0), 0);
        let v_before = simcore_web_frame_version();
        assert_eq!(simcore_web_drag(3, 100.0, 170.0, 4.0), 1);
        assert_eq!(simcore_web_drag(3, 100.0, 140.0, 8.0), 1);
        assert_eq!(simcore_web_frame_version(), v_before, "coalesced move skips scene rebuild");
        assert_eq!(simcore_web_drag(1, 100.0, 120.0, 12.0), 1);
        assert_ne!(simcore_web_frame_version(), v_before, "final move rebuilds scene");
    }

    #[test]
    fn coalesced_gesture_move_skips_redundant_dispatch() {
        thread_local! { static EVENTS: RefCell<Vec<(i32, f64, f64)>> = const { RefCell::new(Vec::new()) }; }
        unsafe extern "C" fn on_gesture(
            _: *mut c_void,
            phase: i32,
            _: f64,
            pts: *const f64,
            n_pts: usize,
            _: *const *const c_char,
            _: *const f64,
            _: usize,
        ) {
            let (x, y) = if n_pts > 0 {
                unsafe { (*pts, *pts.add(1)) }
            } else {
                (0.0, 0.0)
            };
            EVENTS.with(|e| e.borrow_mut().push((phase, x, y)));
        }

        let _ = simcore_web_init(200.0, 300.0, 1.0, 0);
        simcore_runtime_set_gesture_callback(Some(on_gesture));
        let tree = r#"{"kind":"rectangle","id":"r","modifiers":[{"frame":{"width":200.0,"height":300.0}},{"gesture":{"key":"r#g0","pans":true}}]}"#;
        assert_eq!(unsafe { simcore_submit_tree(tree.as_ptr(), tree.len(), 1) }, 0);
        EVENTS.with(|e| e.borrow_mut().clear());

        assert_eq!(simcore_web_drag(0, 100.0, 150.0, 0.0), 1);
        assert_eq!(simcore_web_drag(3, 110.0, 155.0, 4.0), 1);
        assert_eq!(simcore_web_drag(3, 120.0, 160.0, 8.0), 1);
        assert_eq!(
            EVENTS.with(|e| e.borrow().clone()),
            vec![(0, 100.0, 150.0)],
            "coalesced pure Move samples should not redundantly dispatch to guest"
        );
        assert_eq!(simcore_web_drag(1, 130.0, 165.0, 12.0), 1);
        assert_eq!(simcore_web_drag(2, 130.0, 165.0, 16.0), 1);
        assert_eq!(
            EVENTS.with(|e| e.borrow().clone()),
            vec![(0, 100.0, 150.0), (1, 130.0, 165.0), (2, 130.0, 165.0)],
            "final move and end dispatch normally"
        );
    }
}
