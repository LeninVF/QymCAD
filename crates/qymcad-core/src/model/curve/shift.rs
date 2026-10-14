//! A CURVE MOVED AND A CURVE OFFSET: taken through a map of the sheet - a shift, a turn, a mirror, any of them after
//! another - and laid at a distance to one side of itself.
use super::*;

/// A MAP OF THE SHEET that keeps lengths: a point `p` goes to `origin + p.x * x + p.y * y`, `x` and `y` square unit
/// vectors - turned, or turned over for a mirror.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Map {
    pub origin: Point2,
    pub x: Point2,
    pub y: Point2,
}

impl Map {
    /// A shift by `by`.
    pub fn shift(by: Point2) -> Map {
        Map { origin: by, x: Point2::new(1.0, 0.0), y: Point2::new(0.0, 1.0) }
    }

    /// A turn by `angle` radians about `centre`.
    pub fn turn(centre: Point2, angle: f64) -> Map {
        let (s, c) = angle.sin_cos();
        let (x, y) = (Point2::new(c, s), Point2::new(-s, c));
        Map { origin: Point2::new(centre.x - (centre.x * x.x + centre.y * y.x), centre.y - (centre.x * x.y + centre.y * y.y)), x, y }
    }

    /// A mirror about the line through `a` and `b`.
    pub fn mirror(line: [Point2; 2]) -> Map {
        let [a, b] = line;
        let d = b - a;
        let l = d.len().max(1e-300);
        let (ux, uy) = (d.x / l, d.y / l);
        // reflection across a line through the origin of direction u: [[2ux^2-1, 2uxuy], [2uxuy, 2uy^2-1]]
        let (x, y) = (Point2::new(2.0 * ux * ux - 1.0, 2.0 * ux * uy), Point2::new(2.0 * ux * uy, 2.0 * uy * uy - 1.0));
        Map { origin: Point2::new(a.x - (a.x * x.x + a.y * y.x), a.y - (a.x * x.y + a.y * y.y)), x, y }
    }

    /// Where point `p` goes.
    pub fn point(&self, p: Point2) -> Point2 {
        Point2::new(self.origin.x + p.x * self.x.x + p.y * self.y.x, self.origin.y + p.x * self.x.y + p.y * self.y.y)
    }

    /// Where vector `v` goes: turned, the shift left out.
    pub fn vector(&self, v: Point2) -> Point2 {
        Point2::new(v.x * self.x.x + v.y * self.y.x, v.x * self.x.y + v.y * self.y.y)
    }

    /// Whether the map turns the sheet over - a mirror - and every way round goes the other way.
    pub fn turns_over(&self) -> bool {
        self.x.x * self.y.y - self.x.y * self.y.x < 0.0
    }
}

/// The least distance from `p` to a polyline.
fn off_polyline(p: Point2, line: &[Point2]) -> f64 {
    line.windows(2).map(|w| Curve::Line { a: w[0], b: w[1] }.nearest(p).off).fold(f64::MAX, f64::min)
}

/// The tolerance an offset of an ellipse or a spline is laid within, in millimetres.
pub const OFFSET_TOLERANCE: f64 = 0.01;

impl Curve {
    /// THE CURVE TAKEN THROUGH `map`, the same kind: an arc turned over runs the other way round.
    pub fn mapped(&self, map: &Map) -> Curve {
        match self {
            Curve::Line { a, b } => Curve::Line { a: map.point(*a), b: map.point(*b) },
            Curve::Round { centre, r, from, sweep } => {
                let start = map.vector(Point2::new(from.cos(), from.sin()));
                let sweep = if map.turns_over() { -sweep } else { *sweep };
                Curve::Round { centre: map.point(*centre), r: *r, from: start.y.atan2(start.x), sweep }
            }
            Curve::Oval { centre, major, minor, from, sweep } => Curve::Oval { centre: map.point(*centre), major: map.vector(*major), minor: map.vector(*minor), from: *from, sweep: *sweep },
            Curve::Spline { nodes, tangents, ends } => Curve::Spline {
                nodes: nodes.iter().map(|p| map.point(*p)).collect(),
                tangents: tangents.iter().map(|t| NodeTangents { arrive: map.vector(t.arrive), leave: map.vector(t.leave) }).collect(),
                ends: *ends,
            },
        }
    }

    /// THE CURVE LAID `d` TO THE LEFT OF ITSELF - looking along it; a negative `d` to the right: a line and an arc or a
    /// circle exactly, the same kind; an ellipse and a spline - whose offsets are no ellipse and no spline - as a spline
    /// through points of the offset, as many as keep the middle of every span within `OFFSET_TOLERANCE` of the true offset. `None` where the offset folds away: an arc offset past its centre.
    pub fn offset(&self, d: f64) -> Option<Curve> {
        match self {
            Curve::Line { a, b } => {
                let t = self.tangent(0.0);
                let n = Point2::new(-t.y * d, t.x * d);
                Some(Curve::Line { a: *a + n, b: *b + n })
            }
            Curve::Round { centre, r, from, sweep } => {
                // the left of a way round counter-clockwise is its centre
                let r2 = if *sweep >= 0.0 { r - d } else { r + d };
                (r2 > 1e-9).then_some(Curve::Round { centre: *centre, r: r2, from: *from, sweep: *sweep })
            }
            Curve::Oval { .. } | Curve::Spline { .. } => {
                let true_at = |t: f64| {
                    let (p, u) = (self.at(t), self.tangent(t));
                    Point2::new(p.x - u.y * d, p.y + u.x * d)
                };
                let closed = self.closed();
                // the true offset, densely: what the laid spline is held to
                let dense: Vec<Point2> = (0..=4000).map(|k| true_at(k as f64 / 4000.0)).collect();
                let mut n = (self.spans() * 8).max(16);
                loop {
                    let count = if closed { n } else { n + 1 };
                    let pts: Vec<Point2> = (0..count).map(|k| true_at(k as f64 / n as f64)).collect();
                    let ends = if closed { crate::feature::Ends::Closed } else { crate::feature::Ends::Open };
                    let tangents = (0..pts.len()).map(|i| tess::spline_tangents_at(&pts, &[], i, ends)).collect();
                    let laid = Curve::Spline { nodes: pts, tangents, ends };
                    let spans = laid.spans();
                    let worst = (0..spans).map(|k| (k as f64 + 0.5) / spans as f64).map(|t| off_polyline(laid.at(t), &dense)).fold(0.0, f64::max);
                    if worst <= OFFSET_TOLERANCE || n >= 4096 {
                        return Some(laid);
                    }
                    n *= 2;
                }
            }
        }
    }
}
