//! Selection state, range extension and actions for Android Compose text.

use crate::Rect;
use std::ops::Range;

type SceneTextPosition = (usize, usize);
const HANDLE_DRAG_Y_OFFSET: f64 = 18.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelectionControl {
    Start,
    End,
    Copy,
    SelectAll,
    ReadAloud,
}

/// An action initiated on the selection toolbar or by the copy shortcut.
#[derive(Debug, PartialEq, Eq)]
pub enum SelectionOutput {
    Copy(String),
    ReadAloud(String),
}

#[derive(Debug, Clone)]
struct TextPoint {
    id: String,
    text: String,
    byte: usize,
}

impl TextPoint {
    fn at(node: &crate::Node, x: f64, y: f64) -> Option<Self> {
        let text = &node.text.as_ref()?.text;
        let layout = node.compose_text.as_ref()?;
        layout.selection_color?;
        let byte = layout.offset_at(text, x - node.bounds.x, y - node.bounds.y);
        Self::from_byte(node, byte)
    }

    fn from_byte(node: &crate::Node, byte: usize) -> Option<Self> {
        Some(Self {
            id: node.id.clone(),
            text: node.text.as_ref()?.text.clone(),
            byte,
        })
    }

    fn locate(&self, scene: &crate::Scene) -> Option<usize> {
        scene.nodes.iter().position(|node| {
            node.id == self.id
                && node.text.as_ref().is_some_and(|run| run.text == self.text)
                && node
                    .compose_text
                    .as_ref()
                    .is_some_and(|layout| layout.selection_color.is_some())
        })
    }
}

#[derive(Debug, Default)]
pub(crate) struct TextSelection {
    range: Option<(TextPoint, TextPoint)>,
    drag: Option<(TextPoint, TextPoint)>,
    pressed: Option<TextPress>,
    handle: Option<SelectionControl>,
    toolbar_pressed: Option<SelectionControl>,
    touch: bool,
    magnifier: Option<(f64, f64)>,
    output: Option<SelectionOutput>,
}

#[derive(Debug)]
struct TextPress {
    id: String,
    x: f64,
    y: f64,
    start_ms: f64,
    elapsed: f64,
    selected: bool,
}

impl TextSelection {
    pub fn down(&mut self, scene: &crate::Scene, x: f64, y: f64, time: f64) {
        self.pressed = scene.selectable_text_at(x, y).map(|i| TextPress {
            id: scene.nodes[i].id.clone(),
            x,
            y,
            start_ms: time,
            elapsed: 0.0,
            selected: false,
        });
    }

    pub fn release(&mut self, scene: &crate::Scene, x: f64, y: f64, time: f64) -> Option<String> {
        self.magnifier = None;
        let press = self.pressed.take()?;
        if press.selected {
            return Some(press.id);
        }
        if time - press.start_ms >= 500.0
            && scene
                .selectable_text_at(x, y)
                .is_some_and(|i| scene.nodes[i].id == press.id)
        {
            return self.word(scene, press.x, press.y);
        }
        None
    }

    pub fn waiting(&self) -> bool {
        self.pressed.as_ref().is_some_and(|press| !press.selected)
    }

    pub fn tick(&mut self, scene: &crate::Scene, dt: f64) -> bool {
        let Some(press) = self.pressed.as_mut().filter(|p| !p.selected) else {
            return false;
        };
        press.elapsed += dt;
        if press.elapsed < 0.5 {
            return false;
        }
        let (x, y) = (press.x, press.y);
        if !scene
            .selectable_text_at(x, y)
            .is_some_and(|i| scene.nodes[i].id == press.id)
        {
            self.pressed = None;
            return false;
        }
        let selected = self.word(scene, x, y).is_some();
        if selected {
            self.magnifier = Some((x, y));
            self.pressed
                .as_mut()
                .expect("text press is still active")
                .selected = true;
        } else {
            self.pressed = None;
        }
        selected
    }

    pub fn cancel_press(&mut self) {
        self.pressed = None;
        self.magnifier = None;
    }

    pub fn clear(&mut self) -> bool {
        self.cancel_press();
        self.range.take().is_some()
    }

