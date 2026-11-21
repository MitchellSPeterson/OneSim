//! `ShareLink`'s share sheet (protocol section U), as the Simulator shows it for text: a floating glass card with
//! the item (a text tile and its title), an app row (Reminders, More) and an action row (Copy, Save to Files).
use super::*;

impl Ctx<'_> {
    /// `copy` runs on Copy; every other cell, a tap outside and a swipe down run `dismiss`.
    pub(super) fn share_sheet(&mut self, id: &str, title: &str, copy: Option<u64>, dismiss: Option<u64>, g: &Geo) -> Lay {
        let (w, h) = (g.w, g.h);
        let m = crate::look::look(self.ios).share;
        let (cw, ch) = (w - 2.0 * m.inset.0, m.card_h);
        let top = h - m.inset.1 - ch;
        let primary = self.semantic(Semantic::Primary);
        let secondary = self.semantic(Semantic::Secondary);
        let tile_fill = self.pick(SHARE_TILE);
        let mut kids: Vec<(f64, f64, Lay)> = vec![];
        // Header: the item's tile and title.
        kids.push((m.tile.0, m.tile.1, self.share_tile(&format!("{id}#tile"), SHARE_TILE_SIZE, SHARE_TILE_RADIUS, tile_fill, None, "character.cursor.ibeam", SHARE_TILE_GLYPH, 300, secondary)));
        let tsz = SHARE_TITLE_PT;
        let tl = self.text_lay(&format!("{id}#title"), title, FontSpec::new(tsz, self.line_h(tsz), 400, TextAlign::Leading), secondary, None);
        let tl = clip_w(tl, cw - m.title.0 - m.tile.0);
        kids.push((m.title.0, m.title.1, tl));
        for (i, y) in [(0, m.header_h), (1, m.sep.0)] {
            let mut sep = Node::new(format!("{id}#sep{i}"), "separator");
            sep.fill = Some(self.semantic(Semantic::Separator));
            sep.inspect = false;
            kids.push((m.sep.1, y, Lay::new(Size { w: cw - 2.0 * m.sep.1, h: 1.0 / self.scale.max(1.0) }, Some(sep), vec![])));
        }
        // App row, then action row: one cell per activity (icon, label under it).
        let (isz, ix, iy) = m.app_icon;
        let cells: [(&str, &str, bool); 4] = [("Reminders", "", true), ("More", "ellipsis", true), ("Copy", "doc.on.doc", false), ("Save to Files", "folder", false)];
        for (k, (label, symbol, app)) in cells.into_iter().enumerate() {
            let col = (k % 2) as f64;
            let (cell_y, label_y) = if app { (m.header_h, m.label_y.0) } else { (m.sep.0 + 1.0, m.label_y.1) };
            let cx = m.cell.0 + col * m.cell.1;
            let icon = if app && symbol.is_empty() {
                self.reminders_icon(&format!("{id}#cell{k}#icon"), isz)
            } else if app {
                self.share_tile(&format!("{id}#cell{k}#icon"), isz, SHARE_APP_RADIUS, tile_fill, None, symbol, SHARE_MORE_GLYPH, 700, self.pick(SHARE_MORE_FG))
            } else {
                self.share_tile(&format!("{id}#cell{k}#icon"), isz, isz / 2.0, self.pick(SHARE_ACTION_FILL), None, symbol, SHARE_ACTION_GLYPH, 400, primary)
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
            b.action = if label == "Copy" { copy } else { dismiss };
            let cell_h = if app { m.sep.0 - m.header_h } else { ch - m.sep.0 - 1.0 };
            kids.push((cx, cell_y, Lay::new(Size { w: m.cell.2, h: cell_h }, Some(b), vec![(ix, icon_y, icon), ((m.cell.2 - lw) / 2.0, label_y, ll)])));
        }
        let mut card = Node::new(id, "sheet");
        card.radius = SHARE_RADIUS;
        card.clip_radius = Some(SHARE_RADIUS);
        card.glass = Some(crate::glass::GlassSpec { blur: Some(SHARE_BLUR), mix: Some(if self.dark { SHARE_GLASS.1 } else { SHARE_GLASS.0 }), shadow: None, ..self.glass_spec(GlassVariant::Regular, None, false) });
        // Swipe down dismisses it like a sheet with one detent.
        card.sheet = Some(crate::SheetInfo { tops: vec![top], detent: 0, top, dismiss, scrolls_first: false });
        let scrim = Lay::new(Size { w, h }, Some(self.scrim(&format!("{id}#scrim"), (w, h), 0.0, dismiss)), vec![]);
        Lay::new(Size { w, h }, None, vec![(0.0, 0.0, scrim), (m.inset.0, top, Lay::new(Size { w: cw, h: ch }, Some(card), kids))])
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
