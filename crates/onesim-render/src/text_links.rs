//! Host-side interactions for individual links within Compose text.
use crate::Scene;
use onesim_protocol::{ViewKind, ViewNode};
use std::borrow::Cow;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Link {
    node: String,
    pub action: u64,
    pub range: [usize; 2],
}

impl Link {
    pub fn at(scene: &Scene, x: f64, y: f64) -> Option<Self> {
        let node = &scene.nodes[scene.hit_test(x, y)?];
        let text = node.compose_text.as_ref()?;
        let offset = text.click_offset(&node.text.as_ref()?.text, x - node.bounds.x, y - node.bounds.y)?;
        let range = *text.link_ranges.iter().rev().find(|[start, end]| (*start..*end).contains(&offset))?;
        Some(Self { node: node.id.clone(), action: text.action?, range })
    }

    fn visible(scene: &Scene) -> impl Iterator<Item = Self> + '_ {
        scene.nodes.iter().enumerate().filter_map(|(index, node)| {
            let layout = node.compose_text.as_ref()?;
            if layout.link_ranges.is_empty() { return None; }
            let text = &node.text.as_ref()?.text;
            Some((index, node, layout, text, layout.action?))
        }).flat_map(move |(index, node, layout, text, action)| {
            layout.link_ranges.iter().copied().filter_map(move |range| {
                let visible = layout.link_points(text, range)
                    .any(|(x, y)| scene.hit_test(node.bounds.x + x, node.bounds.y + y) == Some(index));
                visible.then(|| Self { node: node.id.clone(), action, range })
            })
        })
    }
}

#[derive(Debug, Default)]
pub(crate) struct TextLinks {
    pub focused: Option<Link>,
    pub hovered: Option<Link>,
    pub pressed: Option<Link>,
}

impl TextLinks {
    pub fn traverse(&mut self, scene: &Scene, backwards: bool) -> Option<bool> {
        let links: Vec<_> = Link::visible(scene).collect();
        if links.is_empty() {
            return self.focused.take().map(|_| true);
        }
        let index = self.focused.as_ref().and_then(|current| links.iter().position(|link| link == current));
        let next = match (index, backwards) {
            (Some(index), true) => index.checked_sub(1),
            (Some(index), false) => Some(index + 1),
            (None, true) => Some(links.len() - 1),
            (None, false) => Some(0),
        };
        self.focused = next.and_then(|i| links.get(i).cloned());
        Some(self.focused.is_some())
    }

    pub fn focused_in(&self, scene: &Scene) -> Option<&Link> {
        self.focused.as_ref().filter(|focused| Link::visible(scene).any(|link| &link == *focused))
    }

    pub fn styled_tree<'a>(&self, root: &'a ViewNode) -> Cow<'a, ViewNode> {
        let active = [&self.focused, &self.hovered, &self.pressed];
        if active.iter().all(|link| link.is_none()) || !matches!(root.kind, ViewKind::Compose { .. }) {
            return Cow::Borrowed(root);
        }
        fn apply(node: &mut ViewNode, active: [&Option<Link>; 3]) {
            let ViewKind::Compose { props, children, .. } = &mut node.kind else { return };
            let mut overlays = vec![];
            for (state, link) in active.into_iter().enumerate() {
                let Some(link) = link.as_ref().filter(|link| link.node == node.id && props["textClick"] == link.action) else { continue };
                let Some(ranges) = props["linkRanges"].as_array() else { continue };
                let Some(index) = ranges.iter().rposition(|range| range[0] == link.range[0] && range[1] == link.range[1]) else { continue };
                let overlay = &props["linkStyles"][index][state];
                if overlay.is_object() {
                    overlays.push(overlay.clone());
                }
            }
            if !overlays.is_empty() {
                let mut spans = props["spans"].as_array().cloned().unwrap_or_default();
                spans.extend(overlays);
                props["spans"] = serde_json::Value::Array(spans);
            }
            for child in children {
                apply(child, active);
            }
        }
        let mut root = root.clone();
        apply(&mut root, active);
        Cow::Owned(root)
    }
}