    pub fn word(&mut self, scene: &crate::Scene, x: f64, y: f64) -> Option<String> {
        let i = scene.selectable_text_at(x, y)?;
        let mut point = TextPoint::at(&scene.nodes[i], x, y)?;
        let byte = point.byte.min(point.text.len().saturating_sub(1));
        let (start, finish) = {
            let mut boundaries = crate::line_break::word_boundaries(&point.text);
            let mut start = boundaries.next()?;
            let finish = boundaries.find(|&end| {
                if end > byte { true } else { start = end; false }
            })?;
            (start, finish)
        };
        point.byte = start;
        let end = TextPoint { byte: finish, ..point.clone() };
        let id = point.id.clone();
        self.range = Some((point, end));
        self.touch = true;
        Some(id)
    }

    pub fn begin_drag(&mut self, scene: &crate::Scene, x: f64, y: f64) -> Option<String> {
        self.drag = None;
        self.handle = None;
        self.toolbar_pressed = None;
        let press = self.pressed.take();
        if let Some(control @ (SelectionControl::Start | SelectionControl::End)) =
            Self::control_at(scene, x, y)
        {
            let (start, end) = self.range.clone()?;
            let id = start.id.clone();
            self.drag = Some((start, end));
            self.handle = Some(control);
            self.magnifier = Some((x, y - HANDLE_DRAG_Y_OFFSET));
            return Some(id);
        }
        let i = scene.selectable_text_at(x, y)?;
        let point = TextPoint::at(&scene.nodes[i], x, y)?;
        let id = point.id.clone();
        let word = press.is_some_and(|p| p.selected && p.id == id);
        self.touch = word;
        self.drag = Some(if word {
            self.range.clone().unwrap_or_else(|| (point.clone(), point))
        } else {
            self.range = None;
            (point.clone(), point)
        });
        Some(id)
    }

    pub fn drag(&mut self, scene: &crate::Scene, x: f64, y: f64, end: bool) -> Option<String> {
        let (first, last) = if end {
            self.drag.take()?
        } else {
            self.drag.as_ref()?.clone()
        };
        let y = if self.handle.is_some() {
            y - HANDLE_DRAG_Y_OFFSET
        } else {
            y
        };
        self.magnifier = (self.touch && !end).then_some((x, y));
        let handle = self.handle;
        if end {
            self.handle = None;
        }
        let id = first.id.clone();
        if let Some((start, group)) = first
            .locate(scene)
            .and_then(|i| Some((i, scene.selection_group(i)?)))
        {
            let distance = |b: Rect| {
                (x - x.clamp(b.x, b.x + b.w)).powi(2) + (y - y.clamp(b.y, b.y + b.h)).powi(2)
            };
            let target = scene
                .nodes
                .iter()
                .enumerate()
                .filter(|(i, n)| {
                    n.compose_text
                        .as_ref()
                        .is_some_and(|t| t.selection_color.is_some())
                        && scene.selection_group(*i) == Some(group)
                })
                .min_by(|(_, a), (_, b)| distance(a.bounds).total_cmp(&distance(b.bounds)));
            self.range = target
                .and_then(|(i, n)| Some((i, TextPoint::at(n, x, y)?)))
                .map(|(i, focus)| {
                    if handle == Some(SelectionControl::Start) {
                        (focus, last)
                    } else if handle == Some(SelectionControl::End) {
                        (first, focus)
                    } else if (i, focus.byte) < (start, first.byte) {
                        (focus, last)
                    } else if (i, focus.byte) > (start, last.byte) {
                        (first, focus)
                    } else {
                        (first, last)
                    }
                });
        }
        Some(id)
    }

    fn control_at(scene: &crate::Scene, x: f64, y: f64) -> Option<SelectionControl> {
        scene
            .nodes
            .iter()
            .rev()
            .find_map(|n| n.selection_control.filter(|_| n.bounds.contains(x, y)))
    }

    pub fn pointer_control(
        &mut self,
        scene: &crate::Scene,
        ev: onesim_protocol::PointerEvent,
    ) -> Option<String> {
        use onesim_protocol::PointerKind;
        let control = Self::control_at(scene, ev.x, ev.y);
        match ev.kind {
            PointerKind::Down => {
                self.toolbar_pressed = control;
                control?;
            }
            PointerKind::Up => {
                let pressed = self.toolbar_pressed.take()?;
                if control == Some(pressed) {
                    self.activate(scene, pressed);
                }
            }
            PointerKind::Click => self.activate(scene, control?),
            _ => return None,
        }
        Some(format!(
            "compose-selection-{}",
            control.map_or("toolbar", SelectionControl::suffix)
        ))
    }

