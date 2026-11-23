//! SwiftUI gradients sampled in the coordinate space of the painted shape or Canvas.

use std::f64::consts::TAU;

use onesim_protocol::{GradientColorSpace, GradientPaint, GradientStop, MeshPoint};
use tiny_skia::{Mask, Pixmap, PixmapPaint, Transform};

use crate::{Color, Rect};

#[derive(Clone, Debug)]
pub(crate) struct Gradient {
    data: std::sync::Arc<GradientData>,
    opacity: f32,
    space: GradientColorSpace,
}

#[derive(Clone, Debug)]
enum GradientData {
    Stops {
        stops: Vec<(f64, [f64; 4])>,
        geometry: Geometry,
        absolute: bool,
        options: u32,
    },
    Mesh {
        width: usize,
        height: usize,
        points: Vec<MeshPoint>,
        colors: Vec<[f64; 4]>,
        background: Color,
        smooth: bool,
    },
}

#[derive(Clone, Copy, Debug)]
enum Geometry {
    Linear([f64; 2], [f64; 2]),
    Radial([f64; 2], Option<[f64; 2]>, f64, f64, bool),
    Angular([f64; 2], f64, f64),
}

fn linear(c: f64) -> f64 {
    if c.abs() <= 0.04045 {
        c / 12.92
    } else {
        c.signum() * ((c.abs() + 0.055) / 1.055).powf(2.4)
    }
}

fn srgb(c: f64) -> f64 {
    if c.abs() <= 0.0031308 {
        c * 12.92
    } else {
        c.signum() * (1.055 * c.abs().powf(1.0 / 2.4) - 0.055)
    }
}

fn cube_mix(c: [f64; 3], [r, g, b]: [f64; 3]) -> f64 {
    c[0] * r + c[1] * g + c[2] * b
}

fn components(color: Color, space: GradientColorSpace) -> [f64; 4] {
    let mut rgb = [color.r as f64, color.g as f64, color.b as f64];
    if space != GradientColorSpace::Device {
        rgb = rgb.map(linear);
    }
    if space == GradientColorSpace::Perceptual {
        let lms = [
            cube_mix([0.4122214708, 0.5363325363, 0.0514459929], rgb).cbrt(),
            cube_mix([0.2119034982, 0.6806995451, 0.1073969566], rgb).cbrt(),
            cube_mix([0.0883024619, 0.2817188376, 0.6299787005], rgb).cbrt(),
        ];
        rgb = [
            cube_mix([0.2104542553, 0.7936177850, -0.0040720468], lms),
            cube_mix([1.9779984951, -2.4285922050, 0.4505937099], lms),
            cube_mix([0.0259040371, 0.7827717662, -0.8086757660], lms),
        ];
    }
    [rgb[0], rgb[1], rgb[2], color.a as f64]
}

fn pixel([r, g, b, a]: [f64; 4], space: GradientColorSpace, alpha: f64) -> [u8; 4] {
    let mut rgb = [r, g, b];
    if space == GradientColorSpace::Perceptual {
        let lms = [
            (r + 0.3963377774 * g + 0.2158037573 * b).powi(3),
            (r - 0.1055613458 * g - 0.0638541728 * b).powi(3),
            (r - 0.0894841775 * g - 1.2914855480 * b).powi(3),
        ];
        rgb = [
            cube_mix([4.0767416621, -3.3077115913, 0.2309699292], lms),
            cube_mix([-1.2684380046, 2.6097574011, -0.3413193965], lms),
            cube_mix([-0.0041960863, -0.7034186147, 1.7076147010], lms),
        ];
    }
    if space != GradientColorSpace::Device {
        rgb = rgb.map(srgb);
    }
    let a = a.clamp(0.0, 1.0) * alpha;
    let byte = |c: f64| (c.clamp(0.0, 1.0) * a * 255.0).round() as u8;
    [
        byte(rgb[0]),
        byte(rgb[1]),
        byte(rgb[2]),
        (a * 255.0).round() as u8,
    ]
}

fn lerp<const N: usize>(a: [f64; N], b: [f64; N], t: f64) -> [f64; N] {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}

