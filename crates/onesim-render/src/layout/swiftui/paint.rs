use super::*;

#[derive(Clone)]
pub(super) enum ResolvedPaint {
    Color(Color),
    Gradient(Arc<crate::paint::Gradient>),
}

impl ResolvedPaint {
    pub(super) fn color(&self) -> Color {
        match self {
            Self::Color(color) => *color,
            Self::Gradient(_) => Color::rgba(1.0, 1.0, 1.0, 1.0),
        }
    }

    pub(super) fn opacity(self, amount: f32) -> Self {
        match self {
            Self::Color(color) => Self::Color(Color { a: color.a * amount, ..color }),
            Self::Gradient(gradient) => Self::Gradient(Arc::new(gradient.with_opacity(amount))),
        }
    }

    pub(super) fn gradient(&self) -> Option<Arc<crate::paint::Gradient>> {
        match self {
            Self::Color(_) => None,
            Self::Gradient(paint) => Some(paint.clone()),
        }
    }
}


impl Ctx<'_> {
    /// Offset along a scroll axis: within `[0, max]`, or up to one viewport `len` past an edge
    /// while overscroll is live (the rubber band never goes further).
    pub(super) fn clamp_offset(&self, o: f64, max: f64, len: f64) -> f64 {
        let slack = if self.overscroll { len } else { 0.0 };
        o.clamp(-slack, max + slack)
    }

    pub(super) fn semantic(&self, s: Semantic) -> Color {
        let d = self.dark;
        match s {
            Semantic::Primary if d => rgba(255, 255, 255, 1.0),
            Semantic::Primary => rgba(0, 0, 0, 1.0),
            Semantic::Secondary if d => rgba(235, 235, 245, 0.6),
            Semantic::Secondary => rgba(60, 60, 67, 0.6),
            Semantic::Tertiary if d => rgba(235, 235, 245, 0.3),
            Semantic::Tertiary => rgba(60, 60, 67, 0.3),
            Semantic::Accent if d => rgba(10, 145, 255, 1.0),
            Semantic::Accent => rgba(0, 136, 255, 1.0),
            Semantic::Background if d => rgba(0, 0, 0, 1.0),
            Semantic::Background => rgba(255, 255, 255, 1.0),
            Semantic::Separator if d => rgba(56, 56, 59, 1.0),
            Semantic::Separator => rgba(232, 232, 232, 1.0),
            Semantic::GroupedBackground if d && self.elevated => rgba(28, 28, 30, 1.0),
            Semantic::GroupedBackground if d => rgba(0, 0, 0, 1.0),
            Semantic::GroupedBackground => rgba(242, 242, 247, 1.0),
            Semantic::SecondaryGroupedBackground if d && self.elevated => rgba(44, 44, 46, 1.0),
            Semantic::SecondaryGroupedBackground if d => rgba(28, 28, 30, 1.0),
            Semantic::SecondaryGroupedBackground => rgba(255, 255, 255, 1.0),
        }
    }

    pub(super) fn glass_spec(&self, variant: GlassVariant, tint: Option<&onesim_protocol::Color>, interactive: bool) -> crate::glass::GlassSpec {
        crate::glass::GlassSpec { variant, tint: tint.map(|c| self.color(c)), interactive, blur: None, mix: None, shadow: None, flat: None }
    }

    /// System red: destructive menu items and tab badges.
    pub(super) fn destructive(&self) -> Color {
        if self.dark { rgba(255, 66, 69, 1.0) } else { rgba(255, 56, 60, 1.0) }
    }

    /// Opaque (light, dark) color pair for the current appearance.
    pub(super) fn pick(&self, (l, d): ((u8, u8, u8), (u8, u8, u8))) -> Color {
        let c = if self.dark { d } else { l };
        rgba(c.0, c.1, c.2, 1.0)
    }

    /// Neutral control fill (systemFill-like) of the given strength.
    pub(super) fn control_fill(&self, light_a: f32) -> Color {
        control_fill_for(self.dark, light_a)
    }

    pub(super) fn color(&self, c: &onesim_protocol::Color) -> Color {
        match c {
            onesim_protocol::Color::Semantic { semantic, a } => {
                let color = self.semantic(*semantic);
                Color { a: (f64::from(color.a) * a.unwrap_or(1.0)).clamp(0.0, 1.0) as f32, ..color }
            }
            onesim_protocol::Color::Hierarchical { semantic, .. } => self.semantic(*semantic),
            onesim_protocol::Color::Rgba { r, g, b, a, dark } => {
                let [r, g, b] = if self.dark { dark.as_deref().copied().unwrap_or([*r, *g, *b]) } else { [*r, *g, *b] };
                Color::rgba(r as f32, g as f32, b as f32, a.clamp(0.0, 1.0) as f32)
            }
        }
    }

    pub(super) fn foreground_paint(&self, env: Env) -> ResolvedPaint {
        env.foreground_paint.map_or_else(
            || ResolvedPaint::Color(env.fg.unwrap_or_else(|| self.semantic(Semantic::Primary))),
            |i| self.paints[i].clone(),
        )
    }

    pub(super) fn hierarchical_paint(&self, style: StyleReference, env: Env) -> ResolvedPaint {
        let supplied = match style {
            StyleReference::Secondary => env.secondary_paint,
            StyleReference::Tertiary => env.tertiary_paint,
            _ => None,
        };
        if let Some(index) = supplied {
            return self.paints[index].clone();
        }
        let (factor, default) = match style {
            StyleReference::Primary => (1.0, self.semantic(Semantic::Primary)),
            StyleReference::Secondary => (0.60, self.semantic(Semantic::Secondary)),
            StyleReference::Tertiary => (0.30, self.semantic(Semantic::Tertiary)),
            StyleReference::Quaternary => (0.18, Color { a: 0.18, ..self.semantic(Semantic::Primary) }),
            StyleReference::Quinary => (0.12, Color { a: 0.12, ..self.semantic(Semantic::Primary) }),
            _ => unreachable!(),
        };
        if let Some(index) = env.foreground_paint {
            return self.paints[index].clone().opacity(factor);
        }
        if let Some(tint) = env.tint {
            let factor = match style {
                StyleReference::Secondary => 0.5,
                StyleReference::Tertiary => 0.25,
                _ => factor,
            };
            return ResolvedPaint::Color(tint).opacity(factor);
        }
        env.fg.map_or(ResolvedPaint::Color(default), |fg| ResolvedPaint::Color(fg).opacity(factor))
    }

    pub(super) fn tint_color(&self, env: Env) -> Option<Color> {
        env.tint_paint.map(|i| self.paints[i].color()).or(env.tint)
    }

    pub(super) fn inherited_paint(&self, color: &onesim_protocol::Color, env: Env) -> Option<ResolvedPaint> {
        let onesim_protocol::Color::Hierarchical { semantic, hierarchical: true } = color else { return None };
        let style = match semantic {
            Semantic::Primary => StyleReference::Primary,
            Semantic::Secondary => StyleReference::Secondary,
            Semantic::Tertiary => StyleReference::Tertiary,
            _ => return None,
        };
        Some(self.hierarchical_paint(style, env))
    }

    pub(super) fn shape_paint(&self, paint: &ShapePaint, env: Env) -> ResolvedPaint {
        use StyleReference::*;
        let color = match paint {
            ShapePaint::Color(c) => return self.inherited_paint(c, env).unwrap_or_else(|| ResolvedPaint::Color(self.color(c))),
            ShapePaint::Gradient { gradient } => {
                return ResolvedPaint::Gradient(Arc::new(crate::paint::Gradient::resolve(gradient, |c| self.color(c))));
            }
            ShapePaint::Style { style } => match style {
                Foreground => return self.foreground_paint(env),
                Background => {
                    if let Some(i) = env.background_paint { return self.paints[i].clone(); }
                    self.semantic(Semantic::Background)
                }
                Tint => {
                    if let Some(i) = env.tint_paint { return self.paints[i].clone(); }
                    env.tint.unwrap_or_else(|| self.semantic(Semantic::Accent))
                }
                Primary | Secondary | Tertiary | Quaternary | Quinary => return self.hierarchical_paint(*style, env),
                Fill => self.control_fill(0.20),
                SecondaryFill => self.control_fill(0.16),
                TertiaryFill => self.control_fill(0.12),
                QuaternaryFill => self.control_fill(0.08),
                QuinaryFill => self.control_fill(0.05),
                Link => self.semantic(Semantic::Accent),
                Placeholder => self.semantic(Semantic::Tertiary),
                Selection => Color { a: 0.25, ..self.semantic(Semantic::Accent) },
                Separator => self.semantic(Semantic::Separator),
                WindowBackground => self.semantic(Semantic::Background),
            },
        };
        ResolvedPaint::Color(color)
    }

    pub(super) fn store_paint(&mut self, paint: ResolvedPaint) -> usize {
        let index = self.paints.len();
        self.paints.push(paint);
        index
    }

    pub(super) fn visual_op(&self, id: String, op: VisualOp, inner: Lay) -> Lay {
        let color = match &op {
            VisualOp::ColorMultiply { color } | VisualOp::AlphaThreshold { color, .. } => Some(self.color(color)),
            _ => None,
        };
        layer(id, "visual_op", inner, |n| n.composite = Some(Box::new(crate::effects::Composite::Effect(op, color))))
    }

}
