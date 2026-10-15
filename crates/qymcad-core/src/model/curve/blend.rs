//! THE CUT OF A CORNER TWO CURVES OF ANY KIND MAKE: where a fillet of a radius touches them, and where a chamfer of its
//! legs cuts them. A corner is two curves leaving one point, each from one of its ends.
//!
//! The centre of a fillet stands at the radius from both curves: it is where their offsets by the radius meet, on the
//! sides that face each other, narrowed by Newton's steps on the two places it touches - the offset of a spline is itself
//! laid within `OFFSET_TOLERANCE`, which would leave the arc 0.01 mm off the curve. A leg of a chamfer is measured
//! straight from the corner, as on a line.
use super::*;

/// ONE SIDE OF A CORNER: a curve and the end of it standing on the corner.
#[derive(Clone, Copy, Debug)]
pub struct Arm<'a> {
    pub curve: &'a Curve,
    pub end: End,
}

/// A FILLET OF A CORNER: the centre of its arc and the places it touches the two curves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rounding {
    pub centre: Point2,
    pub at: [f64; 2],
}

/// How many points a curve is looked along from its end for the place a leg reaches.
const LOOKS: usize = 400;

impl Arm<'_> {
    /// The place of the end on the corner.
    pub fn place(&self) -> f64 {
        match self.end {
            End::Start => 0.0,
            End::Finish => 1.0,
        }
    }

    /// The corner point.
    pub fn corner(&self) -> Point2 {
        self.curve.at(self.place())
    }

    /// THE WAY THE CURVE LEAVES THE CORNER, a unit vector into it.
    pub fn leaving(&self) -> Point2 {
        let u = self.curve.tangent(self.place());
        match self.end {
            End::Start => u,
            End::Finish => mul(u, -1.0),
        }
    }

    /// THE PLACE `d` STRAIGHT FROM THE CORNER, the first one looking along the curve from it; `None` where the curve does
    /// not reach that far.
    pub fn reach(&self, d: f64) -> Option<f64> {
        let corner = self.corner();
        let from = self.place();
        let at = |k: usize| from + (1.0 - 2.0 * from) * k as f64 / LOOKS as f64;
        let k = (1..=LOOKS).find(|k| self.curve.at(at(*k)).dist(corner) >= d)?;
        let (mut lo, mut hi) = (at(k - 1), at(k));
        for _ in 0..60 {
            let mid = (lo + hi) / 2.0;
            if self.curve.at(mid).dist(corner) >= d {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        Some(hi)
    }

    /// Whether place `t` lies inside the curve and off its corner end.
    fn holds(&self, t: f64) -> bool {
        (-1e-9..=1.0 + 1e-9).contains(&t) && (t - self.place()).abs() > 1e-9
    }
}

/// The left normal of a unit direction.
fn left(u: Point2) -> Point2 {
    Point2::new(-u.y, u.x)
}

/// THE FILLET OF RADIUS `r` IN THE CORNER the two arms make: of the centres at the radius from both, the one touching
/// both curves nearest the corner. `None` where the radius does not fit.
pub fn rounding(arms: [Arm; 2], r: f64) -> Option<Rounding> {
    if r <= 1e-9 {
        return None;
    }
    let corner = arms[0].corner();
    let mut best: Option<Rounding> = None;
    for s0 in [-1.0, 1.0] {
        for s1 in [-1.0, 1.0] {
            let sides = [s0 * r, s1 * r];
            let starts: Vec<Point2> = arms[0].curve.offset(sides[0]).iter().flat_map(|a| arms[1].curve.offset(sides[1]).into_iter().flat_map(move |b| a.meet(&b)).map(|m| m.at)).collect();
            for start in starts {
                let Some(found) = settle_centre(arms, sides, start) else { continue };
                if !(arms[0].holds(found.at[0]) && arms[1].holds(found.at[1])) {
                    continue;
                }
                let nearer = best.is_none_or(|b| found.centre.dist(corner) < b.centre.dist(corner) - 1e-9);
                if nearer {
                    best = Some(found);
                }
            }
        }
    }
    best
}

/// The centre at the signed distances `sides` to the left of both curves, by Newton's steps on the two places it touches
/// from the places nearest `start`.
fn settle_centre(arms: [Arm; 2], sides: [f64; 2], start: Point2) -> Option<Rounding> {
    let off = |k: usize, t: f64| {
        let c = arms[k].curve;
        let t = t.clamp(0.0, 1.0);
        let n = left(c.tangent(t));
        c.at(t) + mul(n, sides[k])
    };
    let mut t = [arms[0].curve.nearest(start).t, arms[1].curve.nearest(start).t];
    let h = 1e-7;
    for _ in 0..50 {
        let f = off(0, t[0]) - off(1, t[1]);
        if f.x.hypot(f.y) < 1e-11 {
            break;
        }
        let d0 = mul(off(0, t[0] + h) - off(0, t[0] - h), 0.5 / h);
        let d1 = mul(off(1, t[1] + h) - off(1, t[1] - h), -0.5 / h);
        let det = d0.x * d1.y - d0.y * d1.x;
        if det.abs() < 1e-18 {
            return None;
        }
        t[0] -= (f.x * d1.y - f.y * d1.x) / det;
        t[1] -= (d0.x * f.y - d0.y * f.x) / det;
        if !t.iter().all(|x| x.is_finite()) {
            return None;
        }
    }
    let gap = off(0, t[0]) - off(1, t[1]);
    (gap.x.hypot(gap.y) < 1e-8 && t.iter().all(|x| (-1e-9..=1.0 + 1e-9).contains(x))).then(|| Rounding { centre: off(0, t[0]), at: t.map(|x| x.clamp(0.0, 1.0)) })
}

/// THE PLACES A CHAMFER OF `legs` CUTS THE TWO ARMS AT: a symmetric one as far from the corner on both, its cut as long
/// as asked; two legs each straight from the corner; a leg on the first and the cut at the angle from the first's way to
/// the corner. `None` where a curve does not reach as far.
pub fn chamfer(arms: [Arm; 2], legs: ChamferLegs) -> Option<[f64; 2]> {
    let corner = arms[0].corner();
    match legs.mode {
        crate::feature::ChamferMode::TwoDist => Some([arms[0].reach(legs.first)?, arms[1].reach(legs.second)?]),
        crate::feature::ChamferMode::Symmetric => {
            // the cut grows with the legs near the corner: the leg whose cut is as long as asked, by halving
            let cut = |d: f64| -> Option<f64> { Some(arms[0].curve.at(arms[0].reach(d)?).dist(arms[1].curve.at(arms[1].reach(d)?))) };
            let (mut lo, mut hi) = (0.0, legs.first);
            while cut(hi)? < legs.first {
                lo = hi;
                hi *= 2.0;
            }
            for _ in 0..60 {
                let mid = (lo + hi) / 2.0;
                if cut(mid)? >= legs.first {
                    hi = mid;
                } else {
                    lo = mid;
                }
            }
            Some([arms[0].reach(hi)?, arms[1].reach(hi)?])
        }
        crate::feature::ChamferMode::DistAngle => {
            let t0 = arms[0].reach(legs.first)?;
            let p0 = arms[0].curve.at(t0);
            let back = corner - p0;
            let want = legs.second.to_radians();
            // the angle at the end on the first curve between its way to the corner and the cut, growing with the leg
            // along the second
            let angle = |d: f64| -> Option<f64> {
                let p1 = arms[1].curve.at(arms[1].reach(d)?) - p0;
                Some(((back.x * p1.x + back.y * p1.y) / (back.x.hypot(back.y) * p1.x.hypot(p1.y)).max(1e-18)).clamp(-1.0, 1.0).acos())
            };
            let (mut lo, mut hi) = (0.0, legs.first);
            while angle(hi)? < want {
                lo = hi;
                hi *= 2.0;
            }
            for _ in 0..60 {
                let mid = (lo + hi) / 2.0;
                if angle(mid)? >= want {
                    hi = mid;
                } else {
                    lo = mid;
                }
            }
            Some([t0, arms[1].reach(hi)?])
        }
    }
}