impl Gradient {
    pub(crate) fn differs(&self, other: &Self) -> bool {
        if std::sync::Arc::ptr_eq(&self.data, &other.data)
            && (self.opacity - other.opacity).abs() <= 0.002
        {
            return false;
        }
        if self.space != other.space {
            return false;
        }
        if (self.opacity - other.opacity).abs() > 0.002 {
            return true;
        }
        match (&*self.data, &*other.data) {
            (
                GradientData::Stops { stops: sa, geometry: ga, absolute: aa, options: oa },
                GradientData::Stops { stops: sb, geometry: gb, absolute: ab, options: ob },
            ) => {
                if sa.len() != sb.len() || aa != ab || oa != ob {
                    return false;
                }
                let geom_same_kind = matches!(
                    (ga, gb),
                    (Geometry::Linear(..), Geometry::Linear(..))
                        | (Geometry::Radial(..), Geometry::Radial(..))
                        | (Geometry::Angular(..), Geometry::Angular(..))
                );
                if !geom_same_kind {
                    return false;
                }
                sa.iter().zip(sb).any(|((la, ca), (lb, cb))| {
                    (la - lb).abs() > 1e-4
                        || (0..4).any(|i| (ca[i] - cb[i]).abs() > 0.002)
                })
            }
            (
                GradientData::Mesh { width: wa, height: ha, colors: ca, background: ba, smooth: sma, .. },
                GradientData::Mesh { width: wb, height: hb, colors: cb, background: bb, smooth: smb, .. },
            ) => {
                wa == wb
                    && ha == hb
                    && sma == smb
                    && ca.len() == cb.len()
                    && (ca.iter().zip(cb).any(|(a, b)| (0..4).any(|i| (a[i] - b[i]).abs() > 0.002))
                        || (ba.r - bb.r).abs() > 0.002
                        || (ba.g - bb.g).abs() > 0.002
                        || (ba.b - bb.b).abs() > 0.002
                        || (ba.a - bb.a).abs() > 0.002)
            }
            _ => false,
        }
    }

    pub(crate) fn lerp(&self, other: &Self, t: f64) -> Option<Self> {
        if self.space != other.space {
            return None;
        }
        let opacity = self.opacity + (other.opacity - self.opacity) * (t as f32);
        let data = match (&*self.data, &*other.data) {
            (
                GradientData::Stops { stops: sa, geometry: ga, absolute: aa, options: oa },
                GradientData::Stops { stops: sb, geometry: gb, absolute: ab, options: ob },
            ) if sa.len() == sb.len() && aa == ab && oa == ob => {
                let geometry = match (*ga, *gb) {
                    (Geometry::Linear(a0, a1), Geometry::Linear(b0, b1)) => Geometry::Linear(
                        [a0[0] + (b0[0] - a0[0]) * t, a0[1] + (b0[1] - a0[1]) * t],
                        [a1[0] + (b1[0] - a1[0]) * t, a1[1] + (b1[1] - a1[1]) * t],
                    ),
                    (Geometry::Radial(ca, ea, r0a, r1a, ela), Geometry::Radial(cb, eb, r0b, r1b, elb)) if ela == elb => {
                        let c = [ca[0] + (cb[0] - ca[0]) * t, ca[1] + (cb[1] - ca[1]) * t];
                        let e = match (ea, eb) {
                            (Some(a), Some(b)) => Some([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]),
                            _ => eb,
                        };
                        Geometry::Radial(c, e, r0a + (r0b - r0a) * t, r1a + (r1b - r1a) * t, ela)
                    }
                    (Geometry::Angular(ca, s0a, s1a), Geometry::Angular(cb, s0b, s1b)) => Geometry::Angular(
                        [ca[0] + (cb[0] - ca[0]) * t, ca[1] + (cb[1] - ca[1]) * t],
                        s0a + (s0b - s0a) * t,
                        s1a + (s1b - s1a) * t,
                    ),
                    _ => return None,
                };
                let stops = sa
                    .iter()
                    .zip(sb)
                    .map(|((la, ca), (lb, cb))| (la + (lb - la) * t, lerp(*ca, *cb, t)))
                    .collect();
                GradientData::Stops { stops, geometry, absolute: *aa, options: *oa }
            }
            (
                GradientData::Mesh { width: wa, height: ha, points: pa, colors: ca, background: ba, smooth: sma },
                GradientData::Mesh { width: wb, height: hb, colors: cb, background: bb, smooth: smb, .. },
            ) if wa == wb && ha == hb && sma == smb && ca.len() == cb.len() => {
                let colors = ca.iter().zip(cb).map(|(a, b)| lerp(*a, *b, t)).collect();
                let tf = t as f32;
                let background = Color {
                    r: (ba.r + (bb.r - ba.r) * tf).clamp(0.0, 1.0),
                    g: (ba.g + (bb.g - ba.g) * tf).clamp(0.0, 1.0),
                    b: (ba.b + (bb.b - ba.b) * tf).clamp(0.0, 1.0),
                    a: (ba.a + (bb.a - ba.a) * tf).clamp(0.0, 1.0),
                };
                GradientData::Mesh {
                    width: *wa,
                    height: *ha,
                    points: pa.clone(),
                    colors,
                    background,
                    smooth: *sma,
                }
            }
            _ => return None,
        };
        Some(Self { data: std::sync::Arc::new(data), opacity: opacity.clamp(0.0, 1.0), space: self.space })
    }

