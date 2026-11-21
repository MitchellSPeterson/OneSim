//! Merge accessibility modifiers onto scene nodes and resolve container semantics.

use std::collections::BTreeMap;

use onesim_protocol::AccessibilityMod;

use super::*;
use crate::scene::A11y;

/// Attach `d` to the inspectable content inside `lay`.
///
/// Opacity, clip, shadow, and the other layout-only wrappers have `inspect` false, and
/// `ui_tree` skips them without moving semantics. Background, overlay, and mask keep the
/// modified view in `content_child` beside a decoration. Stamping `first_node` would put the
/// record on the wrapper, or drop it when that wrapper has no single node.
pub(super) fn stamp_on(lay: &mut Lay, d: &AccessibilityMod) {
    if let Some(n) = target(lay) {
        stamp(n, d);
    }
}

fn target(l: &mut Lay) -> Option<&mut Node> {
    if l.node.as_ref().is_some_and(|n| n.inspect) {
        return l.node.as_deref_mut();
    }
    let idx = match l.content_child {
        Some(i) => i as usize,
        None if l.kids.len() == 1 => 0,
        None => return None,
    };
    target(&mut l.kids.get_mut(idx)?.2)
}

fn stamp(node: &mut Node, d: &AccessibilityMod) {
    let a = node.a11y.get_or_insert_with(|| Box::new(A11y::default()));
    if let Some(v) = &d.label {
        a.label = Some(v.clone());
        a.explicit_label = true;
    }
    if let Some(v) = &d.value {
        a.value = Some(v.clone());
    }
    if let Some(v) = &d.hint {
        a.hint = Some(v.clone());
    }
    if let Some(v) = &d.identifier {
        a.identifier = Some(v.clone());
    }
    if let Some(v) = d.hidden {
        a.hidden = Some(v);
    }
    for t in &d.add_traits {
        if !a.traits.iter().any(|e| e == t) {
            a.traits.push(t.clone());
        }
    }
    if !d.remove_traits.is_empty() {
        a.traits.retain(|t| !d.remove_traits.iter().any(|r| r == t));
    }
    if let Some(v) = d.sort_priority {
        a.sort = Some(v);
    }
    if let Some(v) = d.heading {
        a.heading = Some(v);
    }
    if !d.input_labels.is_empty() {
        a.input_labels = d.input_labels.clone();
    }
    if let Some(v) = &d.children {
        a.children = Some(v.clone());
    }
    a.actions.extend(d.actions.iter().cloned());
    if let Some(v) = &d.adjustable {
        a.adjustable = Some(v.clone());
    }
    if let Some(v) = &d.scroll {
        let slot = a.scroll.get_or_insert_with(Default::default);
        if v.top.is_some() {
            slot.top = v.top;
        }
        if v.bottom.is_some() {
            slot.bottom = v.bottom;
        }
        if v.leading.is_some() {
            slot.leading = v.leading;
        }
        if v.trailing.is_some() {
            slot.trailing = v.trailing;
        }
    }
    if let Some(v) = &d.zoom {
        let slot = a.zoom.get_or_insert_with(Default::default);
        if v.zoom_in.is_some() {
            slot.zoom_in = v.zoom_in;
        }
        if v.zoom_out.is_some() {
            slot.zoom_out = v.zoom_out;
        }
    }
    if let Some(v) = &d.activation {
        a.activation = Some(v.clone());
    }
    if let Some(v) = &d.drag {
        a.drag = Some(v.clone());
    }
    if let Some(v) = &d.drop {
        a.drop = Some(v.clone());
    }
    a.custom.extend(d.custom.iter().cloned());
    if !d.custom_remove.is_empty() {
        a.custom.retain(|c| !d.custom_remove.iter().any(|r| c.id.as_deref() == Some(r) || c.key == *r));
    }
    a.rotors.extend(d.rotors.iter().cloned());
    if let Some(v) = &d.rotor_entry {
        a.rotor_entry = Some(v.clone());
    }
    if !d.elements.is_empty() {
        a.elements = d.elements.clone();
    }
    if let Some(v) = d.focused {
        a.focused = v;
    }
    if d.default_focus {
        a.default_focus = true;
    }
    if !d.technologies.is_empty() {
        a.technologies = d.technologies.clone();
    }
    if let Some(v) = d.responds {
        a.responds = Some(v);
    }
    if let Some(v) = d.ignores_invert_colors {
        a.ignores_invert = v;
    }
    if let Some(v) = d.direct_touch {
        a.direct_touch = v;
    }
    if !d.direct_touch_options.is_empty() {
        a.direct_touch_options = d.direct_touch_options.clone();
    }
    if let Some(v) = &d.text_content_type {
        a.text_content = Some(v.clone());
    }
    if let Some(v) = &d.scroll_status {
        a.scroll_status = Some(v.clone());
    }
    if let Some(v) = d.large_content {
        a.large_content = v;
    }
    if let Some(v) = &d.large_label {
        a.large_label = Some(v.clone());
    }
    if let Some(v) = d.chart {
        a.chart = v;
    }
    if let Some(v) = &d.pair {
        a.pair = Some(v.clone());
    }
    if let Some(v) = &d.linked {
        a.linked = Some(v.clone());
    }
}

