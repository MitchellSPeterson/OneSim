use super::*;

pub(super) fn container_shape_layer(id: String, spec: ContainerShapeSpec, inner: Lay) -> Lay {
    let mut n = Node::new(id, "container_shape");
    n.container_shape = Some(Box::new(spec));
    n.inspect = false;
    Lay::new(inner.size, Some(n), vec![(0.0, 0.0, inner)])
}

/// Same-frame `.matchedGeometryEffect(isSource: false)`: align each non-source subtree to its
/// source `(namespace, id)` according to `properties` and `anchor`.

#[derive(Clone, Copy)]
enum ResolvedContainer {
    Rectangle,
    Circle,
    Capsule { continuous: bool },
    Ellipse,
    RoundedRect { radii: [(f64, f64); 4], continuous: bool },
}

fn resolve_container_spec(
    spec: ContainerShapeSpec,
    bounds: Rect,
    ancestor: Option<(Rect, ResolvedContainer)>,
) -> ResolvedContainer {
    match spec {
        ContainerShapeSpec::Rectangle => ResolvedContainer::Rectangle,
        ContainerShapeSpec::Circle => ResolvedContainer::Circle,
        ContainerShapeSpec::Capsule { corner_style } => ResolvedContainer::Capsule { continuous: corner_style == RoundedCornerStyle::Continuous },
        ContainerShapeSpec::Ellipse => ResolvedContainer::Ellipse,
        ContainerShapeSpec::RoundedRectangle { corner_radius, corner_size, corner_style } => {
            let (rx, ry) = corner_size.map(|[w, h]| (w, h)).unwrap_or_else(|| {
                let r = corner_radius.unwrap_or(0.0);
                (r, r)
            });
            ResolvedContainer::RoundedRect { radii: [(rx, ry); 4], continuous: corner_style == RoundedCornerStyle::Continuous }
        }
        ContainerShapeSpec::UnevenRoundedRectangle { corner_radii, corner_style } => ResolvedContainer::RoundedRect {
            radii: corner_radii.map(|r| (r, r)),
            continuous: corner_style == RoundedCornerStyle::Continuous,
        },
        ContainerShapeSpec::ContainerRelative => resolve_relative_container(bounds, ancestor),
        ContainerShapeSpec::ConcentricRectangle { concentric } => {
            let radii = resolve_concentric_radii(concentric, bounds, ancestor);
            ResolvedContainer::RoundedRect { radii: radii.map(|r| (r, r)), continuous: true }
        }
    }
}

fn edge_distances(inner: Rect, outer: Rect) -> (f64, f64, f64, f64) {
    let left = (inner.x - outer.x).max(0.0);
    let top = (inner.y - outer.y).max(0.0);
    let right = ((outer.x + outer.w) - (inner.x + inner.w)).max(0.0);
    let bottom = ((outer.y + outer.h) - (inner.y + inner.h)).max(0.0);
    (left, top, right, bottom)
}

fn container_corner_radii(outer_rect: Rect, outer: ResolvedContainer) -> [f64; 4] {
    let half = (outer_rect.w.min(outer_rect.h) / 2.0).max(0.0);
    match outer {
        ResolvedContainer::Rectangle | ResolvedContainer::Ellipse => [0.0; 4],
        ResolvedContainer::Circle | ResolvedContainer::Capsule { .. } => [half; 4],
        ResolvedContainer::RoundedRect { radii, .. } => radii.map(|(rx, ry)| rx.min(ry).min(half).max(0.0)),
    }
}

fn resolve_relative_container(bounds: Rect, ancestor: Option<(Rect, ResolvedContainer)>) -> ResolvedContainer {
    let Some((outer_rect, outer)) = ancestor else {
        return ResolvedContainer::Rectangle;
    };
    let (dl, dt, dr, db) = edge_distances(bounds, outer_rect);
    let pads = [(dl, dt), (dr, dt), (dr, db), (dl, db)];
    match outer {
        ResolvedContainer::Rectangle => ResolvedContainer::Rectangle,
        ResolvedContainer::Circle => ResolvedContainer::Circle,
        ResolvedContainer::Capsule { continuous } => ResolvedContainer::Capsule { continuous },
        ResolvedContainer::Ellipse => ResolvedContainer::Ellipse,
        ResolvedContainer::RoundedRect { radii, continuous } => {
            let mut out = [(0.0, 0.0); 4];
            for i in 0..4 {
                let (rx, ry) = radii[i];
                let (dx, dy) = pads[i];
                // Uniform inset reduces both axes by the corner inset; asymmetric padding keeps concentric curve.
                let pad = dx.max(dy);
                out[i] = ((rx - pad).max(0.0), (ry - pad).max(0.0));
            }
            ResolvedContainer::RoundedRect { radii: out, continuous }
        }
    }
}

