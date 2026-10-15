//! A CORNER OF CURVES OF ANY KIND ROUNDED OR CHAMFERED: the corner a spline or an arc of an ellipse makes with any curve.
//! A corner of two lines, two arcs or a line and an arc keeps its own path in `corners.rs`, with the shape of the corner
//! held by tangencies and its sharp held on both lines; here the geometry comes from `curve::rounding` and
//! `curve::chamfer`, each side is cut back to where the cut meets it and laid again as a piece of its own kind, and the
//! side that is a line or an arc is held to the fillet as there.
use super::*;
use crate::model::curve::{chamfer, rounding, Arm, Curve, End};

/// A SIDE OF A CORNER IN A SKETCH: the entity, its curve, the end of it on the corner, and the point at its other end.
struct Leg {
    eid: Id,
    curve: Curve,
    end: End,
    far: Id,
}

impl Leg {
    fn arm(&self) -> Arm<'_> {
        Arm { curve: &self.curve, end: self.end }
    }
}

/// AN EDGE AT A CORNER: the curve and the point of the corner it ends at.
#[derive(Clone, Copy, Debug)]
pub(crate) struct EdgeAt {
    pub(crate) edge: Id,
    pub(crate) corner: Id,
}

/// A CORNER OF TWO SIDES: the sides and the point they meet at.
struct Corner {
    legs: [Leg; 2],
    point: Id,
}

impl Project {
    /// The side of the corner that the edge makes; `None` where the edge does not end there.
    fn leg(&self, si: usize, at: EdgeAt) -> Option<Leg> {
        let EdgeAt { edge: eid, corner: pid } = at;
        let s = self.sketches.get(si)?;
        let e = s.entity(eid)?;
        let [a, b] = entity_ends(e)?;
        let (end, far) = if a == pid {
            (End::Start, b)
        } else if b == pid {
            (End::Finish, a)
        } else {
            return None;
        };
        Some(Leg { eid, curve: Curve::of(s, e)?, end, far })
    }

    /// THE WAY THE EDGE LEAVES ITS CORNER, a unit vector along it: a line towards its far end, any other curve along its
    /// tangent there.
    pub(crate) fn leaving(&self, si: usize, at: EdgeAt) -> Option<Point2> {
        self.leg(si, at).map(|l| l.arm().leaving())
    }

    /// Whether both entities are lines or arcs: the corners that keep their own path.
    pub(crate) fn straight_or_round(&self, si: usize, eids: [Id; 2]) -> bool {
        let Some(s) = self.sketches.get(si) else { return false };
        eids.iter().all(|id| s.entity(*id).is_some_and(|e| matches!(e.kind, EntityKind::Line { .. } | EntityKind::Arc { .. })))
    }

    /// The two sides of the corner the pair makes, and the corner point.
    fn legs_of(&self, si: usize, [e1, e2]: [Id; 2]) -> Option<Corner> {
        let corner = self.shared_vertex(si, e1, e2)?;
        Some(Corner { legs: [self.leg(si, EdgeAt { edge: e1, corner })?, self.leg(si, EdgeAt { edge: e2, corner })?], point: corner })
    }

    /// The radius a fillet of `size` takes on the corner of the two sides: a chord or an arc length makes it on the turn
    /// between the ways the sides leave the corner.
    fn radius_for(legs: &[Leg; 2], size: FilletSize) -> Option<f64> {
        if size.by == FilletBy::Radius {
            return Some(size.value);
        }
        let [u, v] = [legs[0].arm().leaving(), legs[1].arm().leaving()];
        size.radius_on(std::f64::consts::PI - (u.x * v.x + u.y * v.y).clamp(-1.0, 1.0).acos())
    }

    /// WHAT THE CUT WOULD LEAVE AT THE CORNER OF ANY TWO CURVES, changing nothing: `corner_blend` for a pair that is not
    /// two lines and arcs.
    pub(crate) fn blend_any(&self, si: usize, pair: [Id; 2], cut: CornerCut) -> Option<CornerBlend> {
        let legs = self.legs_of(si, pair)?.legs;
        let corner = legs[0].arm().corner();
        let arms = [legs[0].arm(), legs[1].arm()];
        let (places, arc) = match cut {
            CornerCut::Fillet(size) => {
                let r = Self::radius_for(&legs, size)?;
                let found = rounding(arms, r)?;
                (found.at, Some(([found.centre.x, found.centre.y], r)))
            }
            CornerCut::Chamfer(legs_of_cut) => (chamfer(arms, legs_of_cut)?, None),
        };
        let end = |k: usize| {
            let p = legs[k].curve.at(places[k]);
            [p.x, p.y]
        };
        Some(CornerBlend { vertex: [corner.x, corner.y], ends: [end(0), end(1)], arc })
    }

    /// CUT A SIDE BACK TO PLACE `t`: the piece of it from there to its far end laid in its stead, under its own id, of its
    /// own kind. Answers the point at `t`.
    fn cut_leg_back(&mut self, si: usize, leg: &Leg, t: f64) -> Option<Id> {
        let run = match leg.end {
            End::Start => [t, 1.0],
            End::Finish => [0.0, t],
        };
        let piece = leg.curve.piece(run);
        let how = Self::lay_as_of(self.sketches[si].entity(leg.eid)?);
        let kind = self.curve_kind(si, &piece, how);
        let at = self.point_exactly_at(si, leg.curve.at(t));
        self.sketches[si].entities.iter_mut().find(|e| e.id == leg.eid)?.kind = kind;
        Some(at)
    }

