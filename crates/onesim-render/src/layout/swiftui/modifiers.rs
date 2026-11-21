use super::*;

impl Ctx<'_> {
    pub(super) fn line_h(&self, size: f32) -> f32 {
        let s = self.scale as f32;
        (size * 1.1934 * s).ceil() / s
    }

    pub(super) fn spec(&self, env: &Env) -> FontSpec {
        let mut spec = FontSpec::new(env.size, self.line_h(env.size), env.weight.unwrap_or(env.base_weight), env.align);
        if env.secondary {
            // Glyphs shrink. The line box stays the body's, so a secondary row keeps the body pitch.
            let k = crate::text::SECONDARY_TEXT_SCALE;
            spec.size *= k;
            spec.pt *= k;
            spec.tracking = crate::text::tracking_em(spec.pt, spec.weight);
        }
        spec.extra = env.extra;
        spec.design = env.design;
        spec.width = env.width;
        spec.mono = env.mono || env.design == 3;
        spec.digits = env.digits;
        spec.trunc = env.trunc;
        spec.baseline = env.baseline;
        spec.case_fold = env.case_fold;
        spec.tighten = env.tighten;
        spec.min_scale = env.min_scale;
        spec.lines = env.max_lines;
        spec.underline = env.underline;
        spec.italic = env.italic;
        spec.line_pat = env.under_pat;
        spec.under_color = env.under_color;
        spec.strike_pat = env.strike_pat;
        spec.strike_color = env.strike_color;
        spec.reserve = env.reserve_lines;
        spec
    }

    pub(super) fn hairline(&self) -> f64 {
        1.0 / self.scale
    }

    /// A text leaf with an explicit font.
    pub(super) fn text_lay(&mut self, id: &str, text: &str, spec: FontSpec, color: Color, wrap: Option<f32>) -> Lay {
        self.text_lay_runs(id, text, spec, color, wrap, vec![])
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn text_lay_runs(&mut self, id: &str, text: &str, spec: FontSpec, color: Color, wrap: Option<f32>, spans: Vec<Span>) -> Lay {
        let (text, spans) = fold_label(text, spans, spec.case_fold);
        let (spec, spans, measured) = self.fit_text(&text, spec, wrap, spans);
        let (w, mut h) = measured.unwrap_or_else(|| self.fonts.measure(&text, &spec, wrap, &spans));
        // A positive baselineOffset lengthens the line downward; the glyphs stay at the top.
        // A negative one lengthens it and the draw shifts the glyphs down into that space.
        let (rise, sink) = crate::text::baseline_extent(&spec, &spans);
        h += f64::from(rise - sink);
        if let Some(n) = spec.reserve.filter(|n| *n > 1) {
            h = h.max(spec.line_height as f64 + (n - 1) as f64 * spec.pitch as f64);
        }
        // SwiftUI rounds text sizes up to the device pixel.
        let w = (w * self.scale).ceil() / self.scale;
        let mut n = Node::new(id, "text");
        n.label = Some(text.clone());
        n.text = Some(TextRun { text, font: spec, color, strike: false, offset_x: 0.0, spans });
        n.role = Role::Text;
        Lay::new(Size { w, h }, Some(n), vec![])
    }

    pub(super) fn view(&mut self, v: &ViewNode, p: Prop, env: Env) -> Lay {
        let bits = |x: Option<f64>| x.map_or(u64::MAX, f64::to_bits);
        let key = (v as *const ViewNode as usize, bits(p.w), bits(p.h));
        if let Some(l) = self.memo.get(&key) {
            return (**l).clone();
        }
        let l = self.mods(v, &v.modifiers, p, env);
        self.memo.insert(key, Arc::new(l.clone()));
        l
    }

    pub(super) fn push_anim(&mut self, spec: &AnimationSpec) -> u32 {
        let idx = self.anims.len() as u32;
        self.anims.push(spec.clone());
        idx
    }

    pub(super) fn stamp_node(&self, n: &mut Node, env: Env) {
        let anim = env.anim.and_then(|i| self.anims.get(i as usize).cloned());
        if anim.is_none() && env.content_transition == ContentTransitionKind::Identity {
            return;
        }
        let slot = n.anim.get_or_insert_with(|| Box::new(crate::scene::NodeAnim::default()));
        if slot.animation.is_none() {
            slot.animation = anim;
        }
        if slot.content_transition == ContentTransitionKind::Identity {
            slot.content_transition = env.content_transition;
        }
    }

    /// Applies modifiers outermost-first by recursion (array order is innermost first).
    pub(super) fn mods(&mut self, v: &ViewNode, mods: &[Modifier], p: Prop, env: Env) -> Lay {
        let Some((last, rest)) = mods.split_last() else {
            let mut out = self.base(v, p, env);
            if let Some(n) = out.node.as_deref_mut() {
                self.stamp_node(n, env);
            }
            return out;
        };
        match last {
            Modifier::Padding(e) => {
                let ph = e.leading + e.trailing;
                let pv = e.top + e.bottom;
                let inner = self.mods(
                    v,
                    rest,
                    Prop { w: p.w.map(|w| (w - ph).max(0.0)), h: p.h.map(|h| (h - pv).max(0.0)) },
                    env,
                );
                let size = Size { w: inner.size.w + ph, h: inner.size.h + pv };
                Lay::new(size, None, vec![(e.leading, e.top, inner)])
            }
            Modifier::Frame(f) => {
                let cw = child_prop(p.w, f.width, f.min_width, f.max_width);
                let ch = child_prop(p.h, f.height, f.min_height, f.max_height);
                let inner = self.mods(v, rest, Prop { w: cw, h: ch }, env);
                let w = frame_len(p.w, inner.size.w, f.width, f.min_width, f.max_width);
                let h = frame_len(p.h, inner.size.h, f.height, f.min_height, f.max_height);
                let (dx, dy) = ((w - inner.size.w) * h_frac(f.alignment), (h - inner.size.h) * v_frac(f.alignment));
                Lay::new(Size { w, h }, None, vec![(dx, dy, inner)])
            }
            Modifier::ForegroundColor(c) => {
                if let Some(paint) = self.inherited_paint(c, env) {
                    let fg = paint.color();
                    let index = self.store_paint(paint);
                    return self.mods(v, rest, p, Env {
                        foreground_paint: Some(index), secondary_paint: None, tertiary_paint: None, fg: Some(fg), ..env
                    });
                }
                let mut env = Env { fg: Some(self.color(c)), foreground_paint: None, secondary_paint: None, tertiary_paint: None, ..env };
                // Inside a tinted control the hierarchical styles stay accent-coloured, dimmed.
                if let (Some(t), onesim_protocol::Color::Semantic { semantic }) = (env.tint, c) {
                    let k = match semantic {
                        Semantic::Primary => Some(1.0),
                        Semantic::Secondary => Some(0.5),
                        Semantic::Tertiary => Some(0.25),
                        _ => None,
                    };
                    if let Some(k) = k {
                        env.fg = Some(Color { a: t.a * k, ..t });
                    }
                }
                self.mods(v, rest, p, env)
            }
            Modifier::ForegroundPaint(paint) => {
                let paint = self.shape_paint(paint, env);
                let fg = paint.color();
                let index = self.store_paint(paint);
                self.mods(v, rest, p, Env {
                    foreground_paint: Some(index), secondary_paint: None, tertiary_paint: None, fg: Some(fg), ..env
                })
            }
            Modifier::ForegroundPalette(palette) => {
                let primary = self.shape_paint(&palette.primary, env);
                let fg = primary.color();
                let primary = self.store_paint(primary);
                let secondary = self.store_paint(self.shape_paint(&palette.secondary, env));
                let tertiary = palette.tertiary.as_ref().map(|p| self.store_paint(self.shape_paint(p, env)));
                self.mods(v, rest, p, Env {
                    foreground_paint: Some(primary), secondary_paint: Some(secondary), tertiary_paint: tertiary, fg: Some(fg), ..env
                })
            }
            Modifier::BackgroundPaint(paint) => {
                let paint = self.shape_paint(paint, env);
                let index = self.store_paint(paint);
                self.mods(v, rest, p, Env { background_paint: Some(index), ..env })
            }
            Modifier::Tint(paint) => {
                let index = paint.as_ref().map(|paint| {
                    let paint = self.shape_paint(paint, env);
                    self.store_paint(paint)
                });
                self.mods(v, rest, p, Env { tint_paint: index, ..env })
            }
            Modifier::Effect(_) => {
                let start = rest.iter().rposition(|m| !matches!(m, Modifier::Effect(_))).map_or(0, |i| i + 1);
                let inner = self.mods(v, &mods[..start], p, env);
                mods[start..].iter().enumerate().fold(inner, |inner, (i, modifier)| {
                    let Modifier::Effect(op) = modifier else { unreachable!() };
                    self.visual_op(format!("{}#effect{}", v.id, start + i), op.as_ref().clone(), inner)
                })
            }
            Modifier::Mask(o) | Modifier::InvertedMask(o) => {
                let inner = self.mods(v, rest, p, env);
                let mask = self.view(&o.view, Prop { w: Some(inner.size.w), h: Some(inner.size.h) }, Env { axis: None, ..env });
                let (dx, dy) = ((inner.size.w - mask.size.w) * h_frac(o.alignment), (inner.size.h - mask.size.h) * v_frac(o.alignment));
                let mask = layer(format!("{}#mask-source{}", v.id, rest.len()), "mask_source", mask, |n| n.decoration = true);
                let mut n = Node::new(format!("{}#mask{}", v.id, rest.len()), "mask");
                n.inspect = false;
                n.composite = Some(Box::new(crate::effects::Composite::Mask { inverted: matches!(last, Modifier::InvertedMask(_)) }));
                with_content(Lay::new(inner.size, Some(n), vec![(0.0, 0.0, inner), (dx, dy, mask)]), 0)
            }
            Modifier::AlignedBackground(o) => {
                let inner = self.mods(v, rest, p, env);
                let bg = self.view(&o.view, Prop { w: Some(inner.size.w), h: Some(inner.size.h) }, Env { axis: None, ..env });
                let dx = (inner.size.w - bg.size.w) * h_frac(o.alignment);
                let dy = (inner.size.h - bg.size.h) * v_frac(o.alignment);
                with_content(Lay::new(inner.size, None, vec![(dx, dy, bg), (0.0, 0.0, inner)]), 1)
            }
            Modifier::Background(bg) => {
                let inner = self.mods(v, rest, p, env);
                let bgl = self.view(bg, Prop { w: Some(inner.size.w), h: Some(inner.size.h) }, Env { axis: None, ..env });
                let dx = (inner.size.w - bgl.size.w) * 0.5;
                let dy = (inner.size.h - bgl.size.h) * 0.5;
                with_content(Lay::new(inner.size, None, vec![(dx, dy, bgl), (0.0, 0.0, inner)]), 1)
            }
            Modifier::Overlay(o) => {
                let inner = self.mods(v, rest, p, env);
                let ovl = self.view(&o.view, Prop { w: Some(inner.size.w), h: Some(inner.size.h) }, Env { axis: None, ..env });
                let (dx, dy) = ((inner.size.w - ovl.size.w) * h_frac(o.alignment), (inner.size.h - ovl.size.h) * v_frac(o.alignment));
                with_content(Lay::new(inner.size, None, vec![(0.0, 0.0, inner), (dx, dy, ovl)]), 0)
            }
            Modifier::CornerRadius(r) => {
                let inner = self.mods(v, rest, p, env);
                layer(format!("{}#clip{}", v.id, rest.len()), "clip", inner, |n| n.clip_radius = Some(*r))
            }
            Modifier::Opacity(o) => {
                let inner = self.mods(v, rest, p, env);
                let mut out = layer(format!("{}#opacity{}", v.id, rest.len()), "opacity", inner, |n| n.opacity = *o as f32);
                if let Some(n) = out.node.as_deref_mut() {
                    self.stamp_node(n, env);
                }
                out
            }
            Modifier::Shadow(s) => {
                let inner = self.mods(v, rest, p, env);
                let (radius, shadow) = (lay_radius(&inner), Shadow { color: self.color(&s.color), radius: s.radius, dx: s.x, dy: s.y });
                layer(format!("{}#shadow{}", v.id, rest.len()), "shadow", inner, |n| {
                    n.radius = radius;
                    n.shadow = Some(shadow);
                })
            }
            Modifier::Font(f) => {
                let (size, weight, design) = self.font_size_weight(f);
                let mut env = Env { size: size as f32, base_weight: weight.value(), ..env };
                if let Some(d) = design {
                    env.design = d;
                    env.mono = d == 3;
                }
                self.mods(v, rest, p, env)
            }
            Modifier::FontWeight(w) => {
                let weight = env.weight.or(Some(w.value()));
                self.mods(v, rest, p, Env { weight, ..env })
            }
            Modifier::Strikethrough(b) => self.mods(v, rest, p, Env { strike: *b, ..env }),
            Modifier::MultilineTextAlignment(a) => {
                let align = match a {
                    Alignment::Center | Alignment::Top | Alignment::Bottom => TextAlign::Center,
                    a if h_frac(*a) == 1.0 => TextAlign::Trailing,
                    _ => TextAlign::Leading,
                };
                self.mods(v, rest, p, Env { align, ..env })
            }
            Modifier::ImageScale(img) => self.mods(v, rest, p, Env { img: *img, ..env }),
            Modifier::NavigationTitle(_) => self.mods(v, rest, p, env),
            Modifier::Glass(g) => {
                let inner = self.mods(v, rest, p, env);
                let radius = match (g.shape.shape, g.shape.corner_radius) {
                    (GlassShapeKind::Rect, None) => 0.0,
                    (GlassShapeKind::Rect | GlassShapeKind::RoundedRectangle, r) => r.unwrap_or(0.0),
                    _ => PILL,
                };
                let spec = self.glass_spec(g.variant, g.tint.as_ref(), g.interactive);
                layer(format!("{}#glass{}", v.id, rest.len()), "glass", inner, |n| {
                    n.radius = radius;
                    n.glass = Some(spec);
                })
            }
            // Layout keeps the original place; drawing and hit testing use the moved one.
            Modifier::Offset(o) => {
                let inner = self.mods(v, rest, p, env);
                Lay::new(inner.size, None, vec![(o.x, o.y, inner)])
            }
            Modifier::Gesture(g) => {
                let inner = self.mods(v, rest, p, env);
                gesture_layer(format!("{}#gesture{}", v.id, rest.len()), g, inner)
            }
            Modifier::ContainerShape(spec) => {
                let inner = self.mods(v, rest, p, env);
                self.has_container_shapes = true;
                container_shape_layer(format!("{}#cshape{}", v.id, rest.len()), **spec, inner)
            }
            Modifier::Identity(id) => {
                let mut inner = self.mods(v, rest, p, env);
                if let Some(n) = first_node(&mut inner) {
                    n.anchor = Some(id.clone());
                }
                inner
            }
            Modifier::Pin(pin) => {
                let mut inner = self.mods(v, rest, p, env);
                if let Some(n) = first_node(&mut inner) {
                    n.pinned = Some(pin.clone());
                }
                inner
            }
            Modifier::ScrollTargetLayout(enabled) => {
                let mut inner = self.mods(v, rest, p, env);
                if *enabled {
                    for (_, _, kid) in &mut inner.kids {
                        if let Some(n) = direct_target(kid) {
                            n.scroll_target = true;
                        }
                    }
                }
                inner
            }
            // Read by the owning container (grid columns, scroll margins and snap).
            Modifier::GridCellColumns(_)
            | Modifier::RowMinHeight(_)
            | Modifier::ListRow(_)
            | Modifier::ContentMargins(_)
            | Modifier::ScrollIndicators(_)
            | Modifier::ScrollTargetBehavior(_)
            | Modifier::ScrollPosition => self.mods(v, rest, p, env),
            Modifier::Animation(opt) => {
                let anim = opt.as_deref().map(|s| self.push_anim(s));
                self.mods(v, rest, p, Env { anim, ..env })
            }
            Modifier::Transition(t) => {
                let inner = self.mods(v, rest, p, env);
                let anim = env.anim.and_then(|i| self.anims.get(i as usize).cloned());
                anim_layer(format!("{}#transition{}", v.id, rest.len()), "transition", inner, anim, Some((**t).clone()), None)
            }
            Modifier::MatchedGeometry(m) => {
                let inner = self.mods(v, rest, p, env);
                let anim = env.anim.and_then(|i| self.anims.get(i as usize).cloned());
                anim_layer(format!("{}#match{}", v.id, rest.len()), "matched", inner, anim, None, Some((**m).clone()))
            }
            Modifier::ContentTransition(ct) => self.mods(v, rest, p, Env { content_transition: *ct, ..env }),
            Modifier::Kerning(k) => self.mods(v, rest, p, Env { extra: env.extra + *k as f32, ..env }),
            Modifier::Tracking(k) => self.mods(v, rest, p, Env { extra: env.extra + *k as f32, ..env }),
            Modifier::BaselineOffset(k) => self.mods(v, rest, p, Env { baseline: *k as f32, ..env }),
            Modifier::FontDesign(d) => {
                let design = d.map(design_code).unwrap_or(0);
                self.mods(v, rest, p, Env { design, mono: design == 3 || (d.is_none() && env.mono), ..env })
            }
            Modifier::FontWidth(w) => self.mods(v, rest, p, Env { width: w.map(width_code).unwrap_or(0), ..env }),
            Modifier::Monospaced(on) => self.mods(v, rest, p, Env { mono: *on, ..env }),
            Modifier::MonospacedDigit(on) => self.mods(v, rest, p, Env { digits: *on, ..env }),
            Modifier::TextScale(s) => self.mods(v, rest, p, Env { secondary: *s == onesim_protocol::TextScale::Secondary, ..env }),
            Modifier::TruncationMode(m) => self.mods(v, rest, p, Env { trunc: trunc_code(*m), ..env }),
            Modifier::LineLimit(lim) => {
                let (max_lines, reserve_lines) = lim.map(|l| {
                    let max = l.max.map(|n| n as usize);
                    let reserve = if l.reserves { l.max.or(l.min).map(|n| n as usize) } else { l.min.map(|n| n as usize) };
                    (max, reserve)
                }).unwrap_or((None, None));
                self.mods(v, rest, p, Env { max_lines, reserve_lines, ..env })
            }
            Modifier::Underline(s) => self.mods(v, rest, p, Env {
                underline: s.active,
                under_pat: pattern_code(s.pattern),
                under_color: s.color.as_ref().map(|c| self.color(c)),
                ..env
            }),
            Modifier::Italic(on) => self.mods(v, rest, p, Env { italic: *on, ..env }),
            Modifier::TextCase(c) => self.mods(v, rest, p, Env { case_fold: c.map(case_code).unwrap_or(0), ..env }),
            Modifier::AllowsTightening(on) => self.mods(v, rest, p, Env { tighten: *on, ..env }),
            Modifier::MinimumScaleFactor(k) => self.mods(v, rest, p, Env { min_scale: (*k as f32).clamp(0.01, 1.0), ..env }),
            Modifier::SymbolRenderingMode(m) => self.mods(v, rest, p, Env { sym_mode: m.map(symbol_mode_code).unwrap_or(0), ..env }),
            Modifier::SymbolVariableValueMode(m) => self.mods(v, rest, p, Env { sym_var: m.map(|v| if v == onesim_protocol::SymbolVariable::Draw { 2 } else { 1 }).unwrap_or(0), ..env }),
            Modifier::SymbolColorRenderingMode(m) => self.mods(v, rest, p, Env { sym_grad: m == &Some(onesim_protocol::SymbolColorMode::Gradient), ..env }),
            Modifier::AllowedDynamicRange(_) => self.mods(v, rest, p, env),
            Modifier::StrikethroughStyle(s) => self.mods(v, rest, p, Env {
                strike: s.active,
                strike_pat: pattern_code(s.pattern),
                strike_color: s.color.as_ref().map(|c| self.color(c)),
                ..env
            }),
            Modifier::Accessibility(ax) => {
                let mut inner = self.mods(v, rest, p, env);
                super::accessibility::stamp_on(&mut inner, ax);
                inner
            }
        }
    }

    /// Point size (at the current Dynamic Type size), weight and design of a `Font`.
    pub(super) fn font_size_weight(&self, f: &Font) -> (f64, onesim_protocol::Weight, Option<u8>) {
        match f {
            Font::Style { style, weight } => (style.size_at(self.dts), weight.unwrap_or(style.spec().1), None),
            Font::System { size, weight, design } => (*size, *weight, design.map(design_code)),
        }
    }

    #[inline(never)]
    pub(super) fn base(&mut self, v: &ViewNode, p: Prop, env: Env) -> Lay {
        let fg = env.fg.unwrap_or_else(|| self.semantic(Semantic::Primary));
        let child_env = Env { axis: None, ..env };
        match &v.kind {
            ViewKind::Text { text, runs } => {
                let wrap = p.w.filter(|w| w.is_finite()).map(|w| w.max(1.0) as f32);
                let mut spec = self.spec(&env);
                let spans: Vec<Span> = runs.iter().map(|r| self.span_of(r, &env, fg)).collect();
                // Line metrics come from the largest run. Keep the paragraph's truncation and tracking.
                if let Some(big) = spans.iter().max_by(|a, b| a.pt.total_cmp(&b.pt)) {
                    let keep = spec;
                    spec = FontSpec::new(big.pt, self.line_h(big.pt), big.weight, env.align);
                    spec.extra = keep.extra;
                    spec.trunc = keep.trunc;
                    spec.baseline = keep.baseline;
                    spec.case_fold = keep.case_fold;
                    spec.tighten = keep.tighten;
                    spec.min_scale = keep.min_scale;
                    spec.lines = keep.lines;
                    spec.underline = keep.underline;
                    spec.italic = keep.italic;
                    spec.line_pat = keep.line_pat;
                    spec.under_color = keep.under_color;
                    spec.strike_pat = keep.strike_pat;
                    spec.strike_color = keep.strike_color;
                    spec.reserve = keep.reserve;
                    spec.mono = big.mono;
                    spec.digits = big.digits;
                    spec.design = big.design;
                    spec.width = big.width;
                }
                let mut l = self.text_lay_runs(&v.id, text, spec, fg, wrap, spans);
                if let Some(n) = l.node.as_mut() {
                    if let Some(t) = n.text.as_mut() { t.strike = env.strike; }
                    n.content_paint = self.foreground_paint(env).gradient();
                }
                l
            }
            ViewKind::VStack { spacing, alignment, children, .. } => {
                self.stack(v, "vstack", children, *spacing, *alignment, true, p, child_env)
            }
            ViewKind::HStack { spacing, alignment, children, .. } => {
                self.stack(v, "hstack", children, *spacing, *alignment, false, p, child_env)
            }
            ViewKind::ZStack { alignment, children } => {
                let lays: Vec<Lay> = children.iter().map(|c| self.view(c, p, child_env)).collect();
                let size = Size {
                    w: lays.iter().map(|l| l.size.w).fold(0.0, f64::max),
                    h: lays.iter().map(|l| l.size.h).fold(0.0, f64::max),
                };
                let kids = lays
                    .into_iter()
                    .map(|l| ((size.w - l.size.w) * h_frac(*alignment), (size.h - l.size.h) * v_frac(*alignment), l))
                    .collect();
                Lay::new(size, Some(Node::new(&v.id, "zstack")), kids)
            }
            ViewKind::Spacer { min_length } => {
                let min = min_length.unwrap_or(0.0);
                let main = |prop: Option<f64>| prop.unwrap_or(0.0).max(min);
                let size = match env.axis {
                    Some(true) => Size { w: 0.0, h: main(p.h) },
                    Some(false) => Size { w: main(p.w), h: 0.0 },
                    None => Size { w: min, h: min },
                };
                Lay::new(size, Some(Node::new(&v.id, "spacer")), vec![])
            }
            ViewKind::Button { action, label, style, disabled, selected, .. } => match selected {
                Some(on) => self.toggle_button(v, *action, label, *on, *disabled, p, env),
                None => self.button(v, *action, label, *style, *disabled, p, env),
            },
            ViewKind::Color { color } => {
                let mut n = Node::new(&v.id, "color");
                n.fill = Some(self.color(color));
                Lay::new(greedy(p), Some(n), vec![])
            }
            ViewKind::Rectangle => {
                let mut n = Node::new(&v.id, "rectangle");
                n.fill = Some(fg);
                Lay::new(greedy(p), Some(n), vec![])
            }
            ViewKind::Shape { .. } => self.shape_view(v, p, env),
            ViewKind::Canvas { proposal, children, opaque } => {
                let size = greedy(p);
                let mut n = Node::new(&v.id, "canvas");
                n.custom_shape = Some(proposal.map_or(CustomShapeEval::Pending, CustomShapeEval::Evaluated));
                n.clip_radius = Some(0.0);
                if *opaque {
                    n.fill = Some(Color::rgba(0.0, 0.0, 0.0, 1.0));
                }
                let proposal = Prop { w: Some(size.w), h: Some(size.h) };
                let kids = children.iter().map(|child| {
                    let item = self.view(child, proposal, child_env);
                    let item = layer(format!("{}#draw", child.id), "canvas_draw", item, |n| n.decoration = true);
                    (0.0, 0.0, item)
                }).collect();
                let inner = Lay::new(size, Some(n), kids);
                self.visual_op(format!("{}#canvas", v.id), VisualOp::Group { opaque: false }, inner)
            }
            ViewKind::CanvasItem { child, origin, anchor, size } => {
                let proposal = size.map_or(Prop { w: None, h: None }, |[w, h]| Prop { w: Some(w), h: Some(h) });
                let lay = self.view(child, proposal, child_env);
                let [x, y] = *origin;
                let [u, v] = *anchor;
                let dx = x - u * lay.size.w + proposal.w.map_or(0.0, |w| (w - lay.size.w) * 0.5);
                let dy = y - v * lay.size.h + proposal.h.map_or(0.0, |h| (h - lay.size.h) * 0.5);
                Lay::new(greedy(p), None, vec![(dx, dy, lay)])
            }
            ViewKind::VisualEffect { child, effects, frame } => {
                let inner = self.view(child, p, env);
                let inner = effects.iter().enumerate().fold(inner, |inner, (i, op)| {
                    self.visual_op(format!("{}#visual{i}", v.id), op.clone(), inner)
                });
                layer(v.id.clone(), "visual_geometry", inner, |n| n.visual_frame = Some(*frame))
            }
            ViewKind::Divider => {
                let hl = self.hairline();
                let len = |x: Option<f64>| x.filter(|x| x.is_finite()).unwrap_or(0.0);
                let size = if env.axis == Some(false) { Size { w: hl, h: len(p.h) } } else { Size { w: len(p.w), h: hl } };
                let mut n = Node::new(&v.id, "divider");
                // UIKit's translucent separator (measured 198 on white).
                n.fill = Some(if self.dark { rgba(84, 84, 88, 0.6) } else { rgba(60, 60, 67, 0.29) });
                Lay::new(size, Some(n), vec![])
            }
            ViewKind::Rn { component, .. } if component == onesim_protocol::EXPO_UI_RN_HOST => {
                let limit = |axis: Option<f64>| axis.filter(|n| n.is_finite());
                let embedded = crate::layout::rn::embedded(v, (limit(p.w), limit(p.h)), self.params, self.fonts);
                let mut laid = Lay::new(Size { w: embedded.size.0, h: embedded.size.1 }, None, vec![]);
                laid.embedded = Some(embedded);
                laid
            }
            ViewKind::Empty | ViewKind::Rn { .. } | ViewKind::Compose { .. } => Lay::new(Size::default(), Some(Node::new(&v.id, "empty")), vec![]),
            // Only the root's presentation layer is drawn (see `layout`); nested, it is transparent.
            ViewKind::Presentation { content, .. } => self.view(content, p, env),
            ViewKind::Scroll { axis, child, .. } => self.scroll(v, *axis == Axis::Vertical, child, p, env),
            ViewKind::Grid { alignment, horizontal_spacing, vertical_spacing, rows } => {
                self.grid(v, *alignment, *horizontal_spacing, *vertical_spacing, rows, p, child_env)
            }
            ViewKind::LazyGrid { axis, alignment, spacing, tracks, children } => {
                self.lazy_grid(v, *axis, *alignment, *spacing, tracks, children, p, child_env)
            }
            ViewKind::Geometry { proposal, child } => self.geometry(v, *proposal, child, p, child_env),
            ViewKind::ViewThatFits { axes, children } => self.view_that_fits(v, *axes, children, p, child_env),
            ViewKind::List { style, sections } => self.list(v, *style, sections, p, env),
            ViewKind::TabView { selected, tabs, content } => self.tab_view(v, *selected, tabs, content, p, env),
            ViewKind::PageTabView { selected, pages, index_display, index_background } => {
                self.page_tab_view(v, *selected, pages, (*index_display, *index_background), p, child_env)
            }
            ViewKind::Navigation { title, display_mode, back, chrome, content } => {
                self.navigation(v, title.as_deref(), *display_mode, back.as_deref(), chrome.as_deref(), content, p, env)
            }
            ViewKind::Toggle { is_on, action, label, disabled } => self.toggle(v, *is_on, *action, label, *disabled, p, env),
            ViewKind::Stepper { label, decrement, increment } => self.stepper(v, label, *decrement, *increment, p, env),
            ViewKind::TextField { placeholder, text, field, secure, disabled, input } =>
                self.text_field(v, placeholder, text, *field, *secure, *disabled, input, p, env),
            ViewKind::Picker { label, style, options, selected, open, disabled } => {
                match style {
                    PickerStyle::Segmented => self.segmented(v, options, *selected, *disabled, p),
                    PickerStyle::Wheel => self.wheel(v, label, options, *selected, *disabled, p),
                    PickerStyle::Menu => self.menu_picker(v, label, options, *selected, *open, *disabled, p, child_env),
                }
            }
            ViewKind::Wheel { columns, width, height, disabled, popover } => self.multi_wheel(v, columns, (*width, *height), *disabled, *popover, p),
            ViewKind::Slider { value, min, max, field, disabled, .. } => self.slider(v, *value, (*min, *max), *field, *disabled, p),
            ViewKind::Progress { value, label, current_value_label } => {
                self.progress(v, *value, label.as_deref(), current_value_label.as_deref(), p, child_env)
            }
            ViewKind::Gauge { style, value, label, current_value_label, minimum_value_label, maximum_value_label } => {
                let labels = GaugeLabels {
                    label: label.as_deref(),
                    current: current_value_label.as_deref(),
                    min: minimum_value_label.as_deref(),
                    max: maximum_value_label.as_deref(),
                };
                self.gauge(v, *style, *value, labels, p, child_env)
            }
            ViewKind::Image {
                system_name, resizable, content_mode, scale, rendering_mode, symbol_rendering_mode, interpolation, antialiased,
                symbol_color_rendering_mode, symbol_variable_value_mode, bitmap, ..
            } => {
                let mut env = env;
                if let Some(m) = symbol_rendering_mode {
                    env.sym_mode = symbol_mode_code(*m);
                }
                if let Some(m) = rendering_mode {
                    match m {
                        onesim_protocol::TemplateMode::Template => {
                            env.sym_mode = 0;
                            env.sym_grad = false;
                            env.sym_original = false;
                        }
                        onesim_protocol::TemplateMode::Original => env.sym_original = true,
                    }
                }
                if let Some(m) = interpolation {
                    env.img_filter = match m {
                        onesim_protocol::Interpolation::None => 0,
                        onesim_protocol::Interpolation::Low => 1,
                        onesim_protocol::Interpolation::Medium | onesim_protocol::Interpolation::High => 2,
                    };
                }
                if *antialiased == Some(false) {
                    env.img_filter = 0;
                }
                if let Some(m) = symbol_color_rendering_mode {
                    env.sym_grad = *m == onesim_protocol::SymbolColorMode::Gradient;
                }
                if let Some(m) = symbol_variable_value_mode {
                    env.sym_var = if *m == onesim_protocol::SymbolVariable::Draw { 2 } else { 1 };
                }
                if let Some(bm) = bitmap {
                    self.bitmap_image(v, bm, *resizable, *content_mode, env.img_filter, p)
                } else {
                    self.image(v, system_name, *resizable, *content_mode, scale.unwrap_or(env.img), p, env, fg)
                }
            }
            ViewKind::Label { title, icon } => self.label(v, title, icon, p, child_env),
        }
    }

    pub(super) fn span_of(&self, r: &onesim_protocol::Run, env: &Env, fg: Color) -> Span {
        let (mut pt, weight, design) = match &r.font {
            Some(f) => {
                let (size, w, d) = self.font_size_weight(f);
                (size as f32, r.font_weight.map(|w| w.value()).unwrap_or(w.value()), d.unwrap_or(env.design))
            }
            None => (env.size, r.font_weight.map(|w| w.value()).unwrap_or(env.weight.unwrap_or(env.base_weight)), env.design),
        };
        let secondary = match r.text_scale {
            Some(onesim_protocol::TextScale::Secondary) => true,
            Some(onesim_protocol::TextScale::Default) => false,
            None => env.secondary,
        };
        if secondary {
            pt *= crate::text::SECONDARY_TEXT_SCALE;
        }
        let design = r.font_design.map(design_code).unwrap_or(design);
        let width = r.font_width.map(width_code).unwrap_or(env.width);
        // A run's kerning and tracking add to the parent's. They do not replace it.
        let extra = env.extra + r.kerning.unwrap_or(0.0) as f32 + r.tracking.unwrap_or(0.0) as f32;
        let color = r.foreground_color.as_ref().map(|c| self.color(c)).unwrap_or(fg);
        Span {
            italic: r.italic || env.italic,
            underline: r.underline || env.underline,
            strike: r.strikethrough || env.strike,
            mono: r.monospaced || env.mono || design == 3,
            extra,
            design,
            width,
            digits: r.monospaced_digit || env.digits,
            baseline: r.baseline_offset.map(|b| b as f32).unwrap_or(env.baseline),
            line_pat: r.underline_pattern.map(pattern_code).unwrap_or(env.under_pat),
            under_color: r.underline_color.as_ref().map(|c| self.color(c)).or(env.under_color),
            strike_pat: r.strikethrough_pattern.map(pattern_code).unwrap_or(env.strike_pat),
            strike_color: r.strikethrough_color.as_ref().map(|c| self.color(c)).or(env.strike_color),
            ..Span::basic(&r.text, pt, weight, color)
        }
    }

    /// Tighten tracking, then scale down to `min_scale`, when the text is wider than the wrap or uses more lines than the cap.
    pub(super) fn fit_text(&mut self, text: &str, spec: FontSpec, wrap: Option<f32>, spans: Vec<Span>) -> (FontSpec, Vec<Span>, Option<(f64, f64)>) {
        if !spec.tighten && spec.min_scale >= 0.999 {
            return (spec, spans, None);
        }
        let Some(limit) = wrap else { return (spec, spans, None) };
        let measure_overflow = |fonts: &mut Fonts, spec: &FontSpec, spans: &[Span]| -> (bool, (f64, f64)) {
            let probe = FontSpec { lines: None, ..*spec };
            let (w, h) = fonts.measure(text, &probe, Some(limit), spans);
            if w > limit as f64 + 0.51 {
                return (true, (w, h));
            }
            let Some(max) = spec.lines else { return (false, (w, h)) };
            let cap = f64::from(spec.line_height) + (max.saturating_sub(1) as f64) * f64::from(spec.pitch);
            (h > cap + 0.51, (w, h))
        };
        let (over, sz) = measure_overflow(self.fonts, &spec, &spans);
        if !over {
            return (spec, spans, Some(sz));
        }
        let overflows = |fonts: &mut Fonts, spec: &FontSpec, spans: &[Span]| measure_overflow(fonts, spec, spans).0;
        let (mut spec, mut spans) = (spec, spans);
        if spec.tighten {
            let max_pull = 0.05 * spec.pt;
            let mut lo = 0.0f32;
            let mut hi = max_pull;
            for _ in 0..7 {
                let mid = (lo + hi) / 2.0;
                let (ts, tsp) = pull_tracking(&spec, &spans, mid);
                if overflows(self.fonts, &ts, &tsp) { lo = mid; } else { hi = mid; }
            }
            (spec, spans) = pull_tracking(&spec, &spans, hi);
        }
        if spec.min_scale < 0.999 && overflows(self.fonts, &spec, &spans) {
            let floor = spec.min_scale;
            let mut lo = floor;
            let mut hi = 1.0f32;
            for _ in 0..7 {
                let mid = (lo + hi) / 2.0;
                let (ts, tsp) = self.scale_type(&spec, &spans, mid);
                if overflows(self.fonts, &ts, &tsp) { hi = mid; } else { lo = mid; }
            }
            let (trial, trial_spans) = self.scale_type(&spec, &spans, lo);
            let k = if overflows(self.fonts, &trial, &trial_spans) { floor } else { lo };
            (spec, spans) = self.scale_type(&spec, &spans, k);
        }
        (spec, spans, None)
    }

    pub(super) fn scale_type(&self, spec: &FontSpec, spans: &[Span], k: f32) -> (FontSpec, Vec<Span>) {
        let mut s = *spec;
        let pt = (spec.pt * k).max(1.0);
        let ratio = if spec.pt > 0.0 { spec.size / spec.pt } else { 1.0 };
        s.pt = pt;
        s.size = pt * ratio;
        s.line_height = self.line_h(pt);
        s.pitch = if pt > 34.0 { pt * 1.12 + 2.64 } else { s.line_height.max(pt * 22.0 / 17.0) };
        s.extra *= k;
        s.baseline *= k;
        s.tracking = crate::text::tracking_em(pt, s.weight);
        let spans = spans.iter().map(|sp| Span { pt: sp.pt * k, extra: sp.extra * k, baseline: sp.baseline * k, ..sp.clone() }).collect();
        (s, spans)
    }

    pub(super) fn bitmap_image(&mut self, v: &ViewNode, bm: &onesim_protocol::Bitmap, resizable: bool, mode: Option<ContentMode>, filter: u8, p: Prop) -> Lay {
        let pixmap = crate::image::load(&bm.path);
        let scale = if bm.scale > 0.0 { bm.scale } else { 1.0 };
        let (pw, ph) = pixmap.as_ref().map(|m| (m.width() as f64 / scale, m.height() as f64 / scale)).unwrap_or((1.0, 1.0));
        let (nw, nh) = (if bm.width > 0.0 { bm.width } else { pw }, if bm.height > 0.0 { bm.height } else { ph });
        let ratio = if nh > 0.0 { nw / nh } else { 1.0 };
        let fin = |x: Option<f64>| x.filter(|x| x.is_finite());
        let (pw, ph) = (fin(p.w), fin(p.h));
        let (size, dest) = if !resizable {
            (Size { w: nw, h: nh }, Rect { x: 0.0, y: 0.0, w: nw, h: nh })
        } else {
            match mode {
                None => {
                    let s = Size { w: pw.unwrap_or(nw), h: ph.unwrap_or(nh) };
                    (s, Rect { x: 0.0, y: 0.0, w: s.w, h: s.h })
                }
                Some(m) => {
                    let (bw, bh) = match (pw, ph) {
                        (Some(w), Some(h)) => (w, h),
                        (Some(w), None) => (w, w / ratio),
                        (None, Some(h)) => (h * ratio, h),
                        (None, None) => (nw, nh),
                    };
                    let by_w = (bw, bw / ratio);
                    let by_h = (bh * ratio, bh);
                    let fit = if by_w.1 <= bh { by_w } else { by_h };
                    let fill = if by_w.1 >= bh { by_w } else { by_h };
                    match m {
                        ContentMode::Fit => (Size { w: fit.0, h: fit.1 }, Rect { x: 0.0, y: 0.0, w: fit.0, h: fit.1 }),
                        ContentMode::Fill => {
                            let dx = (bw - fill.0) / 2.0;
                            let dy = (bh - fill.1) / 2.0;
                            (Size { w: bw, h: bh }, Rect { x: dx, y: dy, w: fill.0, h: fill.1 })
                        }
                    }
                }
            }
        };
        let mut n = Node::new(&v.id, "image");
        n.role = Role::Image;
        if let Some(pixmap) = pixmap {
            n.image = Some(crate::scene::ImageRun { pixmap, dest, rotation: 0.0, filter });
        }
        Lay::new(size, Some(n), vec![])
    }

    /// SF Symbol. Non-resizable images are glyph-sized from the font; resizable ones stretch to
    /// the proposal (or fit / fill it keeping the symbol's aspect ratio).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn image(&mut self, v: &ViewNode, name: &str, resizable: bool, mode: Option<ContentMode>, scale: ImageScale, p: Prop, env: Env, fg: Color) -> Lay {
        let sym_mode = if env.sym_original && env.sym_mode == 0 { 3 } else { env.sym_mode };
        let spec = SymbolSpec {
            name: name.to_string(),
            point_size: env.size as f64,
            weight: env.weight.unwrap_or(env.base_weight),
            scale,
            mode: sym_mode,
            variable: env.sym_var,
            gradient: env.sym_grad,
        };
        let nat = symbols::metrics(&spec);
        let fin = |x: Option<f64>| x.filter(|x| x.is_finite());
        let (nw, nh) = nat.map_or((env.size as f64, env.size as f64), |m| (m.w, m.h));
        let (size, glyph) = if !resizable {
            (Size { w: nw, h: nh }, (nw, nh))
        } else {
            let (pw, ph) = (fin(p.w), fin(p.h));
            let ratio = nw / nh;
            match mode {
                None => {
                    let s = Size { w: pw.unwrap_or(nw), h: ph.unwrap_or(nh) };
                    (s, (s.w, s.h))
                }
                Some(m) => {
                    let (bw, bh) = match (pw, ph) {
                        (Some(w), Some(h)) => (w, h),
                        (Some(w), None) => (w, w / ratio),
                        (None, Some(h)) => (h * ratio, h),
                        (None, None) => (nw, nh),
                    };
                    let by_w = (bw, bw / ratio);
                    let by_h = (bh * ratio, bh);
                    let fit = if by_w.1 <= bh { by_w } else { by_h };
                    let fill = if by_w.1 >= bh { by_w } else { by_h };
                    match m {
                        // Fit shrink-wraps the symbol; fill occupies the whole proposal and crops.
                        ContentMode::Fit => (Size { w: fit.0, h: fit.1 }, fit),
                        ContentMode::Fill => (Size { w: bw, h: bh }, fill),
                    }
                }
            }
        };
        let mut n = Node::new(&v.id, "image");
        n.role = Role::Image;
        n.label = Some(name.to_string());
        if nat.is_some() {
            n.symbol = Some(SymbolRun { spec, color: fg, size: glyph });
        } else {
            // Placeholder: rounded-square outline, reported as `unsupported_symbol`.
            self.unsupported.insert(name.to_string());
            n.stroke = Some((fg, 1.0));
            n.radius = size.w.min(size.h) * 0.2;
        }
        Lay::new(size, Some(n), vec![])
    }

}
