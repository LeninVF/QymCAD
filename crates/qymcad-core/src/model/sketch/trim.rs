//! TRIM, EXTEND AND BREAK: the curves of a sketch cut where they cross.

use super::*;
use crate::model::curve::Curve;
use crate::model::curve::Trimmed;
use crate::model::sketch::crossings::TrimCut;

impl Project {
    /// TRIM CURVE `eid` OF ANY KIND AT THE CLICK, as its preview draws it (`trim_cut`): the piece under the click, between
    /// the two crossings on either side of it - or an end - goes, and the rest stays whole: an open curve keeps the pieces
    /// before and after, a closed one the one piece round from the far crossing to the near one. A curve crossed nowhere,
    /// or a closed one crossed once, goes whole. A line keeps its pieces on its straight, an arc or a circle its centre,
    /// and their cut points stay on what crosses them.
    pub fn trim_curve(&mut self, si: usize, eid: Id, clickx: f64, clicky: f64) -> bool {
        self.cut_projection(si, eid, |p| p.trim_curve_here(si, eid, Point2::new(clickx, clicky)))
    }
    pub(crate) fn trim_curve_here(&mut self, si: usize, eid: Id, click: Point2) -> bool {
        let Some(TrimCut { curve, run }) = self.trim_cut(si, eid, click) else { return false };
        let Some([from, to]) = run else { return self.trim_away_whole(si, eid) };
        // what stays: round from the far crossing to the near one on a closed curve, before and after the run on an open one
        let kept: Vec<[f64; 2]> = if curve.closed() {
            vec![if to <= 1.0 { [to, from + 1.0] } else { [to - 1.0, from] }]
        } else {
            [[0.0, from], [to, 1.0]].into_iter().filter(|[a, b]| b - a > 1e-9).collect()
        };
        self.keep_pieces(si, eid, &curve, Trimmed { kept, run: [from, to], click })
    }
    /// After a cut: coincident cut points stitched so the pieces stay connected, no dangling point left, and the sketch
    /// solved - the edit ends at the solver, not at a rebuild (`regen_sketch` recomputes the contours and checks no
    /// constraint).
    pub(crate) fn settle_after_cut(&mut self, si: usize) {
        self.merge_close_points(si, 1e-3);
        self.prune_orphan_sketch_points(si);
        self.solve_sketch(si);
    }

    /// THE CURVES LYING OVER CURVE `eid` - `eid` first, then the others in the order of the sketch: a line on the same
    /// straight with a stretch in common, an arc or a circle on the same circle with a stretch of it in common. A side
    /// drawn over another (two rectangles drawn against each other) is a curve of its own, and a click takes either of the
    /// two by chance: the list lets a person name the one meant.
    pub fn overlapping_curves(&self, si: usize, eid: Id) -> Vec<Id> {
        let Some(s) = self.sketches.get(si) else { return Vec::new() };
        let at = |id: Id| s.points.iter().find(|q| q.id == id).map(|q| (q.x, q.y));
        let Some(me) = s.entities.iter().find(|e| e.id == eid) else { return Vec::new() };
        const TOL: f64 = 1e-6;
        // a stretch along the straight through `a` and `b` that `p` and `q` span, when both stand on it
        let line_span = |a: (f64, f64), b: (f64, f64), p: (f64, f64), q: (f64, f64)| -> Option<Stretch> {
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let l = dx.hypot(dy);
            let off = |r: (f64, f64)| ((r.0 - a.0) * dy - (r.1 - a.1) * dx).abs() / l;
            if l < TOL || off(p) > TOL || off(q) > TOL {
                return None;
            }
            let t = |r: (f64, f64)| ((r.0 - a.0) * dx + (r.1 - a.1) * dy) / l;
            Some(Stretch { from: t(p).min(t(q)), to: t(p).max(t(q)) })
        };
        let round = |e: &super::SketchEntity| -> Option<OnCircle> {
            let circle = e.kind.circle_by(&|id| at(id).map(|(x, y)| Point2::new(x, y)))?;
            let c = at(circle.center)?;
            let ang = |p: (f64, f64)| (p.1 - c.1).atan2(p.0 - c.0);
            let span = match e.kind.as_arc() {
                Some(arc) => {
                    let (pa, pb) = (at(arc.a)?, at(arc.b)?);
                    let (from, to) = if arc.ccw { (ang(pa), ang(pb)) } else { (ang(pb), ang(pa)) };
                    Some(Stretch { from, to })
                }
                None => None,
            };
            Some(OnCircle { centre: c, r: circle.r, span })
        };
        // whether an angle stands on a counter-clockwise stretch of a circle (none: the whole of it)
        let inside = |span: Option<Stretch>, t: f64| match span {
            None => true,
            Some(Stretch { from, to }) => (t - from).rem_euclid(std::f64::consts::TAU) <= (to - from).rem_euclid(std::f64::consts::TAU),
        };
        // whether two stretches of one circle share more than a point: sampled along the circle
        let share = |x: Option<Stretch>, y: Option<Stretch>| -> bool {
            (0..64).map(|k| f64::from(k) / 64.0 * std::f64::consts::TAU).any(|t| inside(x, t) && inside(y, t) && inside(x, t + 0.01) && inside(y, t + 0.01))
        };
        let over = |e: &super::SketchEntity| -> bool {
            match (me.kind.as_line(), e.kind.as_line()) {
                (Some(mine), Some(other)) => {
                    let (Some(a), Some(b), Some(c), Some(d)) = (at(mine.a), at(mine.b), at(other.a), at(other.b)) else { return false };
                    let Some(mine) = line_span(a, b, a, b) else { return false };
                    line_span(a, b, c, d).is_some_and(|other| other.to.min(mine.to) - other.from.max(mine.from) > TOL)
                }
                _ => match (round(me), round(e)) {
                    (Some(x), Some(y)) => (x.centre.0 - y.centre.0).hypot(x.centre.1 - y.centre.1) < TOL && (x.r - y.r).abs() < TOL && share(x.span, y.span),
                    _ => false,
                },
            }
        };
        std::iter::once(eid).chain(s.entities.iter().filter(|e| e.id != eid && over(e)).map(|e| e.id)).collect()
    }

