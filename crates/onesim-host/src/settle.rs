//! Layout settling for React Native and SwiftUI sessions.

use crate::session::State;
use crate::Host;
use onesim_protocol::{ApiError, GeoBox, GuestMessage, HostMessage};
use onesim_render::rn_input::RnCtx;
use onesim_render::{scroll_anchor_offset, Node, Scene};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// React Native: how long the guest must be quiet before a snapshot, and the most a snapshot waits.
const QUIET: Duration = Duration::from_millis(150);
const SETTLE_MAX: Duration = Duration::from_millis(2500);
/// React Native: when trees keep arriving but the layout no longer changes (an endless animation, e.g. a
/// rotating logo), a snapshot stops waiting after this long; startup waits `STARTUP_MAX` (splash, first layout).
const ANIM_MAX: Duration = Duration::from_millis(500);
pub(crate) const STARTUP_MAX: Duration = Duration::from_millis(2500);

fn offset_changed(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() > 0.0 || (a.1 - b.1).abs() > 0.0
}

/// `index` is a descendant of `ancestor` by walking `Node::parent`.
fn is_descendant(nodes: &[Node], mut index: usize, ancestor: usize) -> bool {
    for _ in 0..nodes.len() {
        let Some(parent) = nodes.get(index).and_then(|n| n.parent) else { return false };
        if parent == ancestor {
            return true;
        }
        index = parent;
    }
    false
}

/// Scroll offset for `scroll_to_id`, or `None` when the scroll or the anchored descendant is missing.
fn scroll_to_offset(scene: &Scene, scroll_id: &str, target: &str, anchor: Option<[f64; 2]>) -> Option<(f64, f64)> {
    let scroll_idx = scene.nodes.iter().position(|n| n.id == scroll_id && n.scroll.is_some())?;
    let target_idx = (0..scene.nodes.len()).find(|&i| scene.nodes[i].anchor.as_deref() == Some(target) && is_descendant(&scene.nodes, i, scroll_idx))?;
    let sb = scene.nodes[scroll_idx].bounds;
    let info = scene.nodes[scroll_idx].scroll?;
    let tb = scene.nodes[target_idx].bounds;
    let a = anchor.unwrap_or([0.0, 0.0]);
    let x = scroll_anchor_offset(tb.x - sb.x + info.offset.0, sb.w, tb.w, a[0], info.max.0);
    let y = scroll_anchor_offset(tb.y - sb.y + info.offset.1, sb.h, tb.h, a[1], info.max.1);
    Some((x, y))
}

/// Anchored id whose content position on the scroll's main axis is the greatest still `<= offset + 1`.
fn current_anchor(scene: &Scene, scroll_id: &str) -> Option<String> {
    let idx = scene.nodes.iter().position(|n| n.id == scroll_id && n.scroll.is_some())?;
    let (vertical, origin, offset) = {
        let n = &scene.nodes[idx];
        let info = n.scroll?;
        if info.vertical { (true, n.bounds.y, info.offset.1) } else { (false, n.bounds.x, info.offset.0) }
    };
    let mut best: Option<(f64, String)> = None;
    for i in 0..scene.nodes.len() {
        let Some(anchor) = scene.nodes[i].anchor.clone() else { continue };
        if !is_descendant(&scene.nodes, i, idx) {
            continue;
        }
        let main = if vertical { scene.nodes[i].bounds.y } else { scene.nodes[i].bounds.x };
        let content = main - origin + offset;
        if content <= offset + 1.0 && best.as_ref().is_none_or(|(b, _)| content > *b) {
            best = Some((content, anchor));
        }
    }
    best.map(|(_, id)| id)
}

fn needs_geometry(n: &Node) -> bool {
    // The guest's first frame is the viewport at the origin. A matching size at a
    // different origin still needs one correction so `frame(in: .global)` is the screen frame.
    n.proposal.is_some_and(|[w, h]| {
        (n.bounds.w - w).abs() > 0.5 || (n.bounds.h - h).abs() > 0.5 || n.bounds.x.abs() > 0.5 || n.bounds.y.abs() > 0.5
    })
}

#[cfg(test)]
fn nodes_needing_geometry(scene: &Scene) -> Vec<&str> {
    scene.nodes.iter().filter(|n| needs_geometry(n)).map(|n| n.id.as_str()).collect()
}

