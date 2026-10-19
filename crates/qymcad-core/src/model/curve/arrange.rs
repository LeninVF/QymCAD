//! HOW A CURVE ENTERS THE ARRANGEMENT OF A SKETCH, out of which its regions are made (`model::tess`): a line, a circle and
//! an arc as the curves they are; an ellipse, an arc of one and a spline as the segments of their points, so they are cut
//! by every other curve and by themselves, and a region closed partly by them, or by a spline crossing itself, is a face
//! like any other. The runs of segments along one curve go back to the curve they are as the regions are made
//! (`tess::exact_runs`). A whole ellipse was a region of its own, outside the graph: no line, spline or arc crossing it
//! cut it.
use super::*;
use crate::model::tess::ArrCurve;

impl Curve {
    /// THE CURVE AS THE ARRANGEMENT TAKES IT: the pieces of its graph. A line or an arc of no size brings nothing.
    pub(in crate::model) fn arranged(&self) -> Vec<ArrCurve> {
        let xy = |p: Point2| (p.x, p.y);
        match *self {
            Curve::Line { a, b } => {
                if a.dist(b) > 1e-9 {
                    vec![ArrCurve::Line { a: xy(a), b: xy(b) }]
                } else {
                    Vec::new()
                }
            }
            Curve::Round { centre, r, .. } if self.closed() => vec![ArrCurve::Circle { c: xy(centre), r: r.max(1e-6) }],
            Curve::Round { centre, r, sweep, .. } => {
                if r <= 1e-9 {
                    return Vec::new();
                }
                let [pa, pb] = self.ends();
                let angle = |p: Point2| (p.y - centre.y).atan2(p.x - centre.x);
                vec![ArrCurve::Arc { c: xy(centre), r, a0: angle(pa), a1: angle(pb), ccw: sweep >= 0.0, pa: xy(pa), pb: xy(pb) }]
            }
            _ => self.polyline().windows(2).filter(|w| w[0].dist(w[1]) > 1e-9).map(|w| ArrCurve::Line { a: xy(w[0]), b: xy(w[1]) }).collect(),
        }
    }
}
