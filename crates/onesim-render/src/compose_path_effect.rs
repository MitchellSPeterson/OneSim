use onesim_protocol::{ComposePathEffect, ComposeStampStyle};
use tiny_skia::{FillRule, Path, PathBuilder, PathSegment, Point, Stroke, StrokeDash, Transform};

use crate::scene::{Rect, VectorGeom};

pub(crate) fn affects_fill(effect: &ComposePathEffect) -> bool {
    match effect {
        ComposePathEffect::Dash { .. } => false,
        ComposePathEffect::Chain { outer, inner } => affects_fill(inner) || affects_fill(outer),
        _ => true,
    }
}

/// Geometry and painting mode after a Compose path effect. Stamps replace the stroke with fills.
pub(crate) struct StrokePath {
    pub path: Path,
    pub stroke: Option<Stroke>,
    pub fill_rule: FillRule,
}

impl StrokePath {
    pub fn new(path: Path, stroke: Stroke) -> Self {
        Self { path, stroke: Some(stroke), fill_rule: FillRule::Winding }
    }

    pub fn apply(self, effect: &ComposePathEffect, unit: f32) -> Option<Self> {
        let mut result = self;
        match effect {
            ComposePathEffect::Chain { outer, inner } => return result.apply(inner, unit)?.apply(outer, unit),
            ComposePathEffect::Dash { intervals, phase } => {
                if let Some(stroke) = &mut result.stroke {
                    if let Some(dash) = StrokeDash::new(intervals.iter().map(|v| v * unit).collect(), phase * unit) {
                        result.path = result.path.dash(&dash, (1.0 / unit).max(1.0))?;
                        stroke.dash = None;
                    }
                }
            }
            ComposePathEffect::Corner { radius } if *radius > 0.0 => {
                result.path = round_corners(&result.path, radius * unit)?;
            }
            ComposePathEffect::Stamp { shape, advance, phase, style } if advance.is_finite() && *advance > 0.0 && phase.is_finite() => {
                let geom = VectorGeom::Path { ops: shape.path.clone(), mirror_x: false };
                if let Some(stamp) = crate::raster::build_vector_path(&geom, Rect::default(), 0.0)
                    .and_then(|path| path.transform(Transform::from_scale(unit, unit)))
                {
                    result.path = stamp_path(&result.path, &stamp, advance * unit, phase * unit, *style, unit)?;
                    result.fill_rule = if shape.even_odd { FillRule::EvenOdd } else { FillRule::Winding };
                    result.stroke = None;
                }
            }
            _ => {}
        }
        Some(result)
    }
}

struct Contour {
    start: Point,
    segments: Vec<PathSegment>,
    closed: bool,
}

fn contours(path: &Path) -> Vec<Contour> {
    let mut vec: Vec<Contour> = Vec::new();
    let mut segments = path.segments();
    segments.set_auto_close(true);
    for segment in segments {
        if let PathSegment::MoveTo(start) = segment {
            vec.push(Contour { start, segments: Vec::new(), closed: false });
        } else if let Some(contour) = vec.last_mut() {
            if matches!(segment, PathSegment::Close) {
                contour.closed = true;
            } else {
                contour.segments.push(segment);
            }
        }
    }
    vec
}

fn endpoint(segment: PathSegment) -> Point {
    match segment {
        PathSegment::MoveTo(p) | PathSegment::LineTo(p) | PathSegment::QuadTo(_, p) | PathSegment::CubicTo(_, _, p) => p,
        PathSegment::Close => unreachable!("contour commands exclude close"),
    }
}

fn add_segment(path: &mut PathBuilder, segment: PathSegment) {
    match segment {
        PathSegment::MoveTo(p) => path.move_to(p.x, p.y),
        PathSegment::LineTo(p) => path.line_to(p.x, p.y),
        PathSegment::QuadTo(c, p) => path.quad_to(c.x, c.y, p.x, p.y),
        PathSegment::CubicTo(a, b, p) => path.cubic_to(a.x, a.y, b.x, b.y, p.x, p.y),
        PathSegment::Close => path.close(),
    }
}

