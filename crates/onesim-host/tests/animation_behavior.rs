//! Deterministic trajectory & interactive behavior test harness (issue #127, Layer 1).
//!
//! Runs in CI with `cargo test --workspace` (no simulator or Swift toolchain required).
//! Steps OneSim's deterministic virtual clock (`clock_ms`) in fixed frame increments and
//! asserts on interpolated `Scene` / `UiNode` trajectories, spring retargeting continuity,
//! incremental gestures, sheet snap-back vs. dismiss, scroll momentum, modal hit-testing,
//! and v1/v2 RPC clock & drag APIs.

use onesim_host::{serve_ws, Host, Server};
use onesim_protocol::*;
use onesim_render::DragPhase;
use serde_json::{json, Value};
use std::net::TcpStream;
use std::time::Duration;
use tungstenite::{Message, WebSocket};

const GUEST: &str = env!("CARGO_BIN_EXE_onesim-fake-guest");

fn motion_guest() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("onesim-motion-{}-{:?}", std::process::id(), std::thread::current().id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("fake-motion-guest").with_extension(std::env::consts::EXE_EXTENSION);
    let _ = std::fs::remove_file(&path);
    #[cfg(unix)]
    std::os::unix::fs::symlink(GUEST, &path).unwrap();
    #[cfg(not(unix))]
    std::fs::copy(GUEST, &path).unwrap();
    path
}

fn start_motion() -> (Host, String, std::path::PathBuf) {
    let g = motion_guest();
    let mut host = Host::new();
    let p: CreateSessionParams = serde_json::from_value(json!({"framework": "swiftui", "app_path": g})).unwrap();
    let id = host.create_session(p).unwrap();
    host.start_session(&id).unwrap();
    (host, id, g)
}

fn find<'a>(n: &'a UiNode, pred: &dyn Fn(&UiNode) -> bool) -> Option<&'a UiNode> {
    if pred(n) {
        return Some(n);
    }
    n.children.iter().find_map(|c| find(c, pred))
}

fn center(n: &UiNode) -> (f64, f64) {
    (n.frame.x + n.frame.width / 2.0, n.frame.y + n.frame.height / 2.0)
}

fn click_id(host: &mut Host, sid: &str, node_id: &str) -> (Option<String>, bool) {
    let root = host.inspect(sid).unwrap().1;
    let n = find(&root, &|n| n.id == node_id).unwrap_or_else(|| panic!("node {node_id:?} not found"));
    let (x, y) = center(n);
    host.pointer(sid, PointerEvent::new(PointerKind::Click, x, y)).unwrap()
}

fn node_frame(host: &mut Host, sid: &str, node_id: &str) -> UiFrame {
    let root = host.inspect(sid).unwrap().1;
    find(&root, &|n| n.id == node_id).unwrap_or_else(|| panic!("node {node_id:?} not found")).frame
}

fn node_opacity(host: &mut Host, sid: &str, node_id: &str) -> f32 {
    let root = host.inspect(sid).unwrap().1;
    find(&root, &|n| n.id == node_id).unwrap_or_else(|| panic!("node {node_id:?} not found")).opacity
}

#[test]
fn spring_overshoots_and_settles_smoothly() {
    let (mut host, sid, _) = start_motion();
    let w0 = node_frame(&mut host, &sid, "m.spring").width;
    assert!((w0 - 60.0).abs() < 0.5, "initial width {w0}");

    assert!(click_id(&mut host, &sid, "m.b.spring").1);
    assert!(host.is_animating(&sid), "spring must be animating right after click");

    let mut widths = vec![node_frame(&mut host, &sid, "m.spring").width];
    // Step in 16ms frames for 1.6s.
    for _ in 0..100 {
        host.tick(&sid, 0.016).unwrap();
        widths.push(node_frame(&mut host, &sid, "m.spring").width);
    }

    // Smoothly interpolated, not snapped on frame 0 or frame 1.
    assert!((widths[0] - 60.0).abs() < 1.0, "t=0 should start at 60, got {}", widths[0]);
    assert!(widths[2] > 61.0 && widths[2] < 200.0, "early frame should be mid-flight, got {}", widths[2]);

    // Underdamped spring (stiffness=200, damping=12) overshoots target 220.
    let peak = widths.iter().copied().fold(0.0_f64, f64::max);
    assert!(peak > 225.0, "underdamped spring should overshoot 220 (peak={peak:.2})");

    // Settles cleanly to target 220.
    let settled = *widths.last().unwrap();
    assert!((settled - 220.0).abs() < 0.5, "settled width should be 220, got {settled}");
    assert!(!host.is_animating(&sid), "spring should be at rest after 1.6s");
    host.shutdown();
}