    /// WHETHER END `p` OF CURVE `eid` IS HELD BY SOMETHING BUT THE DIRECTION OF A LINE: another curve ends there, or a
    /// constraint other than one that turns a line (horizontal, vertical, an angle, parallel, perpendicular,
    /// collinear) or puts the end where it stands holds the point - an anchor, a dimension. A tie that only puts the end
    /// there - a coincidence with a point no curve ends at, a point on a line, an axis or a circle, a middle - is lifted
    /// by the extension (`lift_the_place_of`): a line drawn from the origin, held on it, was not extended at all.
    pub(crate) fn end_held(&self, si: usize, eid: Id, p: Id) -> bool {
        let Some(s) = self.sketches.get(si) else { return false };
        if s.entities.iter().any(|e| e.id != eid && entity_points(e).contains(&p)) {
            return true;
        }
        s.constraints.iter().any(|c| {
            let turns_the_line = matches!(
                c,
                Constraint::Horizontal { .. }
                    | Constraint::Vertical { .. }
                    | Constraint::Orientation { .. }
                    | Constraint::Parallel { .. }
                    | Constraint::Perpendicular { .. }
                    | Constraint::Collinear { .. }
                    | Constraint::AngleLines { .. }
            );
            // a coincidence with the end of another curve joins a corner, and holds
            let a_curve_ends_at = |q: Id| s.entities.iter().any(|e| entity_points(e).contains(&q));
            let places_the_end = match *c {
                Constraint::Coincident { a, b } => (a == p && !a_curve_ends_at(b)) || (b == p && !a_curve_ends_at(a)),
                Constraint::PointOnLine { p: q, .. } | Constraint::Midpoint { p: q, .. } | Constraint::PointOnCircle { p: q, .. } => q == p,
                _ => false,
            };
            !turns_the_line && !places_the_end && c.points().contains(&p)
        })
    }