/// After layout: combine labels, hide descendants, propagate `hidden`, and join labeled pairs.
pub(super) fn apply(nodes: &mut [Node]) {
    let kids = child_lists(nodes);
    let mut joins = Vec::new();
    for (i, node) in nodes.iter().enumerate() {
        let Some(a) = node.a11y.as_ref() else { continue };
        if a.children.as_deref() == Some("combine") && !a.explicit_label {
            let mut parts = Vec::new();
            collect_labels(nodes, &kids, i, &mut parts);
            if !parts.is_empty() {
                joins.push((i, parts.join(", ")));
            }
        }
    }
    for (i, label) in joins {
        if let Some(a) = nodes[i].a11y.as_mut() {
            a.label = Some(label.clone());
        }
        nodes[i].label = Some(label);
    }

    let mut hide = Vec::new();
    for (i, node) in nodes.iter().enumerate() {
        let Some(a) = node.a11y.as_ref() else { continue };
        let mode = a.children.as_deref();
        if mode == Some("ignore") || mode == Some("combine") || !a.elements.is_empty() {
            descendants(&kids, i, &mut hide);
        }
    }
    for i in hide {
        nodes[i].a11y.get_or_insert_with(|| Box::new(A11y::default())).suppressed = true;
    }

    let mut hidden = Vec::new();
    for (i, node) in nodes.iter().enumerate() {
        if node.a11y.as_ref().is_some_and(|a| a.hidden == Some(true)) {
            descendants(&kids, i, &mut hidden);
        }
    }
    for i in hidden {
        nodes[i].a11y.get_or_insert_with(|| Box::new(A11y::default())).hidden = Some(true);
    }

    for n in nodes.iter_mut() {
        let Some(a) = n.a11y.as_ref() else { continue };
        if a.explicit_label {
            if let Some(l) = a.label.clone() {
                n.label = Some(l);
            }
        }
    }

    join_pairs(nodes);
}

fn child_lists(nodes: &[Node]) -> Vec<Vec<usize>> {
    let mut kids = vec![Vec::new(); nodes.len()];
    for (i, n) in nodes.iter().enumerate() {
        if let Some(p) = n.parent {
            if p < kids.len() {
                kids[p].push(i);
            }
        }
    }
    kids
}

fn descendants(kids: &[Vec<usize>], i: usize, out: &mut Vec<usize>) {
    for &c in &kids[i] {
        out.push(c);
        descendants(kids, c, out);
    }
}

fn spoken(n: &Node) -> String {
    n.a11y
        .as_ref()
        .and_then(|a| a.label.clone())
        .or_else(|| n.label.clone())
        .or_else(|| n.text.as_ref().map(|t| t.text.clone()))
        .unwrap_or_default()
}

fn collect_labels(nodes: &[Node], kids: &[Vec<usize>], i: usize, out: &mut Vec<String>) {
    for &c in &kids[i] {
        if nodes[c].a11y.as_ref().is_some_and(|a| a.hidden == Some(true)) {
            continue;
        }
        if nodes[c].a11y.as_ref().is_some_and(|a| a.explicit_label) {
            let l = spoken(&nodes[c]);
            if !l.is_empty() {
                out.push(l);
            }
            continue;
        }
        if kids[c].is_empty() {
            let l = spoken(&nodes[c]);
            if !l.is_empty() {
                out.push(l);
            }
        } else {
            collect_labels(nodes, kids, c, out);
        }
    }
}

fn join_pairs(nodes: &mut [Node]) {
    let mut groups: BTreeMap<String, (Option<usize>, Option<usize>)> = BTreeMap::new();
    for (i, n) in nodes.iter().enumerate() {
        let Some(pair) = n.a11y.as_ref().and_then(|a| a.pair.clone()) else { continue };
        let entry = groups.entry(pair.id).or_insert((None, None));
        if pair.role == "label" && entry.0.is_none() {
            entry.0 = Some(i);
        }
        if pair.role == "content" && entry.1.is_none() {
            entry.1 = Some(i);
        }
    }
    let jobs: Vec<(usize, usize, String)> = groups
        .into_values()
        .filter_map(|(label, content)| {
            let (label, content) = (label?, content?);
            Some((label, content, format!("{}, {}", spoken(&nodes[label]), spoken(&nodes[content]))))
        })
        .collect();
    for (label, content, text) in jobs {
        nodes[label].a11y.get_or_insert_with(|| Box::new(A11y::default())).suppressed = true;
        let a = nodes[content].a11y.get_or_insert_with(|| Box::new(A11y::default()));
        a.label = Some(text.clone());
        a.explicit_label = true;
        nodes[content].label = Some(text);
    }
}