#[test]
fn ease_in_out_s_curve_and_opacity_interpolation() {
    let (mut host, sid, _) = start_motion();
    let w0 = node_frame(&mut host, &sid, "m.ease").width;
    let op0 = node_opacity(&mut host, &sid, "m.ease");
    assert!((w0 - 80.0).abs() < 0.5 && (op0 - 1.0).abs() < 1e-3);

    // At rest, UiNode omits opacity == 1.0 in serialized JSON.
    let raw_before = serde_json::to_value(host.inspect(&sid).unwrap().1).unwrap();
    assert!(raw_before.to_string().contains("\"m.ease\""));

    assert!(click_id(&mut host, &sid, "m.b.ease").1);

    // At t = 100ms (25% of 400ms easeInOut), progress must be < 0.25 (starts slow).
    host.tick(&sid, 0.1).unwrap();
    let w_q1 = node_frame(&mut host, &sid, "m.ease").width;
    let op_q1 = node_opacity(&mut host, &sid, "m.ease");
    let prog_q1 = (w_q1 - 80.0) / 100.0;
    assert!(prog_q1 > 0.05 && prog_q1 < 0.24, "easeInOut at 25% time should be <25% distance, got {prog_q1:.3}");
    assert!(op_q1 < 0.96 && op_q1 > 0.75, "opacity at 25% time should be mid-fade, got {op_q1:.3}");

    // At t = 200ms (50% of 400ms), symmetric midpoint ~ 0.5.
    host.tick(&sid, 0.1).unwrap();
    let w_mid = node_frame(&mut host, &sid, "m.ease").width;
    let op_mid = node_opacity(&mut host, &sid, "m.ease");
    assert!((w_mid - 130.0).abs() < 3.0, "midpoint width ~130, got {w_mid:.2}");
    assert!((op_mid - 0.6).abs() < 0.03, "midpoint opacity ~0.6, got {op_mid:.3}");

    // Settles at t = 400ms.
    host.tick(&sid, 0.2).unwrap();
    assert!((node_frame(&mut host, &sid, "m.ease").width - 180.0).abs() < 0.5);
    assert!((node_opacity(&mut host, &sid, "m.ease") - 0.2).abs() < 0.01);
    assert!(!host.is_animating(&sid));
    host.shutdown();
}

#[test]
fn repeat_forever_keeps_advancing_on_ticks() {
    let (mut host, sid, _) = start_motion();
    assert!(click_id(&mut host, &sid, "m.b.pulse").1);

    // Sample across multiple 200ms autoreversing cycles (total > 2s).
    let mut samples = vec![];
    for _ in 0..25 {
        let active = host.tick(&sid, 0.1).unwrap();
        assert!(active, "repeatForever must remain animating");
        samples.push(node_frame(&mut host, &sid, "m.pulse").width);
    }
    let min_w = samples.iter().copied().fold(f64::INFINITY, f64::min);
    let max_w = samples.iter().copied().fold(0.0_f64, f64::max);
    assert!(min_w < 70.0 && max_w > 150.0, "repeatForever should oscillate 60..160, got {min_w:.1}..{max_w:.1}");
    host.shutdown();
}