fn scaled(point: Point, factor: f32) -> Point {
    point * Point::from_xy(factor, factor)
}

fn round_corners(path: &Path, radius: f32) -> Option<Path> {
    let mut result = PathBuilder::new();
    for contour in contours(path) {
        let mut at = contour.start;
        let mut adjusted = Vec::with_capacity(contour.segments.len());
        for segment in contour.segments {
            let end = endpoint(segment);
            if let PathSegment::LineTo(_) = segment {
                let len = at.distance(end);
                if len == 0.0 {
                    continue;
                }
                let delta = scaled(end - at, (radius / len).min(0.5));
                adjusted.push((at, at + delta, PathSegment::LineTo(end - delta), end));
            } else {
                adjusted.push((at, at, segment, end));
            }
            at = end;
        }
        let Some(&(_, first, _, _)) = adjusted.first() else { continue };
        add_segment(&mut result, PathSegment::MoveTo(if contour.closed { first } else { contour.start }));
        let len = adjusted.len();
        for (i, (at, start, mut segment, end)) in adjusted.into_iter().enumerate() {
            if i > 0 {
                add_segment(&mut result, PathSegment::QuadTo(at, start));
            }
            if !contour.closed && i + 1 == len && matches!(segment, PathSegment::LineTo(_)) {
                segment = PathSegment::LineTo(end);
            }
            add_segment(&mut result, segment);
        }
        if contour.closed {
            add_segment(&mut result, PathSegment::QuadTo(contour.start, first));
            result.close();
        }
    }
    result.finish()
}

// Split curves until the control polygon's excess length is negligible at device scale.
fn flatten(start: Point, controls: &[Point], tolerance: f32, depth: u8, out: &mut Vec<Point>) {
    let end = *controls.last().expect("curve endpoint");
    let polygon = std::iter::once(start).chain(controls.iter().copied()).zip(controls.iter().copied());
    let length: f32 = polygon.map(|(a, b)| a.distance(b)).sum();
    if depth == 12 || length - start.distance(end) <= tolerance {
        out.push(end);
        return;
    }
    let mut points = [start; 4];
    points[1..=controls.len()].copy_from_slice(controls);
    let mut left = [start; 3];
    let mut right = [end; 3];
    let degree = controls.len();
    for level in 1..=degree {
        for i in 0..=degree - level {
            points[i] = scaled(points[i] + points[i + 1], 0.5);
        }
        left[level - 1] = points[0];
        if level < degree {
            right[degree - level - 1] = points[degree - level];
        }
    }
    flatten(start, &left[..degree], tolerance, depth + 1, out);
    flatten(left[degree - 1], &right[..degree], tolerance, depth + 1, out);
}

struct MeasuredContour {
    points: Vec<Point>,
    offsets: Vec<f32>,
}

impl MeasuredContour {
    fn new(contour: Contour, unit: f32) -> Self {
        let mut points = vec![contour.start];
        let mut at = contour.start;
        for segment in contour.segments {
            match segment {
                PathSegment::QuadTo(c, p) => flatten(at, &[c, p], 0.01 * unit, 0, &mut points),
                PathSegment::CubicTo(a, b, p) => flatten(at, &[a, b, p], 0.01 * unit, 0, &mut points),
                segment => points.push(endpoint(segment)),
            }
            at = endpoint(segment);
        }
        points.dedup();
        let mut offsets = vec![0.0];
        let mut distance = 0.0;
        for pair in points.windows(2) {
            distance += pair[0].distance(pair[1]);
            offsets.push(distance);
        }
        Self { points, offsets }
    }

    fn sample(&self, distance: f32) -> (Point, Point) {
        let distance = distance.clamp(0.0, *self.offsets.last().unwrap());
        let i = self.offsets.partition_point(|&at| at < distance).clamp(1, self.points.len() - 1);
        let segment = self.points[i] - self.points[i - 1];
        let tangent = scaled(segment, 1.0 / segment.length());
        (self.points[i - 1] + scaled(tangent, distance - self.offsets[i - 1]), tangent)
    }
}

