use simcore_host::{Host, State};
use simcore_protocol::*;

const GUEST: &str = env!("CARGO_BIN_EXE_simcore-fake-guest");

fn params(path: &str) -> CreateSessionParams {
    serde_json::from_value(serde_json::json!({"framework": "swiftui", "app_path": path})).unwrap()
}

fn find<'a>(n: &'a UiNode, pred: &dyn Fn(&UiNode) -> bool) -> Option<&'a UiNode> {
    if pred(n) {
        return Some(n);
    }
    n.children.iter().find_map(|c| find(c, pred))
}

#[test]
fn counter_flow() {
    let mut host = Host::new();
    let id = host.create_session(params(GUEST)).unwrap();
    assert_eq!(host.state(&id).unwrap(), State::Created);
    host.start_session(&id).unwrap();
    assert_eq!(host.state(&id).unwrap(), State::Running);

    let (w, h, png) = host.screenshot(&id).unwrap();
    assert_eq!((w, h), (786, 1704));
    assert_eq!(&png[1..4], b"PNG");

    let (v1, root) = host.inspect(&id).unwrap();
    assert!(find(&root, &|n| n.text.as_deref() == Some("Count: 0")).is_some());
    let button = find(&root, &|n| n.role.as_deref() == Some("button")).unwrap();
    assert_eq!(button.label.as_deref(), Some("Increment"));
    let f = button.frame;

    let (hit, acted) = host
        .pointer(&id, PointerEvent { kind: PointerKind::Click, x: f.x + f.width / 2.0, y: f.y + f.height / 2.0 })
        .unwrap();
    assert_eq!(hit.as_deref(), Some("0.1"));
    assert!(acted);
    let (v2, root) = host.inspect(&id).unwrap();
    assert!(v2 > v1);
    assert!(find(&root, &|n| n.text.as_deref() == Some("Count: 1")).is_some());

    // click on empty space: no hit
    assert_eq!(host.pointer(&id, PointerEvent { kind: PointerKind::Click, x: 1.0, y: 1.0 }).unwrap(), (None, false));

    host.set_appearance(&id, Appearance::Dark).unwrap();
    assert!(host.inspect(&id).unwrap().0 > v2);
    assert!(host.errors(&id).unwrap().iter().any(|e| e.source == "stderr" && e.message.contains("fake guest started")));

    host.stop_session(&id).unwrap();
    assert_eq!(host.state(&id).unwrap(), State::Stopped);
    assert_eq!(host.pointer(&id, PointerEvent { kind: PointerKind::Click, x: 1.0, y: 1.0 }).unwrap_err().code, ErrorCode::InvalidState);
    assert_eq!(host.start_session("nope").unwrap_err().code, ErrorCode::NotFound);
}

#[test]
fn crash_is_isolated() {
    let dir = std::env::temp_dir().join(format!("simcore-crash-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let crasher = dir.join("fake-crash-guest");
    let _ = std::fs::remove_file(&crasher);
    std::os::unix::fs::symlink(GUEST, &crasher).unwrap();

    let mut host = Host::new();
    let bad = host.create_session(params(crasher.to_str().unwrap())).unwrap();
    let good = host.create_session(params(GUEST)).unwrap();
    host.start_session(&bad).unwrap();
    host.start_session(&good).unwrap();

    // Any action makes the crash guest exit(3).
    let f = host.inspect(&bad).unwrap().1.children[0].children[1].frame;
    let click = PointerEvent { kind: PointerKind::Click, x: f.x + 5.0, y: f.y + 5.0 };
    let _ = host.pointer(&bad, click);
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert_eq!(host.state(&bad).unwrap(), State::Crashed);
    assert_eq!(host.pointer(&bad, click).unwrap_err().code, ErrorCode::InvalidState);

    assert_eq!(host.state(&good).unwrap(), State::Running);
    assert!(host.pointer(&good, click).unwrap().1);
    host.shutdown();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn json_api_basics() {
    let mut host = Host::new();
    let mut out = Vec::new();
    let input = format!(
        r#"{{"id":1,"method":"get_capabilities"}}
{{"id":2,"method":"hello","params":{{"protocol_version":2}}}}
{{"id":3,"method":"hello","params":{{"protocol_version":1}}}}
{{"id":4,"method":"create_session","params":{{"framework":"swiftui","app_path":"{GUEST}"}}}}
{{"id":5,"method":"send_keyboard_event","params":{{"session_id":"s1","event":{{}}}}}}
{{"id":6,"method":"shutdown"}}
"#
    );
    host.serve(input.as_bytes(), &mut out).unwrap();
    let lines: Vec<serde_json::Value> = String::from_utf8(out).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(lines[0]["error"]["code"], "bad_request"); // hello required first
    assert_eq!(lines[1]["error"]["code"], "protocol_version");
    assert_eq!(lines[2]["result"]["protocol_version"], 1);
    assert_eq!(lines[3]["result"]["session_id"], "s1");
    assert_eq!(lines[4]["error"]["code"], "unsupported");
    assert_eq!(lines.len(), 6);
}