#[test]
fn spring_retargeting_mid_flight_preserves_position_and_velocity_continuity() {
    let (mut host, sid, _) = start_motion();
    assert!(click_id(&mut host, &sid, "m.b.spring").1);

    // Advance 7 frames (112ms) into 60 -> 220 expansion; width is actively increasing.
    let mut prev_w = node_frame(&mut host, &sid, "m.spring").width;
    let mut w_before = prev_w;
    for _ in 0..7 {
        host.tick(&sid, 0.016).unwrap();
        prev_w = w_before;
        w_before = node_frame(&mut host, &sid, "m.spring").width;
    }
    let vel_before = (w_before - prev_w) / 0.016;
    assert!(vel_before > 100.0, "should be moving fast in +w before reversal (vel={vel_before:.1})");
    assert!(w_before > 90.0 && w_before < 200.0, "mid-flight width expected, got {w_before:.1}");

    // Interrupt & reverse back to 60 at t = 112ms!
    assert!(click_id(&mut host, &sid, "m.b.spring").1);

    // Position continuity at the reversal instant: no teleporting.
    let w_at_reverse = node_frame(&mut host, &sid, "m.spring").width;
    assert!(
        (w_at_reverse - w_before).abs() < 0.5,
        "retargeting must not teleport: before={w_before:.2}, at_reverse={w_at_reverse:.2}"
    );

    // Velocity continuity: first 16ms frame after reversal must carry forward +w momentum
    // (continuing slightly upward while decelerating) before turning around toward 60.
    host.tick(&sid, 0.016).unwrap();
    let w_after_1 = node_frame(&mut host, &sid, "m.spring").width;
    assert!(
        w_after_1 > w_at_reverse,
        "velocity continuity requires carrying +w momentum before turning around: at_reverse={w_at_reverse:.2}, after_16ms={w_after_1:.2}"
    );

    // Eventually turns around and settles cleanly back at 60.
    host.wait(&sid, 2000.0).unwrap();
    let w_final = node_frame(&mut host, &sid, "m.spring").width;
    assert!((w_final - 60.0).abs() < 0.5, "should settle back at 60, got {w_final:.2}");
    host.shutdown();
}

#[test]
fn transition_insertion_removal_ghosts_and_matched_geometry() {
    let (mut host, sid, _) = start_motion();

    // 1. Insert card with asymmetric transition (opacity 0 + offset y +60).
    assert!(click_id(&mut host, &sid, "m.b.card").1);
    host.tick(&sid, 0.15).unwrap(); // halfway through 300ms linear
    let mid_y = node_frame(&mut host, &sid, "m.card").y;
    let mid_op = node_opacity(&mut host, &sid, "m.card");
    assert!((mid_op - 0.5).abs() < 0.05, "insertion opacity halfway ~0.5, got {mid_op}");
    host.tick(&sid, 0.15).unwrap(); // settled
    let end_y = node_frame(&mut host, &sid, "m.card").y;
    assert!(mid_y > end_y + 20.0, "card should rise from below: mid_y={mid_y:.1}, end_y={end_y:.1}");
    assert!((node_opacity(&mut host, &sid, "m.card") - 1.0).abs() < 1e-3);

    // 2. Remove card: mid-flight removal ghost exists in Scene with action == None (not hittable).
    assert!(click_id(&mut host, &sid, "m.b.card").1);
    host.tick(&sid, 0.15).unwrap();
    let (ghost_present, ghost_interactive, ghost_op) = host
        .with_frame(&sid, |scene, _, _| {
            let g = scene.nodes.iter().find(|n| n.id == "m.card");
            (g.is_some(), g.is_some_and(|n| n.action.is_some()), g.map_or(0.0, |n| n.opacity))
        })
        .unwrap();
    assert!(ghost_present, "removal ghost should be visible mid-flight");
    assert!(!ghost_interactive, "removal ghost must not be interactive/hittable");
    assert!((ghost_op - 0.5).abs() < 0.05, "removal ghost opacity ~0.5, got {ghost_op}");
    host.tick(&sid, 0.2).unwrap();
    let still_there = host.with_frame(&sid, |s, _, _| s.nodes.iter().any(|n| n.id == "m.card")).unwrap();
    assert!(!still_there, "removal ghost must vanish after settling");

    // 3. matchedGeometryEffect pill morphs from TabA (width 70) to TabB (width 110).
    let pill_a = node_frame(&mut host, &sid, "m.pill.0");
    assert!(click_id(&mut host, &sid, "m.b.tab1").1);
    host.tick(&sid, 0.15).unwrap(); // halfway through 300ms linear
    let pill_mid = node_frame(&mut host, &sid, "m.pill.1");
    assert!(pill_mid.x > pill_a.x + 15.0, "pill should slide right: a.x={}, mid.x={}", pill_a.x, pill_mid.x);
    assert!((pill_mid.width - 90.0).abs() < 3.0, "pill width should morph 70->110 (mid ~90), got {}", pill_mid.width);
    host.tick(&sid, 0.2).unwrap();
    let pill_end = node_frame(&mut host, &sid, "m.pill.1");
    assert!((pill_end.width - 110.0).abs() < 0.5, "settled pill width 110, got {}", pill_end.width);
    assert!(pill_end.x > pill_mid.x + 15.0);
    host.shutdown();
}

