//! A CURVE AS THE EDGES OF A PROFILE, AND BACK: what the kernel builds a face from, exact - a line, an arc, a circle, an
//! ellipse or an arc of one, and every span of a spline as a cubic by its Bezier points - and what an edge read from a
//! file is laid into a sketch as.
use super::*;
use crate::geom::ProfEdge;

impl Curve {
    /// THE EXACT EDGES OF THE CURVE, in its order: a span of a spline from `p0` with the tangent `m0` into `p1` with `m1`
    /// is the cubic `p0, p0 + m0 / 3, p1 - m1 / 3, p1`.
    pub fn to_edges(&self) -> Vec<ProfEdge> {
        match self {
            Curve::Line { a, b } => vec![ProfEdge::Line { a: *a, b: *b }],
            Curve::Round { centre, r, .. } if self.closed() => vec![ProfEdge::Circle { center: *centre, r: *r }],
            Curve::Round { centre, sweep, .. } => {
                let [a, b] = self.ends();
                vec![ProfEdge::Arc { a, b, center: *centre, ccw: *sweep >= 0.0 }]
            }
            Curve::Oval { centre, major, minor, from, sweep } => vec![ProfEdge::Oval { centre: *centre, major: *major, minor: *minor, from: *from, sweep: *sweep }],
            Curve::Spline { .. } => {
                (0..self.spans()).filter_map(|k| self.segment(k)).map(|g| ProfEdge::Cubic { a: g.p0, c1: g.p0 + g.m0 * (1.0 / 3.0), c2: g.p1 - g.m1 * (1.0 / 3.0), b: g.p1 }).collect()
            }
        }
    }

    /// THE SPLINE OF CUBIC SPANS joined end to end, a node where each meets the next, each node left and arrived at by the
    /// tangents its spans give - three times the pull of their Bezier points; a loop when the last span ends where the
    /// first begins. `None` for spans that are not cubics joined end to end.
    pub fn of_spans(spans: &[ProfEdge]) -> Option<Curve> {
        let cubics: Vec<[Point2; 4]> = spans
            .iter()
            .map(|e| match *e {
                ProfEdge::Cubic { a, c1, c2, b } => Some([a, c1, c2, b]),
                _ => None,
            })
            .collect::<Option<_>>()?;
        let (first, last) = (cubics.first()?, cubics.last()?);
        if cubics.windows(2).any(|w| w[0][3].dist(w[1][0]) > 1e-9) {
            return None;
        }
        let leave = |s: &[Point2; 4]| (s[1] - s[0]) * 3.0;
        let arrive = |s: &[Point2; 4]| (s[3] - s[2]) * 3.0;
        let closed = cubics.len() > 1 && last[3].dist(first[0]) < 1e-9;
        let mut nodes: Vec<Point2> = cubics.iter().map(|s| s[0]).collect();
        let mut tangents: Vec<NodeTangents> = cubics.iter().enumerate().map(|(k, s)| NodeTangents { arrive: if k == 0 { leave(s) } else { arrive(&cubics[k - 1]) }, leave: leave(s) }).collect();
        if closed {
            tangents[0].arrive = arrive(last);
        } else {
            nodes.push(last[3]);
            tangents.push(NodeTangents { arrive: arrive(last), leave: arrive(last) });
        }
        Some(Curve::Spline { nodes, tangents, ends: if closed { crate::feature::Ends::Closed } else { crate::feature::Ends::Open } })
    }

    /// THE CURVE OF AN EDGE: a cubic is a spline of one span, its tangents three times the pull of its Bezier points.
    pub fn of_edge(e: &ProfEdge) -> Curve {
        let turn = std::f64::consts::TAU;
        match *e {
            ProfEdge::Line { a, b } => Curve::Line { a, b },
            ProfEdge::Arc { a, b, center, ccw } => {
                let (wa, wb) = ((a.y - center.y).atan2(a.x - center.x), (b.y - center.y).atan2(b.x - center.x));
                let sweep = if ccw { (wb - wa).rem_euclid(turn) } else { -(wa - wb).rem_euclid(turn) };
                Curve::Round { centre: center, r: a.dist(center), from: wa, sweep }
            }
            ProfEdge::Circle { center, r } => Curve::Round { centre: center, r, from: 0.0, sweep: turn },
            ProfEdge::Oval { centre, major, minor, from, sweep } => Curve::Oval { centre, major, minor, from, sweep },
            ProfEdge::Cubic { a, c1, c2, b } => {
                let (leave, arrive) = ((c1 - a) * 3.0, (b - c2) * 3.0);
                Curve::Spline { nodes: vec![a, b], tangents: vec![NodeTangents { arrive: leave, leave }, NodeTangents { arrive, leave: arrive }], ends: crate::feature::Ends::Open }
            }
        }
    }
}
