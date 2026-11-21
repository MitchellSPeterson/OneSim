//! Fake guest speaking the guest wire (docs/protocol.md B) with a counter tree.
//! Test double for host integration tests; needs no Swift toolchain.
//! If its executable name contains "crash" it aborts on the first action.
//! If it contains "nav" it serves a small navigation + list app (home, todos, controls, long
//! scroll page) that handles `action` and `text_input`.
//! If it contains "rn" it serves a React Native tree (tag 5 is a pressable counting touch ends,
//! tag 8 a text input; tag 10 logs its focus/blur/text/submit events).

use serde_json::json;
use std::io::{BufRead, Write};

/// The counter screen and a "Hold" view with a gesture; with any `scene_phase`, `open_url` or `gesture` seen, a
/// "Lifecycle: ..." line logs them in order.
fn tree(count: u32, lifecycle: &[String]) -> serde_json::Value {
    let mut t = counter_tree(count);
    let hold = json!({"kind": "text", "id": "0.3", "text": "Hold", "modifiers": [{"gesture": {"key": "0.3#g0", "pans": false}}]});
    t["children"].as_array_mut().unwrap().push(hold);
    if !lifecycle.is_empty() {
        let line = json!({"kind": "text", "id": "0.2", "text": format!("Lifecycle: {}", lifecycle.join(","))});
        t["children"].as_array_mut().unwrap().push(line);
    }
    t
}