fn geometry_changes(scene: &Scene, safe_top: f64, safe_bottom: f64, sent: &BTreeMap<String, GeoBox>) -> BTreeMap<String, GeoBox> {
    let mut frames = BTreeMap::new();
    for n in &scene.nodes {
        if n.proposal.is_none() {
            continue;
        }
        let geo = GeoBox { width: n.bounds.w, height: n.bounds.h, x: n.bounds.x, y: n.bounds.y, safe_top, safe_bottom, safe_leading: 0.0, safe_trailing: 0.0 };
        if let Some(prev) = sent.get(&n.id) {
            // A reader that returns to the origin still differs from the frame already sent.
            if prev != &geo {
                frames.insert(n.id.clone(), geo);
            }
            continue;
        }
        if needs_geometry(n) {
            frames.insert(n.id.clone(), geo);
        }
    }
    frames
}

/// First `.scrollPosition` scroll whose anchored target differs from the last writeback.
fn next_scroll_position(scene: &Scene, sent: &BTreeMap<String, Option<String>>) -> Option<(String, Option<String>)> {
    scene.nodes.iter().find_map(|n| {
        if n.value.as_deref() != Some("position") || n.scroll.is_none() {
            return None;
        }
        let target = current_anchor(scene, &n.id);
        match sent.get(&n.id) {
            Some(prev) if prev == &target => None,
            _ => Some((n.id.clone(), target)),
        }
    })
}

impl Host {
    /// Scene of the current tree. React Native sessions first settle: the guest's trees, layout
    /// frames, focus requests and `scroll_to` replies are exchanged until nothing changes.
    /// Running SwiftUI sessions settle `scroll_to_id`, geometry and `scroll_position` (4 rounds).
    pub(crate) fn scene(&mut self, id: &str) -> Result<(Arc<Scene>, u64, f64), ApiError> {
        self.scene_within(id, ANIM_MAX)
    }

    /// `scene`, where an animation that keeps trees coming (layout unchanged) is waited on for `anim` at most.
    /// Paused sessions skip settling. SwiftUI uses a 4-round settle and ignores `anim`.
    pub(crate) fn scene_within(&mut self, id: &str, anim: Duration) -> Result<(Arc<Scene>, u64, f64), ApiError> {
        // A paused guest cannot answer: serve the last frame as is.
        if self.get(id)?.state() == State::Paused {
            return self.build(id);
        }
        if !self.get(id)?.is_rn() {
            return if self.get(id)?.is_swiftui() { self.swiftui_settle(id) } else { self.compose_settle(id) };
        }
        let settle = self.get(id)?.settle;
        let (start, deadline) = (Instant::now(), Instant::now() + SETTLE_MAX.max(anim));
        loop {
            if settle {
                self.get(id)?.wait_quiet(QUIET, anim);
            }
            let generation = self.get(id)?.generation();
            let out = self.build(id)?;
            let acted = self.rn_apply_requests(id, &out.0);
            let s = self.get(id)?;
            let sent = s.sync_layout(&out.0);
            let animating = !acted && !sent && start.elapsed() >= anim;
            let done = !acted && !sent && s.generation() == generation;
            // Live: no waiting, but a host-side change (`scroll_to`, focus) still needs one rebuild,
            // since the guest may never send a tree for it.
            let live_done = !settle && !acted;
            if done || animating || live_done || Instant::now() > deadline {
                return Ok(out);
            }
        }
    }

    fn compose_settle(&mut self, id: &str) -> Result<(Arc<Scene>, u64, f64), ApiError> {
        let mut negotiating = false;
        for round in 0..64 {
            let out = self.build(id)?;
            let pending = out.0.pending_compose_auto_size();
            if round >= 4 && !pending && !negotiating {
                return Ok(out);
            }
            negotiating = pending;
            let layouts = out.0.compose_text_layouts();
            let session = self.get(id)?;
            if layouts == session.sent_text_layouts {
                return Ok(out);
            }
            let changed = layouts.iter().filter(|layout| !session.sent_text_layouts.contains(layout)).cloned().collect::<Vec<_>>();
            if !changed.is_empty() {
                session.send_and_wait_tree(&HostMessage::TextLayout { version: out.1, layouts: changed })?;
            }
            session.sent_text_layouts = layouts;
        }
        self.build(id)
    }

    /// Apply the guest's `focus_request` / `scroll_to` messages and drop focus on a vanished field.
    /// Returns whether the interaction state changed (the scene must be rebuilt).
    fn rn_apply_requests(&mut self, id: &str, scene: &Scene) -> bool {
        let Some(s) = self.sessions.get_mut(id) else { return false };
        let Some((_, root)) = s.tree() else { return false };
        let cx = RnCtx { scene, root: &root, keyboard: s.keyboard_height() };
        let requests = s.take_requests();
        let (changed, out) = s.rn.apply_requests(&mut s.inter, &cx, requests);
        if changed {
            s.invalidate_scene();
        }
        let _ = s.send_out(out);
        changed
    }