    /// EXTEND CURVE `eid` OF ANY KIND AT THE CLICK, as its preview draws it (`extended`): from its end nearer the click to
    /// the nearest curve on its way - a line along its straight, an arc round its circle, an arc of an ellipse round its
    /// ellipse, a spline by a straight span along the tangent of its end. An end joined to other geometry stays.
    pub fn extend_curve(&mut self, si: usize, eid: Id, clickx: f64, clicky: f64) -> bool {
        let ask = ExtendAsk { pointer: Point2::new(clickx, clicky), over: None, sides: ExtendSides::Nearer };
        self.curve_extension(si, eid, &ask).is_some_and(|ext| self.extend_curve_by(si, eid, &ext))
    }
    pub(crate) fn line_ends(&self, si: usize, eid: Id) -> Option<(Id, Id)> {
        let s = self.sketches.get(si)?;
        s.entity(eid).and_then(|e| match e.kind.as_line() {
            Some(crate::model::LineParts { a, b }) => Some((a, b)),
            _ => None,
        })
    }
    /// Intersections of entity `eid` with the other entities (construction included) that lie on `eid` - inside it for an
    /// open curve - in world coordinates. Used for the hover preview of trim and break.
    pub fn entity_intersections(&self, si: usize, eid: Id) -> Vec<(f64, f64)> {
        let Some(s) = self.sketches.get(si) else { return Vec::new() };
        let closed = s.entities.iter().find(|e| e.id == eid).and_then(|e| crate::model::curve::Curve::of(s, e)).is_some_and(|c| c.closed());
        self.crossings_on(si, eid).into_iter().filter(|x| closed || (x.t > 1e-6 && x.t < 1.0 - 1e-6)).map(|x| (x.at.x, x.at.y)).collect()
    }
    /// TRIM THAT LEAVES NOTHING: the click lay on an entity with no crossing inside it - a piece bounded by its own
    /// ends, or one crossing nothing at all. The whole of it is the piece under the click, so it goes, with what it
    /// orphans. Reported behaviour (issue #31): such a piece answered "The operation did not apply" and had to be
    /// deleted by hand, and every piece an earlier trim leaves is one.
    pub(crate) fn trim_away_whole(&mut self, si: usize, eid: Id) -> bool {
        self.delete_entities(si, &[eid]);
        self.solve_sketch(si);
        true
    }
    /// BREAK CURVE `eid` OF ANY KIND AT THE CLICK, where its preview cuts (`break_at`): an open curve into two at the place
    /// nearest the click, a closed one into two halves there and opposite. A line keeps what it was held by in each half;
    /// an arc or a circle becomes arcs about its own centre.
    pub fn break_curve(&mut self, si: usize, eid: Id, clickx: f64, clicky: f64) -> bool {
        self.cut_projection(si, eid, |p| p.break_curve_here(si, eid, Point2::new(clickx, clicky)))
    }
    pub(crate) fn break_curve_here(&mut self, si: usize, eid: Id, click: Point2) -> bool {
        let Some(curve) = self.sketches.get(si).and_then(|s| s.entity(eid).and_then(|e| Curve::of(s, e))) else { return false };
        let before: Vec<Id> = self.sketches[si].entities.iter().map(|e| e.id).collect();
        // where the piece keeping the id of the curve starts: at the start of an open curve, at the place of the click
        // on a closed one
        let start = if curve.closed() { curve.at(curve.nearest(click).t) } else { curve.at(0.0) };
        let broken = self.break_into_pieces(si, eid, &curve, click);
        if broken {
            self.piece_keeps_the_id(si, eid, &before, start);
        }
        broken
    }
    /// THE PIECE OF BROKEN CURVE `eid` THAT STARTS AT `start` TAKES ITS ID, of whatever kind the curve was: what was built
    /// from the curve - the wall swept from it, and what stands on that wall - goes on naming it, as with a line broken in
    /// two (`break_line_at`), where the first half is the line it was.
    fn piece_keeps_the_id(&mut self, si: usize, eid: Id, before: &[Id], start: Point2) {
        let s = &self.sketches[si];
        if s.entity(eid).is_some() {
            return;
        }
        let piece = s.entities.iter().filter(|e| !before.contains(&e.id)).find(|e| Curve::of(s, e).is_some_and(|c| c.at(0.0).dist(start) < 1e-6)).map(|e| e.id);
        if let Some(e) = piece.and_then(|id| self.sketches[si].entities.iter_mut().find(|e| e.id == id)) {
            e.id = eid;
        }
    }
    /// WHERE A POINT STANDS in the drawing, or `None` where the sketch has no such point.
    pub fn point_xy(&self, si: usize, id: Id) -> Option<(f64, f64)> {
        let s = self.sketches.get(si)?;
        s.point(id).map(|p| (p.x, p.y))
    }
}
