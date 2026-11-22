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

fn run_motion() {
    struct Motion {
        spring_big: bool,
        spring_tok: u64,
        ease_dim: bool,
        ease_tok: u64,
        pulse_on: bool,
        pulse_tok: u64,
        show_card: bool,
        card_tok: u64,
        tab: u8,
        tab_tok: u64,
        phase: u32,
        phase_running: bool,
        drag_offset: (f64, f64),
        drag_live: (f64, f64),
        drag_start: Option<(f64, f64)>,
        sheet_open: bool,
        underlying_taps: u32,
        sheet_taps: u32,
    }
    let mut m = Motion {
        spring_big: false,
        spring_tok: 0,
        ease_dim: false,
        ease_tok: 0,
        pulse_on: false,
        pulse_tok: 0,
        show_card: false,
        card_tok: 0,
        tab: 0,
        tab_tok: 0,
        phase: 0,
        phase_running: false,
        drag_offset: (0.0, 0.0),
        drag_live: (0.0, 0.0),
        drag_start: None,
        sheet_open: false,
        underlying_taps: 0,
        sheet_taps: 0,
    };
    let build = |m: &Motion| {
        let spring_w = if m.spring_big { 220.0 } else { 60.0 };
        let ease_op = if m.ease_dim { 0.2 } else { 1.0 };
        let ease_w = if m.ease_dim { 180.0 } else { 80.0 };
        let pulse_w = if m.pulse_on { 160.0 } else { 60.0 };
        let btn = |id: &str, action: u64, label: &str| {
            json!({"kind": "button", "id": id, "action": action, "label": {
                "kind": "text", "id": format!("{id}.l"), "text": label,
                "modifiers": [{"padding": {"top": 6.0, "leading": 10.0, "bottom": 6.0, "trailing": 10.0}}]
            }})
        };
        let mut children = vec![
            json!({"kind": "hstack", "id": "m.bar1", "spacing": 6.0, "children": [
                btn("m.b.spring", 1, "Spring"),
                btn("m.b.ease", 2, "Ease"),
                btn("m.b.pulse", 3, "Pulse"),
                btn("m.b.card", 4, "Card"),
            ]}),
            json!({"kind": "hstack", "id": "m.bar2", "spacing": 6.0, "children": [
                btn("m.b.tab0", 5, "TabA"),
                btn("m.b.tab1", 6, "TabB"),
                btn("m.b.phase", 7, "Phase"),
                btn("m.b.sheet", 8, "Sheet"),
            ]}),
            // Underdamped spring box
            json!({"kind": "rectangle", "id": "m.spring", "modifiers": [
                {"frame": {"width": spring_w, "height": 28.0}},
                {"animation": {"curve": {"spring": {"mass": 1.0, "stiffness": 200.0, "damping": 12.0, "settling": 0.6}}, "duration": 0.6, "token": m.spring_tok}}
            ]}),
            // EaseInOut box (frame + opacity)
            json!({"kind": "rectangle", "id": "m.ease", "modifiers": [
                {"frame": {"width": ease_w, "height": 28.0}},
                {"opacity": ease_op},
                {"animation": {"curve": "easeInOut", "duration": 0.4, "token": m.ease_tok}}
            ]}),
            // repeatForever box
            json!({"kind": "rectangle", "id": "m.pulse", "modifiers": [
                {"frame": {"width": pulse_w, "height": 24.0}},
                {"animation": {"curve": "linear", "duration": 0.2, "repeat": "forever", "autoreverses": true, "token": m.pulse_tok}}
            ]}),
            // matchedGeometryEffect tabs
            json!({"kind": "hstack", "id": "m.tabs", "spacing": 24.0, "children": [
                json!({"kind": "zstack", "id": "m.t0", "children": if m.tab == 0 {
                    vec![
                        json!({"kind": "rectangle", "id": "m.pill.0", "modifiers": [
                            {"frame": {"width": 70.0, "height": 28.0}},
                            {"matchedGeometry": {"id": "pill", "namespace": "ns", "properties": "frame", "isSource": true}},
                            {"animation": {"curve": "linear", "duration": 0.3, "token": m.tab_tok}}
                        ]}),
                        t("m.t0.l", "A")
                    ]
                } else {
                    vec![json!({"kind": "text", "id": "m.t0.l", "text": "A", "modifiers": [{"frame": {"width": 70.0, "height": 28.0}}]})]
                }}),
                json!({"kind": "zstack", "id": "m.t1", "children": if m.tab == 1 {
                    vec![
                        json!({"kind": "rectangle", "id": "m.pill.1", "modifiers": [
                            {"frame": {"width": 110.0, "height": 28.0}},
                            {"matchedGeometry": {"id": "pill", "namespace": "ns", "properties": "frame", "isSource": true}},
                            {"animation": {"curve": "linear", "duration": 0.3, "token": m.tab_tok}}
                        ]}),
                        t("m.t1.l", "B")
                    ]
                } else {
                    vec![json!({"kind": "text", "id": "m.t1.l", "text": "B", "modifiers": [{"frame": {"width": 110.0, "height": 28.0}}]})]
                }}),
            ]}),
            // PhaseAnimator status
            t("m.phase", &format!("Phase: {}", m.phase)),
            // Draggable orb with @GestureState-like live tracking
            json!({"kind": "rectangle", "id": "m.orb", "modifiers": [
                {"frame": {"width": 80.0, "height": 40.0}},
                {"offset": {"x": m.drag_offset.0 + m.drag_live.0, "y": m.drag_offset.1 + m.drag_live.1}},
                {"gesture": {"key": "m.orb#g0", "pans": true}}
            ]}),
            t("m.orb.pos", &format!("Orb: {},{}", (m.drag_offset.0 + m.drag_live.0).round() as i64, (m.drag_offset.1 + m.drag_live.1).round() as i64)),
            // Underlying counter button (to verify sheet scrim blocks hit-testing)
            btn("m.b.under", 9, &format!("Under: {}", m.underlying_taps)),
        ];
        if m.show_card {
            children.push(json!({"kind": "button", "id": "m.card", "action": 10,
                "label": {"kind": "text", "id": "m.card.l", "text": "Transition Card",
                    "modifiers": [{"frame": {"width": 200.0, "height": 40.0}}]},
                "modifiers": [
                    {"transition": {
                        "insertion": {"opacity": 0.0, "offset": [0.0, 60.0]},
                        "removal": {"opacity": 0.0, "scale": 0.5}
                    }},
                    {"animation": {"curve": "linear", "duration": 0.3, "token": m.card_tok}}
                ]
            }));
        }
        // Scrollable list at the bottom for fling/momentum testing
        children.push(json!({"kind": "scroll", "id": "m.scroll", "axis": "vertical",
            "modifiers": [{"frame": {"height": 220.0}}],
            "child": {"kind": "vstack", "id": "m.sv", "alignment": "leading", "spacing": 10.0,
                "children": (0..30).map(|i| t(&format!("m.sv.{i}"), &format!("Row {i}"))).collect::<Vec<_>>()}}));
        let mut root_mods = vec![json!({"padding": {"top": 60.0, "leading": 12.0, "bottom": 12.0, "trailing": 12.0}})];
        if m.card_tok > 0 {
            root_mods.push(json!({"animation": {"curve": "linear", "duration": 0.3, "token": m.card_tok}}));
        }
        let base = json!({"kind": "vstack", "id": "m.root", "spacing": 8.0, "alignment": "center",
            "modifiers": root_mods,
            "children": children});
        if m.sheet_open {
            json!({"kind": "presentation", "id": "m.pres", "content": base, "layers": [{
                "kind": "sheet", "detents": [{"kind": "medium"}, {"kind": "large"}], "dragIndicator": true, "dismiss": 20,
                "content": {"kind": "vstack", "id": "m.sh", "spacing": 16.0, "alignment": "center",
                    "modifiers": [{"padding": {"top": 40.0, "leading": 16.0, "bottom": 16.0, "trailing": 16.0}}],
                    "children": [
                        t("m.sh.t", &format!("SheetTaps: {}", m.sheet_taps)),
                        btn("m.sh.inc", 21, "SheetTap"),
                        btn("m.sh.done", 20, "Done"),
                    ]}
            }]})
        } else {
            base
        }
    };
    let mut version = 1u64;
    send(json!({"type": "hello", "protocol": 1}));
    send(json!({"type": "tree", "version": version, "root": build(&m)}));
    for line in std::io::stdin().lock().lines() {
        let Ok(msg) = serde_json::from_str::<serde_json::Value>(&line.unwrap_or_default()) else { continue };
        let mut sched: Option<Option<f64>> = None;
        match msg["type"].as_str() {
            Some("action") if msg["version"] == version => {
                match msg["id"].as_u64().unwrap_or(0) {
                    1 => { m.spring_big = !m.spring_big; m.spring_tok += 1; }
                    2 => { m.ease_dim = !m.ease_dim; m.ease_tok += 1; }
                    3 => { m.pulse_on = !m.pulse_on; m.pulse_tok += 1; }
                    4 => { m.show_card = !m.show_card; m.card_tok += 1; }
                    5 => { m.tab = 0; m.tab_tok += 1; }
                    6 => { m.tab = 1; m.tab_tok += 1; }
                    7 => {
                        m.phase = 1;
                        m.phase_running = true;
                        sched = Some(Some(100.0));
                    }
                    8 => m.sheet_open = true,
                    9 => m.underlying_taps += 1,
                    20 => m.sheet_open = false,
                    21 => m.sheet_taps += 1,
                    _ => continue,
                }
            }
            Some("gesture") => {
                let hits_orb = msg["targets"].as_array().is_some_and(|a| a.iter().any(|t| t["key"] == "m.orb#g0"));
                if !hits_orb && m.drag_start.is_none() {
                    continue;
                }
                let pt = msg["points"].as_array().and_then(|a| a.first()).and_then(|p| p.as_array()).map(|p| {
                    (p.first().and_then(|v| v.as_f64()).unwrap_or(0.0), p.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0))
                });
                match (msg["phase"].as_str(), pt) {
                    (Some("start"), Some((x, y))) => {
                        m.drag_start = Some((x, y));
                        m.drag_live = (0.0, 0.0);
                    }
                    (Some("move"), Some((x, y))) => {
                        if let Some((sx, sy)) = m.drag_start {
                            m.drag_live = (x - sx, y - sy);
                        }
                    }
                    (Some("end"), Some((x, y))) => {
                        if let Some((sx, sy)) = m.drag_start.take() {
                            m.drag_offset.0 += x - sx;
                            m.drag_offset.1 += y - sy;
                        }
                        m.drag_live = (0.0, 0.0);
                    }
                    (Some("cancel"), _) => {
                        m.drag_start = None;
                        m.drag_live = (0.0, 0.0);
                    }
                    _ => continue,
                }
            }
            Some("tick") => {
                if m.phase_running {
                    m.phase += 1;
                    if m.phase < 3 {
                        sched = Some(Some(100.0));
                    } else {
                        m.phase_running = false;
                        sched = Some(None);
                    }
                } else {
                    send(json!({"type": "schedule"}));
                    continue;
                }
            }
            Some("shutdown") => return,
            _ => continue,
        }
        version += 1;
        send(json!({"type": "tree", "version": version, "root": build(&m)}));
        if let Some(after) = sched {
            match after {
                Some(ms) => send(json!({"type": "schedule", "afterMs": ms})),
                None => send(json!({"type": "schedule"})),
            }
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
    if std::env::args().next().is_some_and(|a| a.contains("motion")) {
        return run_motion();
    }
    if std::env::args().next().is_some_and(|a| a.contains("anim")) {
        return run_anim();
    }
    let (mut count, mut version) = (0u32, 1u64);
    let mut lifecycle: Vec<String> = vec![];
    send(json!({"type": "hello", "protocol": 1}));
    send(json!({"type": "tree", "version": version, "root": tree(count, &lifecycle)}));
    send_boot_extras();
    // Pasteboard (section W): echo what the host sets, and what it seeded at launch, as app copies.
    if let Ok(pb) = std::env::var("SIMCORE_PASTEBOARD") {
        send(json!({"type": "pasteboard", "items": [format!("launch:{pb}")]}));
    }
    for line in std::io::stdin().lock().lines() {
        let Ok(msg) = serde_json::from_str::<serde_json::Value>(&line.unwrap_or_default()) else { continue };
        match msg["type"].as_str() {
            Some("pasteboard") => send(json!({"type": "pasteboard", "items": msg["items"]})),
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
