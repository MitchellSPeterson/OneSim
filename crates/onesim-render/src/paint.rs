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
                        let t = geometry.parameter(area.point(x, y), area.bounds, *absolute);
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

impl Geometry {
    fn parameter(self, point: [f64; 2], bounds: Rect, absolute: bool) -> f64 {
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
                let length = x.hypot(y);
                if length == 0.0 {
                    0.0
                } else {
                    ((point[0] - ax) * (x / length) + (point[1] - ay) * (y / length)) / length
                }
            }
            Self::Radial(center, start, a, b, elliptical) => {
                let [cx, cy] = point_at(center);
                let (x, y) = (point[0] - cx, point[1] - cy);
                let r = if elliptical {
                    (x / bounds.w).hypot(y / bounds.h)
                } else {
                    x.hypot(y)
                };
                if let Some(start) = start.filter(|s| *s != center) {
                    let [sx, sy] = point_at(start);
                    return circle_parameter(
                        [point[0] - sx, point[1] - sy],
                        [cx - sx, cy - sy],
                        a,
                        b,
                    );
                }
                if a == b {
                    f64::from(r >= a)
                } else {
                    (r - a) / (b - a)
                }
            }
            Self::Angular(center, start, end) => {
                let [cx, cy] = point_at(center);
                let span = end - start;
                let sweep = span.abs().min(TAU);
                let start = if span.abs() > TAU {
                    end - TAU.copysign(span)
                } else {
                    start
                };
                let angle = ((point[1] - cy).atan2(point[0] - cx) - start) * span.signum();
                let angle = angle.rem_euclid(TAU);
                if sweep == 0.0 {
                    return f64::from(angle < TAU * 0.5);
                }
                if angle <= sweep {
                    // For multiple turns the last revolution covers only the final stops.
                    (span.abs() - sweep + angle) / span.abs()
                } else {
                    f64::from(angle < (sweep + TAU) * 0.5)
                }
            }
        }
    }
}

fn circle_parameter([x, y]: [f64; 2], [dx, dy]: [f64; 2], r0: f64, r1: f64) -> f64 {
    let dr = r1 - r0;
    let a = dx * dx + dy * dy - dr * dr;
    let b = -2.0 * (x * dx + y * dy + r0 * dr);
    let c = x * x + y * y - r0 * r0;
    if a.abs() < f64::EPSILON {
        return if b == 0.0 { 0.0 } else { -c / b };
    }
    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 {
        return 0.0;
    }
    let root = discriminant.sqrt();
    let first = (-b - root) / (2.0 * a);
    let second = (-b + root) / (2.0 * a);
    if r0 + dr * first >= 0.0 {
        first
    } else {
        second
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

fn bezier(curve: Curve, t: f64) -> [f64; 2] {
    let s = 1.0 - t;
    std::array::from_fn(|i| {
        s * s * s * curve[0][i]
            + 3.0 * t * s * s * curve[1][i]
            + 3.0 * t * t * s * curve[2][i]
            + t * t * t * curve[3][i]
    })
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
    fn point(&self, u: f64, v: f64) -> [f64; 2] {
        let a = lerp(bezier(self.top, u), bezier(self.bottom, u), v);
        let b = lerp(bezier(self.left, v), bezier(self.right, v), u);
        let bilinear = lerp(
            lerp(self.top[0], self.top[3], u),
            lerp(self.bottom[0], self.bottom[3], u),
            v,
        );
        [a[0] + b[0] - bilinear[0], a[1] + b[1] - bilinear[1]]
    }

    fn coordinates(&self, point: [f64; 2], mut u: f64, mut v: f64) -> Option<(f64, f64)> {
        let e = 0.00001;
        // Invert the patch locally. Divergent guesses on folded or degenerate patches are discarded.
        for _ in 0..12 {
            let [x, y] = self.point(u, v);
            let (dx, dy) = (point[0] - x, point[1] - y);
            if dx.abs().max(dy.abs()) <= e {
                return ((-e..=1.0 + e).contains(&u) && (-e..=1.0 + e).contains(&v))
                    .then_some((u.clamp(0.0, 1.0), v.clamp(0.0, 1.0)));
            }
            let [ux, uy] = self.point(u + e, v);
            let [vx, vy] = self.point(u, v + e);
            let (ux, uy, vx, vy) = (ux - x, uy - y, vx - x, vy - y);
            let det = ux * vy - uy * vx;
            if det.abs() < f64::EPSILON {
                break;
            }
            u += e * (dx * vy - dy * vx) / det;
            v += e * (ux * dy - uy * dx) / det;
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
        for y in top..bottom {
            for x in left..right {
                let i = y * area.w as usize + x;
                let a = coverage.pixels()[i].alpha();
                if a == 0 {
                    continue;
                }
                let [px, py] = area.point(x, y);
                let point = [px / area.bounds.w, py / area.bounds.h];
                let u = ((point[0] - min[0]) / (max[0] - min[0])).clamp(0.0, 1.0);
                let v = ((point[1] - min[1]) / (max[1] - min[1])).clamp(0.0, 1.0);
                if let Some((u, v)) = self.coordinates(point, u, v) {
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