fn resolve_concentric_radii(
    spec: ConcentricCorners,
    bounds: Rect,
    ancestor: Option<(Rect, ResolvedContainer)>,
) -> [f64; 4] {
    let (outer_r, pads) = match ancestor {
        Some((outer_rect, outer)) => {
            let (dl, dt, dr, db) = edge_distances(bounds, outer_rect);
            (container_corner_radii(outer_rect, outer), [dl.max(dt), dr.max(dt), dr.max(db), dl.max(db)])
        }
        None => ([0.0; 4], [0.0; 4]),
    };
    let mut r = [0.0f64; 4];
    for i in 0..4 {
        r[i] = match spec.corners[i] {
            CornerStyleSpec::Fixed(v) => v.max(0.0),
            CornerStyleSpec::Concentric(min_r) => (outer_r[i] - pads[i]).max(min_r.unwrap_or(0.0)).max(0.0),
        };
    }
    let unify = |r: &mut [f64; 4], idxs: &[usize]| {
        let mut val = 0.0f64;
        for &k in idxs {
            val = val.max(r[k]);
        }
        for &k in idxs {
            if matches!(spec.corners[k], CornerStyleSpec::Concentric(_)) {
                r[k] = val;
            }
        }
    };
    match spec.uniform {
        Some(UniformCorners::All) => unify(&mut r, &[0, 1, 2, 3]),
        Some(UniformCorners::Top) => unify(&mut r, &[0, 1]),
        Some(UniformCorners::Bottom) => unify(&mut r, &[2, 3]),
        Some(UniformCorners::Leading) => unify(&mut r, &[0, 3]),
        Some(UniformCorners::Trailing) => unify(&mut r, &[1, 2]),
        Some(UniformCorners::TopAndBottom) => {
            unify(&mut r, &[0, 1]);
            unify(&mut r, &[2, 3]);
        }
        Some(UniformCorners::LeadingAndTrailing) => {
            unify(&mut r, &[0, 3]);
            unify(&mut r, &[1, 2]);
        }
        None => {}
    }
    r
}

fn resolved_to_geom(rc: ResolvedContainer) -> VectorGeom {
    match rc {
        ResolvedContainer::Rectangle => VectorGeom::RoundedRect { radii: [(0.0, 0.0); 4], continuous: false },
        ResolvedContainer::Circle => VectorGeom::RoundedRect { radii: [(PILL, PILL); 4], continuous: false },
        ResolvedContainer::Capsule { continuous } => VectorGeom::RoundedRect { radii: [(PILL, PILL); 4], continuous },
        ResolvedContainer::Ellipse => VectorGeom::Ellipse,
        ResolvedContainer::RoundedRect { radii, continuous } => VectorGeom::RoundedRect { radii, continuous },
    }
}

/// Resolve `ContainerRelativeShape` and `ConcentricRectangle` nodes against their nearest enclosing `.containerShape`.
pub(super) fn resolve_container_shapes(nodes: &mut [Node]) {
    let mut resolved: Vec<Option<(Rect, ResolvedContainer)>> = vec![None; nodes.len()];
    for i in 0..nodes.len() {
        let ancestor = {
            let mut p = nodes[i].parent;
            let mut found = None;
            while let Some(idx) = p {
                if let Some(rc) = resolved[idx] {
                    found = Some(rc);
                    break;
                }
                p = nodes[idx].parent;
            }
            found
        };
        if let Some(spec) = nodes[i].container_shape.as_deref().copied() {
            let rc = resolve_container_spec(spec, nodes[i].bounds, ancestor);
            resolved[i] = Some((nodes[i].bounds, rc));
        }
        let bounds = nodes[i].bounds;
        if let Some(vec) = nodes[i].vector.as_mut() {
            match vec.geom {
                VectorGeom::ContainerRelative => {
                    vec.geom = resolved_to_geom(resolve_relative_container(bounds, ancestor));
                }
                VectorGeom::Concentric(spec) => {
                    let r = resolve_concentric_radii(spec, bounds, ancestor);
                    vec.geom = VectorGeom::RoundedRect { radii: r.map(|v| (v, v)), continuous: true };
                }
                _ => {}
            }
        }
    }
}


