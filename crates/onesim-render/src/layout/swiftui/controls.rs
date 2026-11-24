use super::*;

impl Ctx<'_> {
    pub(super) fn label(&mut self, v: &ViewNode, title: &ViewNode, icon: &ViewNode, p: Prop, env: Env) -> Lay {
        let icon_fg = env.fg.or_else(|| env.in_row.then(|| self.semantic(Semantic::Accent)));
        // ponytail: list rows force imageScale large on the icon, an explicit imageScale on the Label is ignored there.
        let img = if env.in_row { ImageScale::Large } else { env.img };
        let il = self.view(icon, Prop { w: None, h: None }, Env { fg: icon_fg, img, ..env });
        let (col, gap) = if env.in_row { (LABEL_ICON_COL, LABEL_ROW_GAP) } else { (il.size.w, LABEL_GAP) };
        let tw = p.w.filter(|w| w.is_finite()).map(|w| (w - col - gap).max(0.0));
        let tl = self.view(title, Prop { w: tw, h: None }, env);
        let h = il.size.h.max(tl.size.h);
        let w = col + gap + tl.size.w;
        let ix = (col - il.size.w) / 2.0;
        let mut n = Node::new(&v.id, "label");
        n.label = first_text(title);
        Lay::new(
            Size { w, h },
            Some(n),
            vec![(ix, (h - il.size.h) / 2.0, il), (col + gap, (h - tl.size.h) / 2.0, tl)],
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn button(&mut self, v: &ViewNode, action: u64, label: &ViewNode, style: ButtonStyle, disabled: bool, role: Option<onesim_protocol::ButtonRole>, p: Prop, env: Env) -> Lay {
        let child_env = Env { axis: None, ..env };
        let destructive = role == Some(onesim_protocol::ButtonRole::Destructive);
        let explicit_tint = self.tint_color(env);
        let accent = if destructive { self.destructive() } else { explicit_tint.unwrap_or_else(|| self.semantic(Semantic::Accent)) };
        let dim = self.semantic(Semantic::Tertiary);
        let mut n = Node::new(&v.id, "button");
        n.role = Role::Button;
        n.action = (!disabled).then_some(action);
        n.disabled = disabled;
        n.label = first_text(label);
        let label = match (&label.kind, env.in_toolbar, style) {
            (ViewKind::Label { icon, .. }, true, ButtonStyle::Automatic) => icon.as_ref(),
            _ => label,
        };
        match style {
            ButtonStyle::Automatic | ButtonStyle::Plain => {
                let (fg, tint) = match (style, disabled) {
                    (_, true) => (dim, None),
                    (ButtonStyle::Automatic, _) if env.in_toolbar => {
                        let c = if destructive { self.destructive() } else { explicit_tint.unwrap_or_else(|| self.semantic(Semantic::Primary)) };
                        (c, if destructive { Some(c) } else { explicit_tint })
                    }
                    (ButtonStyle::Automatic, _) => (accent, Some(accent)),
                    _ => (env.fg.unwrap_or_else(|| self.semantic(Semantic::Primary)), None),
                };
                let l = self.view(label, p, Env { fg: Some(fg), tint, ..child_env });
                Lay::new(l.size, Some(n), vec![(0.0, 0.0, l)])
            }
            ButtonStyle::Bordered | ButtonStyle::BorderedProminent => {
                let prominent = style == ButtonStyle::BorderedProminent;
                let bordered_bg = if destructive || explicit_tint.is_some() {
                    Color { a: if self.dark { 0.24 } else { 0.15 }, ..accent }
                } else {
                    self.control_fill(0.16)
                };
                let (fg, bg) = match (prominent, disabled) {
                    (_, true) => (dim, self.control_fill(0.16)),
                    (true, _) => (rgba(255, 255, 255, 1.0), if self.desat { let g = if self.dark { ALERT_DESAT.1 } else { ALERT_DESAT.0 }; rgba(g, g, g, 1.0) } else { accent }),
                    (false, _) => (accent, bordered_bg),
                };
                let (hp, vp) = (12.0, 7.0);
                let inner_p = Prop { w: p.w.map(|w| (w - 2.0 * hp).max(0.0)), h: None };
                let l = self.view(label, inner_p, Env { fg: Some(fg), tint: None, ..child_env });
                let size = Size { w: l.size.w + 2.0 * hp, h: l.size.h + 2.0 * vp };
                n.fill = Some(bg);
                n.radius = PILL;
                Lay::new(size, Some(n), vec![(hp, vp, l)])
            }
            ButtonStyle::PasteButton => {
                // UIPasteControl: white label on an accent rounded rect (gray under a presented layer), 34.67 tall.
                let bg = if self.desat { let g = if self.dark { PASTE_DESAT.1 } else { PASTE_DESAT.0 }; rgba(g, g, g, 1.0) } else { accent };
                let fg = rgba(255, 255, 255, 1.0);
                let l = self.view(label, Prop { w: None, h: None }, Env { fg: Some(fg), tint: None, ..child_env });
                let size = Size { w: l.size.w + 2.0 * PASTE_PAD.0, h: PASTE_PAD.1 };
                n.fill = Some(bg);
                n.radius = PASTE_RADIUS;
                let (lw, lh) = (l.size.w, l.size.h);
                Lay::new(size, Some(n), vec![((size.w - lw) / 2.0, (size.h - lh) / 2.0, l)])
            }
            ButtonStyle::Glass | ButtonStyle::GlassProminent => {
                let prominent = style == ButtonStyle::GlassProminent;
                let fg = match (prominent, disabled) {
                    (_, true) => dim,
                    (true, _) => rgba(255, 255, 255, 1.0),
                    (false, _) => self.semantic(Semantic::Primary),
                };
                let (hp, vp) = (12.0, 7.0);
                let inner_p = Prop { w: p.w.map(|w| (w - 2.0 * hp).max(0.0)), h: None };
                let l = self.view(label, inner_p, Env { fg: Some(fg), tint: None, ..child_env });
                let size = Size { w: l.size.w + 2.0 * hp, h: l.size.h + 2.0 * vp };
                n.glass = Some(self.glass_spec(GlassVariant::Regular, prominent.then_some(onesim_protocol::Color::Semantic { semantic: Semantic::Accent }).as_ref(), true));
                n.radius = PILL;
                Lay::new(size, Some(n), vec![(hp, vp, l)])
            }
            ButtonStyle::NavigationLink => {
                let fg = if disabled { dim } else { env.fg.unwrap_or_else(|| self.semantic(Semantic::Primary)) };
                // 7x12 at body size, growing with Dynamic Type (21.7x37.3, 8.6pt inset at AX5; measured).
                let k = (onesim_protocol::TextStyle::Body.size_at(self.dts) / 17.0).max(1.0);
                let (chev_w, chev_h, inset) = (7.0 * k, 12.0 * k, 2.3 + 2.97 * (k - 1.0));
                let inner_p = Prop { w: p.w.filter(|w| w.is_finite()).map(|w| (w - chev_w - 12.0).max(0.0)), h: None };
                let l = self.view(label, inner_p, Env { fg: Some(fg), tint: None, ..child_env });
                let w = p.w.filter(|w| w.is_finite()).unwrap_or(l.size.w + 12.0 + chev_w);
                let h = l.size.h.max(chev_h);
                let mut c = Node::new(format!("{}#chevron", v.id), "chevron");
                c.glyph = Some((Glyph::Chevron, self.semantic(Semantic::Tertiary)));
                c.inspect = false;
                let cl = Lay::new(Size { w: chev_w, h: chev_h }, Some(c), vec![]);
                let ly = (h - l.size.h) / 2.0;
                Lay::new(Size { w, h }, Some(n), vec![(0.0, ly, l), (w - chev_w - inset, (h - chev_h) / 2.0, cl)])
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn toggle(&mut self, v: &ViewNode, on: bool, action: u64, label: &ViewNode, disabled: bool, p: Prop, env: Env) -> Lay {
        let child_env = Env { axis: None, ..env };
        let fin = p.w.filter(|w| w.is_finite());
        let l = self.view(label, Prop { w: fin.map(|w| (w - SWITCH.0 - 8.0).max(0.0)), h: None }, child_env);
        let w = fin.unwrap_or(l.size.w + 8.0 + SWITCH.0);
        let h = l.size.h.max(SWITCH.1);
        let mut sw = Node::new(format!("{}#switch", v.id), "switch");
        let knob = switch_style(&mut sw, &v.id, self.dark, on, disabled, on.then(|| self.tint_color(env)).flatten(), None);
        sw.label = first_text(label);
        sw.action = (!disabled).then_some(action);
        let (kw, kh) = KNOB;
        let kx = if on { SWITCH.0 - 2.0 - kw } else { 2.0 };
        let kl = Lay::new(Size { w: kw, h: kh }, Some(knob), vec![]);
        let sl = Lay::new(Size { w: SWITCH.0, h: SWITCH.1 }, Some(sw), vec![(kx, (SWITCH.1 - kh) / 2.0, kl)]);
        let ly = (h - l.size.h) / 2.0;
        Lay::new(Size { w, h }, Some(Node::new(&v.id, "toggle")), vec![(0.0, ly, l), (w - SWITCH.0 + 2.0, (h - SWITCH.1) / 2.0, sl)])
    }

    pub(super) fn stepper(&mut self, v: &ViewNode, label: &ViewNode, dec: Option<u64>, inc: Option<u64>, p: Prop, env: Env) -> Lay {
        let child_env = Env { axis: None, ..env };
        let fin = p.w.filter(|w| w.is_finite());
        let l = self.view(label, Prop { w: fin.map(|w| (w - STEPPER.0 - 8.0).max(0.0)), h: None }, child_env);
        let w = fin.unwrap_or(l.size.w + 8.0 + STEPPER.0);
        let h = l.size.h.max(STEPPER.1);
        let mut root = Node::new(&v.id, "stepper");
        root.role = Role::Stepper;
        root.label = first_text(label);
        let mut ctl = Node::new(format!("{}#control", v.id), "stepper-control");
        ctl.fill = Some(self.control_fill(0.12));
        ctl.radius = PILL;
        ctl.inspect = false;
        let half = STEPPER.0 / 2.0;
        let mut kids = vec![];
        for (name, action, glyph, x) in [("decrement", dec, Glyph::Minus, 0.0), ("increment", inc, Glyph::Plus, half)] {
            let mut b = Node::new(format!("{}#{name}", v.id), "button");
            b.role = Role::Button;
            b.label = Some(name.into());
            b.action = action;
            b.disabled = action.is_none();
            let c = if action.is_some() { self.semantic(Semantic::Primary) } else { self.semantic(Semantic::Tertiary) };
            b.glyph = Some((glyph, c));
            kids.push((x, 0.0, Lay::new(Size { w: half, h: STEPPER.1 }, Some(b), vec![])));
        }
        let mut div = Node::new(format!("{}#divider", v.id), "divider");
        div.fill = Some(self.semantic(Semantic::Tertiary));
        div.inspect = false;
        kids.push((half - self.hairline() / 2.0, 8.0, Lay::new(Size { w: self.hairline(), h: 16.0 }, Some(div), vec![])));
        let cl = Lay::new(Size { w: STEPPER.0, h: STEPPER.1 }, Some(ctl), kids);
        let ly = (h - l.size.h) / 2.0;
        Lay::new(Size { w, h }, Some(root), vec![(0.0, ly, l), (w - STEPPER.0, (h - STEPPER.1) / 2.0, cl)])
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn text_field(&mut self, v: &ViewNode, placeholder: &str, text: &str, field: u64, secure: bool, disabled: bool, input: &TextInputOptions, p: Prop, env: Env) -> Lay {
        let w = p.w.filter(|w| w.is_finite()).unwrap_or(120.0);
        let focused = !disabled && self.focus == Some(v.id.as_str());
        let multiline = input.editor || input.axis == Some(Axis::Vertical);
        let mut n = Node::new(&v.id, "text_field");
        n.role = Role::TextField;
        n.field = (!disabled).then_some(field);
        n.disabled = disabled;
        n.secure = secure;
        n.input = Some(Box::new(input.clone()));
        n.value = Some(text.to_string());
        n.label = Some(placeholder.to_string());
        n.focused = focused;

        let spec = self.spec(&Env { max_lines: None, reserve_lines: None, ..env });
        let line_h = f64::from(spec.line_height);
        let (pad_x, pad_y, min_h) = match input.input_style {
            Some(TextInputStyle::RoundedBorder) => {
                n.radius = 6.0;
                n.fill = Some(self.semantic(Semantic::Background));
                n.stroke = Some((self.semantic(Semantic::Separator), self.hairline()));
                (7.0, 5.0, 34.0)
            }
            Some(TextInputStyle::Bordered) => {
                n.stroke = Some((self.semantic(Semantic::Separator), 1.0));
                (4.0, 3.0, 28.0)
            }
            _ if input.editor => {
                n.fill = Some(self.semantic(Semantic::Background));
                (5.0, 8.0, 38.0)
            }
            Some(TextInputStyle::Plain) => (0.0, 0.0, (line_h * self.scale).ceil() / self.scale),
            _ => (0.0, 0.0, 22.0),
        };
        if self.ios >= IosVersion::Ios27 && n.stroke.is_some() {
            match input.input_border_shape {
                Some(TextInputBorderShape::Capsule) => n.radius = PILL,
                Some(TextInputBorderShape::RoundedRectangle) => n.radius = 6.0,
                Some(TextInputBorderShape::Automatic) | None => {}
            }
        }
        n.clip_radius = Some(n.radius);
        let inner_w = (w - 2.0 * pad_x).max(0.0);
        let wrap = multiline.then_some(inner_w.max(1.0) as f32);
        let empty = text.is_empty();
        let fg = env.fg.unwrap_or_else(|| self.semantic(Semantic::Primary));
        let placeholder_fg = self.semantic(if input.search_field { Semantic::Secondary } else { Semantic::Tertiary });
        let (d, pitch, x0) = SECURE_BULLET;
        let bullets = if secure { text.graphemes(true).count() } else { 0 };
        let shown = if empty { placeholder } else if secure { "" } else { text };
        let shown = if !multiline && shown.contains(['\n', '\r']) {
            Cow::Owned(shown.replace(['\n', '\r'], " "))
        } else {
            Cow::Borrowed(shown)
        };
        let prompt = input.prompt.as_deref().filter(|_| empty);
        let mut t = match prompt {
            Some(prompt) => self.view(prompt, Prop { w: wrap.map(f64::from), h: None }, Env { fg: Some(placeholder_fg), foreground_paint: None, ..env }),
            None => self.text_lay(&format!("{}#text", v.id), &shown, spec, if empty { placeholder_fg } else { fg }, wrap),
        };
        let line_pitch = f64::from(spec.pitch);
        let line_space = |lines: usize| line_h + lines.saturating_sub(1) as f64 * line_pitch;
        let natural_h = t.size.h;
        let h = if input.editor {
            p.h.filter(|h| h.is_finite()).unwrap_or_else(|| (natural_h + 2.0 * pad_y).max(min_h)).max(0.0)
        } else if multiline {
            let cap = env.max_lines.map_or(f64::INFINITY, line_space);
            (natural_h.min(cap).max(env.reserve_lines.map_or(0.0, line_space)) + 2.0 * pad_y).max(min_h)
        } else {
            (natural_h + 2.0 * pad_y).max(min_h)
        };
        let max_scroll = (natural_h + 2.0 * pad_y - h).max(0.0);
        let offset = self.scroll.get(&v.id).map_or(if focused { max_scroll } else { 0.0 }, |(_, y)| *y).clamp(0.0, max_scroll);
        if multiline && max_scroll > 0.0 && !disabled {
            n.scroll = Some(ScrollInfo {
                vertical: true, offset: (0.0, offset), max: (0.0, max_scroll),
                snap: None, content: None, page: None, aligned: false,
            });
        }
        let y = if multiline { pad_y - offset } else { (h - t.size.h) / 2.0 };
        let (caret_x, caret_y) = if bullets > 0 {
            (x0 + (bullets - 1) as f64 * pitch + d, 0.0)
        } else if empty {
            (0.0, 0.0)
        } else if focused {
            self.fonts.text_end(&shown, &spec, wrap)
        } else {
            (t.size.w, 0.0)
        };
        let overflow = if multiline || empty { 0.0 } else { (caret_x - (inner_w - 2.0)).max(0.0) };
        let ink_w = if bullets == 0 { t.size.w } else { caret_x };
        let align = match env.align {
            TextAlign::Leading | TextAlign::Justify | TextAlign::Left => 0.0,
            TextAlign::Center => 0.5,
            TextAlign::Trailing | TextAlign::End => 1.0,
        };
        let x = pad_x + if multiline { 0.0 } else { (inner_w - ink_w).max(0.0) * align };
        if let Some(tn) = t.node.as_mut() {
            tn.inspect = false;
            if let Some(run) = tn.text.as_mut() {
                run.offset_x = -overflow as f32;
            }
        }
        if multiline && prompt.is_none() {
            // Text draws using its bounds as the wrap width. Keep the original proposal.
            t.size.w = inner_w;
        }
        let mut kids = vec![];
        if bullets == 0 {
            kids.push((x, y, t));
        } else {
            // Emit only the visible dots, even for a long paste.
            let start = ((overflow - x0 - d) / pitch).floor().max(0.0) as usize;
            let end = ((overflow + inner_w - x0) / pitch).ceil().max(0.0) as usize;
            for i in start..bullets.min(end) {
                let mut b = Node::new(format!("{}#bullet{i}", v.id), "bullet");
                b.fill = Some(fg);
                b.radius = PILL;
                b.inspect = false;
                kids.push((x + x0 + i as f64 * pitch - overflow, (h - d) / 2.0, Lay::new(Size { w: d, h: d }, Some(b), vec![])));
            }
        }
        if focused {
            let mut c = Node::new(format!("{}#caret", v.id), "caret");
            c.fill = Some(env.tint.unwrap_or_else(|| self.semantic(Semantic::Accent)));
            c.radius = 1.0;
            c.inspect = false;
            let caret_h = line_h.min((h - 2.0).max(0.0));
            let caret_origin = if empty { pad_x + inner_w * align } else { x + caret_x - overflow };
            let x = (caret_origin + 1.0).clamp(pad_x, pad_x + (inner_w - 2.0).max(0.0));
            let y = if multiline { y + caret_y } else { (h - caret_h) / 2.0 };
            kids.push((x, y, Lay::new(Size { w: 2.0, h: caret_h }, Some(c), vec![])));
        }
        Lay::new(Size { w, h }, Some(n), kids)
    }

    /// An SF Symbol leaf at `pt` points.
    pub(super) fn symbol_lay(&mut self, id: String, name: &str, pt: f64, weight: u16, color: Color) -> Lay {
        let spec = SymbolSpec { name: name.into(), point_size: pt, weight, scale: ImageScale::Medium, ..SymbolSpec::default() };
        let (w, h) = symbols::metrics(&spec).map_or((pt, pt), |m| (m.w, m.h));
        let mut n = Node::new(id, "image");
        n.symbol = Some(SymbolRun { spec, color, size: (w, h) });
        n.inspect = false;
        Lay::new(Size { w, h }, Some(n), vec![])
    }

    /// Menu `Picker` row: label leading; selected value and chevron.up.chevron.down trailing, secondary.
    /// The whole control is a button that opens the menu layer.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn menu_picker(&mut self, v: &ViewNode, label: &ViewNode, options: &[PickerOption], selected: Option<usize>, open: Option<u64>, disabled: bool, p: Prop, env: Env) -> Lay {
        let fin = p.w.filter(|w| w.is_finite());
        let tint = if disabled { self.semantic(Semantic::Tertiary) } else { self.semantic(Semantic::Secondary) };
        let text = selected.and_then(|i| options.get(i)).map_or("", |o| o.text.as_str());
        let spec = self.spec(&Env { align: TextAlign::Leading, ..env });
        let mut vl = self.text_lay(&format!("{}#value", v.id), text, spec, tint, None);
        if let Some(n) = vl.node.as_mut() {
            n.inspect = false;
        }
        let cl = self.symbol_lay(format!("{}#chevron", v.id), "chevron.up.chevron.down", PICKER_CHEVRON.0, PICKER_CHEVRON.1, tint);
        let trail = vl.size.w + PICKER_CHEVRON_GAP + cl.size.w;
        let l = self.view(label, Prop { w: fin.map(|w| (w - trail - 8.0).max(0.0)), h: None }, env);
        let w = fin.unwrap_or(l.size.w + 8.0 + trail);
        let h = l.size.h.max(vl.size.h);
        let mut n = Node::new(&v.id, "picker");
        n.role = Role::Button;
        n.action = open.filter(|_| !disabled);
        n.disabled = disabled;
        n.label = first_text(label);
        n.value = Some(text.to_string());
        let (lh, vh, ch) = (l.size.h, vl.size.h, cl.size.h);
        Lay::new(Size { w, h }, Some(n), vec![(0.0, (h - lh) / 2.0, l), (w - trail - PICKER_TRAIL_INSET, (h - vh) / 2.0, vl), (w - cl.size.w - PICKER_TRAIL_INSET, (h - ch) / 2.0, cl)])
    }

    /// Segmented `Picker`: a capsule track, the selected segment a raised white capsule.
    pub(super) fn segmented(&mut self, v: &ViewNode, options: &[PickerOption], selected: Option<usize>, disabled: bool, p: Prop) -> Lay {
        let w = p.w.filter(|w| w.is_finite()).unwrap_or(options.len() as f64 * 80.0);
        let h = SEGMENTED_H;
        let mut root = Node::new(&v.id, "segmented");
        root.fill = Some(self.control_fill(SEGMENTED_TRACK_A));
        root.radius = PILL;
        root.disabled = disabled;
        root.value = selected.and_then(|i| options.get(i)).map(|o| o.text.clone());
        let n = options.len().max(1) as f64;
        let sw = w / n;
        let primary = self.semantic(Semantic::Primary);
        let mut kids = vec![];
        for (i, o) in options.iter().enumerate() {
            let on = selected == Some(i);
            if on {
                let mut thumb = Node::new(format!("{}#thumb", v.id), "segment-thumb");
                thumb.fill = Some(if self.dark { rgba(SEGMENTED_THUMB_DARK.0, SEGMENTED_THUMB_DARK.1, SEGMENTED_THUMB_DARK.2, 1.0) } else { rgba(255, 255, 255, 1.0) });
                thumb.radius = PILL;
                thumb.shadow = (!self.dark).then_some(Shadow { color: Color::rgba(0.0, 0.0, 0.0, 0.12), radius: 4.0, dx: 0.0, dy: 2.0 });
                thumb.inspect = false;
                kids.push((i as f64 * sw + SEGMENTED_INSET, SEGMENTED_INSET, Lay::new(Size { w: sw - 2.0 * SEGMENTED_INSET, h: h - 2.0 * SEGMENTED_INSET }, Some(thumb), vec![])));
            }
            let fs = SEGMENTED_FONT.0;
            let spec = FontSpec::new(fs, self.line_h(fs), if on { SEGMENTED_FONT.2 } else { SEGMENTED_FONT.1 }, TextAlign::Leading);
            let mut t = self.text_lay(&format!("{}#seg{i}#text", v.id), &o.text, spec, if disabled { self.semantic(Semantic::Tertiary) } else { primary }, None);
            if let Some(tn) = t.node.as_mut() {
                tn.inspect = false;
            }
            let mut b = Node::new(format!("{}#seg{i}", v.id), "button");
            b.role = Role::Button;
            b.label = Some(o.text.clone());
            b.action = (!disabled && !on).then_some(o.action);
            b.disabled = disabled;
            let (tw, th) = (t.size.w, t.size.h);
            kids.push((i as f64 * sw, 0.0, Lay::new(Size { w: sw, h }, Some(b), vec![((sw - tw) / 2.0, (h - th) / 2.0, t)])));
        }
        Lay::new(Size { w, h }, Some(root), kids)
    }

    /// `Slider`: capsule track, accent fill up to the value, a white capsule thumb. Tapping or
    /// dragging sets the value (see `Interaction`), through `field`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn slider(&mut self, v: &ViewNode, value: f64, range: (f64, f64), field: Option<u64>, disabled: bool, p: Prop, env: Env) -> Lay {
        let w = p.w.filter(|w| w.is_finite()).unwrap_or(200.0);
        let h = SLIDER_H;
        let span = range.1 - range.0;
        let frac = if span > 0.0 { ((value - range.0) / span).clamp(0.0, 1.0) } else { 0.0 };
        let (tw, th) = SLIDER_THUMB;
        let tx = frac * (w - tw);
        let mut n = Node::new(&v.id, "slider");
        n.role = Role::Slider;
        n.field = field.filter(|_| !disabled);
        n.range = Some((range.0, range.1, tw));
        n.value = Some(format!("{value}"));
        n.disabled = disabled;
        let ty = (h - SLIDER_TRACK) / 2.0;
        let mut track = Node::new(format!("{}#track", v.id), "slider-track");
        track.fill = Some(self.pick(SLIDER_TRACK_FILL));
        track.radius = PILL;
        track.inspect = false;
        let mut fill = Node::new(format!("{}#fill", v.id), "slider-fill");
        fill.fill = Some(self.tint_color(env).unwrap_or_else(|| self.semantic(Semantic::Accent)));
        fill.radius = PILL;
        fill.inspect = false;
        let mut thumb = Node::new(format!("{}#thumb", v.id), "slider-thumb");
        thumb.fill = Some(rgba(255, 255, 255, 1.0));
        thumb.radius = PILL;
        thumb.shadow = Some(Shadow { color: Color::rgba(0.0, 0.0, 0.0, SLIDER_SHADOW.0), radius: SLIDER_SHADOW.1, dx: 0.0, dy: SLIDER_SHADOW.2 });
        thumb.inspect = false;
        let kids = vec![
            (0.0, ty, Lay::new(Size { w, h: SLIDER_TRACK }, Some(track), vec![])),
            (0.0, ty, Lay::new(Size { w: tx + tw / 2.0, h: SLIDER_TRACK }, Some(fill), vec![])),
            (tx, (h - th) / 2.0, Lay::new(Size { w: tw, h: th }, Some(thumb), vec![])),
        ];
        let mut l = Lay::new(Size { w, h }, Some(n), kids);
        if disabled {
            if let Some(n) = l.node.as_mut() {
                n.opacity = 0.5;
            }
        }
        l
    }

    /// `ProgressView`: a linear capsule bar with a value, else the spinner; a label sits above the
    /// bar or below the spinner.
    pub(super) fn progress(&mut self, v: &ViewNode, value: Option<f64>, label: Option<&ViewNode>, current: Option<&ViewNode>, p: Prop, env: Env) -> Lay {
        let fin = p.w.filter(|w| w.is_finite());
        let ll = label.map(|l| self.view(l, Prop { w: fin, h: None }, env));
        let gap = if value.is_some() { PROGRESS_BAR_LABEL_GAP } else { PROGRESS_LABEL_GAP };
        let lh = ll.as_ref().map_or(0.0, |l| l.size.h + gap);
        let mut n = Node::new(&v.id, "progress");
        n.value = value.map(|f| format!("{f}"));
        n.label = label.and_then(first_text);
        let mut kids = vec![];
        let size = if let Some(f) = value {
            let w = fin.unwrap_or(200.0);
            let mut track = Node::new(format!("{}#track", v.id), "progress-track");
            track.fill = Some(self.pick(PROGRESS_TRACK_FILL));
            track.radius = PILL;
            track.inspect = false;
            let mut bar = Node::new(format!("{}#bar", v.id), "progress-bar");
            bar.fill = Some(self.tint_color(env).unwrap_or_else(|| self.semantic(Semantic::Accent)));
            bar.radius = PILL;
            bar.inspect = false;
            let fw = (w * f.clamp(0.0, 1.0)).max(if f > 0.0 { PROGRESS_H } else { 0.0 });
            let bl = Lay::new(Size { w: fw, h: PROGRESS_H }, Some(bar), vec![]);
            kids.push((0.0, lh, Lay::new(Size { w, h: PROGRESS_H }, Some(track), vec![(0.0, 0.0, bl)])));
            if let Some(l) = ll {
                kids.push((0.0, 0.0, l));
            }
            let mut h = lh + PROGRESS_H;
            if let Some(c) = current {
                // currentValueLabel: caption, secondary, under the bar.
                let cenv = Env { size: PROGRESS_VALUE_PT, weight: None, fg: Some(self.semantic(Semantic::Secondary)), ..env };
                let cl = self.view(c, Prop { w: fin, h: None }, cenv);
                h += PROGRESS_VALUE_GAP;
                let ch = cl.size.h;
                kids.push((0.0, h, cl));
                h += ch;
            }
            Size { w, h }
        } else {
            let lw = ll.as_ref().map_or(0.0, |l| l.size.w);
            let w = SPINNER.max(lw);
            let mut sp = Node::new(format!("{}#spinner", v.id), "spinner");
            sp.glyph = Some((Glyph::Spinner { step: 0, spins: false }, self.semantic(Semantic::Secondary)));
            sp.inspect = false;
            kids.push(((w - SPINNER) / 2.0, 0.0, Lay::new(Size { w: SPINNER, h: SPINNER }, Some(sp), vec![])));
            if let Some(l) = ll {
                kids.push(((w - lw) / 2.0, SPINNER + PROGRESS_LABEL_GAP, l));
            }
            Size { w, h: SPINNER + lh }
        };
        Lay::new(size, Some(n), kids)
    }

    /// Menu of a menu `Picker` (iOS 26): a glass card under (or over) the anchor row's trailing
    /// edge, one row per option, a checkmark before the selected one. A `Menu`'s card (`centered`) is
    /// centred on its button, items may carry a leading icon, red when destructive, and a divider opens a gap
    /// with a separator. A tap outside closes it.
    pub(super) fn menu(&mut self, items: &[PickerOption], anchor: Option<Rect>, dismiss: Option<u64>, centered: bool, g: &Geo) -> Lay {
        let (w, h) = (g.w, g.h);
        let cardl = self.menu_card(items, w);
        let (cw, ch) = (cardl.size.w, cardl.size.h);
        let a = anchor.unwrap_or(Rect { x: w / 2.0, y: h / 2.0, w: 0.0, h: 0.0 });
        let (x, y) = if centered {
            // Centred on the button, its top just above the button's centre; kept on screen.
            let x = (a.center().0 - cw / 2.0).clamp(POPOVER_MARGIN, w - cw - POPOVER_MARGIN);
            (x, (a.center().1 - MENU_CENTER_DY).min(h - g.dev_bottom - ch).max(g.safe_top))
        } else {
            // Trailing and bottom edges line up with the picker's button; it hangs down from it when there is no room above.
            let x = (a.x + a.w - cw + MENU_DX).clamp(POPOVER_MARGIN, w - cw - POPOVER_MARGIN);
            let (top, bottom) = (a.center().1 - MENU_DY, a.center().1 + MENU_DY);
            (x, if bottom - ch >= g.safe_top { bottom - ch } else { top.min(h - g.dev_bottom - ch) })
        };
        let scrim = Lay::new(Size { w, h }, Some(self.scrim("menu#scrim", (w, h), 0.0, dismiss)), vec![]);
        Lay::new(Size { w, h }, None, vec![(0.0, 0.0, scrim), (x, y, cardl)])
    }

    /// The glass card of a menu: one 42 pt row per item (icon, text, checkmark), separators at dividers.
    pub(super) fn menu_card(&mut self, items: &[PickerOption], w: f64) -> Lay {
        let primary = self.semantic(Semantic::Primary);
        let red = self.destructive();
        let dim = self.semantic(Semantic::Tertiary);
        let icons = items.iter().any(|o| o.system_image.is_some());
        let text_x = if icons { MENU_ICON_TEXT_X } else { MENU_TEXT_X };
        let spec = FontSpec::new(17.0, self.line_h(17.0), 400, TextAlign::Leading);
        let texts: Vec<Lay> = items.iter().enumerate().map(|(i, o)| {
            let fg = if o.disabled { dim } else if o.destructive { red } else { primary };
            let mut t = self.text_lay(&format!("menu#{i}#text"), &o.text, spec, fg, None);
            if let Some(n) = t.node.as_mut() {
                n.inspect = false;
            }
            t
        }).collect();
        let cw = (texts.iter().map(|t| t.size.w).fold(0.0, f64::max) + text_x + MENU_PAD_R).clamp(MENU_MIN_W, w - 2.0 * POPOVER_MARGIN);
        let gaps = items.iter().skip(1).filter(|o| o.divider).count() as f64;
        let ch = items.len() as f64 * MENU_ROW_H + gaps * MENU_DIVIDER_GAP + 2.0 * MENU_PAD_V;
        let mut kids = vec![];
        let mut y = MENU_PAD_V;
        for (i, (o, t)) in items.iter().zip(texts).enumerate() {
            if o.divider && i > 0 {
                let mut sep = Node::new(format!("menu#sep{i}"), "separator");
                sep.fill = Some(self.semantic(Semantic::Separator));
                sep.inspect = false;
                kids.push((MENU_SEP_INSET, y + (MENU_DIVIDER_GAP - 1.0) / 2.0, Lay::new(Size { w: cw - 2.0 * MENU_SEP_INSET, h: 1.0 }, Some(sep), vec![])));
                y += MENU_DIVIDER_GAP;
            }
            let fg = if o.disabled { dim } else if o.destructive { red } else { primary };
            let mut b = Node::new(format!("menu#{i}"), "button");
            b.role = Role::Button;
            b.label = Some(o.text.clone());
            b.action = (!o.disabled).then_some(o.action);
            b.disabled = o.disabled;
            b.value = o.checked.then(|| "checked".into());
            let th = t.size.h;
            let mut bk = vec![(text_x, (MENU_ROW_H - th) / 2.0, t)];
            if let Some(name) = &o.system_image {
                let c = self.symbol_lay(format!("menu#{i}#icon"), name, 17.0, 400, fg);
                let (iw, ih) = (c.size.w, c.size.h);
                bk.push((MENU_ICON_X - iw / 2.0, (MENU_ROW_H - ih) / 2.0, c));
            }
            if o.checked {
                let c = self.symbol_lay(format!("menu#{i}#check"), "checkmark", MENU_CHECK.0, MENU_CHECK.1, primary);
                let (cw2, chh) = (c.size.w, c.size.h);
                bk.push((MENU_CHECK_X - cw2 / 2.0, (MENU_ROW_H - chh) / 2.0, c));
            }
            kids.push((0.0, y, Lay::new(Size { w: cw, h: MENU_ROW_H }, Some(b), bk)));
            y += MENU_ROW_H;
        }
        let mut card = Node::new("menu", "menu");
        card.radius = MENU_RADIUS;
        card.glass = Some(crate::glass::GlassSpec { blur: Some(PRESENT_BLUR), mix: Some(if self.dark { MENU_GLASS.1 } else { MENU_GLASS.0 }), shadow: Some(POPOVER_SHADOW), ..self.glass_spec(GlassVariant::Regular, None, false) });
        Lay::new(Size { w: cw, h: ch }, Some(card), kids)
    }

    /// `.toggleStyle(.button)`: bordered-button metrics, accent label; when on, an accent-tinted capsule.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn toggle_button(&mut self, v: &ViewNode, action: u64, label: &ViewNode, on: bool, disabled: bool, p: Prop, env: Env) -> Lay {
        let accent = self.semantic(Semantic::Accent);
        let fg = if disabled { self.semantic(Semantic::Tertiary) } else { accent };
        let mut n = Node::new(&v.id, "button");
        n.role = Role::Toggle;
        n.action = (!disabled).then_some(action);
        n.disabled = disabled;
        n.label = first_text(label);
        n.value = Some(if on { "on" } else { "off" }.into());
        let (hp, vp) = (12.0, 7.0);
        let inner_p = Prop { w: p.w.map(|w| (w - 2.0 * hp).max(0.0)), h: None };
        let l = self.view(label, inner_p, Env { fg: Some(fg), tint: None, axis: None, ..env });
        let size = Size { w: l.size.w + 2.0 * hp, h: l.size.h + 2.0 * vp };
        if on {
            n.fill = Some(Color { a: fg.a * TOGGLE_BUTTON_ON_A, ..fg });
        }
        n.radius = PILL;
        Lay::new(size, Some(n), vec![(hp, vp, l)])
    }

    /// `Gauge`: linear styles are a capsule bar (accent fill on a gray track, or a primary bar with a marker),
    /// circular ones a ring or an open arc with the current value inside.
    pub(super) fn gauge(&mut self, v: &ViewNode, style: GaugeStyle, value: f64, l: GaugeLabels, p: Prop, env: Env) -> Lay {
        let f = if value.is_finite() { value.clamp(0.0, 1.0) } else { 0.0 };
        let mut n = Node::new(&v.id, "gauge");
        n.value = Some(format!("{f}"));
        n.label = l.label.and_then(first_text);
        if matches!(style, GaugeStyle::AccessoryCircular | GaugeStyle::AccessoryCircularCapacity) {
            return self.gauge_circular(v, n, style == GaugeStyle::AccessoryCircular, f, l, env);
        }
        let fin = p.w.filter(|w| w.is_finite());
        let w = fin.unwrap_or(200.0);
        let primary = self.semantic(Semantic::Primary);
        let accent = self.tint_color(env).unwrap_or_else(|| self.semantic(Semantic::Accent));
        let (bar_h, track_c, gap, leading) = match style {
            GaugeStyle::AccessoryLinearCapacity => (PROGRESS_H, self.pick(PROGRESS_TRACK_FILL), GAUGE_ACC_LABEL_GAP, true),
            GaugeStyle::AccessoryLinear => (GAUGE_ACC_BAR, primary, GAUGE_ACC_LABEL_GAP, true),
            _ => (GAUGE_BAR, self.control_fill(0.12), GAUGE_LABEL_GAP, false),
        };
        let marker = style == GaugeStyle::AccessoryLinear;
        let mut kids = vec![];
        let mut y = 0.0;
        // accessoryLinear shows no label (like iOS).
        if let Some(lv) = l.label.filter(|_| !marker) {
            let ll = self.view(lv, Prop { w: fin, h: None }, env);
            let (lw, lh) = (ll.size.w, ll.size.h);
            kids.push((if leading { 0.0 } else { (w - lw) / 2.0 }, 0.0, ll));
            y += lh + gap;
        }
        let side = |cx: &mut Self, lv: Option<&ViewNode>| lv.map(|lv| cx.view(lv, Prop { w: None, h: None }, env));
        let (minl, maxl) = (side(self, l.min), side(self, l.max));
        let lead = minl.as_ref().map_or(0.0, |m| m.size.w + DEFAULT_SPACING);
        let trail = maxl.as_ref().map_or(0.0, |m| m.size.w + DEFAULT_SPACING);
        let bw = (w - lead - trail).max(0.0);
        let row_h = [bar_h, minl.as_ref().map_or(0.0, |m| m.size.h), maxl.as_ref().map_or(0.0, |m| m.size.h)].into_iter().fold(0.0, f64::max);
        if let Some(m) = minl {
            let mh = m.size.h;
            kids.push((0.0, y + (row_h - mh) / 2.0, m));
        }
        if let Some(m) = maxl {
            let (mw, mh) = (m.size.w, m.size.h);
            kids.push((w - mw, y + (row_h - mh) / 2.0, m));
        }
        let mut track = Node::new(format!("{}#track", v.id), "gauge-track");
        track.fill = Some(track_c);
        track.radius = PILL;
        track.inspect = false;
        let mut tk = vec![];
        if marker {
            // A primary dot in a cut-out of the background color.
            let (cut, dot) = GAUGE_MARKER;
            let cx = f * bw;
            let mut m = Node::new(format!("{}#marker", v.id), "gauge-marker");
            m.fill = Some(self.semantic(Semantic::Background));
            m.radius = PILL;
            m.inspect = false;
            let mut d = Node::new(format!("{}#dot", v.id), "gauge-dot");
            d.fill = Some(primary);
            d.radius = PILL;
            d.inspect = false;
            let dl = Lay::new(Size { w: dot, h: dot }, Some(d), vec![]);
            tk.push((cx - cut / 2.0, (bar_h - cut) / 2.0, Lay::new(Size { w: cut, h: cut }, Some(m), vec![((cut - dot) / 2.0, (cut - dot) / 2.0, dl)])));
        } else {
            let mut bar = Node::new(format!("{}#bar", v.id), "gauge-bar");
            bar.fill = Some(accent);
            bar.radius = PILL;
            bar.inspect = false;
            let fw = (bw * f).max(if f > 0.0 { bar_h } else { 0.0 });
            tk.push((0.0, 0.0, Lay::new(Size { w: fw, h: bar_h }, Some(bar), vec![])));
        }
        kids.push((lead, y + (row_h - bar_h) / 2.0, Lay::new(Size { w: bw, h: bar_h }, Some(track), tk)));
        y += row_h;
        if let Some(cv) = l.current.filter(|_| !marker) {
            let cl = self.view(cv, Prop { w: fin, h: None }, env);
            let (cw, ch) = (cl.size.w, cl.size.h);
            y += GAUGE_VALUE_GAP;
            kids.push((if leading { 0.0 } else { (w - cw) / 2.0 }, y, cl));
            y += ch;
        }
        Lay::new(Size { w, h: y }, Some(n), kids)
    }

    /// `accessoryCircularCapacity`: a ring filled clockwise from the top. `accessoryCircular`: an arc open at the
    /// bottom with a dot at the value and the label in the opening. Both show the current value inside.
    pub(super) fn gauge_circular(&mut self, v: &ViewNode, n: Node, open: bool, f: f64, l: GaugeLabels, env: Env) -> Lay {
        let s = GAUGE_RING;
        let primary = self.semantic(Semantic::Primary);
        let width = (GAUGE_RING_W * 100.0).round() as u16;
        let arc = |id: String, from: f64, to: f64, c: Color| {
            let mut a = Node::new(id, "gauge-arc");
            a.glyph = Some((Glyph::Arc { from: from.round() as i16, to: to.round() as i16, width }, c));
            a.inspect = false;
            Lay::new(Size { w: s, h: s }, Some(a), vec![])
        };
        let mut kids = vec![];
        if open {
            kids.push((0.0, 0.0, arc(format!("{}#track", v.id), -GAUGE_ARC_ANGLE, GAUGE_ARC_ANGLE, primary)));
            let (cut, dot) = GAUGE_ARC_MARKER;
            let a = (-GAUGE_ARC_ANGLE + 2.0 * GAUGE_ARC_ANGLE * f).to_radians();
            let r = (s - GAUGE_RING_W) / 2.0;
            let (mx, my) = (s / 2.0 + r * a.sin(), s / 2.0 - r * a.cos());
            let mut m = Node::new(format!("{}#marker", v.id), "gauge-marker");
            m.fill = Some(self.semantic(Semantic::Background));
            m.radius = PILL;
            m.inspect = false;
            let mut d = Node::new(format!("{}#dot", v.id), "gauge-dot");
            d.fill = Some(primary);
            d.radius = PILL;
            d.inspect = false;
            let dl = Lay::new(Size { w: dot, h: dot }, Some(d), vec![]);
            kids.push((mx - cut / 2.0, my - cut / 2.0, Lay::new(Size { w: cut, h: cut }, Some(m), vec![((cut - dot) / 2.0, (cut - dot) / 2.0, dl)])));
        } else {
            kids.push((0.0, 0.0, arc(format!("{}#track", v.id), -180.0, 180.0, Color { a: primary.a * GAUGE_RING_TRACK_A, ..primary })));
            if f > 0.0 {
                kids.push((0.0, 0.0, arc(format!("{}#bar", v.id), 0.0, 360.0 * f, primary)));
            }
        }
        if let Some(cv) = l.current {
            let cl = self.view(cv, Prop { w: Some(s - 2.0 * GAUGE_RING_W), h: None }, Env { size: GAUGE_RING_PT, weight: None, align: TextAlign::Center, ..env });
            let (cw, ch) = (cl.size.w, cl.size.h);
            let cy = if open { s / 2.0 - 0.9 } else { s / 2.0 };
            kids.push(((s - cw) / 2.0, cy - ch / 2.0, cl));
        }
        if let Some(lv) = l.label.filter(|_| open) {
            let ll = self.view(lv, Prop { w: None, h: None }, Env { size: GAUGE_ARC_LABEL_PT, weight: Some(600), ..env });
            let (lw, lh) = (ll.size.w, ll.size.h);
            kids.push(((s - lw) / 2.0, s / 2.0 + 19.1 - lh / 2.0, ll));
        }
        Lay::new(Size { w: s, h: s }, Some(n), kids)
    }

    /// Wheel `Picker`: the options on a drum around a highlight band, the selection in it. Rows further from
    /// the centre are squashed and dimmed; tapping one selects it.
    pub(super) fn wheel(&mut self, v: &ViewNode, label: &ViewNode, options: &[PickerOption], selected: Option<usize>, disabled: bool, p: Prop) -> Lay {
        let w = p.w.filter(|w| w.is_finite()).unwrap_or(320.0);
        let (h, cy) = (WHEEL_H, WHEEL_H / 2.0);
        let mut root = Node::new(&v.id, "picker");
        root.label = first_text(label);
        root.disabled = disabled;
        root.value = selected.and_then(|i| options.get(i)).map(|o| o.text.clone());
        let mut band = Node::new(format!("{}#band", v.id), "wheel-band");
        band.fill = Some(self.pick(WHEEL_BAND_FILL));
        band.radius = PILL;
        band.inspect = false;
        let (bh, inset) = WHEEL_BAND;
        let mut kids = vec![(inset, cy - bh / 2.0, Lay::new(Size { w: (w - 2.0 * inset).max(0.0), h: bh }, Some(band), vec![]))];
        let primary = self.semantic(Semantic::Primary);
        let sel = selected.unwrap_or(0) as isize;
        for (i, o) in options.iter().enumerate() {
            let d = i as isize - sel;
            let Some(&(off, sq)) = WHEEL_ROWS.get(d.unsigned_abs()) else { continue };
            let pt = WHEEL_PT * sq as f32;
            let color = if d == 0 { primary } else { Color { a: primary.a * WHEEL_DIM_A, ..primary } };
            let mut t = self.text_lay(&format!("{}#opt{i}#text", v.id), &o.text, FontSpec::new(pt, self.line_h(pt), 400, TextAlign::Leading), color, None);
            if let Some(n) = t.node.as_mut() {
                n.inspect = false;
            }
            let rh = WHEEL_ROWS[1].0 * sq;
            let mut b = Node::new(format!("{}#opt{i}", v.id), "button");
            b.role = Role::Button;
            b.label = Some(o.text.clone());
            b.action = (!disabled && d != 0).then_some(o.action);
            b.disabled = disabled;
            let (tw, th) = (t.size.w, t.size.h);
            let y = cy + off * d.signum() as f64 - rh / 2.0;
            kids.push((0.0, y, Lay::new(Size { w, h: rh }, Some(b), vec![((w - tw) / 2.0, (rh - th) / 2.0, t)])));
        }
        Lay::new(Size { w, h }, Some(root), kids)
    }

    /// Multi-column wheel: the columns centred side by side, one band behind all of them; four rows each side
    /// of the selection, squashed and dimmed with distance. Tapping a row runs its option's action.
    pub(super) fn multi_wheel(&mut self, v: &ViewNode, columns: &[onesim_protocol::WheelColumn], (width, height): (Option<f64>, Option<f64>), disabled: bool, popover: bool, p: Prop) -> Lay {
        let w = width.or(p.w.filter(|w| w.is_finite())).unwrap_or(320.0);
        let h = height.unwrap_or(WHEEL_H);
        let cy = h / 2.0;
        let mut root = Node::new(&v.id, "picker");
        root.disabled = disabled;
        let picked: Vec<&str> = columns.iter().filter_map(|c| c.options.get(c.selected).map(|o| o.text.as_str())).collect();
        root.label = picked.first().map(|s| s.to_string());
        root.value = Some(picked.join(" "));
        root.clip_radius = Some(0.0);
        let mut band = Node::new(format!("{}#band", v.id), "wheel-band");
        band.fill = Some(self.pick(if popover { WHEEL_POPOVER_BAND_FILL } else { WHEEL_BAND_FILL }));
        band.radius = PILL;
        band.inspect = false;
        let bh = WHEEL_BAND.0;
        let (k, c, pitch) = WHEEL_DRUM;
        let r = h * k + c;
        let mut kids = vec![(WHEEL_COLUMN_BAND_INSET, cy - bh / 2.0, Lay::new(Size { w: (w - 2.0 * WHEEL_COLUMN_BAND_INSET).max(0.0), h: bh }, Some(band), vec![]))];
        let primary = self.semantic(Semantic::Primary);
        let total: f64 = columns.iter().map(|c| c.width).sum::<f64>() + WHEEL_COLUMN_GAP * columns.len().saturating_sub(1) as f64;
        let mut x = (w - total) / 2.0;
        for (ci, col) in columns.iter().enumerate() {
            let farthest = WHEEL_COLUMN_ALPHA.len() - 1;
            let start = col.selected.saturating_sub(farthest).min(col.options.len());
            let end = col.selected.saturating_add(farthest + 1).min(col.options.len());
            for (i, o) in col.options.iter().enumerate().take(end).skip(start) {
                let distance = i.abs_diff(col.selected);
                let selected = distance == 0;
                let t = distance as f64 * pitch / r;
                let alpha = WHEEL_COLUMN_ALPHA[distance];
                if t >= std::f64::consts::FRAC_PI_2 {
                    continue;
                }
                let (off, sq) = (r * t.sin(), t.cos());
                let pt = if selected { WHEEL_COLUMN_PT.0 } else { WHEEL_COLUMN_PT.1 };
                let a = if o.disabled { WHEEL_OUT_OF_RANGE_A * alpha } else { alpha };
                let color = Color { a: primary.a * a, ..primary };
                let spec = FontSpec::new(pt, self.line_h(pt), 400, TextAlign::Leading);
                let mut t = self.text_lay(&format!("{}#c{ci}#{i}#text", v.id), &o.text, spec, color, None);
                if let Some(n) = t.node.as_mut() {
                    n.inspect = false;
                    n.text_squash = sq as f32;
                }
                let rh = pitch * sq;
                let mut b = Node::new(format!("{}#c{ci}#{i}", v.id), "button");
                b.role = Role::Button;
                b.label = Some(o.text.clone());
                b.action = (!disabled && !o.disabled && !selected && o.action != 0).then_some(o.action);
                b.disabled = disabled || o.disabled;
                b.value = (selected).then(|| "selected".into());
                let (tw, th) = (t.size.w, t.size.h);
                // The squashed text keeps its line box; the row centres it.
                let inset = if selected { col.selected_inset.unwrap_or(col.inset) } else { col.inset };
                let tx = match col.align {
                    onesim_protocol::WheelAlign::Leading => inset,
                    onesim_protocol::WheelAlign::Center => (col.width - tw) / 2.0,
                    onesim_protocol::WheelAlign::Trailing => col.width - tw - inset,
                };
                // Away from the front of the drum, the receding rows converge toward its centre.
                let recede = (1.0 - sq) * 0.075;
                let tx = tx + (w / 2.0 - x - col.width / 2.0) * recede;
                let y = cy + if i < col.selected { -off } else { off };
                let y = y - rh / 2.0;
                kids.push((x, y, Lay::new(Size { w: col.width, h: rh }, Some(b), vec![(tx, (rh - th) / 2.0 - if selected { 0.83 } else { 0.5 }, t)])));
            }
            x += col.width + WHEEL_COLUMN_GAP;
        }
        Lay::new(Size { w, h }, Some(root), kids)
    }

}