    pub fn take_output(&mut self) -> Option<SelectionOutput> {
        self.output.take()
    }

    pub fn shortcut(&mut self, scene: &crate::Scene, key: &str) -> bool {
        let control = match key {
            "copy" => SelectionControl::Copy,
            "select_all" => SelectionControl::SelectAll,
            _ => return false,
        };
        self.range.is_some() && self.activate_result(scene, control)
    }

    fn activate(&mut self, scene: &crate::Scene, control: SelectionControl) {
        self.activate_result(scene, control);
    }

    fn activate_result(&mut self, scene: &crate::Scene, control: SelectionControl) -> bool {
        let Some((start, end, group)) = self.bounds(scene) else {
            return false;
        };
        match control {
            SelectionControl::Start | SelectionControl::End => false,
            SelectionControl::SelectAll => {
                let mut nodes = scene
                    .nodes
                    .iter()
                    .enumerate()
                    .filter(|(i, n)| {
                        n.compose_text
                            .as_ref()
                            .is_some_and(|t| t.selection_color.is_some())
                            && scene.selection_group(*i) == Some(group)
                    })
                    .map(|(_, n)| n);
                let Some(first) = nodes.next() else {
                    return false;
                };
                let last = nodes.next_back().unwrap_or(first);
                self.range = TextPoint::from_byte(first, 0).zip(TextPoint::from_byte(
                    last,
                    last.text.as_ref().map_or(0, |t| t.text.len()),
                ));
                true
            }
            SelectionControl::Copy | SelectionControl::ReadAloud => {
                let text = (start.0..=end.0)
                    .filter_map(|i| {
                        let (node, range) = Self::selected_range(scene, start, end, group, i)?;
                        let text = &node.text.as_ref()?.text;
                        if control == SelectionControl::ReadAloud {
                            Some(node.compose_text.as_ref()?.speak(text, range))
                        } else {
                            node.compose_text.as_ref()?.selected_string(text, range)
                        }
                    })
                    .filter(|text| !text.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n");
                if text.is_empty() {
                    return false;
                }
                self.output = Some(if control == SelectionControl::Copy {
                    SelectionOutput::Copy(text)
                } else {
                    SelectionOutput::ReadAloud(text)
                });
                self.clear();
                true
            }
        }
    }

    fn bounds(
        &self,
        scene: &crate::Scene,
    ) -> Option<(SceneTextPosition, SceneTextPosition, usize)> {
        let (anchor, focus) = self.range.as_ref()?;
        let a = (anchor.locate(scene)?, anchor.byte);
        let b = (focus.locate(scene)?, focus.byte);
        let group = scene.selection_group(a.0)?;
        (scene.selection_group(b.0) == Some(group)).then_some((a.min(b), a.max(b), group))
    }

    fn selected_range(
        scene: &crate::Scene,
        start: (usize, usize),
        end: (usize, usize),
        group: usize,
        i: usize,
    ) -> Option<(&crate::Node, Range<usize>)> {
        let node = scene.nodes.get(i)?;
        node.compose_text.as_ref()?.selection_color?;
        let len = node.text.as_ref()?.text.len();
        (scene.selection_group(i) == Some(group)).then(|| {
            let lo = if i == start.0 { start.1 } else { 0 };
            let hi = if i == end.0 { end.1 } else { len };
            (node, lo..hi)
        })
    }

    pub fn apply(&self, scene: &mut crate::Scene) {
        let Some((start, end, group)) = self.bounds(scene) else {
            return;
        };
        for i in start.0..=end.0 {
            if let Some((_, range)) = Self::selected_range(scene, start, end, group, i) {
                let n = &mut scene.nodes[i];
                let layout = n.compose_text.as_mut().expect("selectable text layout");
                layout.highlight = layout.rects(range, n.bounds);
            }
        }
        if self.touch && start != end {
            super::selection_chrome::selection(
                scene,
                start,
                end,
                self.magnifier,
                self.drag.is_some(),
            );
        }
    }
}

impl SelectionControl {
    pub fn suffix(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::End => "end",
            Self::Copy => "copy",
            Self::SelectAll => "select-all",
            Self::ReadAloud => "read-aloud",
        }
    }
}