#[test]
fn phase_animator_advances_on_scheduled_ticks() {
    let (mut host, sid, _) = start_motion();
    let phase_text = |host: &mut Host| {
        let root = host.inspect(&sid).unwrap().1;
        find(&root, &|n| n.id == "m.phase").unwrap().text.clone().unwrap()
    };
    assert_eq!(phase_text(&mut host), "Phase: 0");
    assert!(click_id(&mut host, &sid, "m.b.phase").1);
    assert_eq!(phase_text(&mut host), "Phase: 1");

    // Step 50ms: not yet due (afterMs = 100).
    host.wait(&sid, 50.0).unwrap();
    assert_eq!(phase_text(&mut host), "Phase: 1");

    // Step another 60ms (total 110ms): phase 2 fires.
    host.wait(&sid, 60.0).unwrap();
    assert_eq!(phase_text(&mut host), "Phase: 2");

    // Step another 100ms: phase 3 fires and completes.
    host.wait(&sid, 100.0).unwrap();
    assert_eq!(phase_text(&mut host), "Phase: 3");
    assert!(!host.is_animating(&sid));
    host.shutdown();
}

#[test]
fn continuous_drag_tracks_finger_and_commits_on_release() {
    let (mut host, sid, _) = start_motion();
    let f0 = node_frame(&mut host, &sid, "m.orb");
    let (x0, y0) = (f0.x + f0.width / 2.0, f0.y + f0.height / 2.0);

    // Begin + 4 incremental move steps (25pt each) + End.
    host.drag(&sid, DragPhase::Begin, x0, y0, 0.0).unwrap();
    for step in 1..=4 {
        let dx = f64::from(step) * 25.0;
        let t = f64::from(step) * 16.0;
        host.drag(&sid, DragPhase::Move, x0 + dx, y0, t).unwrap();
        let f = node_frame(&mut host, &sid, "m.orb");
        assert!(
            (f.x - (f0.x + dx)).abs() < 1.0,
            "step {step}: orb must track finger live (expected {}, got {})",
            f0.x + dx,
            f.x
        );
    }
    host.drag(&sid, DragPhase::End, x0 + 100.0, y0, 80.0).unwrap();
    let f_end = node_frame(&mut host, &sid, "m.orb");
    assert!((f_end.x - (f0.x + 100.0)).abs() < 1.0, "committed x: {}", f_end.x);
    let root = host.inspect(&sid).unwrap().1;
    assert_eq!(find(&root, &|n| n.id == "m.orb.pos").unwrap().text.as_deref(), Some("Orb: 100,0"));
    host.shutdown();
}

