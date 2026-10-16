//! TRIM, EXTEND AND BREAK: the curves of a sketch cut where they cross.

use super::*;
use crate::model::curve::Curve;
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
        match curve {
            Curve::Line { .. } => self.keep_line_pieces(si, eid, &curve, &kept),
            Curve::Round { .. } => match self.round_pieces(si, eid, &curve, &kept) {
                Some(ends) => {
                    // the ends of the arcs are anchored to the crossing curves, so a cut point does not drift along the
                    // angle on the next solve
                    for p in ends {
                        self.anchor_cut_point(si, p);
                    }
                    self.settle_after_cut(si);
                    true
                }
                None => false,
            },
            _ => self.trim_piece(si, eid, &curve, [from, to], click),
        }
    }
    /// PUT THE PIECES `kept` OF LINE `eid` IN ITS STEAD, as places along it, and hold them as one line: the inner cut
    /// points on the straight through the outermost points, each cut point on the curve crossing it there.
    fn keep_line_pieces(&mut self, si: usize, eid: Id, line: &Curve, kept: &[[f64; 2]]) -> bool {
        let Some((a, b)) = self.line_ends(si, eid) else { return false };
        let con = self.sketches[si].entity(eid).is_some_and(|e| e.construction);
        self.sketches[si].entities.retain(|e| e.id != eid);
        let mut order: Vec<Id> = Vec::new();
        for run in kept {
            let [q0, q1] = line.piece(*run).ends();
            let (p0, p1) = (self.sketch_point_at(si, q0.x, q0.y, 1e-9), self.sketch_point_at(si, q1.x, q1.y, 1e-9));
            let id = self.alloc_id();
            self.sketches[si].entities.push(SketchEntity { id, kind: crate::model::LineParts { a: p0, b: p1 }.into(), construction: con });
            if order.last() != Some(&p0) {
                order.push(p0);
            }
            order.push(p1);
        }
        // The pieces of one line stay on one straight line: the interior cut points are held by `PointOnLine`
        // against the line through the outermost points, which the solver cannot collapse (the residual is
        // divided by the length). Otherwise editing a dimension spreads the pieces into a parallelogram and the
        // notch or arc flies away.
        order.dedup();
        if order.len() >= 3 {
            let (r0, r1) = (order[0], order[order.len() - 1]);
            for &pi in &order[1..order.len() - 1] {
                self.sketches[si].constraints.push(Constraint::PointOnLine { p: pi, a: r0, b: r1 });
            }
        }
        // The cut points (not the original endpoints) are anchored to the crossing curve — a circle, an arc or
        // another line — so a point stays at the intersection instead of sliding along it and the pieces do not
        // spread when a dimension is edited.
        for pi in order.into_iter().filter(|&pi| pi != a && pi != b) {
            self.anchor_cut_point(si, pi);
        }
        self.settle_after_cut(si);
        true
    }
    /// PUT THE PIECES `runs` OF CIRCLE OR ARC `eid` IN ITS STEAD, as places along it: arcs about its own centre, each the
    /// way its piece runs. Answers the ends of the arcs; `None` where `eid` is no circle or arc of the sketch.
    fn round_pieces(&mut self, si: usize, eid: Id, round: &Curve, runs: &[[f64; 2]]) -> Option<Vec<Id>> {
        let s = self.sketches.get(si)?;
        let e = s.entity(eid)?;
        let (con, center) = (e.construction, e.kind.circle_by(&|id| s.point(id).map(|q| Point2::new(q.x, q.y)))?.center);
        self.sketches[si].entities.retain(|x| x.id != eid);
        let mut ends = Vec::new();
        for run in runs {
            let piece = round.piece(*run);
            let Curve::Round { sweep, .. } = piece else { continue };
            let [q0, q1] = piece.ends();
            let (pa, pb) = (self.sketch_point_at(si, q0.x, q0.y, 1e-6), self.sketch_point_at(si, q1.x, q1.y, 1e-6));
            let id = self.alloc_id();
            self.sketches[si].entities.push(SketchEntity { id, kind: crate::model::ArcParts { center, a: pa, b: pb, ccw: sweep > 0.0 }.into(), construction: con });
            ends.extend([pa, pb]);
        }
        Some(ends)
    }
    /// After a cut: coincident cut points stitched so the pieces stay connected, no dangling point left, and the sketch
    /// solved - the edit ends at the solver, not at a rebuild (`regen_sketch` recomputes the contours and checks no
    /// constraint).
    pub(crate) fn settle_after_cut(&mut self, si: usize) {
        self.merge_close_points(si, 1e-3);
        self.prune_orphan_sketch_points(si);
        self.solve_sketch(si);
    }
    /// THE PLACES LINE `eid` CAN BE EXTENDED TO: where its axis meets another curve beyond each end, nearest first, each
    /// with the curve met there. `None` when `eid` is no line of the sketch or has no length. Construction geometry is a
    /// boundary as well.
    pub fn extend_stops(&self, si: usize, eid: Id) -> Option<ExtendStops> {
        let (a, b) = self.line_ends(si, eid)?;
        let ((pax, pay), (pbx, pby)) = (self.point_xy(si, a)?, self.point_xy(si, b)?);
        if (pbx - pax).powi(2) + (pby - pay).powi(2) < 1e-12 {
            return None;
        }
        let mut cand: Vec<ExtendStop> = Vec::new();
        let s = self.sketches.get(si)?;
        for e in s.entities.iter().filter(|e| e.id != eid) {
            let ts = crate::model::curve::Curve::of(s, e).map(|c| c.meets_line([Point2::new(pax, pay), Point2::new(pbx, pby)])).unwrap_or_default();
            cand.extend(ts.into_iter().map(|t| ExtendStop { t, by: e.id }));
        }
        let mut stops = ExtendStops { before_a: cand.iter().copied().filter(|s| s.t < -1e-6).collect(), past_b: cand.into_iter().filter(|s| s.t > 1.0 + 1e-6).collect() };
        stops.before_a.sort_by(|x, y| y.t.total_cmp(&x.t));
        stops.past_b.sort_by(|x, y| x.t.total_cmp(&y.t));
        Some(stops)
    }

    /// WHERE LINE `eid` GOES, ASKED AS THE EXTEND TOOL ASKS: each end taken to the curve the pointer is over when that
    /// curve lies on its side of the axis, else to the nearest curve there; an end with nothing on its side stays. One
    /// end - the one on the side of the curve under the pointer, else the one nearer the pointer - or both.
    ///
    /// AN END JOINED TO OTHER GEOMETRY STAYS (`end_held`): moved, it pulled what it is joined to, and the solve pulled
    /// it back. Reported behaviour: Extend on the side two rectangles share, its end the end of a fillet, moved the
    /// drawing (the ends of the fillet by 5e-6 mm, enough to lose the face of the rectangle), extended nothing and said
    /// it was done. The line answers `EndKept::Joined`, and a new line is drawn from that end instead.
    pub fn line_extension(&self, si: usize, eid: Id, ask: &ExtendAsk) -> LineExtension {
        let free = self.line_extension_free(si, eid, ask);
        let Some((a, b)) = self.line_ends(si, eid) else { return free };
        let (held_a, held_b) = (free.a.is_some() && self.end_held(si, eid, a), free.b.is_some() && self.end_held(si, eid, b));
        LineExtension { a: free.a.filter(|_| !held_a), b: free.b.filter(|_| !held_b), kept: if held_a || held_b { EndKept::Joined } else { EndKept::Free } }
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

    /// WHETHER END `p` OF LINE `eid` IS HELD BY SOMETHING BUT THE DIRECTION OF THE LINE: another curve ends there, or a
    /// constraint other than one that turns the line (horizontal, vertical, an angle, parallel, perpendicular,
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

    pub(crate) fn line_extension_free(&self, si: usize, eid: Id, ask: &ExtendAsk) -> LineExtension {
        let Some(stops) = self.extend_stops(si, eid) else { return LineExtension::default() };
        let to = |side: &[ExtendStop]| side.iter().find(|s| Some(s.by) == ask.over).or(side.first()).map(|s| s.t);
        let (at_a, at_b) = (to(&stops.before_a), to(&stops.past_b));
        if ask.sides == ExtendSides::Both {
            return LineExtension { a: at_a, b: at_b, ..Default::default() };
        }
        let over_side = |side: &[ExtendStop]| ask.over.is_some_and(|o| side.iter().any(|s| s.by == o));
        let past_b = if over_side(&stops.past_b) {
            true
        } else if over_side(&stops.before_a) {
            false
        } else {
            let Some((a, b)) = self.line_ends(si, eid) else { return LineExtension::default() };
            let (Some((pax, pay)), Some((pbx, pby))) = (self.point_xy(si, a), self.point_xy(si, b)) else { return LineExtension::default() };
            let tc = ((ask.pointer.x - pax) * (pbx - pax) + (ask.pointer.y - pay) * (pby - pay)) / ((pbx - pax).powi(2) + (pby - pay).powi(2));
            tc >= 0.5
        };
        if past_b {
            LineExtension { a: None, b: at_b, ..Default::default() }
        } else {
            LineExtension { a: at_a, b: None, ..Default::default() }
        }
    }

    /// EXTEND LINE `eid` AS `ext` SAYS: each end given a place along the axis goes there, and the sketch is solved.
    /// Returns whether an end moved.
    pub fn extend_line_by(&mut self, si: usize, eid: Id, ext: LineExtension) -> bool {
        self.cut_projection(si, eid, |p| p.extend_line_by_here(si, eid, ext))
    }
    pub(crate) fn extend_line_by_here(&mut self, si: usize, eid: Id, ext: LineExtension) -> bool {
        let Some((a, b)) = self.line_ends(si, eid) else { return false };
        let (Some((pax, pay)), Some((pbx, pby))) = (self.point_xy(si, a), self.point_xy(si, b)) else { return false };
        let mut moved = false;
        for (pid, t) in [(a, ext.a), (b, ext.b)] {
            let Some(t) = t else { continue };
            self.lift_the_place_of(si, pid);
            if let Some(p) = self.sketches[si].points.iter_mut().find(|q| q.id == pid) {
                (p.x, p.y) = (pax + (pbx - pax) * t, pay + (pby - pay) * t);
                moved = true;
            }
        }
        if moved {
            self.solve_sketch(si); // the edit ends at the solver, not at a rebuild: `regen_sketch` recomputes the contours and checks no constraint
        }
        moved
    }

    /// EXTEND CURVE `eid` OF ANY KIND AT THE CLICK, as its preview draws it (`extended`): from its end nearer the click to
    /// the nearest curve on its way - a line along its straight, an arc round its circle, an arc of an ellipse round its
    /// ellipse, a spline by a straight span along the tangent of its end. An end joined to other geometry stays.
    pub fn extend_curve(&mut self, si: usize, eid: Id, clickx: f64, clicky: f64) -> bool {
        self.cut_projection(si, eid, |p| p.extend_piece(si, eid, Point2::new(clickx, clicky)))
    }
    /// BREAK LINE `eid` INTO TWO at place `tc` along it, the halves held as the line was.
    fn break_line_at(&mut self, si: usize, eid: Id, tc: f64) -> bool {
        let Some((a, b)) = self.line_ends(si, eid) else { return false };
        let (Some((pax, pay)), Some((pbx, pby))) = (self.point_xy(si, a), self.point_xy(si, b)) else { return false };
        let dlen2 = (pbx - pax).powi(2) + (pby - pay).powi(2);
        if dlen2 < 1e-12 {
            return false;
        }
        let mid = self.sketch_point_at(si, pax + (pbx - pax) * tc, pay + (pby - pay) * tc, 1e-9);
        let con = self.sketches[si].entities.iter().find(|e| e.id == eid).is_some_and(|e| e.construction);
        // THE FIRST HALF IS THE LINE IT WAS, the second a line added: what was built from the line - the wall swept from
        // it, and every rounding and sketch on that wall - goes on naming it. Both halves new, the wall took the name
        // of a half: measured on the case of the examples, breaking the first line of each of its 32 sketches renamed
        // 59 faces that did not move.
        if let Some(line) = self.sketches[si].entities.iter_mut().find(|e| e.id == eid) {
            line.kind = crate::model::LineParts { a, b: mid }.into();
        }
        let id = self.alloc_id();
        self.sketches[si].entities.push(SketchEntity { id, kind: crate::model::LineParts { a: mid, b }.into(), construction: con });
        // THE HALVES KEEP WHAT THE LINE WAS HELD BY: a horizontal or a vertical of its ends becomes one of each half. Left
        // as it stood, it held the far ends only, and the break point was free to leave the line - two constraints and
        // seven freedoms where there are three and six.
        let halves = |c: &Constraint| -> Option<[Constraint; 2]> {
            match *c {
                Constraint::Horizontal { a: x, b: y } if (x == a && y == b) || (x == b && y == a) => Some([Constraint::Horizontal { a, b: mid }, Constraint::Horizontal { a: mid, b }]),
                Constraint::Vertical { a: x, b: y } if (x == a && y == b) || (x == b && y == a) => Some([Constraint::Vertical { a, b: mid }, Constraint::Vertical { a: mid, b }]),
                _ => None,
            }
        };
        let old = std::mem::take(&mut self.sketches[si].constraints);
        for c in old {
            match halves(&c) {
                Some(two) => self.sketches[si].constraints.extend(two),
                None => self.sketches[si].constraints.push(c),
            }
        }
        self.settle_after_cut(si);
        true
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
        let mut places = self.break_at(si, eid, click);
        places.sort_by(f64::total_cmp);
        match curve {
            Curve::Line { .. } => places.first().is_some_and(|t| self.break_line_at(si, eid, *t)),
            Curve::Round { .. } => {
                let runs = match places[..] {
                    [t] => vec![[0.0, t], [t, 1.0]],
                    [t0, t1] => vec![[t0, t1], [t1, t0 + 1.0]],
                    _ => return false,
                };
                if self.round_pieces(si, eid, &curve, &runs).is_none() {
                    return false;
                }
                self.settle_after_cut(si);
                true
            }
            _ => self.break_piece(si, eid, click),
        }
    }
    /// WHERE A POINT STANDS in the drawing, or `None` where the sketch has no such point.
    pub fn point_xy(&self, si: usize, id: Id) -> Option<(f64, f64)> {
        let s = self.sketches.get(si)?;
        s.point(id).map(|p| (p.x, p.y))
    }
}