    pub(crate) fn resolve(
        gradient: &GradientPaint,
        color: impl Fn(&onesim_protocol::Color) -> Color,
    ) -> Self {
        let stops = |stops: &[GradientStop], space, geometry, absolute, options| {
            let mut values: Vec<_> = stops
                .iter()
                .filter(|s| s.location.is_finite())
                .map(|s| (s.location, components(color(&s.color), space)))
                .collect();
            values.sort_by(|a, b| a.0.total_cmp(&b.0));
            Self {
                space,
                data: std::sync::Arc::new(GradientData::Stops {
                    stops: values,
                    geometry,
                    absolute,
                    options,
                }),
                opacity: 1.0,
            }
        };
        match gradient {
            GradientPaint::Linear {
                stops: values,
                color_space,
                start,
                end,
                absolute,
                options,
            } => stops(
                values,
                *color_space,
                Geometry::Linear(*start, *end),
                *absolute,
                *options,
            ),
            GradientPaint::Radial {
                stops: values,
                color_space,
                center,
                start_center,
                start_radius,
                end_radius,
                absolute,
                elliptical,
                options,
            } => stops(
                values,
                *color_space,
                Geometry::Radial(
                    *center,
                    *start_center,
                    *start_radius,
                    *end_radius,
                    *elliptical,
                ),
                *absolute,
                *options,
            ),
            GradientPaint::Angular {
                stops: values,
                color_space,
                center,
                start_angle,
                end_angle,
                absolute,
                options,
            } => stops(
                values,
                *color_space,
                Geometry::Angular(*center, *start_angle, *end_angle),
                *absolute,
                *options,
            ),
            GradientPaint::Mesh {
                width,
                height,
                points,
                colors,
                background,
                smooths_colors,
                color_space,
            } => Self {
                space: *color_space,
                data: std::sync::Arc::new(GradientData::Mesh {
                    width: *width,
                    height: *height,
                    points: points.clone(),
                    smooth: *smooths_colors,
                    colors: colors
                        .iter()
                        .map(|c| components(color(c), *color_space))
                        .collect(),
                    background: color(background),
                }),
                opacity: 1.0,
            },
        }
    }

    pub(crate) fn with_opacity(&self, amount: f32) -> Self {
        Self {
            opacity: self.opacity * amount,
            ..self.clone()
        }
    }

