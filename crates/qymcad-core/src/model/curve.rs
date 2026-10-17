//! THE CURVE OF A SKETCH, ONE FOR EVERY KIND: a line, a circle or an arc of one, an ellipse or an arc of one, a spline -
//! read off its entity once (`Curve::of`) and asked the same questions whatever it is: the point and the direction at a
//! place along it, its length, the place nearest a point, the points it is drawn by, where it meets another curve.
//!
//! The kind of an entity is told apart here and in the model that declares it, and nowhere else: a tool, a snap, a
//! preview or the solver asks the curve, it does not take the kind apart itself.
//!
//! A place along a curve is its parameter `t` from 0 at its start to 1 at its end. On a line that is the share of its
//! length, on a round curve the share of its sweep, on an ellipse the share of the sweep of its eccentric angle, on a
//! spline the span the place lies in plus the share of that span's own parameter - so `t = 1.5 / n` is half way along the
//! second of `n` spans.
use super::*;

mod arrange;
mod blend;
mod cut;
mod edges;
mod fit;
mod marks;
pub mod kinds;
mod meet;
pub mod refs;
mod shift;
mod stretch;
pub(in crate::model) use arrange::Arranged;
pub(crate) use cut::Trimmed;
pub use blend::{centres_touching, chamfer, radius_in, rounding, Arm, Half, Rounding};
pub use marks::OvalAxes;
pub use meet::Meet;
pub use shift::{Map, OFFSET_TOLERANCE};
pub use stretch::End;

/// How far a laid out curve may stray from the true one, at most, in millimetres: 0.005 mm, checked at a quarter, a half
/// and three quarters of every piece - one look at the middle of a piece misses the sag of a spline at a turn of its
/// curvature, measured at 0.03 mm on an S-shaped span laid out by its middles alone.
pub const SAG: f64 = 0.005;

/// A CURVE OF A SKETCH AS GEOMETRY: where it stands, read off its entity.
#[derive(Clone, Debug, PartialEq)]
pub enum Curve {
    /// a straight piece from `a` to `b`
    Line { a: Point2, b: Point2 },
    /// a circle, or an arc of one: from the angle `from`, through the signed `sweep` (a whole turn for a circle)
    Round { centre: Point2, r: f64, from: f64, sweep: f64 },
    /// an ellipse, or an arc of one: its semi-axes as vectors from the centre, from the eccentric angle `from` through the
    /// signed `sweep`
    Oval { centre: Point2, major: Point2, minor: Point2, from: f64, sweep: f64 },
    /// a spline through `nodes`, a loop when `closed`, with the tangents each node is arrived at and left by
    Spline { nodes: Vec<Point2>, tangents: Vec<NodeTangents>, ends: crate::feature::Ends },
}

/// A PLACE ON A CURVE: its parameter, the point there and how far it is from what it was asked about.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct At {
    pub t: f64,
    pub point: Point2,
    pub off: f64,
}

/// A whole turn.
const TURN: f64 = std::f64::consts::TAU;

/// AN ARC OF A CIRCLE BY ITS CENTRE AND ITS ENDS, and the way it runs from the first to the second.
#[derive(Clone, Copy, Debug)]
struct ArcEnds {
    centre: Point2,
    from: Point2,
    to: Point2,
    winding: crate::feature::Winding,
}

impl Curve {
    /// THE CURVE OF ENTITY `e` of sketch `s`, where its points stand; `None` when a point of it is missing.
    pub fn of(s: &Sketch, e: &SketchEntity) -> Option<Curve> {
        Curve::of_placed(&|id: Id| s.point(id).map(|p| Point2::new(p.x, p.y)), e)
    }

    /// THE CURVE OF ENTITY `e` with its points where `at` puts them; `None` when a point of it is missing.
    pub fn of_placed(at: &dyn Fn(Id) -> Option<Point2>, e: &SketchEntity) -> Option<Curve> {
        Some(match &e.kind {
            EntityKind::Line { a, b } => Curve::Line { a: at(*a)?, b: at(*b)? },
            EntityKind::Circle { center, r } => Curve::Round { centre: at(*center)?, r: *r, from: 0.0, sweep: TURN },
            EntityKind::Arc { center, a, b, ccw } => {
                let winding = if *ccw { crate::feature::Winding::Ccw } else { crate::feature::Winding::Cw };
                Curve::arc_of(ArcEnds { centre: at(*center)?, from: at(*a)?, to: at(*b)?, winding })
            }
            EntityKind::Ellipse { c, ma, mi, span } => {
                let (pc, pa, pi) = (at(*c)?, at(*ma)?, at(*mi)?);
                let (major, minor) = (pa - pc, pi - pc);
                let (from, sweep) = match span {
                    None => (0.0, TURN),
                    Some(sp) => {
                        let axes = [pc, pa, pi];
                        let (t0, t1) = (tess::ellipse_angle(axes, at(sp.from)?), tess::ellipse_angle(axes, at(sp.to)?));
                        let sweep = match sp.winding {
                            crate::feature::Winding::Ccw => (t1 - t0).rem_euclid(TURN),
                            crate::feature::Winding::Cw => -(t0 - t1).rem_euclid(TURN),
                        };
                        (t0, sweep)
                    }
                };
                Curve::Oval { centre: pc, major, minor, from, sweep }
            }
            EntityKind::Spline { nodes, handles, ends } => Curve::spline(nodes.iter().map(|id| at(*id)).collect::<Option<_>>()?, handles, *ends),
        })
    }

