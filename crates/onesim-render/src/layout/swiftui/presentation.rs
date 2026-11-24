use super::*;

/// Scales what a node draws (radii, strokes, shadows, type, images, symbols, scroll state) by `k`; not its bounds.
pub(super) fn scale_node(n: &mut Node, k: f64) {
    n.radius = scaled_radius(n.radius, k);
    n.clip_radius = n.clip_radius.map(|x| scaled_radius(x, k));
    n.stroke = n.stroke.map(|(c, w)| (c, w * k));
    if let Some(sh) = n.shadow.as_mut() {
        (sh.radius, sh.dx, sh.dy) = (sh.radius * k, sh.dx * k, sh.dy * k);
    }
    if let Some(t) = n.text.as_mut() {
        let kf = k as f32;
        (t.font.size, t.font.pt, t.font.line_height, t.font.pitch) = (t.font.size * kf, t.font.pt * kf, t.font.line_height * kf, t.font.pitch * kf);
        t.offset_x *= kf;
        t.font.extra *= kf;
        t.font.baseline *= kf;
        for sp in t.spans.iter_mut() {
            sp.pt *= kf;
            sp.extra *= kf;
            sp.baseline *= kf;
        }
    }
    if let Some(im) = n.image.as_mut() {
        im.dest.x *= k;
        im.dest.y *= k;
        im.dest.w *= k;
        im.dest.h *= k;
    }
    if let Some(sy) = n.symbol.as_mut() {
        sy.spec.point_size *= k;
        sy.size = (sy.size.0 * k, sy.size.1 * k);
    }
    n.caret = n.caret.map(|c| c * k);
    if let Some(sc) = n.scroll.as_mut() {
        sc.offset = (sc.offset.0 * k, sc.offset.1 * k);
        sc.max = (sc.max.0 * k, sc.max.1 * k);
        sc.snap = sc.snap.map(|s| s * k);
        sc.page = sc.page.map(|p| p * k);
    }
}

/// A copy of the flattened subtree at `i` grown by `k` about its centre, for a context menu's lifted view: not
/// tappable or inspectable (the layer's menu is), ids prefixed so they never collide with the screen's.
pub(super) fn lifted_copy(nodes: &[Node], i: usize, k: f64, prefix: &str) -> Vec<Node> {
    let base = nodes.len();
    let (cx, cy) = nodes[i].bounds.center();
    let under = |mut j: usize| loop {
        if j == i {
            return true;
        }
        match nodes[j].parent {
            Some(p) if p >= i => j = p,
            _ => return false,
        }
    };
    let mut out = vec![];
    for j in (i..nodes.len()).take_while(|&j| j == i || under(j)) {
        let mut n = nodes[j].clone();
        n.id = format!("{prefix}{}", n.id);
        n.bounds = Rect { x: cx + (n.bounds.x - cx) * k, y: cy + (n.bounds.y - cy) * k, w: n.bounds.w * k, h: n.bounds.h * k };
        n.parent = if j == i { Some(0) } else { n.parent.map(|p| base + p - i) };
        n.action = None;
        n.inspect = false;
        scale_node(&mut n, k);
        out.push(n);
    }
    out
}

/// `presentation*` styling of a sheet layer (section W).
#[derive(Default)]
pub(super) struct SheetStyle<'a> {
    pub background: Option<&'a ViewNode>,
    pub corner_radius: Option<f64>,
    /// `presentationBackgroundInteraction(.enabled)`: the largest sheet height (points) at which the screen below stays live.
    pub interactive_up_through: Option<f64>,
    pub scrolls_first: bool,
}

