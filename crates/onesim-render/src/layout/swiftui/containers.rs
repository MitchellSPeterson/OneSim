use super::*;

struct PinHit {
    path: Vec<usize>,
    natural: f64,
    h: f64,
}

fn find_pins(l: &Lay, main: f64, path: &mut Vec<usize>, vertical: bool, pins: &mut Vec<PinHit>) {
    if !path.is_empty() && l.node.as_ref().is_some_and(|n| n.pinned.as_deref() == Some("header")) {
        let extent = if vertical { l.size.h } else { l.size.w };
        pins.push(PinHit { path: path.clone(), natural: main, h: extent });
    }
    for (i, (dx, dy, k)) in l.kids.iter().enumerate() {
        path.push(i);
        find_pins(k, main + if vertical { *dy } else { *dx }, path, vertical, pins);
        path.pop();
    }
}

/// Index in `path` whose parent lay has a scene node: the stack-child offset, including a
/// node-less frame sitting above the pinned node.
fn place_depth(root: &Lay, path: &[usize]) -> usize {
    let mut lay = root;
    let mut place = 0;
    for (depth, &i) in path.iter().enumerate() {
        if lay.node.is_some() {
            place = depth;
        }
        if depth + 1 == path.len() {
            break;
        }
        lay = &lay.kids[i].2;
    }
    place
}

fn add_main(l: &mut Lay, path: &[usize], at: usize, delta: f64, vertical: bool) {
    let Some(&i) = path.first() else { return };
    let Some(slot) = l.kids.get_mut(i) else { return };
    if at == 0 {
        if vertical { slot.1 += delta; } else { slot.0 += delta; }
    } else {
        add_main(&mut slot.2, &path[1..], at - 1, delta, vertical);
    }
}

/// Sticky section headers on the scroll's main axis. Content position becomes
/// `max(natural, offset)` but not past the next header.
fn pin_headers(root: &mut Lay, offset: f64, vertical: bool) {
    let mut pins = Vec::new();
    let mut path = Vec::new();
    find_pins(root, 0.0, &mut path, vertical, &mut pins);
    if pins.is_empty() {
        return;
    }
    pins.sort_by(|a, b| a.natural.total_cmp(&b.natural));
    let targets: Vec<f64> = pins
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let mut t = p.natural.max(offset);
            if let Some(next) = pins.get(i + 1) {
                t = t.min(next.natural - p.h);
            }
            t
        })
        .collect();
    for (p, target) in pins.iter().zip(targets) {
        let delta = target - p.natural;
        if delta.abs() < 1e-9 {
            continue;
        }
        let at = place_depth(root, &p.path);
        add_main(root, &p.path, at, delta, vertical);
    }
}

struct TrackSlot {
    w: f64,
    gap: f64,
    align: Option<Alignment>,
}

fn track_gap(t: &GridTrack) -> f64 {
    t.spacing.unwrap_or(DEFAULT_SPACING)
}

fn track_min(t: &GridTrack) -> f64 {
    t.min.filter(|v| v.is_finite()).unwrap_or(10.0)
}

fn track_max(t: &GridTrack) -> f64 {
    match t.max {
        Some(m) if m.is_finite() => m,
        _ => INF,
    }
}

fn expand_adaptive(t: &GridTrack, available: f64) -> Vec<TrackSlot> {
    let g = track_gap(t);
    let min = track_min(t);
    let denom = min + g;
    let n = if denom <= 0.0 { 1 } else { ((available + g) / denom).floor().max(1.0) as usize };
    let n = n.max(1);
    let w = (available - g * (n as f64 - 1.0)) / n as f64;
    let w = w.max(0.0);
    (0..n)
        .map(|i| TrackSlot { w, gap: if i + 1 == n { 0.0 } else { g }, align: t.alignment })
        .collect()
}

fn resolve_tracks(tracks: &[GridTrack], available: f64) -> Vec<TrackSlot> {
    if tracks.len() == 1 && tracks[0].size == TrackSize::Adaptive {
        return expand_adaptive(&tracks[0], available);
    }
    let mut slots: Vec<TrackSlot> = Vec::with_capacity(tracks.len());
    let mut flex_at = Vec::new();
    let mut fixed_sum = 0.0;
    for (i, t) in tracks.iter().enumerate() {
        let gap = if i + 1 == tracks.len() { 0.0 } else { track_gap(t) };
        match t.size {
            TrackSize::Fixed => {
                let w = t.value.unwrap_or(10.0);
                fixed_sum += w;
                slots.push(TrackSlot { w, gap, align: t.alignment });
            }
            TrackSize::Flexible | TrackSize::Adaptive => {
                flex_at.push(slots.len());
                slots.push(TrackSlot { w: 0.0, gap, align: t.alignment });
            }
        }
    }
    let gap_sum: f64 = slots.iter().map(|s| s.gap).sum();
    if !flex_at.is_empty() {
        let share = ((available - fixed_sum - gap_sum) / flex_at.len() as f64).max(0.0);
        for i in flex_at {
            let t = &tracks[i];
            let min = track_min(t);
            let max = track_max(t).max(min);
            slots[i].w = share.clamp(min, max);
        }
    }
    slots
}