    /// THE ARC ABOUT ITS CENTRE FROM ONE END ROUND TO THE OTHER, the way `winding` says, of the radius its first end
    /// stands at.
    fn arc_of(ends: ArcEnds) -> Curve {
        let ArcEnds { centre, from, to, winding } = ends;
        let angle = |p: Point2| (p.y - centre.y).atan2(p.x - centre.x);
        let (a0, a1) = (angle(from), angle(to));
        let sweep = match winding {
            crate::feature::Winding::Ccw => (a1 - a0).rem_euclid(TURN),
            crate::feature::Winding::Cw => -(a0 - a1).rem_euclid(TURN),
        };
        Curve::Round { centre, r: centre.dist(from), from: a0, sweep }
    }

    /// THE CURVE A CONSTRAINT REFERS TO (`CurveRef`), its points where `at` puts them and the radius of a circle about a
    /// centre where `radius` reads it: a line between its ends, a circle whole, an arc between its ends, an ellipse whole,
    /// a spline as drawn. `None` for a point, or where a point or a radius is missing.
    pub fn of_ref(on: &crate::model::CurveRef, at: &dyn Fn(Id) -> Option<Point2>, radius: &dyn Fn(Id) -> Option<f64>) -> Option<Curve> {
        use crate::model::CurveRef;
        Some(match on {
            CurveRef::Point { .. } => return None,
            CurveRef::Line { a, b } => Curve::Line { a: at(*a)?, b: at(*b)? },
            CurveRef::Round { c } => Curve::Round { centre: at(*c)?, r: radius(*c)?, from: 0.0, sweep: TURN },
            CurveRef::Arc { c, a, b, winding } => Curve::arc_of(ArcEnds { centre: at(*c)?, from: at(*a)?, to: at(*b)?, winding: *winding }),
            CurveRef::Oval { c, ma, mi } => {
                let centre = at(*c)?;
                Curve::Oval { centre, major: at(*ma)? - centre, minor: at(*mi)? - centre, from: 0.0, sweep: TURN }
            }
            CurveRef::Spline { nodes, handles, ends } => Curve::spline(nodes.iter().map(|id| at(*id)).collect::<Option<_>>()?, handles, *ends),
        })
    }

    /// THE CURVE REACHING `k` OF ITS LENGTHS PAST EACH END ALONG THE STRAIGHT IT LIES ON, where it is a line - as a line is
    /// touched anywhere along it; a curve of any other kind as it is.
    pub fn reaching(&self, k: f64) -> Curve {
        match *self {
            Curve::Line { a, b } => {
                let run = b - a;
                Curve::Line { a: a - Point2::new(run.x * k, run.y * k), b: b + Point2::new(run.x * k, run.y * k) }
            }
            _ => self.clone(),
        }
    }

    /// THE SPLINE THROUGH `nodes` with `handles`, open or closed, as the sketch draws it.
    pub fn spline(nodes: Vec<Point2>, handles: &[Handle], ends: crate::feature::Ends) -> Curve {
        let tangents = (0..nodes.len()).map(|i| tess::spline_tangents_at(&nodes, handles, i, ends)).collect();
        Curve::Spline { nodes, tangents, ends }
    }

    /// Whether the curve closes on itself: a whole circle, a whole ellipse, a closed spline.
    pub fn closed(&self) -> bool {
        match self {
            Curve::Line { .. } => false,
            Curve::Round { sweep, .. } | Curve::Oval { sweep, .. } => sweep.abs() >= TURN - 1e-12,
            Curve::Spline { ends, .. } => *ends == crate::feature::Ends::Closed,
        }
    }