    pub(crate) fn hash_key<H: std::hash::Hasher>(&self, state: &mut H) {
        use std::hash::Hash;
        self.opacity.to_bits().hash(state);
        (self.space as u8).hash(state);
        match self.data.as_ref() {
            GradientData::Stops {
                stops,
                geometry,
                absolute,
                options,
            } => {
                0u8.hash(state);
                stops.len().hash(state);
                for (loc, rgba) in stops {
                    loc.to_bits().hash(state);
                    rgba.map(f64::to_bits).hash(state);
                }
                match geometry {
                    Geometry::Linear(a, b) => {
                        0u8.hash(state);
                        a.map(f64::to_bits).hash(state);
                        b.map(f64::to_bits).hash(state);
                    }
                    Geometry::Radial(c, end, r0, r1, elliptical) => {
                        1u8.hash(state);
                        c.map(f64::to_bits).hash(state);
                        end.map(|p| p.map(f64::to_bits)).hash(state);
                        r0.to_bits().hash(state);
                        r1.to_bits().hash(state);
                        elliptical.hash(state);
                    }
                    Geometry::Angular(c, start, end) => {
                        2u8.hash(state);
                        c.map(f64::to_bits).hash(state);
                        start.to_bits().hash(state);
                        end.to_bits().hash(state);
                    }
                }
                absolute.hash(state);
                options.hash(state);
            }
            GradientData::Mesh {
                width,
                height,
                points,
                colors,
                background,
                smooth,
            } => {
                1u8.hash(state);
                (*width, *height, *smooth).hash(state);
                [background.r, background.g, background.b, background.a]
                    .map(f32::to_bits)
                    .hash(state);
                points.len().hash(state);
                for p in points {
                    p.position.map(f64::to_bits).hash(state);
                    p.controls
                        .map(|cs| cs.map(|c| c.map(f64::to_bits)))
                        .hash(state);
                }
                colors.len().hash(state);
                for c in colors {
                    c.map(f64::to_bits).hash(state);
                }
            }
        }
    }

    pub(crate) fn composite(
        &self,
        target: &mut Pixmap,
        area: Tile,
        coverage: &Pixmap,
        opacity: f32,
        mask: Option<&Mask>,
    ) {
        let Some(mut pixels) = Pixmap::new(area.w, area.h) else {
            return;
        };
        let alpha = f64::from((opacity * self.opacity).clamp(0.0, 1.0)) / 255.0;
        match self.data.as_ref() {
            GradientData::Stops {
                stops,
                geometry,
                absolute,
                options,
            } => {
                if stops.is_empty() {
                    return;
                }
                let geom = geometry.prepare(area.bounds, *absolute);
                for (y, (row, cover)) in pixels
                    .data_mut()
                    .chunks_exact_mut(area.w as usize * 4)
                    .zip(coverage.pixels().chunks_exact(area.w as usize))
                    .enumerate()
                {
                    for (x, (out, cover)) in
                        row.as_chunks_mut::<4>().0.iter_mut().zip(cover).enumerate()
                    {
                        if cover.alpha() == 0 {
                            continue;
                        }
                        let t = geom.parameter(area.point(x, y));
                        out.copy_from_slice(&pixel(
                            sample_stops(stops, t, *options),
                            self.space,
                            alpha * f64::from(cover.alpha()),
                        ));
                    }
                }
            }
            GradientData::Mesh {
                width,
                height,
                points,
                colors,
                background,
                smooth,
            } => {
                let bg = components(*background, self.space);
                for (out, cover) in pixels
                    .data_mut()
                    .as_chunks_mut::<4>()
                    .0
                    .iter_mut()
                    .zip(coverage.pixels())
                {
                    out.copy_from_slice(&pixel(bg, self.space, alpha * f64::from(cover.alpha())));
                }
                if *width >= 2
                    && *height >= 2
                    && width.checked_mul(*height) == Some(points.len())
                    && points.len() == colors.len()
                {
                    let grid = MeshGrid {
                        width: *width,
                        height: *height,
                        points,
                        colors,
                        smooth: *smooth,
                    };
                    for y in 0..height - 1 {
                        for x in 0..width - 1 {
                            grid.patch(x, y)
                                .paint(&mut pixels, coverage, area, self.space, alpha);
                        }
                    }
                }
            }
        }
        target.draw_pixmap(
            area.x,
            area.y,
            pixels.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            mask,
        );
    }
}

