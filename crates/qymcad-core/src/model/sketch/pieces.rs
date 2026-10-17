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
    /// THE POINT OF SKETCH `si` STANDING AT `at` to 1e-9 mm, or a new one there - never a point of the frame of reference
    /// (`sketch_point_at`).
    pub(crate) fn point_exactly_at(&mut self, si: usize, at: Point2) -> Id {
        self.sketch_point_at(si, at.x, at.y, 1e-9)
    }

    /// LAY CURVE `c` INTO SKETCH `si` as an entity of its own kind, its ends on the points standing there (or new
    /// ones), its centre and axes on those of `how`. Answers the id of the entity.
    pub fn lay_curve(&mut self, si: usize, c: &Curve, how: LayAs) -> Id {
        let kind = self.curve_kind(si, c, how);
        let id = self.alloc_id();
        self.sketches[si].entities.push(SketchEntity { id, kind, construction: how.construction });
        id
    }

    /// THE ENTITY KIND OF CURVE `c` in sketch `si`, on the points standing at its ends (new ones where none stands) and
    /// the centre and axes of `how`.
    pub(crate) fn curve_kind(&mut self, si: usize, c: &Curve, how: LayAs) -> EntityKind {
        EntityKind::laid(c, crate::model::Standing { centre: how.centre, axes: how.axes }, &mut |p| self.point_exactly_at(si, p))
    }

    /// How a piece of entity `e` is laid: as construction when it is, on its centre and the ends of its axes.
    pub(crate) fn lay_as_of(e: &SketchEntity) -> LayAs {
        let on = e.kind.standing();
        LayAs { construction: e.construction, centre: on.centre, axes: on.axes }
    }

    /// CUT ENTITY `eid` OF SKETCH `si` AT `places` along it (`Curve` places, 0 to 1) and put its pieces in its stead: an
    /// open curve into the pieces between its ends and the places, a closed one into the pieces between the places round
    /// it - one piece, opened at the place, for a single place. Answers the ids of the pieces in order along the curve;
    /// none when there is no such entity or no place to cut at.
    pub fn cut_curve(&mut self, si: usize, eid: Id, places: &[f64]) -> Vec<Id> {
        let Some(s) = self.sketches.get(si) else { return Vec::new() };
        let Some(e) = s.entities.iter().find(|e| e.id == eid) else { return Vec::new() };
        let Some(c) = Curve::of(s, e) else { return Vec::new() };
        let how = Self::lay_as_of(e);
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
        let held = self.where_refs_hold(si, eid);
        self.sketches[si].entities.retain(|x| x.id != eid);
        let pieces: Vec<Id> = runs.iter().map(|[a, b]| self.lay_curve(si, &c.piece([*a, *b]), how)).collect();
        self.refs_go_to_pieces(si, eid, &held, &pieces);
        self.break_figures_of(si, &[]);
        self.regen_sketch(si);
        pieces
    }
}
