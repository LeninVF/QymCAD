//! TWO CURVES OF A SKETCH MADE TO TOUCH: the constraint of touching, and where the touching would come past an end of an
//! open curve - a line, a spline - that end put on the other curve, so the two touch on themselves and not on the
//! straight one of them goes on along.
use super::*;

impl Project {
    /// THE CONSTRAINTS THAT MAKE CURVES `one` AND `other` OF SKETCH `si` TOUCH: the touching itself, with the place it
    /// stands at now, and an end of either put on the other where the place is past that end. Empty where either is no
    /// curve of the sketch.
    pub fn tangency(&self, si: usize, [one, other]: [Id; 2]) -> Vec<Constraint> {
        let Some(s) = self.sketches.get(si) else { return Vec::new() };
        let (Some(e1), Some(e2)) = (s.entity(one), s.entity(other)) else { return Vec::new() };
        let (r1, r2) = (curve_ref_of(e1), curve_ref_of(e2));
        let near = crate::solver::touching_places(&s.points, &self.entity_radii(si), [&r1, &r2], None);
        // the end the touching comes past, of the first of the two it comes past: one end held on the other curve is the
        // whole of a touching at an end, and both held were one more than the two can meet
        let past = |e: &SketchEntity, place: f64| {
            let [start, finish] = entity_ends(e)?;
            if place < 0.0 {
                Some(start)
            } else if place > 1.0 {
                Some(finish)
            } else {
                None
            }
        };
        let end = near.and_then(|[t1, t2]| past(e1, t1).map(|p| Constraint::PointOnCurve { p, on: r2.clone() }).or_else(|| past(e2, t2).map(|p| Constraint::PointOnCurve { p, on: r1.clone() })));
        let end_id = match &end {
            Some(Constraint::PointOnCurve { p, .. }) => Some(*p),
            _ => None,
        };
        std::iter::once(Constraint::TangentCurves { one: r1, other: r2, near, end: end_id }).chain(end).collect()
    }

    /// WHERE DIMENSION `c` OF SKETCH `si`, A DISTANCE TO A CURVE, IS MEASURED: its two ends, the first on what it is
    /// measured from. `None` for any other constraint.
    pub fn curve_distance_ends(&self, si: usize, c: &Constraint) -> Option<[Point2; 2]> {
        let s = self.sketches.get(si)?;
        let Constraint::CurveDistance { from, to, .. } = c else { return None };
        crate::solver::distance_ends(&s.points, &self.entity_radii(si), [from, to])
    }

    /// EVERY TOUCHING OF SKETCH `si` THAT HAS COME PAST AN END of a line or a spline, held at that end: the end put on the
    /// other curve and the touching read at it. Answers whether any was. A solve moves the curves and the place they touch
    /// with them, and a line touching a spline at 0.83 of its length was carried 1.1 mm past its end.
    pub(crate) fn touchings_held_at_ends(&mut self, si: usize) -> bool {
        let radii = self.entity_radii(si);
        let Some(s) = self.sketches.get_mut(si) else { return false };
        let mut held: Vec<Constraint> = Vec::new();
        for c in s.constraints.iter_mut() {
            let Constraint::TangentCurves { one, other, near, end: end @ None } = c else { continue };
            let Some([t1, t2]) = crate::solver::touching_places(&s.points, &radii, [one, other], *near) else { continue };
            let past = |on: &CurveRef, t: f64| {
                let [start, finish] = open_ends(on)?;
                if t < -1e-6 {
                    Some(start)
                } else if t > 1.0 + 1e-6 {
                    Some(finish)
                } else {
                    None
                }
            };
            if let Some(p) = past(one, t1) {
                held.push(Constraint::PointOnCurve { p, on: other.clone() });
                *end = Some(p);
            } else if let Some(p) = past(other, t2) {
                held.push(Constraint::PointOnCurve { p, on: one.clone() });
                *end = Some(p);
            }
        }
        let any = !held.is_empty();
        s.constraints.extend(held);
        any
    }
}

/// The two ends of an open curve of a constraint: a line's, an open spline's first and last nodes.
fn open_ends(on: &CurveRef) -> Option<[Id; 2]> {
    match on {
        CurveRef::Line { a, b } | CurveRef::Arc { a, b, .. } => Some([*a, *b]),
        CurveRef::Spline { nodes, ends: crate::feature::Ends::Open, .. } => Some([*nodes.first()?, *nodes.last()?]),
        _ => None,
    }
}
