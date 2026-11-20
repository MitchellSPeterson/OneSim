//! Android touch-selection affordances painted above the Compose scene.

use crate::text_selection::SelectionControl;
use crate::scene::{VectorFill, VectorGeom, VectorShape};
use crate::{Color, FontSpec, Node, Rect, Role, Scene, Shadow, TextAlign, TextRun};
use onesim_protocol::Appearance;

pub fn selection(
    scene: &mut Scene,
    start: (usize, usize),
    end: (usize, usize),
    magnifier: Option<(f64, f64)>,
    dragging: bool,
) {
    let Some((first, start_color, start_rtl)) = endpoint(scene, start, false) else {
        return;
    };
    let Some((last, end_color, end_rtl)) = endpoint(scene, end, true) else {
        return;
    };
    handle(
        scene,
        first,
        start_color,
        SelectionControl::Start,
        start_rtl,
    );
    handle(scene, last, end_color, SelectionControl::End, end_rtl);
    if let Some(point) = magnifier {
        magnify(scene, start.0..=end.0, point);
    } else if !dragging {
        toolbar(scene, first, last);
    }
}

fn endpoint(scene: &Scene, (i, byte): (usize, usize), end: bool) -> Option<(Rect, Color, bool)> {
    let node = scene.nodes.get(i)?;
    let layout = node.compose_text.as_ref()?;
    let (r, rtl) = layout.selection_caret(byte, end).or_else(|| {
        let r = if end {
            layout.highlight.last()?
        } else {
            layout.highlight.first()?
        };
        Some((
            Rect {
                x: if end { r.x + r.w } else { r.x },
                w: 0.0,
                ..*r
            },
            false,
        ))
    })?;
    let r = Rect {
        x: r.x + node.bounds.x,
        y: r.y + node.bounds.y,
        ..r
    };
    Some((
        r,
        layout
            .handle_color
            .unwrap_or(Color::rgba(0.26, 0.53, 0.96, 1.0)),
        rtl,
    ))
}

fn overlay(id: &str, kind: &'static str, bounds: Rect) -> Node {
    let mut node = Node::new(id, kind);
    node.bounds = bounds;
    node.parent = Some(0);
    node.inspect = false;
    node
}

fn handle(scene: &mut Scene, r: Rect, color: Color, control: SelectionControl, rtl: bool) {
    let to_left = (control == SelectionControl::Start) != rtl;
    let side = 25.0;
    let bounds = Rect {
        x: if to_left { r.x - side } else { r.x },
        y: r.y + r.h,
        w: side,
        h: side,
    };
    if bounds.x >= scene.size.0 || bounds.y >= scene.size.1 {
        return;
    }
    let id = format!("compose-selection-{}", control.suffix());
    let mut node = overlay(&id, "selection_handle", bounds);
    let round = (side / 2.0, side / 2.0);
    let mut corners = [round; 4];
    corners[usize::from(to_left)] = (0.0, 0.0);
    node.vector = Some(Box::new(VectorShape {
        geom: VectorGeom::RoundedRect {
            radii: corners,
            continuous: false,
        },
        fill: Some(VectorFill {
            color,
            eo_fill: false,
            antialiased: true,
        }),
        stroke: None,
    }));
    node.selection_control = Some(control);
    scene.nodes.push(node);
}

