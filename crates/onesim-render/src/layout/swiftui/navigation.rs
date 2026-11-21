use super::*;

pub(super) struct NavBar<'a> {
    pub(super) id: &'a str,
    pub(super) w: f64,
    pub(super) h: f64,
    pub(super) top: f64,
    pub(super) bar_h: f64,
    pub(super) title: Option<&'a str>,
    pub(super) large: bool,
    pub(super) large_title: Option<Lay>,
    pub(super) title_h: f64,
    pub(super) band: f64,
    pub(super) offset: f64,
    pub(super) back: Option<NavBack>,
}

impl Ctx<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn navigation(
        &mut self,
        v: &ViewNode,
        title: Option<&str>,
        mode: DisplayMode,
        back: Option<&onesim_protocol::BackButton>,
        content: &ViewNode,
        p: Prop,
        env: Env,
    ) -> Lay {
        let w = p.w.filter(|w| w.is_finite()).unwrap_or(402.0);
        let h = p.h.filter(|h| h.is_finite()).unwrap_or(874.0);
        let (top, bottom) = (env.safe_top, env.bottom_inset);
        let large = mode == DisplayMode::Large && title.is_some();
        let (large_title, title_h, band) = self.nav_large(&v.id, w, title, large);
        let bar_h = if title.is_none() && back.is_none() { 0.0 } else { NAV_INLINE_H + band };
        let ctop = top + bar_h;
        let scroller = is_scroller(content);
        let edge_bg = self.semantic(if is_list(content) { Semantic::GroupedBackground } else { Semantic::Background });
        let mut bg = Node::new(&v.id, "navigation");
        bg.fill = Some(self.semantic(if is_list(content) { Semantic::GroupedBackground } else { Semantic::Background }));
        bg.label = title.map(str::to_string);
        let cenv = Env { safe_top: 0.0, bottom_inset: bottom, fg: None, ..env };
        let mut kids = vec![];
        let mut offset = 0.0;
        if scroller {
            // Content scrolls under the bar: the scroller spans the screen with the bar as a top inset.
            let mut c = self.view(content, Prop { w: Some(w), h: Some(h) }, Env { safe_top: ctop, ..cenv });
            if let Some(s) = c.node.as_mut().and_then(|n| n.scroll.as_mut()).filter(|_| large) {
                s.snap = Some(band);
            }
            offset = c.node.as_ref().and_then(|n| n.scroll).map_or(0.0, |s| s.offset.1);
            kids.push((0.0, 0.0, c));
            // Content reaches under the bar once the large title has collapsed.
            if offset > band {
                let mut e = Node::new(format!("{}#edge", v.id), "scrolledge");
                e.edge = Some(crate::glass::ScrollEdge {
                    bar: (top + NAV_INLINE_H) as f32,
                    color: edge_bg,
                    tint: if self.dark { 0.0 } else { 3.0 / 255.0 },
                });
                e.inspect = false;
                kids.push((0.0, 0.0, Lay::new(Size { w, h: ctop + 33.0 }, Some(e), vec![])));
            }
        } else {
            let avail = (h - ctop - bottom).max(0.0);
            let c = self.view(content, Prop { w: Some(w), h: Some(avail) }, Env { bottom_inset: 0.0, ..cenv });
            kids.push(((w - c.size.w) / 2.0, ctop + (avail - c.size.h) / 2.0, c));
        }
        let back = back.map(|b| NavBack { action: Some(b.action), label: b.label.clone(), pill: false });
        let bar = NavBar { id: &v.id, w, h, top, bar_h, title, large, large_title, title_h, band, offset, back };
        kids.push((0.0, top, self.nav_bar(bar)));
        Lay::new(Size { w, h }, Some(bg), kids)
    }

    /// Large title lay (when `large`), its height and the band it occupies below the inline bar.
    /// Large title: LargeTitle bold, growing with Dynamic Type. Its band below the inline bar is
    /// 52pt, or the title's height + 1.33 when taller (measured: 106pt bar at large, 127 at AX5).
    pub(super) fn nav_large(&mut self, id: &str, w: f64, title: Option<&str>, large: bool) -> (Option<Lay>, f64, f64) {
        let primary = self.semantic(Semantic::Primary);
        let large_title = title.filter(|_| large).map(|t| {
            let sz = if self.compact_nav {
                28.0
            } else { onesim_protocol::TextStyle::LargeTitle.size_at(self.dts) as f32
            };
            let spec = FontSpec::new(sz, self.line_h(sz), 700, TextAlign::Leading);
            self.text_lay(&format!("{id}#title"), t, spec, primary, Some((w - 32.0) as f32))
        });
        let title_h = large_title.as_ref().map_or(0.0, |l| l.size.h);
        // Compact bar: 58pt in all, the title inside it (no band to collapse).
        let band = if self.compact_nav {
            4.0
        } else if large { (title_h + 1.33).max(52.0) } else { 0.0 };
        (large_title, title_h, band)
    }

    /// The navigation bar (no background at rest on iOS 26): large + inline title and the glass back
    /// button. Shared by SwiftUI `NavigationStack` and React Native's native stack header.
    pub(super) fn nav_bar(&mut self, nb: NavBar) -> Lay {
        let NavBar { id, w, h, top, bar_h, title, large, large_title, title_h, band, offset, back } = nb;
        let primary = self.semantic(Semantic::Primary);
                // The large title scrolls with the content and disappears under the inline bar; the bar
        // shrinks with it and the inline title fades in over the last INLINE_FADE points.
        let collapse = offset.clamp(0.0, band);
        let mut bar = Node::new(format!("{}#bar", id), "navbar");
        bar.label = title.map(str::to_string);
        let mut bar_kids = vec![];
        let compact = self.compact_nav;
        if let Some(tl) = large_title.filter(|_| compact || offset < band) {
            if compact {
                // Compact bar (iPhone Duo): the title stays at the bar's top-left (measured y 31 in a bar from 24).
                bar_kids.push((self.margin, 7.0, tl));
            } else {
            // Title top: 54pt under the bar's top, lowered when the band has spare room (57.7 at large).
            let ty = NAV_INLINE_H + (band - title_h - 7.6).max(0.0) - offset;
            if offset > 0.0 {
                // Mid-collapse: cut off where it slides under the inline bar.
                let mut clip = Node::new(format!("{}#largetitle", id), "clip");
                clip.clip_radius = Some(0.0);
                clip.inspect = false;
                let clip_h = h - top - NAV_INLINE_H;
                bar_kids.push((0.0, NAV_INLINE_H, Lay::new(Size { w, h: clip_h }, Some(clip), vec![(self.margin, ty - NAV_INLINE_H, tl)])));
            } else {
                bar_kids.push((self.margin, ty, tl));
            }
        }
        }
        if let Some(t) = title.filter(|_| !(self.compact_nav && large)) {
            let alpha = if large { ((collapse - (band - INLINE_FADE)) / INLINE_FADE).clamp(0.0, 1.0) as f32 } else { 1.0 };
            if alpha > 0.0 {
                // Inline titles grow with Dynamic Type but cap at x1.225 (measured at accessibility5).
                let nsz = 17.0 * (onesim_protocol::TextStyle::Body.size_at(self.dts) as f32 / 17.0).min(1.225);
                let spec = FontSpec::new(nsz, self.line_h(nsz), 600, TextAlign::Leading);
                let id = if large { format!("{}#inline", id) } else { format!("{}#title", id) };
                let mut tl = self.text_lay(&id, t, spec, primary, Some((w - 120.0) as f32));
                if let Some(n) = tl.node.as_mut() {
                    n.opacity = alpha;
                }
                let (tw, th) = (tl.size.w, tl.size.h);
                bar_kids.push(((w - tw) / 2.0, 22.0 - th / 2.0, tl));
            }
        }
        if let Some(b) = back {
            let mut n = Node::new(format!("{id}#back"), "button");
            n.role = Role::Button;
            n.action = b.action;
            n.label = Some(if b.label.is_empty() { "Back".into() } else { b.label.clone() });
            if self.sheet_up {
                // Under a sheet the glass goes flat: a barely visible disc remains.
                let a = if self.dark { BACK_FLAT.1 } else { BACK_FLAT.0 };
                n.fill = Some(Color::rgba(1.0, 1.0, 1.0, a));
            } else {
                n.glass = Some(self.glass_spec(GlassVariant::Regular, None, true));
            }
            n.radius = PILL;
            // Under a presentation the chevron desaturates like the rest of the presenter (iOS 26; iOS 27 keeps it).
            let primary = if self.desat && self.ios < IosVersion::Ios27 {
                let g = if self.dark { ALERT_DESAT.1 } else { ALERT_DESAT.0 };
                rgba(g, g, g, 1.0)
            } else {
                primary
            };
            if b.pill {
                // Pill with chevron and the previous title (measured iOS 26.5: chevron centre 19pt in, text at 35, 17.7 trailing).
                let spec = FontSpec::new(17.0, self.line_h(17.0), 400, TextAlign::Leading);
                let mut tl = self.text_lay(&format!("{id}#backtitle"), &b.label, spec, primary, None);
                if let Some(t) = tl.node.as_mut() {
                    t.inspect = false;
                }
                let mut chev = Node::new(format!("{id}#chevron"), "chevron");
                chev.glyph = Some((Glyph::BackChevron, primary));
                chev.inspect = false;
                let (tw, th) = (tl.size.w, tl.size.h);
                let kids = vec![(0.0, 0.0, Lay::new(Size { w: 38.0, h: 44.0 }, Some(chev), vec![])), (35.0, (44.0 - th) / 2.0, tl)];
                bar_kids.push((self.margin, 0.0, Lay::new(Size { w: 35.0 + tw + 17.7, h: 44.0 }, Some(n), kids)));
            } else {
                n.glyph = Some((Glyph::BackChevron, primary));
                if let Some((bx, by)) = self.column_back {
                    // In the status column (iPhone Duo): a 48pt circle, screen-placed (the bar starts at the top inset).
                    bar_kids.push((
                        bx,
                        by - top,
                        Lay::new(Size { w: 48.0, h: 48.0 }, Some(n), vec![]),
                    ));
                } else {
                bar_kids.push((self.margin, 0.0, Lay::new(Size { w: 44.0, h: 44.0 }, Some(n), vec![])));
                }
            }
        }
        Lay::new(Size { w, h: bar_h - collapse }, Some(bar), bar_kids)
    }

    /// The iOS 26 floating Liquid Glass tab bar over a `w` x `h` screen (measured: 188 x 61 pt capsule for two tabs,
    /// 22 pt above the bottom, items 94 x 54 pt at an 86 pt pitch, 4 pt inside the capsule; badge an 18 pt red disc
    /// centred 18 pt right of the item centre, its top at the item's).
    pub(super) fn tab_bar(&mut self, t: &TabBar, host: &str, w: f64, h: f64) -> Lay {
        let n = t.items.len().max(1) as f64;
        let bar_w = 8.0 + 94.0 + (n - 1.0) * 86.0;
        let mut bar = Node::new(format!("{host}#tabbar"), "tabbar");
        bar.radius = PILL;
        bar.glass = Some(crate::glass::GlassSpec { variant: GlassVariant::Regular, tint: None, interactive: false, blur: None, mix: None, shadow: None, flat: None });
        let (label, accent) = if self.dark { (rgba(255, 255, 255, 1.0), rgba(10, 132, 255, 1.0)) } else { (rgba(0, 0, 0, 1.0), rgba(0, 136, 255, 1.0)) };
        let mut kids = vec![];
        for (i, tab) in t.items.iter().enumerate() {
            let sel = i == t.selected;
            let mut item = Node::new(format!("{host}#tab-{}", tab.key), "button");
            item.role = Role::Button;
            item.label = Some(tab.title.clone());
            // React Native items have no action: the host's numeric tag makes them hit-testable (the host sends `SelectTab`).
            item.disabled = tab.action == Some(0);
            item.action = tab.action.or_else(|| host.parse().ok()).filter(|_| !item.disabled);
            item.radius = PILL;
            if sel {
                // iOS 27 sinks the selection into the bar (tabs-home(-dark).png, ios27); iOS 26 raises it.
                item.fill = Some(match (self.dark, self.ios >= IosVersion::Ios27) {
                    (true, true) => rgba(10, 10, 10, 1.0),
                    (false, true) => rgba(225, 225, 231, 1.0),
                    (true, false) => rgba(58, 58, 60, 1.0),
                    (false, false) => rgba(233, 233, 235, 1.0), });
            }
            let tint = if sel { t.tint.unwrap_or(accent) } else { label };
            let path = if sel { tab.selected_icon.as_ref().or(tab.icon.as_ref()) } else { tab.icon.as_ref() };
            let mut icon = Node::new(format!("{host}#tab-{}#icon", tab.key), "image");
            icon.inspect = false;
            let mut isz = (0.0, 0.0);
            if let Some(pm) = path.and_then(|p| crate::image::load(p)) {
                isz = (pm.width() as f64 / self.scale, pm.height() as f64 / self.scale);
                let dest = Rect { x: 0.0, y: 0.0, w: isz.0, h: isz.1 };
                icon.image = tinted(&pm, tint).map(|pixmap| crate::ImageRun { pixmap, dest, rotation: 0.0, filter: 2 });
            } else if let Some(name) = &tab.symbol {
                let spec = crate::SymbolSpec { name: name.clone(), point_size: 22.0, weight: 400, scale: ImageScale::Medium, ..Default::default() };
                isz = crate::symbols::metrics(&spec).map_or((22.0, 22.0), |m| (m.w, m.h));
                icon.symbol = Some(crate::SymbolRun { spec, color: tint, size: isz });
            }
            let mut lb = Node::new(format!("{host}#tab-{}#label", tab.key), "text");
            lb.inspect = false;
            lb.text = Some(TextRun { text: tab.title.clone(), font: FontSpec::new(10.0, 12.0, if sel { 600 } else { 500 }, TextAlign::Center), color: label, strike: false, offset_x: 0.0, spans: vec![] });
            let mut ikids = vec![
                ((94.0 - isz.0) / 2.0,
                    if tab.symbol.is_some() { 3.5 } else { 6.0 }, Lay::new(Size { w: isz.0, h: isz.1 }, Some(icon), vec![])),
                // Label line: ink at 832.7..839.3 in the 17/18 Pro captures (both releases).
                (0.0, 33.3, Lay::new(Size { w: 94.0, h: 14.0 }, Some(lb), vec![])),
            ];
            if let Some(b) = &tab.badge {
                let spec = FontSpec::new(12.0, 14.0, 400, TextAlign::Center);
                let bw = (self.fonts.measure(b, &spec, None, &[]).0 + 10.0).max(18.0);
                let mut bn = Node::new(format!("{host}#tab-{}#badge", tab.key), "badge");
                bn.inspect = false;
                bn.radius = PILL;
                bn.fill = Some(self.destructive());
                let mut bt = Node::new(format!("{host}#tab-{}#badge#text", tab.key), "text");
                bt.inspect = false;
                bt.text = Some(TextRun { text: b.clone(), font: spec, color: rgba(255, 255, 255, 1.0), strike: false, offset_x: 0.0, spans: vec![] });
                let text = Lay::new(Size { w: bw, h: 14.0 }, Some(bt), vec![]);
                ikids.push((65.0 - bw / 2.0, 0.0, Lay::new(Size { w: bw, h: 18.0 }, Some(bn), vec![(0.0, 2.0, text)])));
            }
            kids.push((4.0 + i as f64 * 86.0, 4.0, Lay::new(Size { w: 94.0, h: 54.0 }, Some(item), ikids)));
        }
        let bar = Lay::new(Size { w: bar_w, h: 61.0 }, Some(bar), kids);
        Lay::new(Size { w, h }, None, vec![((w - bar_w) / 2.0, h - 20.0 - 61.0, bar)])
    }

    /// `TabView`: the selected tab's content placed like a root above the tab bar's inset, then the bar.
    pub(super) fn tab_view(&mut self, v: &ViewNode, selected: usize, tabs: &[onesim_protocol::TabItem], content: &ViewNode, p: Prop, env: Env) -> Lay {
        let w = p.w.filter(|w| w.is_finite()).unwrap_or(402.0);
        let h = p.h.filter(|h| h.is_finite()).unwrap_or(874.0);
        let g = Geo { w, h, safe_top: env.safe_top, safe_bottom: env.bottom_inset.max(TAB_BAR_INSET), dev_bottom: TAB_BAR_INSET, body: env.size };
        let inner = self.place_root(content, &g);
        let items = tabs
            .iter()
            .enumerate()
            .map(|(i, t)| {
                // Tab bars draw the filled variant of a symbol when there is one.
                let symbol = t.system_image.as_ref().map(|s| {
                    let fill = format!("{s}.fill");
                    let spec = crate::SymbolSpec { name: fill.clone(), point_size: 22.0, weight: 400, scale: ImageScale::Medium, ..Default::default() };
                    if s.ends_with(".fill") || crate::symbols::metrics(&spec).is_none() { s.clone() } else { fill }
                });
                TabBarItem { key: i.to_string(), title: t.title.clone(), icon: None, selected_icon: None, symbol, badge: t.badge.clone(), action: Some(t.action) }
            })
            .collect();
        let mut n = Node::new(&v.id, "tabview");
        n.inspect = false;
        let mut kids = vec![(0.0, 0.0, inner)];
        if !tabs.is_empty() {
            kids.push((0.0, 0.0, self.tab_bar(&TabBar { items, selected, tint: None }, &v.id, w, h)));
        }
        Lay::new(Size { w, h }, Some(n), kids)
    }

    pub(super) fn page_tab_view(&mut self, v: &ViewNode, selected: usize, pages: &[TabPage], index: (PageIndexDisplay, PageIndexBackground), p: Prop, env: Env) -> Lay {
        let w = p.w.filter(|w| w.is_finite()).unwrap_or(10.0);
        let h = p.h.filter(|h| h.is_finite()).unwrap_or(10.0);
        let size = Size { w, h };
        let mut n = Node::new(&v.id, "pagetabview");
        n.clip_radius = Some(0.0);
        if pages.is_empty() || w <= 0.0 || h <= 0.0 {
            return Lay::new(size, Some(n), vec![]);
        }
        let selected = selected.min(pages.len() - 1);
        let idle = selected as f64 * w;
        let max = (pages.len() - 1) as f64 * w;
        let drag = self.scroll.get(&v.id).map_or(idle, |off| off.0);
        let offset = self.clamp_offset(drag, max, w);
        if pages.len() > 1 && pages.iter().any(|page| page.action.is_some()) {
            n.scroll = Some(ScrollInfo { vertical: false, offset: (offset, 0.0), max: (max, 0.0), snap: None, content: None, page: Some(w), aligned: false });
            n.pager = Some(Box::new(Pager { selected, actions: pages.iter().map(|page| page.action).collect() }));
        }

        let mut kids = Vec::new();
        for (i, page) in pages.iter().enumerate() {
            let x = i as f64 * w - offset;
            if x <= -w || x >= w {
                continue;
            }
            let content = self.view(&page.content, Prop { w: Some(w), h: Some(h) }, Env { axis: None, ..env });
            let position = ((w - content.size.w) / 2.0, (h - content.size.h) / 2.0, content);
            let mut page = Node::new(format!("{}#page-{i}", v.id), "page");
            page.inspect = false;
            page.clip_radius = Some(0.0);
            kids.push((x, 0.0, Lay::new(size, Some(page), vec![position])));
        }
        if index.0 == PageIndexDisplay::Always || (index.0 == PageIndexDisplay::Automatic && pages.len() > 1) {
            let dots = self.page_index(v, pages, selected, index.1, w);
            kids.push(((w - dots.size.w) / 2.0, (h - dots.size.h).max(0.0), dots));
        }
        Lay::new(size, Some(n), kids)
    }

    pub(super) fn page_index(&self, v: &ViewNode, pages: &[TabPage], selected: usize, background: PageIndexBackground, width: f64) -> Lay {
        let pitch = ((width - 16.0) / pages.len() as f64).clamp(7.0, 16.0);
        let w = pages.len() as f64 * pitch + 16.0;
        let h = 26.0;
        let mut index = Node::new(format!("{}#index", v.id), "page-index");
        index.label = Some(format!("Page {} of {}", selected + 1, pages.len()));
        let material = background == PageIndexBackground::Always;
        index.radius = PILL;
        index.fill = material.then(|| rgba(118, 118, 128, 0.35));
        let kids = pages.iter().enumerate().map(|(i, page)| {
            let mut dot = Node::new(format!("{}#index-{i}", v.id), "page-dot");
            dot.role = Role::Button;
            dot.label = Some(format!("Page {}", i + 1));
            dot.action = page.action;
            dot.disabled = page.action.is_none();
            let mut mark = Node::new(format!("{}#index-{i}#mark", v.id), "circle");
            mark.inspect = false;
            mark.radius = PILL;
            let alpha = if i == selected { 1.0 } else { 0.3 };
            mark.fill = Some(rgba(255, 255, 255, alpha));
            let mark = Lay::new(Size { w: 7.0, h: 7.0 }, Some(mark), vec![]);
            let child = Lay::new(Size { w: pitch, h }, Some(dot), vec![((pitch - 7.0) / 2.0, (h - 7.0) / 2.0, mark)]);
            (8.0 + i as f64 * pitch, 0.0, child)
        }).collect();
        Lay::new(Size { w, h }, Some(index), kids)
    }

    pub(super) fn status_bar(&mut self, w: f64, dev: &Device, ios: IosVersion) -> Lay {
        let sb = dev.status_bar_for(ios);
        let primary = self.semantic(Semantic::Primary);
        let k = sb.icon_scale;
        let text = |cx: &mut Self, id: &str, s: &str, weight: u16, align: TextAlign| {
            let fs = sb.font as f32;
            let spec = FontSpec::new(fs, cx.line_h(fs), weight, align);
            let mut t = cx.text_lay(id, s, spec, primary, None);
            if let Some(n) = t.node.as_mut() {
                n.inspect = false;
            }
            t
        };
        let glyph = |id: &str, g: Glyph, c: Color, sz: Size| {
            let mut n = Node::new(id, "statusbar");
            n.glyph = Some((g, c));
            n.inspect = false;
            Lay::new(sz, Some(n), vec![])
        };
        let ios27 = ios >= IosVersion::Ios27;
        let wifi = glyph("statusbar#wifi",
            if ios27 { Glyph::Wifi27 } else { Glyph::Wifi }, primary, Size { w: 16.0 * k, h: 12.0 * k });
        let batt = glyph("statusbar#battery",
            if ios27 {
                Glyph::Battery27
            } else { Glyph::Battery
            }, primary, Size { w: 27.0 * k, h: 13.0 * k });
        let right = sb.icons.0 + (w - dev.viewport.width); // anchored to the trailing edge
        let dy = sb.icons.1;
        let mut root = Node::new("statusbar", "statusbar");
        root.inspect = false;
        let t = text(self, "statusbar#time", if dev.home_button { "9:41 AM" } else { "9:41" }, 600, TextAlign::Leading);
        let kids = if dev.safe_right > 0.0 {
            // Status column (iPhone Duo): clock, then the battery ring around the wifi glyph, then the dot arc.
            let cx = sb.time.0 + (w - dev.viewport.width);
            let ring = glyph(
                "statusbar#battery",
                Glyph::BatteryRing,
                primary,
                Size { w: 41.0, h: 41.0 },
            );
            let dots = glyph(
                "statusbar#signal",
                Glyph::DotsArc,
                self.semantic(Semantic::Tertiary),
                Size { w: 24.0, h: 7.0 },
            );
            vec![
                (cx - t.size.w / 2.0, sb.time.1 - t.size.h / 2.0, t),
                (cx + 0.4 - 20.5, 73.75 - 20.5, ring),
                (cx - 10.0, 70.4, wifi),
                (cx - 11.6, 91.5, dots),
            ]
        } else if dev.home_button {
            // Classic layout (home-button phones): carrier + wifi left, centred time, battery right.
            let carrier = text(self, "statusbar#carrier", "Carrier", 400, TextAlign::Leading);
            let ch = carrier.size.h;
            vec![
                (6.5, sb.time.1 - ch / 2.0, carrier),
                (49.7, 4.5 + dy, wifi),
                (sb.time.0 + (w - dev.viewport.width) / 2.0 - t.size.w / 2.0, sb.time.1 - t.size.h / 2.0, t),
                (right, 4.6 + dy, batt),
            ]
        } else {
            let dots = glyph("statusbar#signal",
                if ios27 {
                    Glyph::SignalDots27
                } else { Glyph::SignalDots
                }, self.semantic(Semantic::Tertiary), Size { w: 20.0 * k, h: 4.0 * k });
            let (wx, wy) = (right - 24.7 * k, 26.2 + dy);
            vec![
                (sb.time.0 - t.size.w / 2.0, sb.time.1 - t.size.h / 2.0, t),
                (wx - 27.3 * k, wy + 8.5 * k, dots),
                (wx, wy, wifi),
                (wx + 24.1 * k, wy - 0.2 * k, batt),
            ]
        };
        Lay::new(Size { w, h: dev.safe_top }, Some(root), kids)
    }

}