fn sample_stops(stops: &[(f64, [f64; 4])], t: f64, options: u32) -> [f64; 4] {
    let t = if options & 4 != 0 {
        1.0 - (t.rem_euclid(2.0) - 1.0).abs()
    } else if options & 2 != 0 {
        t.rem_euclid(1.0)
    } else {
        t
    };
    let right = stops.partition_point(|stop| stop.0 <= t);
    match right {
        0 => stops[0].1,
        n if n == stops.len() => stops[n - 1].1,
        n => {
            let ((a, ca), (b, cb)) = (stops[n - 1], stops[n]);
            let t = (t - a) / (b - a);
            let alpha = ca[3] + t * (cb[3] - ca[3]);
            if alpha <= 0.0 {
                return [0.0; 4];
            }
            let mix = |i: usize| ((1.0 - t) * ca[i] * ca[3] + t * cb[i] * cb[3]) / alpha;
            [mix(0), mix(1), mix(2), alpha]
        }
    }
}

enum PreparedGeometry {
    Linear {
        ax: f64,
        ay: f64,
        dx: f64,
        dy: f64,
    },
    RadialConcentric {
        cx: f64,
        cy: f64,
        a: f64,
        b: f64,
        inv_span: f64,
        inv_w: f64,
        inv_h: f64,
        elliptical: bool,
    },
    RadialTwoCircle {
        sx: f64,
        sy: f64,
        dx: f64,
        dy: f64,
        r0: f64,
        dr: f64,
        quad_a: f64,
    },
    Angular {
        cx: f64,
        cy: f64,
        start: f64,
        sign: f64,
        abs_span: f64,
        sweep: f64,
        mid: f64,
    },
}

impl Geometry {
    fn prepare(self, bounds: Rect, absolute: bool) -> PreparedGeometry {
        let point_at = |[u, v]: [f64; 2]| {
            if absolute {
                [u, v]
            } else {
                [u * bounds.w, v * bounds.h]
            }
        };
        match self {
            Self::Linear(start, end) => {
                let [ax, ay] = point_at(start);
                let [bx, by] = point_at(end);
                let (x, y) = (bx - ax, by - ay);
                let len_sq = x * x + y * y;
                if len_sq == 0.0 {
                    PreparedGeometry::Linear { ax, ay, dx: 0.0, dy: 0.0 }
                } else {
                    PreparedGeometry::Linear { ax, ay, dx: x / len_sq, dy: y / len_sq }
                }
            }
            Self::Radial(center, start, a, b, elliptical) => {
                let [cx, cy] = point_at(center);
                if let Some(start) = start.filter(|s| *s != center) {
                    let [sx, sy] = point_at(start);
                    let (dx, dy) = (cx - sx, cy - sy);
                    let dr = b - a;
                    let quad_a = dx * dx + dy * dy - dr * dr;
                    PreparedGeometry::RadialTwoCircle { sx, sy, dx, dy, r0: a, dr, quad_a }
                } else {
                    PreparedGeometry::RadialConcentric {
                        cx,
                        cy,
                        a,
                        b,
                        inv_span: if a == b { 0.0 } else { 1.0 / (b - a) },
                        inv_w: if bounds.w != 0.0 { 1.0 / bounds.w } else { 0.0 },
                        inv_h: if bounds.h != 0.0 { 1.0 / bounds.h } else { 0.0 },
                        elliptical,
                    }
                }
            }
            Self::Angular(center, start, end) => {
                let [cx, cy] = point_at(center);
                let span = end - start;
                let abs_span = span.abs();
                let sweep = abs_span.min(TAU);
                let start = if abs_span > TAU {
                    end - TAU.copysign(span)
                } else {
                    start
                };
                PreparedGeometry::Angular {
                    cx,
                    cy,
                    start,
                    sign: span.signum(),
                    abs_span,
                    sweep,
                    mid: (sweep + TAU) * 0.5,
                }
            }
        }
    }
}