#[test]
fn sheet_drag_snap_back_dismiss_and_modal_hit_testing() {
    let (mut host, sid, _) = start_motion();
    let under_btn = find(&host.inspect(&sid).unwrap().1, &|n| n.id == "m.b.under").unwrap().clone();
    let (ux, uy) = center(&under_btn);

    // Open half-sheet.
    assert!(click_id(&mut host, &sid, "m.b.sheet").1);
    assert!(find(&host.inspect(&sid).unwrap().1, &|n| n.id == "m.sh.t").is_some(), "sheet should be open");

    // Underlying button is blocked by the modal scrim.
    let _ = host.pointer(&sid, PointerEvent::new(PointerKind::Click, ux, uy)).unwrap();
    assert_eq!(
        find(&host.inspect(&sid).unwrap().1, &|n| n.id == "m.b.under").and_then(|n| n.label.clone()),
        Some("Under: 0".into()),
        "tapping underlying button while sheet is open must not increment it"
    );

    // Interactive controls inside the sheet work.
    assert!(click_id(&mut host, &sid, "m.sh.inc").1);
    assert_eq!(
        find(&host.inspect(&sid).unwrap().1, &|n| n.id == "m.sh.t").and_then(|n| n.text.clone()),
        Some("SheetTaps: 1".into())
    );

    // Drag sheet down slightly (40pt, below dismiss threshold): snaps back and stays open.
    let sheet_top = host
        .with_frame(&sid, |s, _, _| s.nodes.iter().find_map(|n| n.sheet.as_ref().map(|sh| sh.top)).unwrap())
        .unwrap();
    let grab_y = sheet_top + 20.0;
    host.drag(&sid, DragPhase::Begin, 200.0, grab_y, 0.0).unwrap();
    host.drag(&sid, DragPhase::Move, 200.0, grab_y + 40.0, 32.0).unwrap();
    let dragged_top = host
        .with_frame(&sid, |s, _, _| s.nodes.iter().find_map(|n| n.sheet.as_ref().map(|sh| sh.top)).unwrap())
        .unwrap();
    assert!(dragged_top > sheet_top + 20.0, "sheet should follow finger down: {sheet_top} -> {dragged_top}");
    host.drag(&sid, DragPhase::End, 200.0, grab_y + 40.0, 200.0).unwrap();
    assert!(find(&host.inspect(&sid).unwrap().1, &|n| n.id == "m.sh.t").is_some(), "small drag must snap back, not dismiss");

    // Drag sheet down past dismiss threshold (300pt): dismisses!
    host.drag(&sid, DragPhase::Begin, 200.0, grab_y, 300.0).unwrap();
    host.drag(&sid, DragPhase::Move, 200.0, grab_y + 150.0, 332.0).unwrap();
    host.drag(&sid, DragPhase::Move, 200.0, grab_y + 300.0, 364.0).unwrap();
    host.drag(&sid, DragPhase::End, 200.0, grab_y + 300.0, 396.0).unwrap();
    assert!(find(&host.inspect(&sid).unwrap().1, &|n| n.id == "m.sh.t").is_none(), "large drag down must dismiss sheet");

    // Underlying button is hittable again after dismissal!
    assert!(host.pointer(&sid, PointerEvent::new(PointerKind::Click, ux, uy)).unwrap().1);
    assert_eq!(
        find(&host.inspect(&sid).unwrap().1, &|n| n.id == "m.b.under").and_then(|n| n.label.clone()),
        Some("Under: 1".into()),
        "underlying button must be hittable again after sheet dismisses"
    );
    host.shutdown();
}

#[test]
fn scroll_momentum_decelerates_after_fling() {
    let (mut host, sid, _) = start_motion();
    let sc = node_frame(&mut host, &sid, "m.scroll");
    let (sx, sy) = (sc.x + sc.width / 2.0, sc.y + sc.height * 0.7);
    let y0 = node_frame(&mut host, &sid, "m.sv.0").y;

    // Fast upward fling (dragging finger up by 120pt in 48ms).
    host.drag(&sid, DragPhase::Begin, sx, sy, 0.0).unwrap();
    host.drag(&sid, DragPhase::Move, sx, sy - 40.0, 16.0).unwrap();
    host.drag(&sid, DragPhase::Move, sx, sy - 80.0, 32.0).unwrap();
    host.drag(&sid, DragPhase::Move, sx, sy - 120.0, 48.0).unwrap();
    host.drag(&sid, DragPhase::End, sx, sy - 120.0, 56.0).unwrap();

    let y_release = node_frame(&mut host, &sid, "m.sv.0").y;
    assert!(y_release < y0 - 80.0, "drag should have scrolled content up: y0={y0}, y_release={y_release}");

    // Momentum ticks should keep scrolling further and eventually come to rest.
    let mut still_moving = false;
    let mut moved_after_release = false;
    let mut prev_y = y_release;
    for _ in 0..180 {
        still_moving = host.tick(&sid, 0.016).unwrap();
        let y = node_frame(&mut host, &sid, "m.sv.0").y;
        if y < prev_y - 0.1 {
            moved_after_release = true;
        }
        prev_y = y;
        if !still_moving {
            break;
        }
    }
    assert!(moved_after_release, "fling must produce post-release scroll momentum");
    assert!(!still_moving, "scroll momentum must decelerate to rest");
    host.shutdown();
}