impl Ctx<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn grid(&mut self, v: &ViewNode, alignment: Alignment, hs: Option<f64>, vs: Option<f64>, rows: &[onesim_protocol::GridLine], _p: Prop, env: Env) -> Lay {
        let hgap = hs.unwrap_or(DEFAULT_SPACING);
        let vgap = vs.unwrap_or(DEFAULT_SPACING);
        let spans: Vec<Vec<usize>> = rows.iter().map(|r| r.cells.iter().map(cell_columns).collect()).collect();
        let cols = spans.iter().map(|r| r.iter().sum::<usize>()).max().unwrap_or(0);
        let mut ideals = Vec::with_capacity(rows.len());
        for row in rows {
            let mut ws = Vec::with_capacity(row.cells.len());
            for cell in &row.cells {
                ws.push(self.view(cell, Prop { w: None, h: None }, env).size.w);
            }
            ideals.push(ws);
        }
        let mut col_w = vec![0.0_f64; cols];
        for (ri, row) in rows.iter().enumerate() {
            let mut col = 0;
            for (ci, _) in row.cells.iter().enumerate() {
                let span = spans[ri][ci];
                if span == 1 && col < cols {
                    col_w[col] = col_w[col].max(ideals[ri][ci]);
                }
                col += span;
            }
        }
        for _ in 0..4 {
            let mut changed = false;
            for (ri, row) in rows.iter().enumerate() {
                let mut col = 0;
                for (ci, _) in row.cells.iter().enumerate() {
                    let span = spans[ri][ci];
                    if span > 1 && col + span <= cols {
                        let sum: f64 = col_w[col..col + span].iter().sum();
                        let gaps = hgap * (span - 1) as f64;
                        let need = ideals[ri][ci];
                        if need > sum + gaps + 1e-6 {
                            let add = (need - sum - gaps) / span as f64;
                            for w in &mut col_w[col..col + span] {
                                *w += add;
                            }
                            changed = true;
                        }
                    }
                    col += span;
                }
            }
            if !changed {
                break;
            }
        }
        let mut col_x = vec![0.0; cols];
        let mut x = 0.0;
        for i in 0..cols {
            col_x[i] = x;
            x += col_w[i];
            if i + 1 != cols {
                x += hgap;
            }
        }
        let grid_w = x;
        let mut y = 0.0;
        let mut kids = Vec::new();
        for (ri, row) in rows.iter().enumerate() {
            let align_v = row.alignment.unwrap_or(alignment);
            let mut laid_row = Vec::new();
            let mut col = 0;
            let mut rh = 0.0_f64;
            for (ci, cell) in row.cells.iter().enumerate() {
                let span = spans[ri][ci];
                let slot = slot_width(&col_w, col, span, hgap);
                let laid = self.view(cell, Prop { w: Some(slot), h: None }, env);
                rh = rh.max(laid.size.h);
                laid_row.push((col, slot, laid));
                col += span;
            }
            for (col, slot, laid) in laid_row {
                let dx = col_x.get(col).copied().unwrap_or(0.0) + (slot - laid.size.w) * h_frac(alignment);
                let dy = y + (rh - laid.size.h) * v_frac(align_v);
                kids.push((dx, dy, laid));
            }
            y += rh;
            if ri + 1 != rows.len() {
                y += vgap;
            }
        }
        Lay::new(Size { w: grid_w, h: y }, Some(Node::new(&v.id, "grid")), kids)
    }

    /// `LazyVGrid` / `LazyHGrid`. Tracks run along the cross axis; `spacing` separates lines.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn lazy_grid(&mut self, v: &ViewNode, axis: Axis, alignment: Alignment, spacing: Option<f64>, tracks: &[GridTrack], children: &[ViewNode], p: Prop, env: Env) -> Lay {
        let vertical = axis == Axis::Vertical;
        let cross_prop = if vertical { p.w } else { p.h };
        let available = cross_prop.filter(|x| x.is_finite()).unwrap_or(10.0);
        let slots = resolve_tracks(tracks, available);
        let n = slots.len();
        if n == 0 {
            return Lay::new(Size::default(), Some(Node::new(&v.id, "lazygrid")), vec![]);
        }
        let line_gap = spacing.unwrap_or(DEFAULT_SPACING);
        let mut items = Vec::with_capacity(children.len());
        for (i, child) in children.iter().enumerate() {
            let col = i % n;
            let line = i / n;
            let prop = if vertical { Prop { w: Some(slots[col].w), h: None } } else { Prop { w: None, h: Some(slots[col].w) } };
            items.push((col, line, self.view(child, prop, env)));
        }
        let nlines = items.iter().map(|(_, line, _)| *line + 1).max().unwrap_or(0);
        let mut line_main = vec![0.0_f64; nlines];
        for (_, line, laid) in &items {
            let main = if vertical { laid.size.h } else { laid.size.w };
            line_main[*line] = line_main[*line].max(main);
        }
        let mut cross_at = vec![0.0; n];
        let mut acc = 0.0;
        for (i, slot) in slots.iter().enumerate() {
            cross_at[i] = acc;
            acc += slot.w + slot.gap;
        }
        let cross_total = acc;
        let mut line_at = vec![0.0; nlines];
        let mut acc = 0.0;
        for i in 0..nlines {
            line_at[i] = acc;
            acc += line_main[i];
            if i + 1 != nlines {
                acc += line_gap;
            }
        }
        let main_total = acc;
        let kids = items
            .into_iter()
            .map(|(col, line, laid)| {
                let align = slots[col].align.unwrap_or(alignment);
                if vertical {
                    let dx = cross_at[col] + (slots[col].w - laid.size.w) * h_frac(align);
                    let dy = line_at[line] + (line_main[line] - laid.size.h) * v_frac(align);
                    (dx, dy, laid)
                } else {
                    let dx = line_at[line] + (line_main[line] - laid.size.w) * h_frac(align);
                    let dy = cross_at[col] + (slots[col].w - laid.size.h) * v_frac(align);
                    (dx, dy, laid)
                }
            })
            .collect();
        let size = if vertical { Size { w: cross_total, h: main_total } } else { Size { w: main_total, h: cross_total } };
        Lay::new(size, Some(Node::new(&v.id, "lazygrid")), kids)
    }

    /// `GeometryReader`: greedy on each finite parent axis, otherwise the guest's proposal.
    pub(super) fn geometry(&mut self, v: &ViewNode, proposal: [f64; 2], child: &ViewNode, p: Prop, env: Env) -> Lay {
        let axis = |prop: Option<f64>, guest: f64| prop.filter(|x| x.is_finite()).unwrap_or(guest);
        let (w, h) = (axis(p.w, proposal[0]), axis(p.h, proposal[1]));
        let child = self.view(child, Prop { w: Some(w), h: Some(h) }, env);
        let mut n = Node::new(&v.id, "geometry");
        n.proposal = Some(proposal);
        Lay::new(Size { w, h }, Some(n), vec![(0.0, 0.0, child)])
    }

    /// `ViewThatFits`: the first child whose ideal size fits the checked axes, else the last.
    /// The probe is unconstrained on those axes, so wrapping text is not treated as fitting.
    /// The chosen child is then laid out with the real proposal.
    pub(super) fn view_that_fits(&mut self, v: &ViewNode, axes: FitAxes, children: &[ViewNode], p: Prop, env: Env) -> Lay {
        let fits = |size: Size| match axes {
            FitAxes::Both => fits_axis(size.w, p.w) && fits_axis(size.h, p.h),
            FitAxes::Horizontal => fits_axis(size.w, p.w),
            FitAxes::Vertical => fits_axis(size.h, p.h),
        };
        let probe = match axes {
            FitAxes::Both => Prop { w: None, h: None },
            FitAxes::Horizontal => Prop { w: None, h: p.h },
            FitAxes::Vertical => Prop { w: p.w, h: None },
        };
        let mut index = None;
        for (i, c) in children.iter().enumerate() {
            let laid = self.view(c, probe, env);
            if fits(laid.size) || i + 1 == children.len() {
                index = Some(i);
                break;
            }
        }
        let inner = match index {
            Some(i) => self.view(&children[i], p, env),
            None => Lay::new(Size::default(), None, vec![]),
        };
        let size = inner.size;
        Lay::new(size, Some(Node::new(&v.id, "viewThatFits")), vec![(0.0, 0.0, inner)])
    }

    pub(super) fn scroll(&mut self, v: &ViewNode, vertical: bool, child: &ViewNode, p: Prop, env: Env) -> Lay {
        let (top, bottom) = (env.safe_top, env.bottom_inset);
        let margins = content_margins(&v.modifiers);
        let child_env = Env { axis: None, safe_top: 0.0, bottom_inset: 0.0, ..env };
        let cp = if vertical {
            Prop { w: p.w.map(|w| (w - margins.leading - margins.trailing).max(0.0)), h: None }
        } else {
            Prop { w: None, h: p.h.map(|h| (h - margins.top - margins.bottom).max(0.0)) }
        };
        let cache = self.list_cache.take();
        let mut c = self.view(child, cp, child_env);
        self.list_cache = cache;
        let fin = |x: Option<f64>| x.filter(|x| x.is_finite());
        let (cw, ch) = if vertical {
            (c.size.w, c.size.h + margins.top + margins.bottom + top + bottom)
        } else {
            (c.size.w + margins.leading + margins.trailing, c.size.h)
        };
        let w = fin(p.w).unwrap_or(cw);
        let h = fin(p.h).unwrap_or(ch);
        let max = ((cw - w).max(0.0), (ch - h).max(0.0));
        let off = self.scroll.get(&v.id).copied().unwrap_or((0.0, 0.0));
        let off = if vertical { (0.0, self.clamp_offset(off.1, max.1, h)) } else { (self.clamp_offset(off.0, max.0, w), 0.0) };
        pin_headers(&mut c, if vertical { off.1 } else { off.0 }, vertical);
        let mut page = None;
        let mut aligned = false;
        for m in &v.modifiers {
            if let Modifier::ScrollTargetBehavior(behavior) = m {
                match behavior {
                    ScrollSnap::Paging => {
                        page = Some(if vertical { h } else { w });
                        aligned = false;
                    }
                    ScrollSnap::ViewAligned => {
                        aligned = true;
                        page = None;
                    }
                }
            }
        }
        let mut n = Node::new(&v.id, "scroll");
        n.clip_radius = Some(0.0);
        n.scroll = Some(ScrollInfo { vertical, offset: off, max, snap: None, content: None, page, aligned });
        if v.modifiers.iter().any(|m| matches!(m, Modifier::ScrollPosition)) {
            n.value = Some("position".to_string());
        }
        n.inspect = true;
        // No margins: keep the old cross-axis placement (centered when vertical).
        let (x0, y0) = if vertical {
            let cx = if margins.leading == 0.0 && margins.trailing == 0.0 { (w - c.size.w) / 2.0 } else { margins.leading };
            (cx, top + margins.top)
        } else {
            let cy = if margins.top == 0.0 && margins.bottom == 0.0 { top } else { top + margins.top };
            (margins.leading, cy)
        };
        Lay::new(Size { w, h }, Some(n), vec![(x0 - off.0, y0 - off.1, c)])
    }

    pub(super) fn list(&mut self, v: &ViewNode, style: ListStyle, sections: &[Section], p: Prop, env: Env) -> Lay {
        let w = p.w.filter(|w| w.is_finite()).unwrap_or(402.0);
        let fin_h = p.h.filter(|h| h.is_finite());
        let key = (self.list_cache.is_some() && !env.in_row && w > 0.0 && fin_h.is_some()
            && env.anim.is_none() && env.content_transition == ContentTransitionKind::Identity).then(|| ListLayoutKey {
            node: v as *const ViewNode as usize,
            width: w,
            env,
            scale: self.scale,
            margin: self.margin,
            row_pad: self.row_pad,
            dark: self.dark,
            dynamic_type: self.dts,
            ios: self.ios,
            system_fonts: self.fonts.is_sf(),
            under_inline_bar: self.under_inline_bar,
        });
        let cached = key.as_ref().and_then(|key| self.list_cache.as_deref()?.list.as_ref().filter(|(cached_key, _)| cached_key == key));
        let content: Arc<ListContent> = if let Some((_, content)) = cached {
            Arc::clone(content)
        } else {
            let content = Arc::new(self.list_content(v, style, sections, w, env));
            if let Some((key, cache)) = key.zip(self.list_cache.as_deref_mut()) {
                // A sibling list follows the normal layout path while this tree owns the cache.
                if cache.list.as_ref().is_none_or(|(cached_key, _)| cached_key.node == key.node) && cacheable_list(v, sections) {
                    cache.list = Some((key, Arc::clone(&content)));
                }
            }
            content
        };
        let ch = content.bottom + env.bottom_inset;
        let h = fin_h.unwrap_or(ch);
        let max = (0.0, (ch - h).max(0.0));
        let off = self.clamp_offset(self.scroll.get(&v.id).map_or(0.0, |o| o.1), max.1, h);
        let mut n = Node::new(&v.id, "list");
        n.fill = (!self.floating).then(|| self.semantic(if on_grouped_background(style) { Semantic::GroupedBackground } else { Semantic::Background }));
        n.clip_radius = Some(0.0);
        n.scroll = Some(ScrollInfo { vertical: true, offset: (0.0, off), max, snap: None, content: None, page: None, aligned: false });
        let mut lay = Lay::new(Size { w, h }, Some(n), vec![]);
        lay.shared_kids = Some((-off, content));
        lay
    }

    pub(super) fn list_content(&mut self, v: &ViewNode, style: ListStyle, sections: &[Section], w: f64, env: Env) -> ListContent {
        // Inset grouped and sidebar: rounded cards with a margin. Grouped: full-width white sections with hairlines.
        // Plain and inset: rows on the background, separators under every row, sections a gap apart.
        // Only this list sits under the title; lists nested in its rows do not.
        let under_bar = std::mem::take(&mut self.under_inline_bar);
        let content = self.list_sections(v, style, sections, w, env, under_bar);
        self.under_inline_bar = under_bar;
        content
    }

    fn list_sections(&mut self, v: &ViewNode, style: ListStyle, sections: &[Section], w: f64, env: Env, under_bar: bool) -> ListContent {
        let card = matches!(style, ListStyle::InsetGrouped | ListStyle::Sidebar);
        let grouped = style == ListStyle::Grouped;
        let plain_like = matches!(style, ListStyle::Plain | ListStyle::Inset);
        let margin = if card { self.margin } else { 0.0 };
        let row_pad = if card { self.row_pad } else { self.margin };
        let gw = w - 2.0 * margin;
        let inner_w = gw - 2.0 * row_pad;
        let row_env = Env { axis: None, safe_top: 0.0, bottom_inset: 0.0, fg: None, in_row: true, ..env };
        let sec_col = if self.floating { let c = if self.dark { FLOAT_CARD.1 } else { FLOAT_CARD.0 }; rgba(c.0, c.1, c.2, 1.0) } else { self.semantic(Semantic::SecondaryGroupedBackground) };
        let sep_col = if self.floating { let c = if self.dark { FLOAT_SEP.1 } else { FLOAT_SEP.0 }; rgba(c.0, c.1, c.2, 1.0) } else { self.semantic(Semantic::Separator) };
        let secondary = self.semantic(Semantic::Secondary);
        let mut y = env.safe_top;
        let mut kids: Vec<(f64, f64, Lay)> = vec![];
        let line = |id: String, w: f64| {
            let mut sp = Node::new(id, "separator");
            sp.fill = Some(sep_col);
            sp.inspect = false;
            Lay::new(Size { w: w.max(0.0), h: 1.0 }, Some(sp), vec![])
        };
        for (si, sec) in sections.iter().enumerate() {
            let sid = format!("{}#s{si}", v.id);
            if plain_like {
                y += PLAIN_SECTION_GAP;
            }
            let has_header = sec.header.is_some() || sec.header_view.is_some();
            if has_header {
                let hs = onesim_protocol::TextStyle::Headline.size_at(self.dts) as f32;
                let mut t = if let Some(h) = &sec.header {
                    let spec = FontSpec::new(hs, self.line_h(hs), 600, TextAlign::Leading);
                    self.text_lay(&format!("{sid}#header"), h, spec, secondary, Some(inner_w as f32))
                } else {
                    let hv = sec.header_view.as_deref().expect("header view");
                    let henv = Env { fg: Some(secondary), size: hs, base_weight: 600, in_row: false, ..row_env };
                    self.view(hv, Prop { w: Some(inner_w), h: None }, henv)
                };
                if let Some(n) = t.node.as_mut() {
                    if n.text.is_some() {
                        n.role = Role::Text;
                    }
                }
                let hh = t.size.h + HEADER_PAD;
                let ty = y + (hh - t.size.h) / 2.0;
                if let Some(expanded) = sec.expanded {
                    // A collapsible (sidebar) header: the whole header toggles; a chevron at the trailing edge.
                    let name = if expanded { "chevron.down" } else { "chevron.right" };
                    let c = self.symbol_lay(format!("{sid}#chevron"), name, SECTION_CHEVRON.0, SECTION_CHEVRON.1, self.semantic(Semantic::Accent));
                    let (cw, ch) = (c.size.w, c.size.h);
                    let mut b = Node::new(format!("{sid}#toggle"), "button");
                    b.role = Role::Button;
                    b.action = sec.toggle;
                    b.label = sec.header.clone();
                    b.value = Some(if expanded { "expanded" } else { "collapsed" }.into());
                    let bl = Lay::new(Size { w: gw, h: hh }, Some(b), vec![(gw - row_pad - cw, (hh - ch) / 2.0, c)]);
                    kids.push((margin, y, bl));
                }
                kids.push((margin + row_pad, ty, t));
                y += hh;
            } else if !plain_like {
                // A header-less section has an empty 17.3pt header: 35pt below a card, 47pt below a footer, and
                // 35pt below the top of the list.
                y += match si {
                    0 if under_bar => SECTION_GAP + HEADERLESS_EXTRA_GAP,
                    0 => 0.0,
                    _ => HEADERLESS_EXTRA_GAP,
                };
            }
            let mut sec_kids: Vec<(f64, f64, Lay)> = vec![];
            let mut lifted: Vec<(f64, f64, Lay)> = vec![];
            let mut ry = 0.0;
            if grouped {
                sec_kids.push((-margin, 0.0, line(format!("{sid}#top"), w)));
            }
            let nrows = sec.rows.len();
            for (ri, row) in sec.rows.iter().enumerate() {
                let lr = list_row(row);
                let mut c = self.view(row, Prop { w: Some(inner_w), h: None }, row_env);
                // Measured: 34.3pt buttons and 44pt content get 15pt vertical padding, 32pt steppers 10pt.
                let vpad = if has_bordered(row) || c.size.h > 33.0 { ROW_VPAD_BORDERED } else { ROW_VPAD };
                let rh = row_min_height(row).max(c.size.h + 2.0 * vpad);
                let yo = (rh - c.size.h) / 2.0;
                expand_button(&mut c, 0.0, yo, (-row_pad, 0.0, gw - row_pad, rh));
                let tx = first_text_x(&c, 0.0).unwrap_or(0.0);
                let mut rn = Node::new(format!("{sid}#r{ri}"), "row");
                rn.inspect = false;
                if let Some(g) = lr.and_then(|l| l.gesture.as_ref()) {
                    rn.gesture = Some(Box::new(g.clone()));
                }
                let last = ri + 1 == nrows;
                let mut rkids = vec![];
                let swipe = lr.and_then(|l| l.swipe.as_ref());
                let shift = match swipe {
                    Some(sw) => self.swipe_actions(&format!("{sid}#r{ri}"), sw, gw, rh, ri == 0, last && !grouped && !plain_like, &mut rkids),
                    None => 0.0,
                };
                if shift != 0.0 {
                    rkids.push(Self::swiped_cell(&format!("{sid}#r{ri}"), gw, rh, shift, (row_pad + shift, yo, c)));
                } else {
                    rkids.push((row_pad, yo, c));
                }
                let sep_below = if plain_like { style == ListStyle::Plain || !last || (sec.footer.is_none() && sec.footer_view.is_none()) } else { !last };
                if sep_below && shift == 0.0 {
                    let x0 = row_pad + tx;
                    let x1 = gw - row_pad;
                    // Measured: 1pt (3px @3x) solid line ending at the row boundary.
                    rkids.push((x0, rh - 1.0, line(format!("{sid}#r{ri}#sep"), x1 - x0)));
                } else if grouped && last {
                    rkids.push((-margin + if tx > 0.0 { row_pad + tx } else { 0.0 }, rh - 1.0, line(format!("{sid}#r{ri}#sep"), w)));
                }
                if let Some(g) = lr.and_then(|l| l.reorder.as_ref()) {
                    let mut h = Node::new(format!("{sid}#r{ri}#reorder"), "reorder");
                    h.gesture = Some(Box::new(g.clone()));
                    h.inspect = false;
                    rkids.push((gw - REORDER_STRIP, 0.0, Lay::new(Size { w: REORDER_STRIP, h: rh }, Some(h), vec![])));
                }
                let lift = lr.map_or(0.0, |l| l.lift);
                let rl = Lay::new(Size { w: gw, h: rh }, Some(rn), rkids);
                if lift != 0.0 {
                    lifted.push((0.0, ry + lift, rl));
                } else {
                    sec_kids.push((0.0, ry, rl));
                }
                ry += rh;
            }
            sec_kids.extend(lifted);
            let mut sn = Node::new(&sid, "section");
            if card {
                sn.fill = Some(sec_col);
                sn.radius = CARD_RADIUS;
            } else if grouped && !self.floating {
                sn.fill = Some(sec_col);
            }
            sn.inspect = true;
            kids.push((margin, y, Lay::new(Size { w: gw, h: ry }, Some(sn), sec_kids)));
            y += ry;
            let has_footer = sec.footer.is_some() || sec.footer_view.is_some();
            if has_footer && style == ListStyle::Plain {
                // Plain lists draw the footer as one more row (body text, separator under it).
                let ft = if let Some(f) = &sec.footer {
                    let spec = self.spec(&Env { align: TextAlign::Leading, ..row_env });
                    self.text_lay(&format!("{sid}#footer"), f, spec, self.semantic(Semantic::Primary), Some(inner_w as f32))
                } else {
                    self.view(sec.footer_view.as_deref().expect("footer view"), Prop { w: Some(inner_w), h: None }, row_env)
                };
                let rh = ROW_MIN_H.max(ft.size.h + 2.0 * ROW_VPAD);
                let tx = first_text_x(&ft, 0.0).unwrap_or(0.0);
                let fh = ft.size.h;
                kids.push((margin + row_pad, y + (rh - fh) / 2.0, ft));
                kids.push((margin + row_pad + tx, y + rh - 1.0, line(format!("{sid}#footer#sep"), gw - 2.0 * row_pad - tx)));
                y += rh;
            } else if has_footer {
                let ft = if let Some(f) = &sec.footer {
                    let spec = FontSpec::new(FOOTER_PT, self.line_h(FOOTER_PT), 400, TextAlign::Leading);
                    self.text_lay(&format!("{sid}#footer"), f, spec, secondary, Some(inner_w as f32))
                } else {
                    let fenv = Env { fg: Some(secondary), size: FOOTER_PT, base_weight: 400, in_row: false, ..row_env };
                    self.view(sec.footer_view.as_deref().expect("footer view"), Prop { w: Some(inner_w), h: None }, fenv)
                };
                y += FOOTER_TOP;
                let th = ft.size.h;
                kids.push((margin + row_pad, y, ft));
                y += th + FOOTER_BOTTOM;
            }
            if plain_like {
                continue;
            }
            // Between sections 17.7 (a footer takes its place); after the last one the list ends 20pt above the
            // bottom inset (measured).
            if si + 1 == sections.len() {
                if !has_footer {
                    y += 20.0;
                }
            } else if !has_footer {
                y += SECTION_GAP;
            }
        }
        if plain_like {
            y += PLAIN_SECTION_GAP;
        }
        ListContent { bottom: y, kids }
    }

    /// Lay out a row's swipe actions into `kids` (behind the row content) and return how far the content moves.
    /// Open trailing actions sit right-aligned inside the row, outermost rightmost; leading ones mirror that.
    #[allow(clippy::too_many_arguments)]
    fn swipe_actions(&mut self, id: &str, sw: &onesim_protocol::RowSwipe, gw: f64, rh: f64, first: bool, last: bool, kids: &mut Vec<(f64, f64, Lay)>) -> f64 {
        let white = rgba(255, 255, 255, 1.0);
        let spec = FontSpec::new(SWIPE_FONT, self.line_h(SWIPE_FONT), 500, TextAlign::Center);
        let mut pills = |acts: &[onesim_protocol::SwipeAction], side: &str| -> Vec<(f64, Lay)> {
            acts.iter()
                .enumerate()
                .map(|(i, a)| {
                    let label = first_text(&a.label).unwrap_or_default();
                    let ph = rh - 2.0 * SWIPE_PILL_INSET;
                    // A `Label` shows its symbol before the title.
                    let mut parts = vec![];
                    if let Some(sym) = first_symbol(&a.label) {
                        parts.push(self.symbol_lay(format!("{id}#{side}{i}#i"), &sym, f64::from(SWIPE_FONT), 500, white));
                    }
                    if !label.is_empty() {
                        parts.push(self.text_lay(&format!("{id}#{side}{i}#t"), &label, spec, white, None));
                    }
                    let cw = parts.iter().map(|l| l.size.w).sum::<f64>() + SWIPE_ICON_GAP * (parts.len().saturating_sub(1)) as f64;
                    let pw = cw + 2.0 * SWIPE_PILL_PAD;
                    let fill = match (&a.tint, a.role) {
                        (Some(c), _) => self.color(c),
                        (None, Some(onesim_protocol::ButtonRole::Destructive)) => self.pick(SWIPE_DESTRUCTIVE),
                        _ => self.pick(SWIPE_DEFAULT),
                    };
                    let mut n = Node::new(format!("{id}#{side}{i}"), "button");
                    n.role = Role::Button;
                    n.action = Some(a.action);
                    n.label = Some(label);
                    n.fill = Some(fill);
                    n.radius = SWIPE_PILL_RADIUS;
                    // The content as one group (centred when the pill is widened).
                    let mut gx = 0.0;
                    let mut gk = vec![];
                    for l in parts {
                        let (w, h) = (l.size.w, l.size.h);
                        gk.push((gx, (ph - h) / 2.0, l));
                        gx += w + SWIPE_ICON_GAP;
                    }
                    let group = Lay::new(Size { w: cw, h: ph }, None, gk);
                    (pw, Lay::new(Size { w: pw, h: ph }, Some(n), vec![((pw - cw) / 2.0, 0.0, group)]))
                })
                .collect()
        };
        let equal = |mut p: Vec<(f64, Lay)>| {
            let w = p.iter().map(|(w, _)| *w).fold(0.0, f64::max);
            for (pw, l) in &mut p {
                if let Some(k) = l.kids.first_mut() {
                    k.0 += (w - *pw) / 2.0;
                }
                l.size.w = w;
                *pw = w;
            }
            p
        };
        let trailing = equal(pills(&sw.trailing, "t"));
        let leading = equal(pills(&sw.leading, "l"));
        let span = |p: &[(f64, Lay)]| if p.is_empty() { 0.0 } else { p.iter().map(|(w, _)| w).sum::<f64>() + SWIPE_GAP * p.len() as f64 + SWIPE_EDGE };
        let base = match sw.open {
            Some(onesim_protocol::HEdge::Trailing) => -span(&trailing),
            Some(onesim_protocol::HEdge::Leading) => span(&leading),
            None => 0.0,
        };
        let shift = (base + sw.offset).clamp(-gw, gw);
        if shift == 0.0 {
            return 0.0;
        }
        // The revealed part of the row shows the list's background, not the card.
        let mut under = Node::new(format!("{id}#revealed"), "swiperevealed");
        under.fill = Some(self.semantic(Semantic::GroupedBackground));
        under.inspect = false;
        kids.push((0.0, 0.0, Lay::new(Size { w: gw, h: rh }, Some(under), vec![])));
        // A full swipe's action fills the revealed span, its label centred.
        let stretch = |l: &mut Lay, span: f64, pw: f64| {
            let w = (span - SWIPE_EDGE - SWIPE_GAP).max(pw);
            l.size.w = w;
            if let Some(k) = l.kids.first_mut() {
                k.0 = (w - k.2.size.w) / 2.0;
            }
            w
        };
        // Outermost first: trailing pills run right to left from the edge, leading ones left to right.
        if shift < 0.0 {
            let mut x = gw - SWIPE_EDGE;
            for (i, (pw, mut l)) in trailing.into_iter().enumerate() {
                if sw.full && i == 0 {
                    let w = stretch(&mut l, -shift, pw);
                    kids.push((gw - SWIPE_EDGE - w, SWIPE_PILL_INSET, l));
                    break;
                }
                x -= pw;
                kids.push((x, SWIPE_PILL_INSET, l));
                x -= SWIPE_GAP;
            }
        } else {
            let mut x = SWIPE_EDGE;
            for (i, (pw, mut l)) in leading.into_iter().enumerate() {
                if sw.full && i == 0 {
                    stretch(&mut l, shift, pw);
                    kids.push((SWIPE_EDGE, SWIPE_PILL_INSET, l));
                    break;
                }
                kids.push((x, SWIPE_PILL_INSET, l));
                x += pw + SWIPE_GAP;
            }
        }
        // The moved row: a highlighted rounded block, square where it meets the card's straight edges.
        let mut bg = Node::new(format!("{id}#swiped"), "swiped");
        bg.fill = Some(if self.dark { rgba(SWIPED_ROW.1 .0, SWIPED_ROW.1 .1, SWIPED_ROW.1 .2, 1.0) } else { rgba(SWIPED_ROW.0 .0, SWIPED_ROW.0 .1, SWIPED_ROW.0 .2, 1.0) });
        bg.radius = CARD_RADIUS;
        bg.inspect = false;
        let bw = gw - shift.abs();
        let bx = shift.max(0.0);
        let mut patches = vec![];
        // The card's own corner is rounded only on its first / last row; other corners on the card side are square.
        let card_side_x = if shift < 0.0 { 0.0 } else { bw - CARD_RADIUS };
        for (yy, round) in [(0.0, first), (rh - CARD_RADIUS, last)] {
            if !round {
                let mut p = Node::new(format!("{id}#swiped#{yy}"), "swiped");
                p.fill = bg.fill;
                p.inspect = false;
                patches.push((card_side_x, yy, Lay::new(Size { w: CARD_RADIUS, h: CARD_RADIUS }, Some(p), vec![])));
            }
        }
        kids.push((bx, 0.0, Lay::new(Size { w: bw, h: rh }, Some(bg), patches)));
        shift
    }

    /// The moved cell of a swiped row: its content clipped to the cell (`kids` already holds the highlight).
    fn swiped_cell(id: &str, gw: f64, rh: f64, shift: f64, content: (f64, f64, Lay)) -> (f64, f64, Lay) {
        let bw = gw - shift.abs();
        let bx = shift.max(0.0);
        let mut clip = Node::new(format!("{id}#cell"), "swipecell");
        clip.clip_radius = Some(0.0);
        clip.inspect = false;
        let (cx, cy, c) = content;
        (bx, 0.0, Lay::new(Size { w: bw, h: rh }, Some(clip), vec![(cx - bx, cy, c)]))
    }

    // Out of line: inlined into the recursive `view` dispatcher it inflates every stack frame.
    #[inline(never)]
    #[allow(clippy::too_many_arguments)]
    pub(super) fn stack(
        &mut self,
        v: &ViewNode,
        kind: &'static str,
        children: &[ViewNode],
        spacing: Option<f64>,
        align: Alignment,
        vertical: bool,
        p: Prop,
        env: Env,
    ) -> Lay {
        let env = Env { axis: Some(vertical), ..env };
        let (pm, pc) = if vertical { (p.h, p.w) } else { (p.w, p.h) };
        let mk = |m: Option<f64>| if vertical { Prop { w: pc, h: m } } else { Prop { w: m, h: pc } };
        let main_cross = |s: Size| if vertical { (s.h, s.w) } else { (s.w, s.h) };
        let spacer = |c: &ViewNode| matches!(c.kind, ViewKind::Spacer { .. });
        let sp = spacing.unwrap_or(DEFAULT_SPACING);
        let gaps: Vec<f64> = children
            .windows(2)
            .map(|w| if spacer(&w[0]) || spacer(&w[1]) { 0.0 } else { sp })
            .collect();
        let total_gap: f64 = gaps.iter().sum();

        let mut lays: Vec<Option<Lay>> = children.iter().map(|_| None).collect();
        match pm.filter(|m| m.is_finite()) {
            // Unconstrained (or probing with infinity): children take their ideal size.
            None => {
                for (i, c) in children.iter().enumerate() {
                    lays[i] = Some(self.view(c, mk(pm), env));
                }
            }
            Some(m) => {
                let mut order: Vec<(usize, f64)> = children
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        let lo = main_cross(self.view(c, mk(Some(0.0)), env).size).0;
                        let hi = main_cross(self.view(c, mk(Some(INF)), env).size).0;
                        (i, hi - lo)
                    })
                    .collect();
                // SwiftUI sizes spacers last: other children share the space among
                // themselves first, spacers split whatever is left.
                order.sort_by(|a, b| {
                    (spacer(&children[a.0]), a.1)
                        .partial_cmp(&(spacer(&children[b.0]), b.1))
                        .unwrap_or(Ordering::Equal)
                });
                let mut remaining = m - total_gap;
                let n_spacers = children.iter().filter(|c| spacer(c)).count();
                let mut left = [children.len() - n_spacers, n_spacers];
                for (i, _) in order {
                    let group = spacer(&children[i]) as usize;
                    let share = (remaining / left[group] as f64).max(0.0);
                    let l = self.view(&children[i], mk(Some(share)), env);
                    remaining -= main_cross(l.size).0;
                    left[group] -= 1;
                    lays[i] = Some(l);
                }
            }
        }

        let lays: Vec<Lay> = lays.into_iter().map(Option::unwrap).collect();
        let cross = lays.iter().map(|l| main_cross(l.size).1).fold(0.0, f64::max);
        let frac = if vertical { h_frac(align) } else { v_frac(align) };
        let mut pos = 0.0;
        let mut kids = Vec::with_capacity(lays.len());
        for (i, l) in lays.into_iter().enumerate() {
            let (m, c) = main_cross(l.size);
            let off = (cross - c) * frac;
            kids.push(if vertical { (off, pos, l) } else { (pos, off, l) });
            pos += m + gaps.get(i).copied().unwrap_or(0.0);
        }
        let size = if vertical { Size { w: cross, h: pos } } else { Size { w: pos, h: cross } };
        Lay::new(size, Some(Node::new(&v.id, kind)), kids)
    }
}