    /// Running SwiftUI: at most 4 builds. `scroll_to_id` rebuilds without waiting. Geometry and
    /// `scroll_position` wait for a tree only when `settle` is set; a silent guest stops the loop.
    fn swiftui_settle(&mut self, id: &str) -> Result<(Arc<Scene>, u64, f64), ApiError> {
        let mut scene = self.build(id)?;
        let mut round = 0;
        loop {
            let rebuild = self.swiftui_round(id, &scene.0)?;
            round += 1;
            if !rebuild || round == 4 {
                return Ok(scene);
            }
            scene = self.build(id)?;
        }
    }

    /// `true` when this round changed host scroll state or the guest answered a correction with a tree.
    fn swiftui_round(&mut self, id: &str, scene: &Scene) -> Result<bool, ApiError> {
        if self.apply_scroll_ids(id, scene) {
            return Ok(true);
        }
        if let Some(again) = self.dispatch_geometry(id, scene)? {
            return Ok(again);
        }
        let frames = scene.pending_shape_frames();
        if !frames.is_empty() {
            return self.get(id)?.send_and_wait_tree(&HostMessage::Layout { frames });
        }
        let next = {
            let sent = &self.get(id)?.sent_positions;
            next_scroll_position(scene, sent)
        };
        if let Some((scroll, target)) = next {
            return self.dispatch_scroll_position(id, scroll, target);
        }
        Ok(false)
    }

    /// Apply queued `scroll_to_id` requests. Missing targets are dropped. Returns whether any offset changed.
    fn apply_scroll_ids(&mut self, id: &str, scene: &Scene) -> bool {
        let Some(s) = self.sessions.get_mut(id) else { return false };
        let mut changed = false;
        for r in s.take_requests() {
            let GuestMessage::ScrollToId { scroll, target, anchor } = r else { continue };
            let Some(off) = scroll_to_offset(scene, &scroll, &target, anchor) else { continue };
            let prev = scene.nodes.iter().find(|n| n.id == scroll && n.scroll.is_some()).and_then(|n| n.scroll).map(|info| info.offset);
            if prev.is_none_or(|p| offset_changed(p, off)) {
                changed = true;
            }
            s.inter.scroll.insert(scroll, off);
        }
        if changed {
            s.invalidate_scene();
        }
        changed
    }

    /// `Some(again)` when a geometry correction was sent. `again` is true only if a new tree arrived.
    fn dispatch_geometry(&mut self, id: &str, scene: &Scene) -> Result<Option<bool>, ApiError> {
        let changed = {
            let s = self.get(id)?;
            geometry_changes(scene, s.device.safe_top, s.device.safe_bottom, &s.sent_geometry)
        };
        if changed.is_empty() {
            return Ok(None);
        }
        let settle = self.get(id)?.settle;
        let msg = HostMessage::Geometry { frames: changed.clone() };
        let arrived = if settle { self.get(id)?.send_and_wait_tree(&msg)? } else { self.get(id)?.send(&msg)?; false };
        self.get(id)?.sent_geometry.extend(changed);
        Ok(Some(settle && arrived))
    }

    /// Send one `scroll_position`. Stores the target only after the send succeeds.
    fn dispatch_scroll_position(&mut self, id: &str, scroll: String, target: Option<String>) -> Result<bool, ApiError> {
        let settle = self.get(id)?.settle;
        let msg = HostMessage::ScrollPosition { scroll: scroll.clone(), target: target.clone() };
        let s = self.get(id)?;
        let arrived = if settle { s.send_and_wait_tree(&msg)? } else { s.send(&msg)?; false };
        s.sent_positions.insert(scroll, target);
        Ok(settle && arrived)
    }
}

#[cfg(test)]
mod settle_tests {
    use super::{current_anchor, geometry_changes, nodes_needing_geometry, scroll_to_offset};
    use onesim_protocol::{Appearance, GeoBox, IosVersion};
    use onesim_render::{Node, Rect, Scene, ScrollInfo};
    use std::collections::BTreeMap;

    fn scene(nodes: Vec<Node>) -> Scene {
        Scene { size: (200.0, 400.0), appearance: Appearance::Light, ios: IosVersion::default(), nodes, unsupported_symbols: vec![], notes: vec![] }
    }

    fn scroll_node(offset_y: f64) -> Node {
        let mut n = Node::new("s", "scroll");
        n.bounds = Rect { x: 0.0, y: 10.0, w: 0.0, h: 100.0 };
        n.scroll = Some(ScrollInfo { vertical: true, offset: (0.0, offset_y), max: (0.0, 500.0), snap: None, content: None, page: None, aligned: false });
        n
    }

