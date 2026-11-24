//! `ShareLink`'s share sheet (protocol section W), as the Simulator shows it for text: a floating glass card with
//! the item (a text tile and its title), an app row (Reminders, More) and an action row (Copy, Save to Files).
use super::*;

/// What one share layer shows.
pub(super) struct ShareSheet<'a> {
    pub title: &'a str,
    /// `title` is a `SharePreview` title (primary, semibold) rather than the item's own text (secondary).
    pub titled: bool,
    pub url: bool,
    /// The `SharePreview` icon or image: a bitmap fills the header tile; a symbol image leaves it blank, as on device.
    pub preview: Option<&'a ViewNode>,
    pub copy: Option<u64>,
    pub activities: &'a std::collections::BTreeMap<String, u64>,
    pub dismiss: Option<u64>,
    /// A link iOS has no preview for (React Native `showShareActionSheetWithOptions`): the compass tile.
    pub link_glyph: bool,
}

impl Ctx<'_> {
    /// Copy runs `copy`, the other cells `activities[name]` (`dismiss` when the guest sent none); a tap outside and a
    /// swipe down run `dismiss`.
    pub(super) fn share_sheet(&mut self, id: &str, s: &ShareSheet, g: &Geo) -> Lay {
        let (w, h) = (g.w, g.h);
        let m = crate::look::look(self.ios).share;
        let (cw, ch) = (w - 2.0 * m.inset.0, m.card_h);
        let top = h - m.inset.1 - ch;
        let primary = self.semantic(Semantic::Primary);
        let secondary = self.semantic(Semantic::Secondary);
        let tile_fill = self.pick(SHARE_TILE);
        let mut kids: Vec<(f64, f64, Lay)> = vec![];
        // Header: the item's tile and title.
        let tile = if s.link_glyph && s.preview.is_none() {
            let (r, g2, b) = SHARE_LINK_GLYPH_FG;
            self.share_tile(&format!("{id}#tile"), SHARE_TILE_SIZE, SHARE_TILE_RADIUS, rgba(255, 255, 255, 1.0), None, "safari", SHARE_TILE_GLYPH, 400, rgba(r, g2, b, 1.0))
        } else if s.url || s.preview.is_some() {
            let mut t = self.share_tile(&format!("{id}#tile"), SHARE_TILE_SIZE, SHARE_TILE_RADIUS, tile_fill, None, "", 0.0, 400, primary);
            if let Some(v) = s.preview.filter(|v| matches!(&v.kind, ViewKind::Image { bitmap: Some(_), .. })) {
                let il = self.view(v, Prop { w: Some(SHARE_TILE_SIZE), h: Some(SHARE_TILE_SIZE) }, Env { size: g.body, ..Env::new() });
                t.kids.push(((SHARE_TILE_SIZE - il.size.w) / 2.0, (SHARE_TILE_SIZE - il.size.h) / 2.0, il));
                if let Some(n) = t.node.as_mut() {
                    n.clip_radius = Some(SHARE_TILE_RADIUS);
                }
            }
            t
        } else {
            self.share_tile(&format!("{id}#tile"), SHARE_TILE_SIZE, SHARE_TILE_RADIUS, tile_fill, None, "character.cursor.ibeam", SHARE_TILE_GLYPH, 300, secondary)
        };
        kids.push((m.tile.0, m.tile.1, tile));
        let tsz = SHARE_TITLE_PT;
        let (tfg, tw, ty) = if s.titled { (primary, 600, m.title.1 + SHARE_TITLED_DY) } else { (secondary, 400, m.title.1) };
        let tl = self.text_lay(&format!("{id}#title"), s.title, FontSpec::new(tsz, self.line_h(tsz), tw, TextAlign::Leading), tfg, None);
        let tl = clip_w(tl, cw - m.title.0 - m.tile.0);
        kids.push((m.title.0, ty, tl));
        for (i, y) in [(0, m.header_h), (1, m.sep.0)] {
            let mut sep = Node::new(format!("{id}#sep{i}"), "separator");
            sep.fill = Some(self.semantic(Semantic::Separator));
            sep.inspect = false;
            kids.push((m.sep.1, y, Lay::new(Size { w: cw - 2.0 * m.sep.1, h: 1.0 / self.scale.max(1.0) }, Some(sep), vec![])));
        }
        // App row, then action row: one cell per activity (icon, label under it).
        let (apps, actions): (&[&str], &[&str]) = if s.url { (m.url_apps, m.url_actions) } else { (&["Reminders", "More"], &["Copy", "Save to Files"]) };
        // An activity the guest left out (React Native `excludedActivityTypes`) has no cell; the rest close up. A guest
        // that sends no activities at all gets every cell (running `dismiss`).
        let offered = |n: &&str| s.activities.is_empty() || if *n == "Copy" { s.copy.is_some() } else { s.activities.contains_key(*n) };
        let (apps, actions): (Vec<&str>, Vec<&str>) = (apps.iter().copied().filter(offered).collect(), actions.iter().copied().filter(offered).collect());
        let cells = apps.iter().enumerate().map(|(i, n)| (i, *n, true)).chain(actions.iter().enumerate().map(|(i, n)| (i, *n, false)));
        let (isz, ix, iy) = m.app_icon;
        for (k, (col, label, app)) in cells.enumerate() {
            let (cell_y, label_y) = if app { (m.header_h, m.label_y.0) } else { (m.sep.0 + 1.0, m.label_y.1) };
            let cx = m.cell.0 + col as f64 * m.cell.1;
            let iid = format!("{id}#cell{k}#icon");
            let icon = match label {
                "Reminders" => self.reminders_icon(&iid, isz),
                "Safari" => self.safari_icon(&iid, isz),
                "More" => self.share_tile(&iid, isz, SHARE_APP_RADIUS, self.pick(SHARE_MORE_TILE), None, "ellipsis", SHARE_MORE_GLYPH, 700, self.pick(SHARE_MORE_FG)),
                _ => {
                    let (sym, pt) = match label {
                        "Copy" => ("doc.on.doc", SHARE_ACTION_GLYPH),
                        "Save to Files" => ("folder", SHARE_ACTION_GLYPH),
                        "Open in Safari" => ("safari", SHARE_SAFARI_GLYPH),
                        _ => ("eyeglasses", SHARE_GLASSES_GLYPH),
                    };
                    self.share_tile(&iid, isz, isz / 2.0, self.pick(SHARE_ACTION_FILL), None, sym, pt, 400, primary)
                }
            };
            let icon_y = if app { iy } else { m.action_y };
            let spec = FontSpec::new(SHARE_LABEL_PT, self.line_h(SHARE_LABEL_PT), 400, TextAlign::Center);
            let mut ll = self.text_lay(&format!("{id}#cell{k}#label"), label, spec, primary, Some(m.cell.2 as f32));
            if let Some(n) = ll.node.as_mut() {
                n.inspect = false;
            }
            let lw = ll.size.w;
            let mut b = Node::new(format!("{id}#cell{k}"), "button");
            b.role = Role::Button;
            b.label = Some(label.into());
            b.action = if label == "Copy" { s.copy } else { s.activities.get(label).copied().or(s.dismiss) };
            let cell_h = if app { m.sep.0 - m.header_h } else { ch - m.sep.0 - 1.0 };
            kids.push((cx, cell_y, Lay::new(Size { w: m.cell.2, h: cell_h }, Some(b), vec![(ix, icon_y, icon), ((m.cell.2 - lw) / 2.0, label_y, ll)])));
        }
        let mut card = Node::new(id, "sheet");
        card.radius = SHARE_RADIUS;
        card.clip_radius = Some(SHARE_RADIUS);
        card.glass = Some(crate::glass::GlassSpec { blur: Some(SHARE_BLUR), mix: Some(if self.dark { SHARE_GLASS.1 } else { SHARE_GLASS.0 }), shadow: None, ..self.glass_spec(GlassVariant::Regular, None, false) });
        // Swipe down dismisses it like a sheet with one detent.
        card.sheet = Some(crate::SheetInfo { tops: vec![top], detent: 0, top, dismiss: s.dismiss, scrolls_first: false });
        let scrim = Lay::new(Size { w, h }, Some(self.scrim(&format!("{id}#scrim"), (w, h), 0.0, s.dismiss)), vec![]);
        Lay::new(Size { w, h }, None, vec![(0.0, 0.0, scrim), (m.inset.0, top, Lay::new(Size { w: cw, h: ch }, Some(card), kids))])
    }

    /// The Safari app icon: a white tile holding a blue compass dial with a red and white needle.
    fn safari_icon(&mut self, id: &str, size: f64) -> Lay {
        let k = size / 66.0;
        let mut tile = Node::new(id, "tile");
        tile.fill = Some(rgba(255, 255, 255, 1.0));
        tile.radius = SHARE_APP_RADIUS;
        tile.stroke = Some((rgba(220, 220, 222, 1.0), 1.0));
        tile.shadow = Some(tile_shadow());
        tile.inspect = false;
        let d = 53.0 * k;
        let (c, r) = (size / 2.0, d / 2.0);
        // Needle: two triangles along the dial's diagonal (red towards the top right).
        let (ax, ay) = (0.62_f64, -0.78_f64); // unit vector to the top-right tip
        let (px, py) = (-ay, ax);
        let tip = 0.78 * r;
        let half = 0.12 * r;
        let tri = |sx: f64| -> serde_json::Value {
            let pts = [(c + sx * ax * tip, c + sx * ay * tip), (c + px * half, c + py * half), (c - px * half, c - py * half)];
            serde_json::json!([{"m": [pts[0].0, pts[0].1]}, {"l": [pts[1].0, pts[1].1]}, {"l": [pts[2].0, pts[2].1]}, "z"])
        };
        let json = serde_json::json!({"kind": "zstack", "id": format!("{id}#dial"), "children": [
            {"kind": "shape", "id": format!("{id}#face"), "shape": "circle", "modifiers": [{"frame": {"width": d, "height": d, "alignment": "center"}}],
             "fill": {"gradient": {"kind": "linear", "start": [0.5, 0], "end": [0.5, 1], "stops": [
                {"location": 0, "color": {"r": 0.2, "g": 0.8, "b": 1.0, "a": 1}}, {"location": 1, "color": {"r": 0.0, "g": 0.42, "b": 0.95, "a": 1}}]}}},
            {"kind": "shape", "id": format!("{id}#red"), "shape": "path", "path": tri(1.0), "modifiers": [{"frame": {"width": size, "height": size, "alignment": "center"}}], "fill": {"r": 1.0, "g": 0.23, "b": 0.19, "a": 1}},
            {"kind": "shape", "id": format!("{id}#white"), "shape": "path", "path": tri(-1.0), "modifiers": [{"frame": {"width": size, "height": size, "alignment": "center"}}], "fill": {"r": 1, "g": 1, "b": 1, "a": 1}}]});
        let mut kids = vec![];
        let v: ViewNode = serde_json::from_value(json).expect("Safari icon view");
        let dial = self.view(&v, Prop { w: Some(size), h: Some(size) }, Env::new());
        kids.push(((size - dial.size.w) / 2.0, (size - dial.size.h) / 2.0, dial));
        Lay::new(Size { w: size, h: size }, Some(tile), kids)
    }

    /// A rounded tile with a symbol centred on it.
    #[allow(clippy::too_many_arguments)]
    fn share_tile(&mut self, id: &str, size: f64, radius: f64, fill: Color, stroke: Option<Color>, symbol: &str, pt: f64, weight: u16, fg: Color) -> Lay {
        let mut n = Node::new(id, "tile");
        n.fill = Some(fill);
        n.radius = radius;
        n.stroke = stroke.map(|c| (c, 1.0));
        n.inspect = false;
        if radius < size / 2.0 {
            n.shadow = Some(tile_shadow());
        }
        if symbol.is_empty() {
            return Lay::new(Size { w: size, h: size }, Some(n), vec![]);
        }
        let s = self.symbol_lay(format!("{id}#symbol"), symbol, pt, weight, fg);
        let (sw, sh) = (s.size.w, s.size.h);
        Lay::new(Size { w: size, h: size }, Some(n), vec![((size - sw) / 2.0, (size - sh) / 2.0, s)])
    }

    /// The Reminders app icon: a white tile with three coloured rings and lines.
    fn reminders_icon(&mut self, id: &str, size: f64) -> Lay {
        let k = size / 66.0;
        let mut tile = Node::new(id, "tile");
        tile.fill = Some(rgba(255, 255, 255, 1.0));
        tile.radius = SHARE_APP_RADIUS;
        tile.stroke = Some((rgba(220, 220, 222, 1.0), 1.0));
        tile.inspect = false;
        tile.shadow = Some(tile_shadow());
        let mut kids = vec![];
        // Measured on the 66 pt icon: rings 11.3 across at x 12.3, rows 16.67 apart from y 16.3, lines from x 24 to 60.
        for (i, c) in [(0, 136, 255), (255, 56, 60), (255, 141, 40)].into_iter().enumerate() {
            let cy = (16.3 + i as f64 * 16.67) * k;
            let (d, hd) = (11.3 * k, 5.6 * k);
            let mut dot = Node::new(format!("{id}#dot{i}"), "dot");
            dot.fill = Some(rgba(c.0, c.1, c.2, 1.0));
            dot.radius = PILL;
            dot.inspect = false;
            let mut hole = Node::new(format!("{id}#hole{i}"), "dot");
            hole.fill = Some(rgba(255, 255, 255, 0.75));
            hole.radius = PILL;
            hole.inspect = false;
            kids.push((12.3 * k - d / 2.0, cy - d / 2.0, Lay::new(Size { w: d, h: d }, Some(dot), vec![((d - hd) / 2.0, (d - hd) / 2.0, Lay::new(Size { w: hd, h: hd }, Some(hole), vec![]))])));
            let mut line = Node::new(format!("{id}#line{i}"), "line");
            line.fill = Some(rgba(190, 190, 194, 1.0));
            line.inspect = false;
            kids.push((24.0 * k, cy - 0.65, Lay::new(Size { w: 36.0 * k, h: 1.3 }, Some(line), vec![])));
        }
        Lay::new(Size { w: size, h: size }, Some(tile), kids)
    }
}

/// The soft shadow under the white app tiles.
fn tile_shadow() -> crate::Shadow {
    crate::Shadow { color: Color::rgba(0.0, 0.0, 0.0, SHARE_TILE_SHADOW.0), radius: SHARE_TILE_SHADOW.1, dx: 0.0, dy: SHARE_TILE_SHADOW.2 }
}

/// Cuts a one-line text to `w` (the title never wraps).
fn clip_w(mut l: Lay, w: f64) -> Lay {
    if l.size.w > w {
        l.size.w = w;
        if let Some(n) = l.node.as_mut() {
            n.clip_radius = Some(0.0);
        }
    }
    l
}