    /// The number of spans of a spline, one for any other curve.
    fn spans(&self) -> usize {
        match self {
            Curve::Spline { nodes, ends, .. } => {
                if *ends == crate::feature::Ends::Closed {
                    nodes.len()
                } else {
                    nodes.len().saturating_sub(1)
                }
            }
            _ => 1,
        }
    }

    /// The span of a spline place `t` lies in, and the share of that span's parameter. The end of the curve is the end
    /// of its last span: `n - 1e-15` rounds back to `n` past 8 spans, and a 4 096-span offset answered the start of its
    /// last span at `t = 1`, 0.3 mm short of its end.
    fn span_of(&self, t: f64) -> SpanAt {
        let n = self.spans().max(1);
        let x = t.clamp(0.0, 1.0) * n as f64;
        let k = (x.floor() as usize).min(n - 1);
        SpanAt { span: k, s: x - k as f64 }
    }

    /// THE HANDLES OF A SPLINE, the tangent each node is arrived at and left by, as a spline entity keeps them; `None`
    /// for a curve of another kind.
    pub fn handles(&self) -> Option<Vec<crate::model::Handle>> {
        let Curve::Spline { tangents, .. } = self else { return None };
        Some(tangents.iter().map(|t| crate::model::Handle { arrive: Some([t.arrive.x, t.arrive.y]), leave: Some([t.leave.x, t.leave.y]) }).collect())
    }

    /// The Hermite segment of span `k` of a spline.
    fn segment(&self, k: usize) -> Option<tess::HermiteSeg> {
        let Curve::Spline { nodes, tangents, .. } = self else { return None };
        let n = nodes.len();
        let (i, j) = (k % n, (k + 1) % n);
        Some(tess::HermiteSeg { p0: nodes[i], m0: tangents[i].leave, p1: nodes[j], m1: tangents[j].arrive })
    }