#[test]
fn v1_and_v2_rpc_tick_wait_drag_and_validation() {
    let g = motion_guest();
    // 1. v1 JSON-lines (`onesim serve`)
    let mut host = Host::new();
    let mut out = Vec::new();
    let g_val = Value::from(g.to_string_lossy().into_owned());
    let input = format!(
        r#"{{"id":1,"method":"hello","params":{{"protocol_version":1}}}}
{{"id":2,"method":"create_session","params":{{"framework":"swiftui","app_path":{g_val}}}}}
{{"id":3,"method":"start_session","params":{{"session_id":"s1"}}}}
{{"id":4,"method":"tick","params":{{"session_id":"s1","dt_ms":-5}}}}
{{"id":5,"method":"tick","params":{{"session_id":"s1","dt_ms":16}}}}
{{"id":6,"method":"wait","params":{{"session_id":"s1","ms":100}}}}
{{"id":7,"method":"send_drag_phase","params":{{"session_id":"s1","phase":"begin","x":200,"y":300}}}}
{{"id":8,"method":"send_drag_phase","params":{{"session_id":"s1","phase":"end","x":200,"y":300}}}}
{{"id":9,"method":"shutdown"}}
"#
    );
    host.serve(input.as_bytes(), &mut out).unwrap();
    let lines: Vec<Value> = String::from_utf8(out).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(lines[3]["error"]["code"], "bad_request");
    assert_eq!(lines[4]["result"]["clock_ms"], 16.0);
    assert_eq!(lines[5]["result"]["clock_ms"], 116.0);
    assert!(lines[6]["ok"].as_bool().unwrap());
    assert!(lines[7]["ok"].as_bool().unwrap());

    // 2. v2 JSON-RPC over WebSocket (`runtime.tick`, `runtime.wait`, `input.drag`)
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    std::thread::spawn(move || serve_ws(listener, Server::new()));
    let stream = TcpStream::connect(&addr).unwrap();
    stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let (mut ws, _): (WebSocket<TcpStream>, _) = tungstenite::client(format!("ws://{addr}/"), stream).unwrap();
    let mut seq = 1u64;
    let mut call = |method: &str, params: Value| -> Value {
        let id = seq;
        seq += 1;
        ws.send(Message::text(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string())).unwrap();
        loop {
            if let Message::Text(t) = ws.read().unwrap() {
                let v: Value = serde_json::from_str(t.as_str()).unwrap();
                if v["id"] == id {
                    return v;
                }
            }
        }
    };
    let caps = call("initialize", json!({"protocolVersion": "2.0"}));
    let methods = caps["result"]["capabilities"]["methods"].as_array().unwrap();
    assert!(methods.contains(&json!("runtime.tick")));
    assert!(methods.contains(&json!("runtime.wait")));
    assert!(methods.contains(&json!("input.drag")));

    call("session.create", json!({"framework": "swiftui", "appPath": g, "load": true}));
    assert_eq!(call("runtime.tick", json!({"sessionId": "s1", "dtMs": -1}))["error"]["data"]["code"], "bad_request");
    let t1 = call("runtime.tick", json!({"sessionId": "s1", "dtMs": 16}));
    assert_eq!(t1["result"]["clockMs"], 16.0);
    let t2 = call("runtime.wait", json!({"sessionId": "s1", "ms": 84}));
    assert_eq!(t2["result"]["clockMs"], 100.0);
    let d1 = call("input.drag", json!({"sessionId": "s1", "phase": "begin", "x": 200, "y": 300}));
    assert!(d1["result"].is_object());
    call("input.drag", json!({"sessionId": "s1", "phase": "end", "x": 200, "y": 300}));
    call("session.destroy", json!({"sessionId": "s1"}));
}