fn stamp_path(path: &Path, stamp: &Path, advance: f32, phase: f32, style: ComposeStampStyle, unit: f32) -> Option<Path> {
    let mut result = PathBuilder::new();
    let mut remaining = (100_000 / stamp.points().len()).max(1);
    for contour in contours(path) {
        let contour = MeasuredContour::new(contour, unit);
        let length = *contour.offsets.last().unwrap();
        let mut distance = (-phase).rem_euclid(advance);
        while distance < length && remaining > 0 {
            let (p, tangent) = contour.sample(distance);
            if style == ComposeStampStyle::Morph {
                morph_stamp(&mut result, stamp, &contour, distance);
            } else {
                let (cos, sin) = if style == ComposeStampStyle::Rotate { (tangent.x, tangent.y) } else { (1.0, 0.0) };
                let transform = Transform::from_row(cos, sin, -sin, cos, p.x, p.y);
                if let Some(path) = stamp.clone().transform(transform) {
                    result.push_path(&path);
                }
            }
            distance += advance;
            remaining -= 1;
        }
    }
    result.finish()
}

fn morph_stamp(result: &mut PathBuilder, stamp: &Path, contour: &MeasuredContour, distance: f32) {
    let map = |p: Point| {
        let (base, tangent) = contour.sample(distance + p.x);
        Point::from_xy(base.x - tangent.y * p.y, base.y + tangent.x * p.y)
    };
    let mut at = Point::zero();
    let mut segments = stamp.segments();
    segments.set_auto_close(true);
    for segment in segments {
        let verb = match segment {
            PathSegment::MoveTo(p) => PathSegment::MoveTo(map(p)),
            PathSegment::LineTo(p) => PathSegment::QuadTo(map(scaled(at + p, 0.5)), map(p)),
            PathSegment::QuadTo(c, p) => PathSegment::QuadTo(map(c), map(p)),
            PathSegment::CubicTo(a, b, p) => PathSegment::CubicTo(map(a), map(b), map(p)),
            PathSegment::Close => PathSegment::Close,
        };
        add_segment(result, verb);
        if !matches!(segment, PathSegment::Close) {
            at = endpoint(segment);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use onesim_protocol::{ComposeStampShape, PathOp};

    fn angle() -> Path {
        let mut path = PathBuilder::new();
        path.move_to(0.0, 0.0);
        path.line_to(30.0, 0.0);
        path.line_to(30.0, 30.0);
        path.finish().unwrap()
    }

    #[test]
    fn chains_run_inner_effect_before_outer() {
        let dash = ComposePathEffect::Dash { intervals: vec![10.0, 2.0], phase: 0.0 };
        let corner = ComposePathEffect::Corner { radius: 5.0 };
        let chain = |outer, inner| ComposePathEffect::Chain { outer: Box::new(outer), inner: Box::new(inner) };
        let run = |effect| StrokePath::new(angle(), Stroke::default()).apply(&effect, 1.0).unwrap().path;
        assert_ne!(run(chain(dash.clone(), corner.clone())), run(chain(corner, dash)));
    }

    #[test]
    fn stamps_are_filled_and_respect_phase_and_orientation() {
        let stamp = |style| ComposePathEffect::Stamp {
            shape: ComposeStampShape {
                path: vec![PathOp::M([0.0, -2.0]), PathOp::L([9.0, 0.0]), PathOp::L([0.0, 2.0]), PathOp::Z],
                even_odd: true,
            },
            advance: 10.0, phase: 3.0, style,
        };
        let run = |style| StrokePath::new(angle(), Stroke::default()).apply(&stamp(style), 1.0).unwrap();
        let translated = run(ComposeStampStyle::Translate);
        let rotated = run(ComposeStampStyle::Rotate);
        let morphed = run(ComposeStampStyle::Morph);
        assert!(translated.stroke.is_none());
        assert_eq!(translated.fill_rule, FillRule::EvenOdd);
        assert_eq!(translated.path.bounds().left(), 7.0);
        assert!(rotated.path.bounds().right() < translated.path.bounds().right());
        assert_ne!(rotated.path, morphed.path);
    }
}