fn toolbar(scene: &mut Scene, first: Rect, last: Rect) {
    let margin = 16.0;
    let width = (scene.size.0 - margin * 2.0).clamp(1.0, 236.0);
    let height = 48.0;
    let middle = (first.x + last.x + last.w) / 2.0;
    let x = (middle - width / 2.0).clamp(margin, (scene.size.0 - margin - width).max(margin));
    let above = first.y - height - 8.0;
    let y = if above >= 40.0 {
        above
    } else {
        (last.y + last.h + 29.0).min(scene.size.1 - height)
    };
    let bounds = Rect {
        x,
        y,
        w: width,
        h: height,
    };
    let mut bar = overlay("compose-selection-toolbar", "selection_toolbar", bounds);
    let dark = scene.appearance == Appearance::Dark;
    bar.fill = Some(if dark {
        rgb(54, 55, 64)
    } else {
        rgb(225, 226, 237)
    });
    bar.radius = height / 2.0;
    bar.shadow = Some(Shadow {
        color: Color::rgba(0.0, 0.0, 0.0, 0.16),
        radius: 2.0,
        dx: 0.0,
        dy: 0.7,
    });
    scene.nodes.push(bar);
    let mut left = x;
    for (label, units, control) in [
        ("Copy", 66.0, SelectionControl::Copy),
        ("Select all", 79.0, SelectionControl::SelectAll),
        ("Read aloud", 91.0, SelectionControl::ReadAloud),
    ] {
        let w = width * units / 236.0;
        let id = format!("compose-selection-{}", control.suffix());
        let mut item = overlay(
            &id,
            "selection_button",
            Rect {
                x: left,
                y,
                w,
                h: height,
            },
        );
        item.inspect = true;
        item.label = Some(label.to_owned());
        item.role = Role::Button;
        item.selection_control = Some(control);
        scene.nodes.push(item);
        let mut spec = FontSpec::new(14.0, 17.0, 400, TextAlign::Center);
        spec.android = true;
        spec.tracking = 0.0;
        spec.pitch = 17.0;
        let mut text = overlay(
            &format!("{id}-label"),
            "text",
            Rect {
                x: left,
                y: y + (height - 17.0) / 2.0,
                w,
                h: 17.0,
            },
        );
        text.text = Some(TextRun {
            text: label.to_owned(),
            font: spec,
            color: if dark {
                rgb(225, 226, 237)
            } else {
                rgb(48, 50, 58)
            },
            strike: false,
            offset_x: 0.0,
            spans: vec![],
        });
        scene.nodes.push(text);
        left += w;
    }
}

fn magnify(
    scene: &mut Scene,
    nodes: std::ops::RangeInclusive<usize>,
    (pointer_x, pointer_y): (f64, f64),
) {
    let Some(source) = nodes
        .filter_map(|i| scene.nodes.get(i))
        .filter(|n| {
            n.text.is_some()
                && n.compose_text
                    .as_ref()
                    .is_some_and(|t| !t.highlight.is_empty())
        })
        .min_by(|a, b| {
            (a.bounds.center().1 - pointer_y)
                .abs()
                .total_cmp(&(b.bounds.center().1 - pointer_y).abs())
        })
    else {
        return;
    };
    let width = scene.size.0.min(100.0);
    let height = 48.0;
    let zoom = 1.25;
    let source_center = (pointer_x - source.bounds.x).clamp(0.0, source.bounds.w);
    let local_y = (pointer_y - source.bounds.y).clamp(0.0, source.bounds.h);
    let origin = source.bounds.y;
    let top = (origin - height - 6.0).max(0.0);
    let x = (pointer_x - width / 2.0).clamp(0.0, scene.size.0 - width);
    let bounds = Rect {
        x,
        y: top,
        w: width,
        h: height,
    };
    let mut lens = overlay("compose-selection-magnifier", "selection_magnifier", bounds);
    lens.radius = height / 2.0;
    lens.clip_radius = Some(lens.radius);
    lens.fill = Some(if scene.appearance == Appearance::Dark {
        rgb(24, 24, 24)
    } else {
        rgb(250, 250, 250)
    });
    lens.shadow = Some(Shadow {
        color: Color::rgba(0.0, 0.0, 0.0, 0.20),
        radius: 3.2,
        dx: 0.0,
        dy: 1.4,
    });
    let mut lens_text = overlay(
        "compose-selection-magnified-text",
        "text",
        Rect {
            x: x + width / 2.0 - source_center * zoom,
            y: top + height / 2.0 - local_y * zoom,
            w: source.bounds.w * zoom,
            h: source.bounds.h * zoom,
        },
    );
    lens_text.text = source.text.clone().map(|mut run| {
        run.font = run.font.scaled(zoom as f32);
        run
    });
    lens_text.compose_text = source.compose_text.clone().map(|mut layout| {
        layout.layout_callback = None;
        layout.action = None;
        layout.inline.clear();
        for rect in &mut layout.highlight {
            *rect = Rect {
                x: rect.x * zoom,
                y: rect.y * zoom,
                w: rect.w * zoom,
                h: rect.h * zoom,
            };
        }
        layout
    });
    lens_text.parent = Some(scene.nodes.len());
    scene.nodes.extend([lens, lens_text]);
}

fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::rgba(
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
        1.0,
    )
}
