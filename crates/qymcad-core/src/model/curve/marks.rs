//! THE MARKED PLACES OF A CURVE a person snaps to: its quadrants, and the point half way along it.
use super::*;

impl Curve {
    /// THE QUADRANTS OF THE CURVE that lie on it: the four points of a circle or an arc at 0, 90, 180 and 270 deg, the
    /// four ends of the axes of an ellipse or an arc of one; none for a line or a spline.
    pub fn quadrants(&self) -> Vec<Point2> {
        let candidates = match self {
            Curve::Round { centre, r, .. } => [[1.0, 0.0], [0.0, 1.0], [-1.0, 0.0], [0.0, -1.0]].map(|[x, y]| Point2::new(centre.x + r * x, centre.y + r * y)).to_vec(),
            Curve::Oval { centre, major, minor, .. } => [*major, *minor].into_iter().flat_map(|v| [*centre + v, *centre - v]).collect(),
            Curve::Line { .. } | Curve::Spline { .. } => Vec::new(),
        };
        candidates.into_iter().filter(|q| self.nearest(*q).off < 1e-9).collect()
    }

    /// THE POINT HALF WAY ALONG THE CURVE by its length: on an arc the middle of its sweep, on any other the place its
    /// laid out points put half way, taken onto the curve.
    pub fn half_way(&self) -> Point2 {
        if let Curve::Round { .. } = self {
            return self.at(0.5);
        }
        let line = self.polyline();
        let total: f64 = line.windows(2).map(|w| w[0].dist(w[1])).sum();
        let mut left = total / 2.0;
        for w in line.windows(2) {
            let l = w[0].dist(w[1]);
            if l >= left && l > 0.0 {
                let k = left / l;
                return self.nearest(Point2::new(w[0].x + (w[1].x - w[0].x) * k, w[0].y + (w[1].y - w[0].y) * k)).point;
            }
            left -= l;
        }
        self.at(0.5)
    }

    /// THE PLACES ON THE STRAIGHT THROUGH `a` AND `b` WHERE IT MEETS THE CURVE, as shares of the way from `a` (0) to `b`
    /// (1), past them as well: a line within its own ends, a circle or an ellipse on the stretch of it the curve runs
    /// over. A spline offers none.
    pub fn meets_line(&self, [a, b]: [Point2; 2]) -> Vec<f64> {
        let (p, q) = ([a.x, a.y], [b.x, b.y]);
        let ts: Vec<f64> = match self {
            Curve::Line { a: c, b: d } => line_seg_t(p, q, [c.x, c.y], [d.x, d.y]).into_iter().collect(),
            Curve::Round { centre, r, .. } => line_circle_t(a.x, a.y, b.x, b.y, centre.x, centre.y, *r),
            Curve::Oval { centre, major, minor, .. } => {
                let (ma, mi) = (*centre + *major, *centre + *minor);
                let (ux, uy, big, small) = ellipse_axes(centre.x, centre.y, ma.x, ma.y, mi.x, mi.y);
                line_ellipse_roots(p, q, [centre.x, centre.y], [ux, uy], big, small)
            }
            Curve::Spline { .. } => Vec::new(),
        };
        // a place on the circle or the ellipse past the ends of an arc of it is no place the curve is met
        ts.into_iter().filter(|t| self.straight() || self.nearest(Point2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)).off < 1e-6).collect()
    }

    /// Whether the curve is a straight line.
    pub fn straight(&self) -> bool {
        matches!(self, Curve::Line { .. })
    }
}