    /// Content position is `bounds.y - 10 + offset_y`.
    fn anchored(id: &str, anchor: &str, parent: usize, content_y: f64, offset_y: f64) -> Node {
        let mut n = Node::new(id, "row");
        n.parent = Some(parent);
        n.anchor = Some(anchor.into());
        n.bounds.y = content_y - offset_y + 10.0;
        n.bounds.h = 20.0;
        n
    }

    #[test]
    fn scroll_to_id_offset_aligns_anchor() {
        let mut mid = Node::new("mid", "box");
        mid.parent = Some(0);
        let mut row = Node::new("row", "row");
        row.parent = Some(1);
        row.anchor = Some("there".into());
        row.bounds.y = 210.0;
        row.bounds.h = 20.0;
        let scene = scene(vec![scroll_node(40.0), mid, row]);
        assert_eq!(scroll_to_offset(&scene, "s", "there", None), Some((0.0, 240.0)));
        assert_eq!(scroll_to_offset(&scene, "s", "there", Some([0.0, 0.5])), Some((0.0, 200.0)));
    }

    #[test]
    fn scroll_position_anchor_is_greatest_at_or_above_the_top() {
        let mut mid = Node::new("mid", "box");
        mid.parent = Some(0);
        let two = scene(vec![scroll_node(100.0), mid.clone(), anchored("row0", "0", 1, 0.0, 100.0), anchored("row180", "180", 0, 180.0, 100.0)]);
        assert_eq!(current_anchor(&two, "s").as_deref(), Some("0"));
        let three = scene(vec![
            scroll_node(100.0),
            mid,
            anchored("row0", "0", 1, 0.0, 100.0),
            anchored("row180", "180", 0, 180.0, 100.0),
            anchored("row150", "150", 0, 150.0, 100.0),
        ]);
        assert_eq!(current_anchor(&three, "s").as_deref(), Some("0"));
        assert_ne!(current_anchor(&three, "s").as_deref(), Some("150"));
        let greatest = scene(vec![
            scroll_node(200.0),
            anchored("low", "low", 0, 40.0, 200.0),
            anchored("high", "high", 0, 180.0, 200.0),
        ]);
        assert_eq!(current_anchor(&greatest, "s").as_deref(), Some("high"));
        let none = scene(vec![scroll_node(0.0), anchored("row50", "50", 0, 50.0, 0.0)]);
        assert_eq!(current_anchor(&none, "s"), None);
    }

    #[test]
    fn geometry_correction_when_bounds_differ_from_proposal() {
        let mut needs = Node::new("needs", "geometry");
        needs.proposal = Some([10.0, 10.0]);
        needs.bounds = Rect { x: 1.0, y: 2.0, w: 120.0, h: 80.0 };
        let mut close = Node::new("close", "geometry");
        close.proposal = Some([120.0, 80.0]);
        close.bounds = Rect { x: 0.0, y: 0.0, w: 120.5, h: 80.5 };
        let mut open = Node::new("open", "geometry");
        open.proposal = None;
        open.bounds = Rect { x: 0.0, y: 0.0, w: 120.0, h: 80.0 };
        let mut placed = Node::new("placed", "geometry");
        placed.proposal = Some([40.0, 20.0]);
        placed.bounds = Rect { x: 4.0, y: 8.0, w: 40.0, h: 20.0 };
        assert_eq!(nodes_needing_geometry(&scene(vec![needs, close, open, placed.clone()])), vec!["needs", "placed"]);
        let placed_scene = scene(vec![placed]);
        let first = geometry_changes(&placed_scene, 62.0, 34.0, &BTreeMap::new());
        assert_eq!((first["placed"].x, first["placed"].y), (4.0, 8.0));
        assert!(geometry_changes(&placed_scene, 62.0, 34.0, &first).is_empty());
        let mut home = Node::new("home", "geometry");
        home.proposal = Some([40.0, 20.0]);
        home.bounds = Rect { x: 0.0, y: 0.0, w: 40.0, h: 20.0 };
        let mut sent = BTreeMap::new();
        sent.insert(
            "home".to_string(),
            GeoBox { width: 40.0, height: 20.0, x: 4.0, y: 8.0, safe_top: 62.0, safe_bottom: 34.0, safe_leading: 0.0, safe_trailing: 0.0 },
        );
        let back = geometry_changes(&scene(vec![home]), 62.0, 34.0, &sent);
        assert_eq!((back["home"].x, back["home"].y), (0.0, 0.0));
    }
}