fn counter_tree(count: u32) -> serde_json::Value {
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

#[derive(Clone, Copy, PartialEq)]
enum Screen {
    Home,
    Todos,
    Controls,
    Long,
}

struct App {
    screen: Screen,
    draft: String,
    items: Vec<(String, bool)>,
    notifications: bool,
    volume: i64,
}

fn t(id: &str, s: &str) -> serde_json::Value {
    json!({"kind": "text", "id": id, "text": s})
}

impl App {
    fn new() -> Self {
        let items = [("Write the PRD", true), ("Build the renderer", true), ("Ship the SwiftUI subset", false)];
        Self {
            screen: Screen::Home,
            draft: String::new(),
            items: items.iter().map(|(a, b)| (a.to_string(), *b)).collect(),
            notifications: true,
            volume: 3,
        }
    }

    fn open(&self) -> usize {
        self.items.iter().filter(|i| !i.1).count()
    }

    fn tree(&self) -> serde_json::Value {
        let link = |i: u64, name: &str| json!({"kind": "button", "id": format!("l.{i}"), "action": i, "style": "navigationLink", "label": t(&format!("l.{i}.t"), name)});
        let back = json!({"action": 100, "label": "Showcase"});
        match self.screen {
            Screen::Home => json!({"kind": "navigation", "id": "n", "title": "Showcase", "displayMode": "large",
                "content": {"kind": "list", "id": "l", "style": "insetGrouped", "sections": [
                    {"header": "Screens", "rows": [link(1, "Typography"), link(2, "Shapes"), link(3, "Todos"), link(4, "Controls")]},
                    {"header": "Summary", "rows": [{"kind": "hstack", "id": "l.s", "children": [
                        t("l.s.0", "Open todos"), {"kind": "spacer", "id": "l.s.1"},
                        {"kind": "text", "id": "l.s.2", "text": self.open().to_string(), "modifiers": [{"foregroundColor": {"semantic": "secondary"}}]}]}]}]}}),
            Screen::Todos => {
                let rows: Vec<_> = self.items.iter().enumerate().map(|(i, (title, done))| json!({
                    "kind": "button", "id": format!("l.i{i}"), "action": 10 + i as u64, "style": "automatic",
                    "label": {"kind": "text", "id": format!("l.i{i}.t"), "text": title, "modifiers": [{"strikethrough": done}]}})).collect();
                json!({"kind": "navigation", "id": "n", "title": "Todos", "displayMode": "large", "back": back,
                    "content": {"kind": "list", "id": "l", "style": "insetGrouped", "sections": [
                        {"rows": [{"kind": "hstack", "id": "l.a", "children": [
                            {"kind": "text_field", "id": "l.a.f", "placeholder": "New todo", "text": self.draft, "field": 1},
                            {"kind": "button", "id": "l.a.b", "action": 50, "disabled": self.draft.is_empty(), "label": t("l.a.bl", "Add")}]}]},
                        {"header": "Items", "rows": rows}]}})
            }
            Screen::Controls => json!({"kind": "navigation", "id": "n", "title": "Controls", "displayMode": "large", "back": back,
                "content": {"kind": "list", "id": "l", "style": "insetGrouped", "sections": [{"rows": [
                    {"kind": "toggle", "id": "l.0", "isOn": self.notifications, "action": 20, "label": t("l.0.l", "Notifications")},
                    {"kind": "stepper", "id": "l.1", "label": t("l.1.l", &format!("Volume: {}", self.volume)),
                        "decrement": (self.volume > 0).then_some(21), "increment": (self.volume < 10).then_some(22)}]}]}}),
            Screen::Long => json!({"kind": "navigation", "id": "n", "title": "Typography", "displayMode": "inline", "back": back,
                "content": {"kind": "scroll", "id": "sc", "axis": "vertical", "child": {"kind": "vstack", "id": "v", "alignment": "leading", "spacing": 12,
                    "modifiers": [{"padding": {"top": 16.0, "leading": 16.0, "bottom": 16.0, "trailing": 16.0}}],
                    "children": (0..40).map(|i| t(&format!("v.{i}"), &format!("Line {i}"))).collect::<Vec<_>>()}}}),
        }
    }

    fn action(&mut self, id: u64) {
        match id {
            1 | 2 => self.screen = Screen::Long,
            3 => self.screen = Screen::Todos,
            4 => self.screen = Screen::Controls,
            100 => self.screen = Screen::Home,
            50 if !self.draft.is_empty() => {
                self.items.push((std::mem::take(&mut self.draft), false));
            }
            10..=19 => {
                if let Some(i) = self.items.get_mut((id - 10) as usize) {
                    i.1 = !i.1;
                }
            }
            20 => self.notifications = !self.notifications,
            21 => self.volume -= 1,
            22 => self.volume += 1,
            _ => {}
        }
    }
}

fn run_nav() {
    let mut app = App::new();
    let mut version = 1u64;
    send(json!({"type": "hello", "protocol": 1}));
    send(json!({"type": "tree", "version": version, "root": app.tree()}));
    for line in std::io::stdin().lock().lines() {
        let Ok(msg) = serde_json::from_str::<serde_json::Value>(&line.unwrap_or_default()) else { continue };
        let current = msg["version"] == version;
        match msg["type"].as_str() {
            Some("action") if current => app.action(msg["id"].as_u64().unwrap_or(0)),
            Some("text_input") if current && msg["field"] == 1 => app.draft = msg["text"].as_str().unwrap_or("").to_string(),
            Some("environment") => {}
            Some("shutdown") => return,
            _ => continue,
        }
        version += 1;
        send(json!({"type": "tree", "version": version, "root": app.tree()}));
    }
}

fn rn_tree(count: u32, frames: usize, text: &str, log: &str, android: bool) -> serde_json::Value {
    let raw = |id: &str, t: &str| json!({"kind": "rn", "id": id, "component": "RCTRawText", "props": {"text": t}, "children": []});
    let mut props = json!({"placeholder": "Name", "__simcoreText": text, "fontSize": 17});
    let input = if android {
        props["multiline"] = json!(true);
        props["submitBehavior"] = json!("submit");
        "AndroidTextInput"
    } else {
        "RCTSinglelineTextInputView"
    };
    json!({"kind": "rn", "id": "1", "component": "RootView", "props": {}, "children": [
        {"kind": "rn", "id": "2", "component": "RCTView", "props": {"flex": 1, "alignItems": "center", "justifyContent": "center"}, "children": [
            {"kind": "rn", "id": "3", "component": "RCTText", "props": {}, "children": [raw("4", &format!("Count: {count} frames:{frames}"))]},
            {"kind": "rn", "id": "5", "component": "RCTView", "props": {"padding": 12, "borderRadius": 8, "backgroundColor": 0xFF0080FFu32}, "children": [
                {"kind": "rn", "id": "6", "component": "RCTText", "props": {"color": 0xFFFFFFFFu32}, "children": [raw("7", "Press")]}]},
            {"kind": "rn", "id": "8", "component": input, "props": props, "children": []},
            {"kind": "rn", "id": "10", "component": "RCTText", "props": {}, "children": [raw("11", &format!("Log:{log}"))]}]}]})
}

fn run_rn() {
    let android = std::env::var("SIMCORE_PLATFORM").as_deref() == Ok("android");
    let (mut count, mut frames, mut version, mut moves) = (0u32, 0usize, 1u64, 0u32);
    let (mut text, mut log) = (String::new(), String::new());
    send(json!({"type": "hello", "protocol": 1}));
    send(json!({"type": "tree", "version": version, "root": rn_tree(count, frames, &text, &log, android)}));
    send_boot_extras();
    for line in std::io::stdin().lock().lines() {
        let Ok(msg) = serde_json::from_str::<serde_json::Value>(&line.unwrap_or_default()) else { continue };
        match msg["type"].as_str() {
            Some("layout") => frames = msg["frames"].as_object().map_or(0, |f| f.len()),
            Some("focus") => log.push_str(" F"),
            Some("blur") => log.push_str(" B"),
            Some("submit") => log.push_str(" S"),
            Some("keyboard") => log.push_str(&format!(" K{}", msg["height"])),
            Some("text_input") if msg["field"] == 8 => {
                text = msg["text"].as_str().unwrap_or("").to_string();
                log.push_str(&format!(" T:{text}"));
            }
            Some("touch") if msg["phase"] == "move" && msg["tag"] == 5 => {
                moves += 1;
                continue;
            }
            Some("touch") if msg["phase"] == "end" && msg["tag"] == 5 => {
                count += 1;
                if moves > 0 {
                    log.push_str(&format!(" D{moves}:"));
                    moves = 0;
                }
            }
            Some("touch") if msg["phase"] == "end" => continue, // a tap that changes nothing
            Some("shutdown") => return,
            _ => continue,
        }
        version += 1;
        send(json!({"type": "tree", "version": version, "root": rn_tree(count, frames, &text, &log, android)}));
    }
}

/// A native-module inventory and a lifecycle event, like the React Native guest sends after boot.
fn send_boot_extras() {
    send(json!({"type": "modules", "modules": [{"name": "clipboard", "status": "simulated", "methods": ["getString"], "events": []}]}));
    send(json!({"type": "event", "name": "bundle.loaded", "detail": {"url": "http://fake"}}));
}

fn send(v: serde_json::Value) {
    let mut o = std::io::stdout().lock();
    writeln!(o, "{v}").unwrap();
    o.flush().unwrap();
}

fn anim_tree(expanded: bool, ticks: u32, clock_ms: u64) -> serde_json::Value {
    let w = if expanded { 200.0 } else { 40.0 };
    json!({
        "kind": "vstack", "id": "a.root", "spacing": 16.0, "alignment": "center",
        "children": [
            {"kind": "rectangle", "id": "a.box", "modifiers": [
                {"frame": {"width": w, "height": 40.0}},
                {"animation": {"curve": "linear", "duration": 0.4}}
            ]},
            {"kind": "text", "id": "a.ticks", "text": format!("Tick: {ticks}")},
            {"kind": "text", "id": "a.clock", "text": format!("Clock: {clock_ms}")},
            {"kind": "button", "id": "a.toggle", "action": 1, "label": {
                "kind": "text", "id": "a.toggle.l", "text": "Toggle",
                "modifiers": [{"padding": {"top": 10.0, "leading": 20.0, "bottom": 10.0, "trailing": 20.0}}]
            }}
        ]
    })
}

fn run_anim() {
    let (mut expanded, mut ticks, mut clock_ms, mut version) = (false, 0u32, 0u64, 1u64);
    send(json!({"type": "hello", "protocol": 1}));
    send(json!({"type": "tree", "version": version, "root": anim_tree(expanded, ticks, clock_ms)}));
    for line in std::io::stdin().lock().lines() {
        let Ok(msg) = serde_json::from_str::<serde_json::Value>(&line.unwrap_or_default()) else { continue };
        match msg["type"].as_str() {
            Some("action") if msg["version"] == version && msg["id"] == 1 => {
                expanded = !expanded;
                send(json!({"type": "schedule", "afterMs": 100.0}));
                version += 1;
                send(json!({"type": "tree", "version": version, "root": anim_tree(expanded, ticks, clock_ms)}));
            }
            Some("tick") => {
                ticks += 1;
                clock_ms = msg["time"].as_f64().unwrap_or(0.0).round() as u64;
                version += 1;
                send(json!({"type": "tree", "version": version, "root": anim_tree(expanded, ticks, clock_ms)}));
                send(json!({"type": "schedule"}));
            }
            Some("shutdown") => return,
            _ => {}
        }
    }
}

fn main() {
    let crash = std::env::args().next().is_some_and(|a| a.contains("crash"));
    eprintln!(
        "fake guest started session={} viewport={} appearance={}",
        std::env::var("SIMCORE_SESSION_ID").unwrap_or_default(),
        std::env::var("SIMCORE_VIEWPORT").unwrap_or_default(),
        std::env::var("SIMCORE_APPEARANCE").unwrap_or_default(),
    );
    if std::env::args().next().is_some_and(|a| a.contains("rn")) {
        return run_rn();
    }
    if std::env::args().next().is_some_and(|a| a.contains("nav")) {
        return run_nav();
    }
    if std::env::args().next().is_some_and(|a| a.contains("anim")) {
        return run_anim();
    }
    let (mut count, mut version) = (0u32, 1u64);
    let mut lifecycle: Vec<String> = vec![];
    send(json!({"type": "hello", "protocol": 1}));
    send(json!({"type": "tree", "version": version, "root": tree(count, &lifecycle)}));
    send_boot_extras();
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
                    send(json!({"type": "tree", "version": version, "root": tree(count, &lifecycle)}));
                }
            }
            Some("environment") => {
                version += 1;
                send(json!({"type": "tree", "version": version, "root": tree(count, &lifecycle)}));
            }
            Some("scene_phase") | Some("open_url") | Some("gesture") => {
                lifecycle.push(match msg["type"].as_str() {
                    Some("open_url") => format!("url:{}", msg["url"].as_str().unwrap_or("")),
                    Some("gesture") => format!("gesture:{}", msg["phase"].as_str().unwrap_or("")),
                    _ => msg["phase"].as_str().unwrap_or("").to_string(),
                });
                version += 1;
                send(json!({"type": "tree", "version": version, "root": tree(count, &lifecycle)}));
            }
            Some("shutdown") => return,
            _ => {}
        }
    }
}
