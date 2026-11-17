//! Fake guest speaking the guest wire (docs/protocol.md B) with a counter tree.
//! Test double for host integration tests; needs no Swift toolchain.
//! If its executable name contains "crash" it aborts on the first action.

use serde_json::json;
use std::io::{BufRead, Write};

fn tree(count: u32) -> serde_json::Value {
    json!({
        "kind": "vstack", "id": "0", "spacing": 16.0, "alignment": "center",
        "children": [
            {"kind": "text", "id": "0.0", "text": format!("Count: {count}")},
            {"kind": "button", "id": "0.1", "action": 1, "label": {
                "kind": "text", "id": "0.1.l", "text": "Increment",
                "modifiers": [
                    {"padding": {"top": 12.0, "leading": 24.0, "bottom": 12.0, "trailing": 24.0}},
                    {"background": {"kind": "color", "id": "0.1.bg", "color": {"semantic": "accent"}}}]}}
        ]
    })
}

fn send(v: serde_json::Value) {
    let mut o = std::io::stdout().lock();
    writeln!(o, "{v}").unwrap();
    o.flush().unwrap();
}

fn main() {
    let crash = std::env::args().next().is_some_and(|a| a.contains("crash"));
    eprintln!(
        "fake guest started session={} viewport={} appearance={}",
        std::env::var("SIMCORE_SESSION_ID").unwrap_or_default(),
        std::env::var("SIMCORE_VIEWPORT").unwrap_or_default(),
        std::env::var("SIMCORE_APPEARANCE").unwrap_or_default(),
    );
    let (mut count, mut version) = (0u32, 1u64);
    send(json!({"type": "hello", "protocol": 1}));
    send(json!({"type": "tree", "version": version, "root": tree(count)}));
    for line in std::io::stdin().lock().lines() {
        let Ok(msg) = serde_json::from_str::<serde_json::Value>(&line.unwrap_or_default()) else { continue };
        match msg["type"].as_str() {
            Some("action") => {
                if crash {
                    eprintln!("fake guest crashing");
                    std::process::exit(3);
                }
                if msg["version"] == version && msg["id"] == 1 {
                    count += 1;
                    version += 1;
                    send(json!({"type": "tree", "version": version, "root": tree(count)}));
                }
            }
            Some("environment") => {
                version += 1;
                send(json!({"type": "tree", "version": version, "root": tree(count)}));
            }
            Some("shutdown") => return,
            _ => {}
        }
    }
}