impl Ctx<'_> {
    pub(super) fn content_popover(&mut self, id: &str, content: &ViewNode, anchor: Option<Rect>, dismiss: Option<u64>, g: &Geo) -> Lay {
        let (w, h) = (g.w, g.h);
        let env = Env { size: g.body, ..Env::new() };
        let inner = self.view(content, Prop { w: None, h: None }, env);
        let (cw, ch) = (inner.size.w, inner.size.h);
        let a = anchor.unwrap_or(Rect { x: w / 2.0, y: h / 2.0, w: 0.0, h: 0.0 });
        let (lo, hi) = (CONTENT_POPOVER_MARGIN.0, (w - cw - CONTENT_POPOVER_MARGIN.1).max(CONTENT_POPOVER_MARGIN.0));
        let trailing = a.x + a.w + CONTENT_POPOVER_TRAILING - cw;
        let x = if trailing >= lo { trailing.min((w - cw - lo).max(lo)) } else { (a.center().0 - cw / 2.0).clamp(lo, hi) };
        let below = a.y + a.h + CONTENT_POPOVER_GAP;
        let y = if below + ch <= h - g.dev_bottom { below } else { (a.y - CONTENT_POPOVER_GAP - ch).max(g.safe_top) };
        let mut card = Node::new(format!("{id}#card"), "popover");
        card.radius = CONTENT_POPOVER_RADIUS;
        card.clip_radius = Some(CONTENT_POPOVER_RADIUS);
        card.glass = Some(crate::glass::GlassSpec { blur: Some(PRESENT_BLUR), mix: Some(if self.dark { CONTENT_POPOVER_GLASS.1 } else { CONTENT_POPOVER_GLASS.0 }), shadow: Some(POPOVER_SHADOW), ..self.glass_spec(GlassVariant::Regular, None, false) });
        let scrim = Lay::new(Size { w, h }, Some(self.scrim(&format!("{id}#scrim"), (w, h), 0.0, dismiss)), vec![]);
        Lay::new(Size { w, h }, None, vec![(0.0, 0.0, scrim), (x, y, Lay::new(Size { w: cw, h: ch }, Some(card), vec![(0.0, 0.0, inner)]))])
    }

    /// `.popover` kept as a popover (`presentationCompactAdaptation(.popover)`): a glass bubble sized to its
    /// content (plus `BUBBLE_PAD_V` above and below) with an arrow whose tip touches the attachment point.
    /// `arrow_edge` is the bubble edge carrying the arrow (`top`: below the anchor); without it the bubble goes
    /// below when there is room, else above. No dim; a tap outside dismisses.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn bubble(&mut self, id: &str, content: &ViewNode, anchor: Option<Rect>, attachment: Option<&onesim_protocol::Attachment>,
                         arrow_edge: Option<&str>, background: Option<&ViewNode>, dismiss: Option<u64>, g: &Geo) -> Lay {
        let (w, h) = (g.w, g.h);
        let env = Env { size: g.body, ..Env::new() };
        let inner = self.view(content, Prop { w: Some(w - 2.0 * POPOVER_MARGIN), h: None }, env);
        let (cw, ch) = (inner.size.w, inner.size.h + 2.0 * BUBBLE_PAD_V);
        let b = anchor.unwrap_or(Rect { x: w / 2.0, y: h / 2.0, w: 0.0, h: 0.0 });
        let a = match attachment {
            Some(onesim_protocol::Attachment::Rect([x, y, rw, rh])) => Rect { x: b.x + x, y: b.y + y, w: *rw, h: *rh },
            Some(onesim_protocol::Attachment::Point([ux, uy])) => Rect { x: b.x + b.w * ux, y: b.y + b.h * uy, w: 0.0, h: 0.0 },
            None => b,
        };
        let (acx, acy) = a.center();
        #[derive(PartialEq, Clone, Copy)]
        enum Side { Below, Above, Right, Left }
        let fits_below = a.y + a.h + BUBBLE_TIP_GAP + POPOVER_ARROW_H + ch <= h - g.dev_bottom;
        let side = match arrow_edge {
            Some("top") => Side::Below,
            Some("bottom") => Side::Above,
            Some("leading") => Side::Right,
            Some("trailing") => Side::Left,
            _ if fits_below => Side::Below,
            _ => Side::Above,
        };
        let xc = |x: f64| x.clamp(POPOVER_MARGIN, (w - cw - POPOVER_MARGIN).max(POPOVER_MARGIN));
        let yc = |y: f64| y.clamp(g.safe_top, (h - g.dev_bottom - ch).max(g.safe_top));
        let (x, y, glyph, (aw, ah), (ax, ay)) = match side {
            Side::Below | Side::Above => {
                let x = xc(acx - cw / 2.0);
                let ax = (acx - POPOVER_ARROW_W / 2.0).clamp(x + BUBBLE_RADIUS, (x + cw - BUBBLE_RADIUS - POPOVER_ARROW_W).max(x + BUBBLE_RADIUS));
                if side == Side::Below {
                    let y = a.y + a.h + BUBBLE_TIP_GAP + POPOVER_ARROW_H;
                    (x, y, Glyph::ArrowUp, (POPOVER_ARROW_W, POPOVER_ARROW_H), (ax, y - POPOVER_ARROW_H + 0.5))
                } else {
                    let y = a.y - BUBBLE_TIP_GAP - POPOVER_ARROW_H - ch;
                    (x, y, Glyph::ArrowDown, (POPOVER_ARROW_W, POPOVER_ARROW_H), (ax, y + ch - 0.5))
                }
            }
            Side::Right | Side::Left => {
                let y = yc(acy - ch / 2.0);
                let ay = (acy - POPOVER_ARROW_W / 2.0).clamp(y + BUBBLE_RADIUS, (y + ch - BUBBLE_RADIUS - POPOVER_ARROW_W).max(y + BUBBLE_RADIUS));
                if side == Side::Right {
                    let x = a.x + a.w + BUBBLE_TIP_GAP + POPOVER_SIDE_ARROW;
                    (x, y, Glyph::ArrowLeft, (POPOVER_SIDE_ARROW, POPOVER_ARROW_W), (x - POPOVER_SIDE_ARROW + 0.5, ay))
                } else {
                    let x = a.x - BUBBLE_TIP_GAP - POPOVER_SIDE_ARROW - cw;
                    (x, y, Glyph::ArrowRight, (POPOVER_SIDE_ARROW, POPOVER_ARROW_W), (x + cw - 0.5, ay))
                }
            }
        };
        let mut card = Node::new(format!("{id}#card"), "popover");
        card.radius = BUBBLE_RADIUS;
        card.clip_radius = Some(BUBBLE_RADIUS);
        let mut kids = vec![];
        if let Some(bg) = background {
            let bl = self.view(bg, Prop { w: Some(cw), h: Some(ch) }, env);
            kids.push(((cw - bl.size.w) / 2.0, (ch - bl.size.h) / 2.0, bl));
        } else {
            card.glass = Some(crate::glass::GlassSpec { blur: Some(PRESENT_BLUR), mix: Some(if self.dark { MENU_GLASS.1 } else { MENU_GLASS.0 }), shadow: Some(POPOVER_SHADOW), ..self.glass_spec(GlassVariant::Regular, None, false) });
        }
        kids.push((0.0, BUBBLE_PAD_V, inner));
        let mut arrow = Node::new(format!("{id}#arrow"), "arrow");
        arrow.glyph = Some((glyph, { let c = if self.dark { POPOVER_ARROW.1 } else { POPOVER_ARROW.0 }; rgba(c.0, c.1, c.2, 1.0) }));
        arrow.inspect = false;
        let scrim = Lay::new(Size { w, h }, Some(self.scrim(&format!("{id}#scrim"), (w, h), 0.0, dismiss)), vec![]);
        Lay::new(Size { w, h }, None, vec![
            (0.0, 0.0, scrim),
            (x, y, Lay::new(Size { w: cw, h: ch }, Some(card), kids)),
            (ax, ay, Lay::new(Size { w: aw, h: ah }, Some(arrow), vec![])),
        ])
    }

    /// `.contextMenu` without a `preview`: the dim and the menu card, 16 pt under the lifted view (`lifted`, the
    /// anchor's frame grown by `CONTEXT_LIFT` about its centre), centred on it; above it when there is no room below.
    /// The caller draws the lifted copy between the two (`mod.rs`). With a `preview`, that view is laid out at
    /// its own size, centred on the anchor, and drawn on a platter instead.
    pub(super) fn context_menu(&mut self, id: &str, items: &[PickerOption], lifted: Rect, preview: Option<&ViewNode>, dismiss: Option<u64>, g: &Geo) -> (Lay, Lay, Rect) {
        let (w, h) = (g.w, g.h);
        let mut lifted = lifted;
        let mut plat = None;
        if let Some(p) = preview {
            let pl = self.view(p, Prop { w: Some(w - 2.0 * POPOVER_MARGIN), h: None }, Env { size: g.body, ..Env::new() });
            let (cx, cy) = lifted.center();
            let r = Rect { x: (cx - pl.size.w / 2.0).clamp(POPOVER_MARGIN, (w - pl.size.w - POPOVER_MARGIN).max(POPOVER_MARGIN)), y: cy - pl.size.h / 2.0, w: pl.size.w, h: pl.size.h };
            let mut n = Node::new(format!("{id}#preview"), "platter");
            n.fill = Some(self.semantic(Semantic::Background));
            n.radius = CONTEXT_PLATTER_RADIUS;
            n.clip_radius = Some(CONTEXT_PLATTER_RADIUS);
            plat = Some((r, Lay::new(pl.size, Some(n), vec![(0.0, 0.0, pl)])));
            lifted = r;
        }
        let card = self.menu_card(items, w);
        let (cw, ch) = (card.size.w, card.size.h);
        let x = (lifted.center().0 - cw / 2.0).clamp(POPOVER_MARGIN, w - cw - POPOVER_MARGIN);
        let below = lifted.y + lifted.h + CONTEXT_MENU_GAP;
        let y = if below + ch <= h - g.dev_bottom { below } else { (lifted.y - CONTEXT_MENU_GAP - ch).max(g.safe_top) };
        let dim = if self.dark { CONTEXT_DIM.1 } else { CONTEXT_DIM.0 };
        let mut under = vec![(0.0, 0.0, Lay::new(Size { w, h }, Some(self.scrim(&format!("{id}#scrim"), (w, h), dim, dismiss)), vec![]))];
        if let Some((r, l)) = plat {
            under.push((r.x, r.y, l));
        }
        (Lay::new(Size { w, h }, None, under), Lay::new(Size { w, h }, None, vec![(x, y, card)]), lifted)
    }

    /// `Grid`: column widths come from the cells. The grid stays content-sized.
    pub(super) fn scale_lay(&mut self, l: &mut Lay, k: f64) {
        let one_line = l.node.as_ref().and_then(|n| n.text.as_ref()).is_some_and(|t| l.size.h <= t.font.line_height as f64 * 1.5);
        l.size = Size { w: l.size.w * k, h: l.size.h * k };
        l.infl = (l.infl.0 * k, l.infl.1 * k, l.infl.2 * k, l.infl.3 * k);
        if let Some(n) = l.node.as_mut() {
            scale_node(n, k);
            // Smaller type tracks differently: re-measure a single line so it never wraps at its own width.
            if let Some(t) = n.text.as_ref().filter(|_| one_line) {
                let (w, _) = self.fonts.measure(&t.text, &FontSpec { align: TextAlign::Leading, ..t.font }, None, &t.spans);
                l.size.w = (w * self.scale).ceil() / self.scale;
            }
        }
        for (dx, dy, c) in l.kids.iter_mut() {
            *dx *= k;
            *dy *= k;
            self.scale_lay(c, k);
        }
    }

    pub(super) fn scrim(&self, id: &str, (w, h): (f64, f64), alpha: f32, tap: Option<u64>) -> Node {
        let mut n = Node::new(id, "scrim");
        n.action = tap;
        n.fill = Some(Color::rgba(0.0, 0.0, 0.0, alpha));
        n.bounds = Rect { x: 0.0, y: 0.0, w, h };
        n.inspect = false;
        n
    }

    /// The root view: full-bleed scrollers span the screen, anything else is centered in the safe area.
    pub(super) fn place_root(&mut self, root: &ViewNode, g: &Geo) -> Lay {
        self.place_root_in(root, g, Env::new())
    }

    /// Like `place_root`, preserving inherited environment fields (e.g. `anim`, `tint`, `content_transition`)
    /// across container boundaries such as `TabView`.
    pub(super) fn place_root_in(&mut self, root: &ViewNode, g: &Geo, base: Env) -> Lay {
        let full_bleed = matches!(root.kind, ViewKind::Navigation { .. } | ViewKind::List { .. } | ViewKind::Scroll { .. } | ViewKind::TabView { .. });
        if full_bleed {
            let env = Env { safe_top: g.safe_top, bottom_inset: g.safe_bottom, size: g.body, ..base };
            let lay = self.view(root, Prop { w: Some(g.w), h: Some(g.h) }, env);
            Lay::new(Size { w: g.w, h: g.h }, None, vec![(0.0, 0.0, lay)])
        } else {
            let sh = (g.h - g.safe_top - g.safe_bottom).max(0.0);
            let lay = self.view(root, Prop { w: Some(g.w), h: Some(sh) }, Env { safe_top: 0.0, bottom_inset: 0.0, size: g.body, ..base });
            let (x, y) = ((g.w - lay.size.w) / 2.0, g.safe_top + (sh - lay.size.h) / 2.0);
            Lay::new(Size { w: g.w, h: g.h }, None, vec![(x, y, lay)])
        }
    }

    /// `.fullScreenCover`: an opaque screen above everything (status bar aside).
    pub(super) fn cover(&mut self, content: &ViewNode, g: &Geo) -> Lay {
        let g = Geo { safe_bottom: g.dev_bottom, ..*g };
        let inner = self.place_root(content, &g);
        let mut card = Node::new("cover", "cover");
        card.fill = Some(self.semantic(Semantic::Background));
        card.inspect = false;
        let scrim = Lay::new(Size { w: g.w, h: g.h }, Some(self.scrim("cover#scrim", (g.w, g.h), 0.0, None)), vec![]);
        let cardl = Lay::new(Size { w: g.w, h: g.h }, Some(card), vec![(0.0, 0.0, inner)]);
        Lay::new(Size { w: g.w, h: g.h }, None, vec![(0.0, 0.0, scrim), (0.0, 0.0, cardl)])
    }

    /// Height of a detent (points). `max` is the large detent's height.
    pub(super) fn detent_h(d: &Detent, h: f64, max: f64) -> f64 {
        match d {
            Detent::Medium => h * SHEET_MEDIUM,
            Detent::Large => max,
            Detent::Fraction { value } => value * max,
            Detent::Height { value } => *value,
        }
        .clamp(1.0, max)
    }

    /// `.sheet`: dimmed screen, a card from its top edge to the bottom. Detents (ascending) set the
    /// resting heights; `pos` is the interaction state (which detent, or a drag in progress). Below its
    /// largest detent the card floats (inset, glass); `depth` is the number of sheets under this one.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn sheet(&mut self, id: &str, c: &ViewNode, detents: &[Detent], indicator: bool, dismiss: Option<u64>, style: &SheetStyle, pos: crate::SheetPos, depth: usize, g: &Geo) -> Lay {
        let (w, h) = (g.w, g.h);
        let large_top = g.safe_top + SHEET_TOP_GAP + depth as f64 * SHEET_STACK_STEP;
        let max = (h - large_top).max(1.0);
        let mut hs: Vec<f64> = if detents.is_empty() { vec![max] } else { detents.iter().map(|d| Self::detent_h(d, h, max)).collect() };
        hs.sort_by(f64::total_cmp);
        hs.dedup_by(|a, b| (*a - *b).abs() < 0.5);
        let tops: Vec<f64> = hs.iter().map(|x| h - x).collect();
        let detent = pos.detent.min(tops.len() - 1);
        let top = pos.top.unwrap_or(tops[detent]).clamp(large_top, h);
        let float = top > large_top + 0.5;
        let (cx, cw) = if float { (SHEET_FLOAT_INSET, w - 2.0 * SHEET_FLOAT_INSET) } else { (0.0, w) };
        let sh = (h - top - if float { SHEET_FLOAT_INSET } else { 0.0 }).max(0.0);
        let radius = style.corner_radius.unwrap_or(if float { SHEET_FLOAT_RADIUS } else { SHEET_RADIUS });
        let list = is_list(c);
        let scroller = is_scroller(c);
        let ctop = SHEET_CONTENT_TOP;
        let env = Env { size: g.body, safe_top: if scroller { ctop } else { 0.0 }, bottom_inset: if float { 0.0 } else { g.dev_bottom }, ..Env::new() };
        (self.elevated, self.floating) = (true, float);
        // Floating: lay the content out as in the large sheet (full width), then shrink it onto the card.
        let k = if float { SHEET_FLOAT_SCALE } else { 1.0 };
        let mut cl = self.view(c, Prop { w: Some(if float { w } else { cw }), h: Some(sh / k) }, env);
        if float {
            self.scale_lay(&mut cl, k);
        }
        let mut card = Node::new(id, "sheet");
        // presentationBackground: a view filling the card replaces the system background.
        let bg = style.background.map(|b| {
            let bl = self.view(b, Prop { w: Some(cw), h: Some(sh) }, Env { size: g.body, ..Env::new() });
            ((cw - bl.size.w) / 2.0, (sh - bl.size.h) / 2.0, bl)
        });
        if bg.is_some() {
        } else if float {
            card.glass = Some(crate::glass::GlassSpec { blur: Some(PRESENT_BLUR), mix: Some(if self.dark { SHEET_GLASS.1 } else { SHEET_GLASS.0 }), ..self.glass_spec(GlassVariant::Regular, None, false) });
        } else {
            card.fill = Some(self.semantic(if list { Semantic::GroupedBackground } else { Semantic::Background }));
        }
        (self.elevated, self.floating) = (false, false);
        card.radius = radius;
        card.clip_radius = Some(radius);
        // Background interaction: the screen under the sheet stays undimmed and live up through a detent.
        let live = style.interactive_up_through.is_some_and(|max_h| h - tops[detent] <= max_h + 0.5);
        card.sheet = Some(crate::SheetInfo { tops, detent, top, dismiss, scrolls_first: style.scrolls_first });
        let (dx, dy) = ((cw - cl.size.w) / 2.0, if scroller { 0.0 } else { (sh - cl.size.h) / 2.0 });
        let mut kids: Vec<(f64, f64, Lay)> = bg.into_iter().collect();
        kids.push((dx, dy, cl));
        if indicator {
            let mut gr = Node::new(format!("{id}#grabber"), "grabber");
            let c = if self.dark { GRABBER_FG.1 } else { GRABBER_FG.0 };
            gr.fill = Some(rgba(c.0, c.1, c.2, 1.0));
            gr.radius = PILL;
            gr.inspect = false;
            kids.push(((cw - GRABBER.0) / 2.0, GRABBER_Y, Lay::new(Size { w: GRABBER.0, h: GRABBER.1 }, Some(gr), vec![])));
        }
        // A full-width card runs past the screen's bottom edge so only its top corners are rounded on screen.
        let ch = if float { sh } else { sh + 2.0 * radius };
        let cardl = Lay::new(Size { w: cw, h: ch }, Some(card), kids);
        // A single dim for stacked sheets (the lower one is covered anyway).
        let dim = if depth > 0 { 0.0 } else if self.dark { SHEET_SCRIM.1 } else { SHEET_SCRIM.0 };
        if live {
            return Lay::new(Size { w, h }, None, vec![(cx, top, cardl)]);
        }
        let scrim = Lay::new(Size { w, h }, Some(self.scrim(&format!("{id}#scrim"), (w, h), dim, None)), vec![]);
        Lay::new(Size { w, h }, None, vec![(0.0, 0.0, scrim), (cx, top, cardl)])
    }

    /// The card of an alert or confirmation popover: title, message (left-aligned) and capsule buttons.
    /// Alerts put two buttons side by side (cancel first) and stack more (cancel last); a popover stacks
    /// them and has no cancel button (tapping outside cancels).
    /// `sheet` (React Native `ActionSheetIOS`): the alert card, its buttons always stacked with cancel last and plain
    /// ones in the label color.
    pub(super) fn alert_card(&mut self, kind: &'static str, title: Option<&str>, message: Option<&ViewNode>, actions: &[ViewNode], card_w: f64) -> Lay {
        let popover = kind == "popover";
        let sheet = kind == "sheet";
        let inner = card_w - 2.0 * ALERT_TEXT_PAD;
        let primary = self.semantic(Semantic::Primary);
        let mut kids: Vec<(f64, f64, Lay)> = vec![];
        let mut y = ALERT_TOP;
        let mut gap = 0.0;
        if let Some(t) = title {
            let tsz = onesim_protocol::TextStyle::Headline.size_at(self.dts) as f32;
            let mut tl = self.text_lay("alert#title", t, FontSpec::new(tsz, self.line_h(tsz), 600, TextAlign::Leading), primary, Some(inner as f32));
            if let Some(n) = tl.node.as_mut() {
                n.inspect = false;
            }
            let th = tl.size.h;
            kids.push((ALERT_TEXT_PAD, y, tl));
            y += th;
            gap = ALERT_TITLE_MESSAGE_GAP;
        }
        if let Some(m) = message {
            let fg = match (self.dark, self.ios >= IosVersion::Ios27, popover) {
                (true, true, false) => ALERT_27_DARK_MESSAGE,
                (true, true, true) => POPOVER_27_DARK_MESSAGE,
                (true, false, _) => ALERT_MESSAGE_FG.1,
                (false, _, _) if sheet => SHEET_MESSAGE_FG,
                _ => ALERT_MESSAGE_FG.0, };
            let env = Env { size: ALERT_MESSAGE_PT, align: TextAlign::Leading, fg: Some(rgba(fg.0, fg.1, fg.2, 1.0)), ..Env::new() };
            let ml = self.view(m, Prop { w: Some(inner), h: None }, env);
            y += gap;
            let mh = ml.size.h;
            kids.push((ALERT_TEXT_PAD, y, ml));
            y += mh;
        }
        y += ALERT_TEXT_BUTTON_GAP;
        let is_cancel = |b: &ViewNode| matches!(b.kind, ViewKind::Button { role: Some(onesim_protocol::ButtonRole::Cancel), .. });
        let mut acts: Vec<&ViewNode> = actions.iter().filter(|b| !(popover && is_cancel(b))).collect();
        let side_by_side = !popover && !sheet && acts.len() == 2;
        acts.sort_by_key(|b| is_cancel(b) != side_by_side); // stable: cancel first when side by side, last when stacked
        let n = acts.len().max(1);
        let bw = if side_by_side { (card_w - 2.0 * ALERT_BTN_PAD - ALERT_BTN_GAP) / 2.0 } else { card_w - 2.0 * ALERT_BTN_PAD };
        let bh = ALERT_BTN_H;
        let accent = self.semantic(Semantic::Accent);
        let dark27 = self.dark && self.ios >= IosVersion::Ios27;
        let (red27, btn27) = if popover {
            (POPOVER_27_DARK_DESTRUCTIVE, POPOVER_27_DARK_BTN)
        } else {
            (ALERT_27_DARK_DESTRUCTIVE, ALERT_27_DARK_BTN)
        };
        let red = if dark27 {
            self.pick((red27, red27))
        } else { self.color(&onesim_protocol::Color::Rgba { r: 1.0, g: 56.0 / 255.0, b: 60.0 / 255.0, a: 1.0, dark: Some(Box::new([1.0, 66.0 / 255.0, 69.0 / 255.0])) })
        };
        let fills = if popover { POPOVER_BTN_FILL } else { ALERT_BTN_FILL
        };
        let fills = if dark27 { (fills.0, btn27) } else { fills };
        for (i, b) in acts.iter().enumerate() {
            let ViewKind::Button { action, label, disabled, role, .. } = &b.kind else { continue };
            let (fg, weight) = match role {
                Some(onesim_protocol::ButtonRole::Destructive) => (red, 600),
                Some(onesim_protocol::ButtonRole::Cancel) => (primary, 600),
                None => (if popover || sheet { primary } else { accent }, 600),
            };
            // Action sheets set every button in the regular weight.
            let weight = if sheet { 400 } else { weight };
            let env = Env { fg: Some(if *disabled { self.semantic(Semantic::Tertiary) } else { fg }), base_weight: weight, align: TextAlign::Center, ..Env::new() };
            let l = self.view(label, Prop { w: Some(bw - 24.0), h: None }, env);
            let mut bn = Node::new(&b.id, "button");
            bn.role = Role::Button;
            bn.action = (!disabled).then_some(*action);
            bn.disabled = *disabled;
            bn.label = first_text(label);
            let f = if self.dark { fills.1 } else { fills.0 };
            bn.fill = Some(rgba(f.0, f.1, f.2, 1.0));
            bn.radius = PILL;
            let (lw, lh) = (l.size.w, l.size.h);
            let bl = Lay::new(Size { w: bw, h: bh }, Some(bn), vec![((bw - lw) / 2.0, (bh - lh) / 2.0, l)]);
            let (bx, by) = if side_by_side {
                (ALERT_BTN_PAD + i as f64 * (bw + ALERT_BTN_GAP), y)
            } else {
                (ALERT_BTN_PAD, y + i as f64 * (bh + ALERT_BTN_GAP))
            };
            kids.push((bx, by, bl));
        }
        let rows = if side_by_side { 1 } else { n };
        y += rows as f64 * bh + (rows - 1) as f64 * ALERT_BTN_GAP + ALERT_BTN_PAD;
        let mut card = Node::new(if popover || sheet { "dialog" } else { "alert" }, kind);
        card.label = title.map(str::to_string);
        card.radius = if popover { POPOVER_RADIUS } else { ALERT_RADIUS };
        let (mix, shadow) = if popover || sheet { (Some(POPOVER_GLASS), Some(POPOVER_SHADOW)) } else { (None, None) };
        let mix = if self.dark && self.ios >= IosVersion::Ios27 {
            Some(PRESENT_GLASS_27_DARK)
        } else {
            mix.filter(|_| !self.dark)
        };
        card.glass = Some(crate::glass::GlassSpec { blur: Some(PRESENT_BLUR), mix, shadow, ..self.glass_spec(GlassVariant::Regular, None, false) });
        Lay::new(Size { w: card_w, h: y }, Some(card), kids)
    }

    /// `.alert`: dimmed screen, a centered Liquid Glass card.
    pub(super) fn alert(&mut self, title: &str, message: Option<&ViewNode>, actions: &[ViewNode], g: &Geo) -> Lay {
        let (w, h) = (g.w, g.h);
        let cardl = self.alert_card("alert", Some(title), message, actions, ALERT_W);
        let y = cardl.size.h;
        let scrim = Lay::new(Size { w, h }, Some(self.scrim("alert#scrim", (w, h), if self.dark { ALERT_SCRIM.1 } else { ALERT_SCRIM.0 }, None)), vec![]);
        Lay::new(Size { w, h }, None, vec![(0.0, 0.0, scrim), ((w - ALERT_W) / 2.0, h / 2.0 + ALERT_DY - y / 2.0, cardl)])
    }

    /// React Native `ActionSheetIOS` (iOS 26+ on iPhone): the alert's dim and centred card with stacked buttons; a
    /// tap on the dim runs `dismiss` (the cancel button, when there is one).
    pub(super) fn action_sheet(&mut self, title: Option<&str>, message: Option<&ViewNode>, actions: &[ViewNode], dismiss: Option<u64>, g: &Geo) -> Lay {
        let (w, h) = (g.w, g.h);
        let cardl = self.alert_card("sheet", title, message, actions, ALERT_W);
        let y = cardl.size.h;
        let scrim = Lay::new(Size { w, h }, Some(self.scrim("dialog#scrim", (w, h), if self.dark { ALERT_SCRIM.1 } else { ALERT_SCRIM.0 }, dismiss)), vec![]);
        Lay::new(Size { w, h }, None, vec![(0.0, 0.0, scrim), ((w - ALERT_W) / 2.0, h / 2.0 + ALERT_DY - y / 2.0, cardl)])
    }

    /// `.confirmationDialog`: a popover pointing at `anchor` (the label of the presenting view): to its
    /// right when there is room, else above, else below. No dim; a tap outside dismisses it.
    pub(super) fn popover(&mut self, title: Option<&str>, message: Option<&ViewNode>, actions: &[ViewNode], anchor: Option<Rect>, dismiss: Option<u64>, g: &Geo) -> Lay {
        let (w, h) = (g.w, g.h);
        let cardl = self.alert_card("popover", title, message, actions, POPOVER_W);
        let ch = cardl.size.h;
        let a = anchor.unwrap_or(Rect { x: w / 2.0, y: h / 2.0, w: 0.0, h: 0.0 });
        let (acx, acy) = a.center();
        #[derive(PartialEq)]
        enum Side { Right, Above, Below }
        let side = if a.x + a.w + POPOVER_SIDE_ARROW + POPOVER_W + POPOVER_MARGIN <= w {
            Side::Right
        } else if a.y + POPOVER_TIP_INSET - POPOVER_ARROW_H - ch >= g.safe_top {
            Side::Above
        } else {
            Side::Below
        };
        let y_clamp = |y: f64| y.clamp(g.safe_top, h - g.dev_bottom - ch);
        let (x, y, glyph, (aw, ah), (ax, ay)) = match side {
            Side::Right => {
                let (x, y) = (a.x + a.w + POPOVER_SIDE_ARROW, y_clamp(acy - ch / 2.0));
                (x, y, Glyph::ArrowLeft, (POPOVER_SIDE_ARROW, POPOVER_ARROW_W), (x - POPOVER_SIDE_ARROW + 0.5, (acy - POPOVER_ARROW_W / 2.0).clamp(y + POPOVER_RADIUS, y + ch - POPOVER_RADIUS - POPOVER_ARROW_W)))
            }
            _ => {
                let x = (acx - POPOVER_W / 2.0).clamp(POPOVER_MARGIN, w - POPOVER_W - POPOVER_MARGIN);
                let ax = (acx - POPOVER_ARROW_W / 2.0).clamp(x + POPOVER_RADIUS, x + POPOVER_W - POPOVER_RADIUS - POPOVER_ARROW_W);
                if side == Side::Above {
                    let y = a.y + POPOVER_TIP_INSET - POPOVER_ARROW_H - ch;
                    (x, y, Glyph::ArrowDown, (POPOVER_ARROW_W, POPOVER_ARROW_H), (ax, y + ch - 0.5))
                } else {
                    let y = a.y + a.h - POPOVER_TIP_INSET + POPOVER_ARROW_H;
                    (x, y, Glyph::ArrowUp, (POPOVER_ARROW_W, POPOVER_ARROW_H), (ax, y - POPOVER_ARROW_H + 0.5))
                }
            }
        };
        let mut arrow = Node::new("dialog#arrow", "arrow");
        arrow.glyph = Some((glyph, { let c = if self.dark { POPOVER_ARROW.1 } else { POPOVER_ARROW.0 }; rgba(c.0, c.1, c.2, 1.0) }));
        arrow.inspect = false;
        let arrowl = Lay::new(Size { w: aw, h: ah }, Some(arrow), vec![]);
        let scrim = Lay::new(Size { w, h }, Some(self.scrim("dialog#scrim", (w, h), 0.0, dismiss)), vec![]);
        Lay::new(Size { w, h }, None, vec![(0.0, 0.0, scrim), (x, y, cardl), (ax, ay, arrowl)])
    }

}
