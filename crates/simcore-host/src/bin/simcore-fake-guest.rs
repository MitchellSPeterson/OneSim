//! Fake guest speaking the guest wire (docs/protocol.md B) with a counter tree.
//! Test double for host integration tests; needs no Swift toolchain.
//! If its executable name contains "crash" it aborts on the first action.
//! If it contains "nav" it serves a small navigation + list app (home, todos, controls, long
//! scroll page) that handles `action` and `text_input`.

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
    if std::env::args().next().is_some_and(|a| a.contains("nav")) {
        return run_nav();
    }
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
