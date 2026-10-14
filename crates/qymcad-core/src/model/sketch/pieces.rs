//! A CURVE OF A SKETCH CUT INTO PIECES: a piece of a curve (`Curve::piece`) laid back as an entity of its own kind, and
//! an entity replaced by its pieces between given places - a line by lines, a circle or an arc by arcs of the same
//! centre, an ellipse or an arc of one by arcs of the same ellipse, a spline by splines of the same shape.

use super::*;
use crate::model::curve::Curve;

/// HOW A PIECE IS LAID: as construction geometry or not, and the points of the curve it was cut from that it stands on
/// as well - the centre of a round curve or of an ellipse, the ends of the axes of an ellipse.
#[derive(Clone, Copy, Debug, Default)]
pub struct LayAs {
    pub construction: bool,
    pub centre: Option<Id>,
    pub axes: Option<[Id; 2]>,
}

impl Project {
    /// THE POINT OF SKETCH `si` STANDING AT `at` to 1e-9 mm, or a new one there.
    fn point_exactly_at(&mut self, si: usize, at: Point2) -> Id {
        if let Some(q) = self.sketches[si].points.iter().find(|q| (q.x - at.x).hypot(q.y - at.y) < 1e-9) {
            return q.id;
        }
        let id = self.alloc_id();
        self.sketches[si].points.push(SketchPoint { id, x: at.x, y: at.y });
        id
    }

    /// LAY CURVE `c` INTO SKETCH `si` as an entity of its own kind, its ends on the points standing there (or new
    /// ones), its centre and axes on those of `how`. Answers the id of the entity.
    pub fn lay_curve(&mut self, si: usize, c: &Curve, how: LayAs) -> Id {
        let kind = match c {
            Curve::Line { a, b } => EntityKind::Line { a: self.point_exactly_at(si, *a), b: self.point_exactly_at(si, *b) },
            Curve::Round { centre, r, sweep, .. } => {
                let centre_id = how.centre.unwrap_or_else(|| self.point_exactly_at(si, *centre));
                if c.closed() {
                    EntityKind::Circle { center: centre_id, r: *r }
                } else {
                    let [a, b] = c.ends();
                    let (pa, pb) = (self.point_exactly_at(si, a), self.point_exactly_at(si, b));
                    // an arc of the sketch runs counter-clockwise from its first end; one cut clockwise is laid the other way
                    if *sweep >= 0.0 {
                        EntityKind::Arc { center: centre_id, a: pa, b: pb, ccw: true }
                    } else {
                        EntityKind::Arc { center: centre_id, a: pb, b: pa, ccw: true }
                    }
                }
            }
            Curve::Oval { centre, major, minor, sweep, .. } => {
                let cid = how.centre.unwrap_or_else(|| self.point_exactly_at(si, *centre));
                let [ma, mi] = how.axes.unwrap_or_else(|| [self.point_exactly_at(si, *centre + *major), self.point_exactly_at(si, *centre + *minor)]);
                let span = (!c.closed()).then(|| {
                    let [a, b] = c.ends();
                    let (from, to) = (self.point_exactly_at(si, a), self.point_exactly_at(si, b));
                    let winding = if *sweep >= 0.0 { crate::feature::Winding::Ccw } else { crate::feature::Winding::Cw };
                    EllipseSpan { from, to, winding }
                });
                EntityKind::Ellipse { c: cid, ma, mi, span }
            }
            Curve::Spline { nodes, tangents, ends } => {
                let ids: Vec<Id> = nodes.iter().map(|q| self.point_exactly_at(si, *q)).collect();
                let handles = tangents.iter().map(|t| Handle { arrive: Some([t.arrive.x, t.arrive.y]), leave: Some([t.leave.x, t.leave.y]) }).collect();
                EntityKind::Spline { nodes: ids, handles, ends: *ends }
            }
        };
        let id = self.alloc_id();
        self.sketches[si].entities.push(SketchEntity { id, kind, construction: how.construction });
        id
    }

    /// CUT ENTITY `eid` OF SKETCH `si` AT `places` along it (`Curve` places, 0 to 1) and put its pieces in its stead: an
    /// open curve into the pieces between its ends and the places, a closed one into the pieces between the places round
    /// it - one piece, opened at the place, for a single place. Answers the ids of the pieces in order along the curve;
    /// none when there is no such entity or no place to cut at.
    pub fn cut_curve(&mut self, si: usize, eid: Id, places: &[f64]) -> Vec<Id> {
        let Some(s) = self.sketches.get(si) else { return Vec::new() };
        let Some(e) = s.entities.iter().find(|e| e.id == eid) else { return Vec::new() };
        let Some(c) = Curve::of(s, e) else { return Vec::new() };
        let how = match &e.kind {
            EntityKind::Arc { center, .. } | EntityKind::Circle { center, .. } => LayAs { construction: e.construction, centre: Some(*center), axes: None },
            EntityKind::Ellipse { c, ma, mi, .. } => LayAs { construction: e.construction, centre: Some(*c), axes: Some([*ma, *mi]) },
            _ => LayAs { construction: e.construction, ..LayAs::default() },
        };
        let mut at: Vec<f64> = places.iter().copied().filter(|t| (0.0..=1.0).contains(t)).collect();
        at.sort_by(f64::total_cmp);
        at.dedup_by(|x, y| (*x - *y).abs() < 1e-12);
        let runs: Vec<[f64; 2]> = if c.closed() {
            if at.is_empty() {
                return Vec::new();
            }
            (0..at.len()).map(|k| [at[k], if k + 1 < at.len() { at[k + 1] } else { at[0] + 1.0 }]).collect()
        } else {
            at.retain(|t| *t > 1e-12 && *t < 1.0 - 1e-12);
            if at.is_empty() {
                return Vec::new();
            }
            std::iter::once(0.0).chain(at.iter().copied()).zip(at.iter().copied().chain(std::iter::once(1.0))).map(|(a, b)| [a, b]).collect()
        };
        self.sketches[si].entities.retain(|x| x.id != eid);
        let pieces: Vec<Id> = runs.iter().map(|[a, b]| self.lay_curve(si, &c.piece([*a, *b]), how)).collect();
        self.break_figures_of(si, &[]);
        self.regen_sketch(si);
        pieces
    }
}