impl PreparedGeometry {
    #[inline]
    fn parameter(&self, point: [f64; 2]) -> f64 {
        match *self {
            Self::Linear { ax, ay, dx, dy } => (point[0] - ax) * dx + (point[1] - ay) * dy,
            Self::RadialConcentric { cx, cy, a, b, inv_span, inv_w, inv_h, elliptical } => {
                let (x, y) = (point[0] - cx, point[1] - cy);
                let r = if elliptical {
                    (x * inv_w).hypot(y * inv_h)
                } else {
                    x.hypot(y)
                };
                if a == b {
                    f64::from(r >= a)
                } else {
                    (r - a) * inv_span
                }
            }
            Self::RadialTwoCircle { sx, sy, dx, dy, r0, dr, quad_a } => {
                let (x, y) = (point[0] - sx, point[1] - sy);
                let b = -2.0 * (x * dx + y * dy + r0 * dr);
                let c = x * x + y * y - r0 * r0;
                if quad_a.abs() < f64::EPSILON {
                    return if b == 0.0 { 0.0 } else { -c / b };
                }
                let discriminant = b * b - 4.0 * quad_a * c;
                if discriminant < 0.0 {
                    return 0.0;
                }
                let root = discriminant.sqrt();
                let inv_2a = 0.5 / quad_a;
                let first = (-b - root) * inv_2a;
                if r0 + dr * first >= 0.0 {
                    first
                } else {
                    (-b + root) * inv_2a
                }
            }
            Self::Angular { cx, cy, start, sign, abs_span, sweep, mid } => {
                let angle = (((point[1] - cy).atan2(point[0] - cx) - start) * sign).rem_euclid(TAU);
                if sweep == 0.0 {
                    return f64::from(angle < TAU * 0.5);
                }
                if angle <= sweep {
                    // For multiple turns the last revolution covers only the final stops.
                    (abs_span - sweep + angle) / abs_span
                } else {
                    f64::from(angle < mid)
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Tile {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub bounds: Rect,
    pub scale: f32,
}

impl Tile {
    pub(crate) fn new(pm: &Pixmap, bounds: Rect, limits: Rect, scale: f32) -> Option<Self> {
        let clip = |v: f64, cap: u32| v.clamp(0.0, f64::from(cap)) as i32;
        let s = f64::from(scale);
        let x = clip((limits.x * s).floor(), pm.width());
        let y = clip((limits.y * s).floor(), pm.height());
        let right = clip(((limits.x + limits.w) * s).ceil(), pm.width());
        let bottom = clip(((limits.y + limits.h) * s).ceil(), pm.height());
        (right > x && bottom > y).then_some(Self {
            x,
            y,
            w: (right - x) as u32,
            h: (bottom - y) as u32,
            bounds,
            scale,
        })
    }

    pub(crate) fn transform(self) -> Transform {
        Transform::from_row(
            self.scale,
            0.0,
            0.0,
            self.scale,
            -self.x as f32,
            -self.y as f32,
        )
    }

    fn point(self, x: usize, y: usize) -> [f64; 2] {
        [
            (f64::from(self.x) + x as f64 + 0.5) / f64::from(self.scale) - self.bounds.x,
            (f64::from(self.y) + y as f64 + 0.5) / f64::from(self.scale) - self.bounds.y,
        ]
    }
}

struct MeshGrid<'a> {
    width: usize,
    height: usize,
    points: &'a [MeshPoint],
    colors: &'a [[f64; 4]],
    smooth: bool,
}

type Curve = [[f64; 2]; 4];

struct Patch {
    top: Curve,
    bottom: Curve,
    left: Curve,
    right: Curve,
    colors: [[[f64; 4]; 4]; 4],
    smooth: bool,
}

impl MeshGrid<'_> {
    fn patch(&self, x: usize, y: usize) -> Patch {
        let at = |x, y| self.points[y * self.width + x];
        let curve = |x, y, horizontal| {
            let (dx, dy, start, end) = if horizontal {
                (1, 0, 2, 0)
            } else {
                (0, 1, 3, 1)
            };
            let a: MeshPoint = at(x, y);
            let b: MeshPoint = at(x + dx, y + dy);
            let control = |p: MeshPoint, neighbor: MeshPoint, direction| {
                p.controls.map_or_else(
                    || lerp(p.position, neighbor.position, 1.0 / 3.0),
                    |c| c[direction],
                )
            };
            [
                a.position,
                control(a, b, start),
                control(b, a, end),
                b.position,
            ]
        };
        let colors = std::array::from_fn(|j| {
            std::array::from_fn(|i| {
                let col = (x + i).saturating_sub(1).min(self.width - 1);
                let row = (y + j).saturating_sub(1).min(self.height - 1);
                self.colors[row * self.width + col]
            })
        });
        Patch {
            top: curve(x, y, true),
            bottom: curve(x, y + 1, true),
            left: curve(x, y, false),
            right: curve(x + 1, y, false),
            colors,
            smooth: self.smooth,
        }
    }
}

fn bezier_and_deriv(curve: Curve, t: f64) -> ([f64; 2], [f64; 2]) {
    let s = 1.0 - t;
    let s2 = s * s;
    let t2 = t * t;
    let pt = std::array::from_fn(|i| {
        s2 * s * curve[0][i]
            + 3.0 * t * s2 * curve[1][i]
            + 3.0 * t2 * s * curve[2][i]
            + t2 * t * curve[3][i]
    });
    let d = std::array::from_fn(|i| {
        3.0 * s2 * (curve[1][i] - curve[0][i])
            + 6.0 * s * t * (curve[2][i] - curve[1][i])
            + 3.0 * t2 * (curve[3][i] - curve[2][i])
    });
    (pt, d)
}

fn spline([a, b, c, d]: [[f64; 4]; 4], t: f64) -> [f64; 4] {
    std::array::from_fn(|i| {
        let (a, b, c, d) = (a[i], b[i], c[i], d[i]);
        let left = if a == b { c - b } else { (c - a) * 0.5 };
        let right = if c == d { c - b } else { (d - b) * 0.5 };
        b + t
            * (left + t * (3.0 * (c - b) - 2.0 * left - right + t * (2.0 * (b - c) + left + right)))
    })
}

impl Patch {
    /// Evaluate the Coons patch and its analytical partial derivatives `(dP/du, dP/dv)`.
    fn point_and_jacobian(&self, u: f64, v: f64) -> ([f64; 2], [f64; 2], [f64; 2]) {
        let (top, dtop) = bezier_and_deriv(self.top, u);
        let (bot, dbot) = bezier_and_deriv(self.bottom, u);
        let (left, dleft) = bezier_and_deriv(self.left, v);
        let (right, dright) = bezier_and_deriv(self.right, v);
        let (c00, c10, c01, c11) = (self.top[0], self.top[3], self.bottom[0], self.bottom[3]);
        let a = lerp(top, bot, v);
        let b = lerp(left, right, u);
        let bilinear = lerp(lerp(c00, c10, u), lerp(c01, c11, u), v);
        let p = [a[0] + b[0] - bilinear[0], a[1] + b[1] - bilinear[1]];
        let du = [
            (1.0 - v) * dtop[0] + v * dbot[0] + (right[0] - left[0])
                - ((1.0 - v) * (c10[0] - c00[0]) + v * (c11[0] - c01[0])),
            (1.0 - v) * dtop[1] + v * dbot[1] + (right[1] - left[1])
                - ((1.0 - v) * (c10[1] - c00[1]) + v * (c11[1] - c01[1])),
        ];
        let dv = [
            (bot[0] - top[0]) + (1.0 - u) * dleft[0] + u * dright[0]
                - ((1.0 - u) * (c01[0] - c00[0]) + u * (c11[0] - c10[0])),
            (bot[1] - top[1]) + (1.0 - u) * dleft[1] + u * dright[1]
                - ((1.0 - u) * (c01[1] - c00[1]) + u * (c11[1] - c10[1])),
        ];
        (p, du, dv)
    }

    fn coordinates(&self, point: [f64; 2], mut u: f64, mut v: f64) -> Option<(f64, f64)> {
        let e = 0.00001;
        // Invert the patch locally using the analytical Jacobian. Divergent guesses on folded or degenerate patches are discarded.
        for _ in 0..12 {
            let ([x, y], [ux, uy], [vx, vy]) = self.point_and_jacobian(u, v);
            let (dx, dy) = (point[0] - x, point[1] - y);
            if dx.abs().max(dy.abs()) <= e {
                return ((-e..=1.0 + e).contains(&u) && (-e..=1.0 + e).contains(&v))
                    .then_some((u.clamp(0.0, 1.0), v.clamp(0.0, 1.0)));
            }
            let det = ux * vy - uy * vx;
            if det.abs() < f64::EPSILON {
                break;
            }
            let inv_det = 1.0 / det;
            u += (dx * vy - dy * vx) * inv_det;
            v += (ux * dy - uy * dx) * inv_det;
        }
        None
    }

    fn paint(
        &self,
        pixels: &mut Pixmap,
        coverage: &Pixmap,
        area: Tile,
        space: GradientColorSpace,
        alpha: f64,
    ) {
        let mut min = [f64::INFINITY; 2];
        let mut max = [f64::NEG_INFINITY; 2];
        for [x, y] in self
            .top
            .into_iter()
            .chain(self.bottom)
            .chain(self.left)
            .chain(self.right)
        {
            min = [min[0].min(x), min[1].min(y)];
            max = [max[0].max(x), max[1].max(y)];
        }
        // A Coons patch can extend beyond its boundary curves. Pad curved patches by the
        // maximum deviation of either boundary from its straight chord.
        let pad = |axis: usize| {
            [self.top, self.bottom, self.left, self.right]
                .iter()
                .fold(0.0_f64, |pad, c| {
                    pad.max((c[1][axis] - lerp(c[0], c[3], 1.0 / 3.0)[axis]).abs())
                        .max((c[2][axis] - lerp(c[0], c[3], 2.0 / 3.0)[axis]).abs())
                })
        };
        let pad_x = pad(0);
        let pad_y = pad(1);
        let s = f64::from(area.scale);
        let tx = |p: f64| (area.bounds.x + area.bounds.w * p) * s - f64::from(area.x);
        let ty = |p: f64| (area.bounds.y + area.bounds.h * p) * s - f64::from(area.y);
        let left = tx(min[0] - pad_x).floor().clamp(0.0, f64::from(area.w)) as usize;
        let right = tx(max[0] + pad_x).ceil().clamp(0.0, f64::from(area.w)) as usize;
        let top = ty(min[1] - pad_y).floor().clamp(0.0, f64::from(area.h)) as usize;
        let bottom = ty(max[1] + pad_y).ceil().clamp(0.0, f64::from(area.h)) as usize;
        if area.bounds.w <= 0.0 || area.bounds.h <= 0.0 {
            return;
        }
        let inv_w = 1.0 / area.bounds.w;
        let inv_h = 1.0 / area.bounds.h;
        let span_x = max[0] - min[0];
        let span_y = max[1] - min[1];
        for y in top..bottom {
            let mut warm: Option<(f64, f64)> = None;
            for x in left..right {
                let i = y * area.w as usize + x;
                let a = coverage.pixels()[i].alpha();
                if a == 0 {
                    warm = None;
                    continue;
                }
                let [px, py] = area.point(x, y);
                let point = [px * inv_w, py * inv_h];
                let hit = warm.and_then(|(wu, wv)| self.coordinates(point, wu, wv)).or_else(|| {
                    let u = ((point[0] - min[0]) / span_x).clamp(0.0, 1.0);
                    let v = ((point[1] - min[1]) / span_y).clamp(0.0, 1.0);
                    self.coordinates(point, u, v)
                });
                warm = hit;
                if let Some((u, v)) = hit {
                    let color = if self.smooth {
                        spline(self.colors.map(|row| spline(row, u)), v)
                    } else {
                        lerp(
                            lerp(self.colors[1][1], self.colors[1][2], u),
                            lerp(self.colors[2][1], self.colors[2][2], u),
                            v,
                        )
                    };
                    pixels.data_mut()[i * 4..i * 4 + 4].copy_from_slice(&pixel(
                        color,
                        space,
                        alpha * f64::from(a),
                    ));
                }
            }
        }
    }
}
