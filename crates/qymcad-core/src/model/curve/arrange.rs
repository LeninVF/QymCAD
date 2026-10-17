//! HOW A CURVE ENTERS THE ARRANGEMENT OF A SKETCH, out of which its regions are made (`model::tess`): a line, a circle and
//! an arc as the curves they are; a whole ellipse as a region of its own, outside the graph of the arrangement; a spline
//! and an arc of an ellipse as the segments of their points, so they are cut by the lines and arcs and by themselves, and
//! a region closed partly by them, or by a spline crossing itself, is a face like any other.
use super::*;
use crate::model::tess::{ellipse_contour, ArrCurve};

/// WHAT A CURVE BRINGS TO THE ARRANGEMENT: pieces of the graph, or a region of its own.
pub(in crate::model) enum Arranged {
    Pieces(Vec<ArrCurve>),
    Region(crate::geom::Contour),
}

impl Curve {
    /// THE CURVE AS THE ARRANGEMENT TAKES IT. A line or an arc of no size brings nothing.
    pub(in crate::model) fn arranged(&self) -> Arranged {
        let xy = |p: Point2| (p.x, p.y);
        match *self {
            Curve::Line { a, b } => Arranged::Pieces(if a.dist(b) > 1e-9 { vec![ArrCurve::Line { a: xy(a), b: xy(b) }] } else { Vec::new() }),
            Curve::Round { centre, r, .. } if self.closed() => Arranged::Pieces(vec![ArrCurve::Circle { c: xy(centre), r: r.max(1e-6) }]),
            Curve::Round { centre, r, sweep, .. } => {
                if r <= 1e-9 {
                    return Arranged::Pieces(Vec::new());
                }
                let [pa, pb] = self.ends();
                let angle = |p: Point2| (p.y - centre.y).atan2(p.x - centre.x);
                Arranged::Pieces(vec![ArrCurve::Arc { c: xy(centre), r, a0: angle(pa), a1: angle(pb), ccw: sweep >= 0.0, pa: xy(pa), pb: xy(pb) }])
            }
            Curve::Oval { centre, major, minor, .. } if self.closed() => Arranged::Region(ellipse_contour(centre, centre + major, centre + minor)),
            _ => Arranged::Pieces(self.polyline().windows(2).filter(|w| w[0].dist(w[1]) > 1e-9).map(|w| ArrCurve::Line { a: xy(w[0]), b: xy(w[1]) }).collect()),
        }
    }
}