    /// THE SHARP OF THE CORNER STAYS where the side went on: on the line of a line, the circle of an arc, the ellipse of an
    /// arc of one. A spline holds no point yet, and the sharp stands where it was.
    fn hold_sharp(&mut self, si: usize, pid: Id, leg: &Leg, cut_at: Id) {
        let Some(kind) = self.sketches[si].entity(leg.eid).map(|e| e.kind.clone()) else { return };
        let held = match kind {
            EntityKind::Line { .. } => Some(Constraint::PointOnLine { p: pid, a: leg.far, b: cut_at }),
            EntityKind::Arc { center, .. } => Some(Constraint::PointOnCircle { p: pid, c: center }),
            EntityKind::Ellipse { c, ma, mi, .. } => Some(Constraint::PointOnEllipse { p: pid, c, ma, mi }),
            _ => None,
        };
        self.sketches[si].constraints.extend(held);
    }

    /// ROUND THE CORNER OF ANY TWO CURVES with an arc of radius `r`: both sides cut back to where it touches them, the
    /// arc between, held to a side that is a line or an arc by a tangency and sized by its radius. Answers whether it
    /// was made - not where the radius does not fit.
    pub(crate) fn round_any(&mut self, si: usize, pair: [Id; 2], r: f64) -> bool {
        let Some(Corner { legs, point: pid }) = self.legs_of(si, pair) else { return false };
        let Some(found) = rounding([legs[0].arm(), legs[1].arm()], r) else { return false };
        let (Some(t1), Some(t2)) = (self.cut_leg_back(si, &legs[0], found.at[0]), self.cut_leg_back(si, &legs[1], found.at[1])) else { return false };
        let cen = self.point_exactly_at(si, found.centre);
        // the short way round from the first point of touching to the second
        let angle = |p: Point2| (p.y - found.centre.y).atan2(p.x - found.centre.x);
        let (a1, a2) = (angle(legs[0].curve.at(found.at[0])), angle(legs[1].curve.at(found.at[1])));
        let ccw = (a2 - a1).rem_euclid(std::f64::consts::TAU) <= std::f64::consts::PI;
        let arc = self.alloc_id();
        self.sketches[si].entities.push(SketchEntity { id: arc, kind: EntityKind::Arc { center: cen, a: t1, b: t2, ccw }, construction: false });
        for (leg, t) in legs.iter().zip([t1, t2]) {
            let kind = self.sketches[si].entity(leg.eid).map(|e| e.kind.clone());
            let touch = match kind {
                Some(EntityKind::Line { .. }) => Some(Constraint::Tangent { a: leg.far, b: t, c: cen, r }),
                Some(EntityKind::Arc { center, .. }) => {
                    let (Some((cx, cy)), Some((tx, ty))) = (self.point_xy(si, center), self.point_xy(si, t)) else { continue };
                    let rad = (tx - cx).hypot(ty - cy);
                    let d = (found.centre.x - cx).hypot(found.centre.y - cy);
                    Some(Constraint::CircleTangent { c1: center, c2: cen, external: ((rad + r) - d).abs() <= ((rad - r).abs() - d).abs() })
                }
                _ => None,
            };
            self.sketches[si].constraints.extend(touch);
            self.hold_sharp(si, pid, leg, t);
        }
        let off = fillet_label_angle(&self.sketches[si].points, cen, t1, t2);
        self.sketches[si].constraints.push(Constraint::Diameter { c: cen, d: r, off, expr: String::new(), driven: false, diam: false, at: None });
        self.regen_sketch(si);
        true
    }

    /// CHAMFER THE CORNER OF ANY TWO CURVES by `legs`, the first leg on the first of the pair: both sides cut back to
    /// where the cut meets them, the cut laid as a line, its size kept as dimensions - the cut of a symmetric one, a leg
    /// straight from the sharp otherwise. Answers whether it was made - not where a side does not reach as far.
    pub(crate) fn chamfer_any(&mut self, si: usize, pair: [Id; 2], legs_of_cut: ChamferLegs) -> bool {
        let Some(Corner { legs, point: pid }) = self.legs_of(si, pair) else { return false };
        let Some(places) = chamfer([legs[0].arm(), legs[1].arm()], legs_of_cut) else { return false };
        let (sharp, at1, at2) = (legs[0].arm().corner(), legs[0].curve.at(places[0]), legs[1].curve.at(places[1]));
        let (Some(t1), Some(t2)) = (self.cut_leg_back(si, &legs[0], places[0]), self.cut_leg_back(si, &legs[1], places[1])) else { return false };
        let seg = self.alloc_id();
        self.sketches[si].entities.push(SketchEntity { id: seg, kind: EntityKind::Line { a: t1, b: t2 }, construction: false });
        let leg_dim = |ends: [DimEnd; 2], side: Side, d: f64| {
            let [a, b] = ends_facing(ends, side);
            Constraint::Distance { a, b, d, off: 0.0, expr: String::new(), driven: false, axis: 0, at: None }
        };
        let (end_c, end_1, end_2) = (DimEnd { id: pid, at: sharp }, DimEnd { id: t1, at: at1 }, DimEnd { id: t2, at: at2 });
        let dims = match legs_of_cut.mode {
            crate::feature::ChamferMode::Symmetric => vec![leg_dim([end_1, end_2], Side::Toward(sharp), legs_of_cut.first)],
            crate::feature::ChamferMode::TwoDist => vec![leg_dim([end_c, end_1], Side::AwayFrom(at2), sharp.dist(at1)), leg_dim([end_c, end_2], Side::AwayFrom(at1), sharp.dist(at2))],
            crate::feature::ChamferMode::DistAngle => vec![leg_dim([end_c, end_1], Side::AwayFrom(at2), sharp.dist(at1))],
        };
        self.sketches[si].constraints.extend(dims);
        for (leg, t) in legs.iter().zip([t1, t2]) {
            self.hold_sharp(si, pid, leg, t);
        }
        self.regen_sketch(si);
        true
    }
}