impl Ctx<'_> {
    pub(super) fn shape_view(&mut self, v: &ViewNode, p: Prop, env: Env) -> Lay {
        let ViewKind::Shape { shape, extra } = &v.kind else {
            unreachable!()
        };
        let onesim_protocol::ShapeExtra {
            corner_radius,
            corner_size,
            corner_radii,
            corner_style,
            concentric,
            path,
            path_size,
            custom,
            size_hint,
            mirror_rtl,
            fill,
            fill_style,
            stroke,
        } = &**extra;
        let square = |p: Prop| {
            let side = match (p.w, p.h) {
                (Some(w), Some(h)) => w.min(h),
                (Some(w), None) => w,
                (None, Some(h)) => h,
                _ => 10.0,
            };
            Size { w: side, h: side }
        };
        let size = match size_hint {
            Some(ShapeSizeHint::Square) => square(p),
            Some(ShapeSizeHint::Fixed([w, h])) => Size { w: *w, h: *h },
            Some(ShapeSizeHint::Aspect(ratio)) if *ratio > 0.0 => match (p.w, p.h) {
                (Some(w), Some(h)) if w.is_finite() && h.is_finite() => {
                    if w / h > *ratio {
                        Size { w: h * ratio, h }
                    } else {
                        Size { w, h: w / ratio }
                    }
                }
                (Some(w), _) if w.is_finite() => Size { w, h: w / ratio },
                (_, Some(h)) if h.is_finite() => Size { w: h * ratio, h },
                _ => Size { w: 10.0 * ratio, h: 10.0 },
            },
            Some(ShapeSizeHint::Ideal([iw, ih])) => Size {
                w: p.w.filter(|x| x.is_finite()).unwrap_or(*iw),
                h: p.h.filter(|x| x.is_finite()).unwrap_or(*ih),
            },
            _ if *shape == ShapeKind::Circle => square(p),
            _ => greedy(p),
        };
        let mut n = Node::new(&v.id, "shape");
        let fs = fill_style.unwrap_or_default();
        let simple_fill = !fs.eo_fill && fs.antialiased;
        let simple_stroke = stroke.as_ref().is_none_or(|st| {
            st.line_cap == LineCap::Butt
                && st.line_join == LineJoin::Miter
                && (st.miter_limit - 10.0).abs() < 1e-6
                && st.dash.is_empty()
                && st.antialiased
                && st.inset == 0.0
        });
        let continuous = *corner_style == Some(RoundedCornerStyle::Continuous);
        let fallback = || self.foreground_paint(env);
        let fill = fill.as_ref().map(|p| self.shape_paint(p, env)).or_else(|| stroke.is_none().then(fallback));
        let stroke_paint = stroke.as_ref().map(|st| st.color.as_ref().map_or_else(fallback, |p| self.shape_paint(p, env)));
        n.fill_paint = fill.as_ref().and_then(ResolvedPaint::gradient);
        n.stroke_paint = stroke_paint.as_ref().and_then(ResolvedPaint::gradient);

        // Fast path for plain rectangular/circular/pill shapes with default fill & stroke.
        if simple_fill && simple_stroke && !*custom && n.fill_paint.is_none() && n.stroke_paint.is_none() {
            let fast_radius = match shape {
                ShapeKind::Rectangle => Some(0.0),
                ShapeKind::Circle => Some(PILL),
                ShapeKind::Capsule if !continuous => Some(PILL),
                ShapeKind::RoundedRectangle if corner_size.is_none() && !continuous => Some(corner_radius.unwrap_or(0.0)),
                _ => None,
            };
            if let Some(radius) = fast_radius {
                n.radius = radius;
                n.fill = fill.as_ref().map(ResolvedPaint::color);
                n.stroke = stroke.as_ref().zip(stroke_paint.as_ref()).map(|(st, p)| (p.color(), st.line_width));
                return Lay::new(size, Some(n), vec![]);
            }
        }

        let geom = match shape {
            ShapeKind::Rectangle => VectorGeom::RoundedRect { radii: [(0.0, 0.0); 4], continuous: false },
            ShapeKind::Circle => VectorGeom::RoundedRect { radii: [(PILL, PILL); 4], continuous: false },
            ShapeKind::Capsule => VectorGeom::RoundedRect { radii: [(PILL, PILL); 4], continuous },
            ShapeKind::RoundedRectangle => {
                let (rx, ry) = corner_size.map(|[w, h]| (w, h)).unwrap_or_else(|| {
                    let r = corner_radius.unwrap_or(0.0);
                    (r, r)
                });
                VectorGeom::RoundedRect { radii: [(rx, ry); 4], continuous }
            }
            ShapeKind::Ellipse => VectorGeom::Ellipse,
            ShapeKind::UnevenRoundedRectangle => {
                let r = corner_radii.unwrap_or([0.0; 4]);
                VectorGeom::RoundedRect { radii: r.map(|v| (v, v)), continuous }
            }
            ShapeKind::ContainerRelative => { self.has_container_shapes = true; VectorGeom::ContainerRelative }
            ShapeKind::ConcentricRectangle => VectorGeom::Concentric(concentric.unwrap_or(ConcentricCorners {
                corners: [CornerStyleSpec::Concentric(None); 4],
                uniform: None,
            })),
            ShapeKind::Path => VectorGeom::Path {
                ops: path.clone().unwrap_or_default(),
                mirror_x: *mirror_rtl,
            },
        };
        let vec_fill = fill.as_ref().map(|p| VectorFill { color: p.color(), eo_fill: fs.eo_fill, antialiased: fs.antialiased });
        let vec_stroke = stroke.as_ref().zip(stroke_paint.as_ref()).map(|(st, p)| VectorStroke {
            color: p.color(),
            line_width: st.line_width,
            line_cap: st.line_cap,
            line_join: st.line_join,
            miter_limit: st.miter_limit,
            dash: st.dash.clone(),
            dash_phase: st.dash_phase,
            compose_effect: None,
            antialiased: st.antialiased,
            inset: st.inset,
        });
        n.vector = Some(Box::new(VectorShape { geom, fill: vec_fill, stroke: vec_stroke }));
        if *custom {
            n.custom_shape = Some(match *path_size {
                Some(sz) => CustomShapeEval::Evaluated(sz),
                None => CustomShapeEval::Pending,
            });
        }
        Lay::new(size, Some(n), vec![])
    }

}