    /// THE POINT at place `t`.
    pub fn at(&self, t: f64) -> Point2 {
        match self {
            Curve::Line { a, b } => Point2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t),
            Curve::Round { centre, r, from, sweep } => {
                let w = from + sweep * t;
                Point2::new(centre.x + r * w.cos(), centre.y + r * w.sin())
            }
            Curve::Oval { centre, major, minor, from, sweep } => {
                let w = from + sweep * t;
                Point2::new(centre.x + major.x * w.cos() + minor.x * w.sin(), centre.y + major.y * w.cos() + minor.y * w.sin())
            }
            Curve::Spline { .. } => {
                let at = self.span_of(t);
                match self.segment(at.span) {
                    Some(g) => tess::hermite_pt(g.p0, g.m0, g.p1, g.m1, at.s),
                    None => Point2::new(f64::NAN, f64::NAN),
                }
            }
        }
    }

    /// THE DERIVATIVE dP/dt at place `t`: the way the curve runs there, as long as it runs per unit of its parameter.
    pub fn velocity(&self, t: f64) -> Point2 {
        match self {
            Curve::Line { a, b } => *b - *a,
            Curve::Round { r, from, sweep, .. } => {
                let w = from + sweep * t;
                Point2::new(-r * sweep * w.sin(), r * sweep * w.cos())
            }
            Curve::Oval { major, minor, from, sweep, .. } => {
                let w = from + sweep * t;
                Point2::new(sweep * (-major.x * w.sin() + minor.x * w.cos()), sweep * (-major.y * w.sin() + minor.y * w.cos()))
            }
            Curve::Spline { .. } => {
                let at = self.span_of(t);
                let Some(g) = self.segment(at.span) else { return Point2::new(0.0, 0.0) };
                mul(hermite_derivative(g, at.s), self.spans() as f64)
            }
        }
    }

    /// THE DIRECTION the curve runs at place `t`, a unit vector; along a degenerate curve, along X.
    pub fn tangent(&self, t: f64) -> Point2 {
        let v = self.velocity(t);
        let l = v.len();
        if l < 1e-15 {
            Point2::new(1.0, 0.0)
        } else {
            Point2::new(v.x / l, v.y / l)
        }
    }

    /// THE START AND THE END of the curve - one point twice for a closed curve.
    pub fn ends(&self) -> [Point2; 2] {
        [self.at(0.0), self.at(1.0)]
    }

    /// THE PLACES THE CURVE IS LAID OUT AT, from 0 to 1, no piece straying from it by more than `SAG`: a line its two
    /// ends; any other curve split until the chord of every piece lies within the sag at a quarter, a half and three
    /// quarters of it.
    pub fn places(&self) -> Vec<f64> {
        if let Curve::Line { .. } = self {
            return vec![0.0, 1.0];
        }
        let n = (self.spans() * 4).max(8);
        let mut out = vec![0.0];
        for k in 0..n {
            let (t0, t1) = (k as f64 / n as f64, (k + 1) as f64 / n as f64);
            self.refine(Piece { from: t0, to: t1 }, 16, &mut out);
        }
        out
    }

    /// The end of `piece` pushed, after the places that split it until it lies within the sag.
    fn refine(&self, piece: Piece, depth: u32, out: &mut Vec<f64>) {
        let (a, b) = (self.at(piece.from), self.at(piece.to));
        let off = [0.25, 0.5, 0.75]
            .iter()
            .map(|k| {
                let p = self.at(piece.from + (piece.to - piece.from) * k);
                let (dx, dy) = (b.x - a.x, b.y - a.y);
                let l = dx.hypot(dy);
                if l < 1e-15 {
                    p.dist(a)
                } else {
                    (dx * (p.y - a.y) - dy * (p.x - a.x)).abs() / l
                }
            })
            .fold(0.0, f64::max);
        if depth == 0 || off <= SAG {
            out.push(piece.to);
        } else {
            let mid = 0.5 * (piece.from + piece.to);
            self.refine(Piece { from: piece.from, to: mid }, depth - 1, out);
            self.refine(Piece { from: mid, to: piece.to }, depth - 1, out);
        }
    }

    /// THE POINTS THE CURVE IS DRAWN BY, at its places (`places`).
    pub fn polyline(&self) -> Vec<Point2> {
        self.places().into_iter().map(|t| self.at(t)).collect()
    }

    /// THE LENGTH of the curve along it: exact for a line and a round curve, along its laid out points otherwise.
    pub fn length(&self) -> f64 {
        match self {
            Curve::Line { a, b } => a.dist(*b),
            Curve::Round { r, sweep, .. } => r * sweep.abs(),
            _ => self.polyline().windows(2).map(|w| w[0].dist(w[1])).sum(),
        }
    }

    /// THE PLACE OF THE CURVE NEAREST TO `p`: exact for a line and a round curve; found among the laid out places and
    /// narrowed by Newton's steps on the derivative of the distance otherwise.
    pub fn nearest(&self, p: Point2) -> At {
        let t = match self {
            Curve::Line { a, b } => {
                let (dx, dy) = (b.x - a.x, b.y - a.y);
                let l2 = dx * dx + dy * dy;
                if l2 < 1e-30 {
                    0.0
                } else {
                    (((p.x - a.x) * dx + (p.y - a.y) * dy) / l2).clamp(0.0, 1.0)
                }
            }
            Curve::Round { centre, from, sweep, .. } => {
                let w = (p.y - centre.y).atan2(p.x - centre.x);
                let inside = ((w - from) * sweep.signum()).rem_euclid(TURN) / sweep.abs();
                if inside <= 1.0 || self.closed() {
                    inside.min(1.0)
                } else if p.dist(self.at(0.0)) <= p.dist(self.at(1.0)) {
                    0.0
                } else {
                    1.0
                }
            }
            _ => {
                let places = self.places();
                let best = places.iter().copied().min_by(|x, y| self.at(*x).dist(p).total_cmp(&self.at(*y).dist(p))).unwrap_or(0.0);
                self.polish(p, best)
            }
        };
        let point = self.at(t);
        At { t, point, off: point.dist(p) }
    }

    /// THE PIECE OF THE CURVE FROM PLACE `run[0]` TO PLACE `run[1]`, the same kind and the same shape: a line of a line, an arc
    /// of a circle or of an arc, an arc of an ellipse of an ellipse or of an arc of one, a spline of a spline. On a closed
    /// curve the second place may go past 1, round its start: the piece from 0.75 to 1.25 is the quarter on either side
    /// of its start.
    ///
    /// A piece of a spline keeps the nodes inside it and gains one at each end. A span cut at the share `s` of its own
    /// parameter is the same cubic over a shorter run of it, so each part of the span keeps its shape exactly once the
    /// tangents at its two ends are scaled by the share of the span it runs over - which is why a node may leave by a
    /// tangent other than the one it arrives by.
    pub fn piece(&self, run: [f64; 2]) -> Curve {
        let [from, to] = run;
        match self {
            Curve::Line { .. } => Curve::Line { a: self.at(from), b: self.at(to) },
            Curve::Round { centre, r, from: w, sweep } => Curve::Round { centre: *centre, r: *r, from: w + sweep * from, sweep: sweep * (to - from) },
            Curve::Oval { centre, major, minor, from: w, sweep } => Curve::Oval { centre: *centre, major: *major, minor: *minor, from: w + sweep * from, sweep: sweep * (to - from) },
            Curve::Spline { nodes, .. } => {
                let n = self.spans();
                let wrap = |t: f64| if self.closed() { t.rem_euclid(1.0 + 1e-15) } else { t.clamp(0.0, 1.0) };
                // the places of the piece's nodes, in units of spans of the whole spline: its two ends and every whole
                // node between them
                let (x0, x1) = (from * n as f64, to * n as f64);
                let mut cuts: Vec<f64> = vec![x0];
                cuts.extend((x0.floor() as i64 + 1..=x1.ceil() as i64 - 1).map(|k| k as f64).filter(|k| *k > x0 + 1e-12 && *k < x1 - 1e-12));
                cuts.push(x1);
                // dP/ds of the span a place lies in: of the span before it at a whole node when `before`, else after it
                let ds = |x: f64, before: bool| {
                    let k = (x.floor() as i64).rem_euclid(n as i64) as usize;
                    let mut sp = SpanAt { span: k, s: x - x.floor() };
                    if before && sp.s < 1e-12 {
                        sp = SpanAt { span: (sp.span + n - 1) % n, s: 1.0 };
                    }
                    hermite_derivative(self.segment(sp.span).expect("a span of the spline"), sp.s)
                };
                let last = cuts.len() - 1;
                let mut out_nodes = Vec::with_capacity(cuts.len());
                let mut out_tangents: Vec<NodeTangents> = Vec::with_capacity(cuts.len());
                for (i, x) in cuts.iter().enumerate() {
                    let at = self.at(wrap(x / n as f64));
                    out_nodes.push(nodes.iter().copied().find(|q| q.dist(at) < 1e-12).unwrap_or(at));
                    let arrive = (i > 0).then(|| mul(ds(*x, true), x - cuts[i - 1]));
                    let leave = (i < last).then(|| mul(ds(*x, false), cuts[i + 1] - x));
                    let (arrive, leave) = match (arrive, leave) {
                        (Some(a), Some(l)) => (a, l),
                        (Some(a), None) => (a, a),
                        (None, Some(l)) => (l, l),
                        (None, None) => (Point2::new(0.0, 0.0), Point2::new(0.0, 0.0)),
                    };
                    out_tangents.push(NodeTangents { arrive, leave });
                }
                Curve::Spline { nodes: out_nodes, tangents: out_tangents, ends: crate::feature::Ends::Open }
            }
        }
    }

    /// The place `t` near `start` where the curve is nearest to `p`, by Newton's steps on the derivative of the squared
    /// distance, held within the curve - on a closed curve going on round past its start: a point 0.001 of the way
    /// before the start of a closed spline, held at 0 by the end of the curve, was answered 0.025 mm off it.
    pub(crate) fn polish(&self, p: Point2, start: f64) -> f64 {
        let closed = self.closed();
        let hold = |t: f64| if closed { t.rem_euclid(1.0) } else { t.clamp(0.0, 1.0) };
        let mut t = start;
        let h = 1e-6;
        for _ in 0..30 {
            let f = |t: f64| {
                let (q, v) = (self.at(hold(t)), self.velocity(hold(t)));
                (q.x - p.x) * v.x + (q.y - p.y) * v.y
            };
            let (lo, hi) = if closed { (t - h, t + h) } else { ((t - h).max(0.0), (t + h).min(1.0)) };
            let (g, dg) = (f(t), (f(hi) - f(lo)) / (hi - lo));
            if dg.abs() < 1e-18 {
                break;
            }
            let next = hold(t - g / dg);
            if (next - t).abs() < 1e-14 {
                t = next;
                break;
            }
            t = next;
        }
        t
    }
}

/// The derivative dP/ds of a Hermite segment at the share `s` of its parameter.
fn hermite_derivative(g: tess::HermiteSeg, s: f64) -> Point2 {
    let (d00, d10, d01, d11) = (6.0 * s * s - 6.0 * s, 3.0 * s * s - 4.0 * s + 1.0, -6.0 * s * s + 6.0 * s, 3.0 * s * s - 2.0 * s);
    Point2::new(d00 * g.p0.x + d10 * g.m0.x + d01 * g.p1.x + d11 * g.m1.x, d00 * g.p0.y + d10 * g.m0.y + d01 * g.p1.y + d11 * g.m1.y)
}

/// A vector `v` stretched by `k`.
fn mul(v: Point2, k: f64) -> Point2 {
    Point2::new(v.x * k, v.y * k)
}

/// A PIECE OF A CURVE between two of its places.
#[derive(Clone, Copy)]
struct Piece {
    from: f64,
    to: f64,
}

/// A PLACE ON A SPLINE: its span, and the share of that span's own parameter.
#[derive(Clone, Copy)]
struct SpanAt {
    span: usize,
    s: f64,
}
