//! TRIM, EXTEND AND BREAK: the curves of a sketch cut where they cross.

use super::*;

/// The sketch `si` of the project; called only with an index the caller has just found a sketch at.
fn s_of(p: &Project, si: usize) -> &Sketch {
    &p.sketches[si]
}

impl Project {
    /// Trim segment `eid` at the clicked point: the segment is divided by its intersections with other
    /// entities and the piece containing the click is removed.
    pub fn trim_line(&mut self, si: usize, eid: Id, clickx: f64, clicky: f64) -> bool {
        self.cut_projection(si, eid, |p| p.trim_line_here(si, eid, clickx, clicky))
    }
    pub(crate) fn trim_line_here(&mut self, si: usize, eid: Id, clickx: f64, clicky: f64) -> bool {
        let Some((a, b)) = self.line_ends(si, eid) else { return false };
        let (Some((pax, pay)), Some((pbx, pby))) = (self.point_xy(si, a), self.point_xy(si, b)) else { return false };
        let dlen2 = (pbx - pax).powi(2) + (pby - pay).powi(2);
        if dlen2 < 1e-12 {
            return false;
        }
        // Cut points from the intersections, as the parameter t along a to b: the places the other curves of the
        // sketch meet it, construction geometry a boundary as well
        let mut cuts: Vec<f64> = self.crossings_on(si, eid).into_iter().map(|x| x.t).filter(|t| *t > 1e-9 && *t < 1.0 - 1e-9).collect();
        if cuts.is_empty() {
            return self.trim_away_whole(si, eid);
        }
        cuts.push(0.0);
        cuts.push(1.0);
        cuts.sort_by(|x, y| x.total_cmp(y));
        cuts.dedup_by(|x, y| (*x - *y).abs() < 1e-6);
        // Parameter of the click.
        let tc = ((clickx - pax) * (pbx - pax) + (clicky - pay) * (pby - pay)) / dlen2;
        let mut keep: Vec<(f64, f64)> = Vec::new();
        let mut removed = false;
        for w in cuts.windows(2) {
            let (t0, t1) = (w[0], w[1]);
            if !removed && tc >= t0 - 1e-9 && tc <= t1 + 1e-9 {
                removed = true;
                continue;
            }
            keep.push((t0, t1));
        }
        if !removed {
            return false;
        }
        // Rebuild: delete the original segment and add the remaining pieces.
        self.sketches[si].entities.retain(|e| e.id != eid);
        let mut seg_pts: Vec<(Id, Id)> = Vec::new();
        for (t0, t1) in keep {
            let p0 = if t0 <= 1e-9 { a } else { self.sketch_point_at(si, pax + (pbx - pax) * t0, pay + (pby - pay) * t0, 1e-9) };
            let p1 = if t1 >= 1.0 - 1e-9 { b } else { self.sketch_point_at(si, pax + (pbx - pax) * t1, pay + (pby - pay) * t1, 1e-9) };
            let id = self.alloc_id();
            self.sketches[si].entities.push(SketchEntity { id, kind: EntityKind::Line { a: p0, b: p1 }, construction: false });
            seg_pts.push((p0, p1));
        }
        // The pieces of one line stay on one straight line: the interior cut points are held by `PointOnLine`
        // against the line through the outermost points, which the solver cannot collapse (the residual is
        // divided by the length). Otherwise editing a dimension spreads the pieces into a parallelogram and the
        // notch or arc flies away.
        let mut order: Vec<Id> = Vec::new();
        for &(p0, p1) in &seg_pts {
            if order.last() != Some(&p0) {
                order.push(p0);
            }
            order.push(p1);
        }
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
        let cut_pts: Vec<Id> = order.iter().copied().filter(|&pi| pi != a && pi != b).collect();
        for pi in cut_pts {
            self.anchor_cut_point(si, pi);
        }
        self.merge_close_points(si, 1e-3); // Stitch coincident cut points so the pieces stay connected.
        self.prune_orphan_sketch_points(si); // Leave no dangling points behind a trim.
        self.solve_sketch(si); // the edit ends at the solver, not at a rebuild: `regen_sketch` recomputes the contours and checks no constraint
        true
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
        for e in self.sketches.get(si)?.entities.iter().filter(|e| e.id != eid) {
            let mut ts: Vec<f64> = Vec::new();
            match e.kind {
                // a spline takes no part in these crossings
                EntityKind::Spline { .. } => {}
                EntityKind::Line { a: c, b: d } => {
                    if let (Some((cx, cy)), Some((dx, dy))) = (self.point_xy(si, c), self.point_xy(si, d)) {
                        ts.extend(line_seg_t([pax, pay], [pbx, pby], [cx, cy], [dx, dy]));
                    }
                }
                EntityKind::Circle { center, r } => {
                    if let Some((cx, cy)) = self.point_xy(si, center) {
                        ts.extend(line_circle_t(pax, pay, pbx, pby, cx, cy, r));
                    }
                }
                EntityKind::Ellipse { c, ma, mi, .. } => {
                    if let (Some((ex, ey)), Some((mx, my)), Some((nx, ny))) = (self.point_xy(si, c), self.point_xy(si, ma), self.point_xy(si, mi)) {
                        let (ux, uy, major, minor) = ellipse_axes(ex, ey, mx, my, nx, ny);
                        ts.extend(line_ellipse_roots([pax, pay], [pbx, pby], [ex, ey], [ux, uy], major, minor));
                    }
                }
                EntityKind::Arc { center, a: aa, b: bb, ccw } => {
                    if let (Some((cx, cy)), Some((sx, sy)), Some((ex, ey))) = (self.point_xy(si, center), self.point_xy(si, aa), self.point_xy(si, bb)) {
                        let r = ((sx - cx).powi(2) + (sy - cy).powi(2)).sqrt();
                        let (a0, a1) = ((sy - cy).atan2(sx - cx), (ey - cy).atan2(ex - cx));
                        for t in line_circle_t(pax, pay, pbx, pby, cx, cy, r) {
                            let (ix, iy) = (pax + (pbx - pax) * t, pay + (pby - pay) * t);
                            if angle_in_arc((iy - cy).atan2(ix - cx), a0, a1, ccw) {
                                ts.push(t);
                            }
                        }
                    }
                }
            }
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
            match e.kind {
                EntityKind::Circle { center, r } => Some(OnCircle { centre: at(center)?, r, span: None }),
                EntityKind::Arc { center, a, b, ccw } => {
                    let (c, pa, pb) = (at(center)?, at(a)?, at(b)?);
                    let ang = |p: (f64, f64)| (p.1 - c.1).atan2(p.0 - c.0);
                    let (from, to) = if ccw { (ang(pa), ang(pb)) } else { (ang(pb), ang(pa)) };
                    Some(OnCircle { centre: c, r: (pa.0 - c.0).hypot(pa.1 - c.1), span: Some(Stretch { from, to }) })
                }
                _ => None,
            }
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
            match (&me.kind, &e.kind) {
                (EntityKind::Line { a, b }, EntityKind::Line { a: c, b: d }) => {
                    let (Some(a), Some(b), Some(c), Some(d)) = (at(*a), at(*b), at(*c), at(*d)) else { return false };
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

    /// Extend: the segment endpoint nearer to the click is stretched to the nearest intersection with another
    /// entity. Returns whether it was extended.
    pub fn extend_line(&mut self, si: usize, eid: Id, clickx: f64, clicky: f64) -> bool {
        let ask = ExtendAsk { pointer: crate::geom::Point2::new(clickx, clicky), over: None, sides: ExtendSides::Nearer };
        let ext = self.line_extension(si, eid, &ask);
        self.extend_line_by(si, eid, ext)
    }
    /// Extend an arc: the endpoint nearer to the click is stretched along its own circle to the nearest
    /// intersection with another entity, within the gap outside the current span of the arc. Returns whether it
    /// was extended.
    pub fn extend_curve(&mut self, si: usize, eid: Id, clickx: f64, clicky: f64) -> bool {
        use std::f64::consts::TAU;
        // a curve of another kind is stretched along its own way (`extend_piece`)
        if !self.sketches.get(si).and_then(|s| s.entities.iter().find(|e| e.id == eid)).is_some_and(|e| matches!(e.kind, EntityKind::Arc { .. })) {
            return self.extend_piece(si, eid, Point2::new(clickx, clicky));
        }
        // Arc geometry; the coordinates are copied to release the borrow before calling
        // `curve_cut_angles`.
        let (center, a, b, ccw, cx, cy, ax, ay, bx, by) = {
            let Some(s) = self.sketches.get(si) else { return false };
            let Some(e) = s.entities.iter().find(|e| e.id == eid) else { return false };
            let p = |id: Id| s.points.iter().find(|q| q.id == id).map(|q| (q.x, q.y));
            let EntityKind::Arc { center, a, b, ccw } = e.kind else { return false };
            let (Some((cx, cy)), Some((ax, ay)), Some((bx, by))) = (p(center), p(a), p(b)) else { return false };
            (center, a, b, ccw, cx, cy, ax, ay, bx, by)
        };
        let r = ((ax - cx).powi(2) + (ay - cy).powi(2)).sqrt();
        let a0 = (ay - cy).atan2(ax - cx);
        let a1 = (by - cy).atan2(bx - cx);
        let sweep = if ccw { (a1 - a0).rem_euclid(TAU) } else { (a0 - a1).rem_euclid(TAU) };
        // Which endpoint is stretched: the one nearer to the click.
        let extend_b = ((clickx - bx).powi(2) + (clicky - by).powi(2)) < ((clickx - ax).powi(2) + (clicky - ay).powi(2));
        // Angles where the full circle meets the other entities, as a parameter along the direction of the
        // arc.
        let cuts = self.curve_cut_angles(si, eid, cx, cy, r);
        let to_param = |ang: f64| if ccw { (ang - a0).rem_euclid(TAU) } else { (a0 - ang).rem_euclid(TAU) };
        let gap: Vec<f64> = cuts.iter().map(|&ang| to_param(ang)).filter(|&pp| pp > sweep + 1e-6 && pp < TAU - 1e-6).collect();
        // Extending b takes the first cut in the gap just past endpoint b; extending a takes the last one,
        // nearer to a from the other side.
        let target = if extend_b { gap.into_iter().min_by(|x, y| x.total_cmp(y)) } else { gap.into_iter().max_by(|x, y| x.total_cmp(y)) };
        let Some(pp) = target else { return false };
        let new_ang = if ccw { a0 + pp } else { a0 - pp };
        let (nx, ny) = (cx + r * new_ang.cos(), cy + r * new_ang.sin());
        let pid = if extend_b { b } else { a };
        let _ = center;
        self.lift_the_place_of(si, pid);
        if let Some(p) = self.sketches[si].points.iter_mut().find(|q| q.id == pid) {
            p.x = nx;
            p.y = ny;
        }
        self.solve_sketch(si); // the edit ends at the solver, not at a rebuild: `regen_sketch` recomputes the contours and checks no constraint
        true
    }
    /// Break: split a segment into two at the clicked point.
    pub fn break_line(&mut self, si: usize, eid: Id, clickx: f64, clicky: f64) -> bool {
        self.cut_projection(si, eid, |p| p.break_line_here(si, eid, clickx, clicky))
    }
    pub(crate) fn break_line_here(&mut self, si: usize, eid: Id, clickx: f64, clicky: f64) -> bool {
        let Some((a, b)) = self.line_ends(si, eid) else { return false };
        let (Some((pax, pay)), Some((pbx, pby))) = (self.point_xy(si, a), self.point_xy(si, b)) else { return false };
        let dlen2 = (pbx - pax).powi(2) + (pby - pay).powi(2);
        if dlen2 < 1e-12 {
            return false;
        }
        let Some(&tc) = self.break_at(si, eid, Point2::new(clickx, clicky)).first() else { return false };
        let mid = self.sketch_point_at(si, pax + (pbx - pax) * tc, pay + (pby - pay) * tc, 1e-9);
        let con = self.sketches[si].entities.iter().find(|e| e.id == eid).is_some_and(|e| e.construction);
        // THE FIRST HALF IS THE LINE IT WAS, the second a line added: what was built from the line - the wall swept from
        // it, and every rounding and sketch on that wall - goes on naming it. Both halves new, the wall took the name
        // of a half: measured on the case of the examples, breaking the first line of each of its 32 sketches renamed
        // 59 faces that did not move.
        if let Some(line) = self.sketches[si].entities.iter_mut().find(|e| e.id == eid) {
            line.kind = EntityKind::Line { a, b: mid };
        }
        let id = self.alloc_id();
        self.sketches[si].entities.push(SketchEntity { id, kind: EntityKind::Line { a: mid, b }, construction: con });
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
        self.merge_close_points(si, 1e-3); // Stitch coincident cut points so the pieces stay connected.
        self.prune_orphan_sketch_points(si); // Leave no dangling points behind a trim.
        self.solve_sketch(si); // the edit ends at the solver, not at a rebuild: `regen_sketch` recomputes the contours and checks no constraint
        true
    }
    pub(crate) fn line_ends(&self, si: usize, eid: Id) -> Option<(Id, Id)> {
        let s = self.sketches.get(si)?;
        s.entity(eid).and_then(|e| match e.kind {
            EntityKind::Line { a, b } => Some((a, b)),
            _ => None,
        })
    }
    /// Angles, about centre (cx, cy) with radius r, of the places the other curves of the sketch, construction geometry
    /// included, meet the whole circle there, excluding `eid_self`. These are the cut points of a circle or an arc.
    pub(crate) fn curve_cut_angles(&self, si: usize, eid_self: Id, cx: f64, cy: f64, r: f64) -> Vec<f64> {
        let full = crate::model::curve::Curve::Round { centre: Point2::new(cx, cy), r, from: 0.0, sweep: std::f64::consts::TAU };
        let mut angs: Vec<f64> = self.meetings(si, &full, eid_self).iter().map(|x| (x.at.y - cy).atan2(x.at.x - cx)).collect();
        angs.sort_by(|a, b| a.total_cmp(b));
        angs.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
        angs
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
    /// Trim a circle or an arc: it is cut at the intersections, the angular span under the click is removed and
    /// the remaining spans become arcs. Lines are handled by `trim_line`. Returns whether it succeeded.
    pub fn trim_curve(&mut self, si: usize, eid: Id, clickx: f64, clicky: f64) -> bool {
        self.cut_projection(si, eid, |p| p.trim_curve_here(si, eid, clickx, clicky))
    }
    pub(crate) fn trim_curve_here(&mut self, si: usize, eid: Id, clickx: f64, clicky: f64) -> bool {
        use std::f64::consts::TAU;
        // an ellipse, an arc of one, a spline: the piece under the click goes (`trim_piece`)
        if !self.sketches.get(si).and_then(|s| s.entities.iter().find(|e| e.id == eid)).is_some_and(|e| round_entity(s_of(self, si), e).is_some()) {
            return self.trim_piece(si, eid, Point2::new(clickx, clicky));
        }
        let Some(s) = self.sketches.get(si) else { return false };
        let Some(e) = s.entities.iter().find(|e| e.id == eid) else { return false };
        let con = e.construction;
        let Some(Round { center_id, cx, cy, r, span }) = round_entity(s, e) else { return false };
        let cuts = self.curve_cut_angles(si, eid, cx, cy, r);
        if cuts.is_empty() {
            return self.trim_away_whole(si, eid);
        }
        let click_ang = (clicky - cy).atan2(clickx - cx);
        // One click removes only the span under the cursor, between the two nearest cut points, and the rest
        // stays whole: a circle leaves one remaining arc and an arc leaves up to two pieces, before and after
        // the removed window. Cutting at every intersection at once shatters a circle into a pile of arcs, which
        // breaks selection and radius editing and makes a pie slice impossible to cut out. Each remaining arc is
        // a tuple of (angle0, angle1, ccw).
        let mut kept: Vec<(f64, f64, bool)> = Vec::new();
        match span {
            None => {
                // A circle: one span under the click is removed and the remainder is a single arc, running
                // around the circle past the other cuts.
                let mut a: Vec<f64> = cuts.iter().map(|x| x.rem_euclid(TAU)).collect();
                a.sort_by(|x, y| x.total_cmp(y));
                a.dedup_by(|x, y| (*x - *y).abs() < 1e-6);
                if a.len() < 2 {
                    // touched at one point at most: there is no span to cut out, so the circle goes
                    return self.trim_away_whole(si, eid);
                }
                let ca = click_ang.rem_euclid(TAU);
                let n = a.len();
                for i in 0..n {
                    let (lo, hi) = (a[i], if i + 1 < n { a[i + 1] } else { a[0] + TAU });
                    if (ca >= lo - 1e-9 && ca < hi) || (ca + TAU >= lo && ca + TAU < hi) {
                        kept.push((hi, lo + TAU, true)); // The remainder is one arc, from hi around to lo.
                        break;
                    }
                }
            }
            Some((a0, a1, ccw)) => {
                // An arc: the parameter runs along its direction over [0, sweep]; the window under the click is
                // removed and the pieces before and after it stay whole.
                let to_param = |ang: f64| if ccw { (ang - a0).rem_euclid(TAU) } else { (a0 - ang).rem_euclid(TAU) };
                let sweep = if ccw { (a1 - a0).rem_euclid(TAU) } else { (a0 - a1).rem_euclid(TAU) };
                let mut ps: Vec<f64> = cuts.iter().map(|&x| to_param(x)).filter(|&v| v > 1e-6 && v < sweep - 1e-6).collect();
                ps.push(0.0);
                ps.push(sweep);
                ps.sort_by(|x, y| x.total_cmp(y));
                ps.dedup_by(|x, y| (*x - *y).abs() < 1e-6);
                let cp = to_param(click_ang);
                let ang = |param: f64| if ccw { a0 + param } else { a0 - param };
                // an arc crossed at its ends only is one window from end to end: nothing of it is left
                if ps.len() == 2 && cp > 1e-9 && cp < sweep - 1e-9 {
                    return self.trim_away_whole(si, eid);
                }
                for w in ps.windows(2) {
                    if cp > w[0] + 1e-9 && cp < w[1] - 1e-9 {
                        // Remove the window [w0, w1] under the click and keep the pieces before and after it,
                        // each as a single arc.
                        if w[0] > 1e-9 {
                            kept.push((ang(0.0), ang(w[0]), ccw));
                        }
                        if w[1] < sweep - 1e-9 {
                            kept.push((ang(w[1]), ang(sweep), ccw));
                        }
                        break;
                    }
                }
            }
        }
        if kept.is_empty() {
            return false;
        }
        // Replace the original entity with the remaining arcs.
        self.sketches[si].entities.retain(|x| x.id != eid);
        for (g0, g1, accw) in kept {
            let pa = self.sketch_point_at(si, cx + r * g0.cos(), cy + r * g0.sin(), 1e-6);
            let pb = self.sketch_point_at(si, cx + r * g1.cos(), cy + r * g1.sin(), 1e-6);
            let id = self.alloc_id();
            self.sketches[si].entities.push(SketchEntity { id, kind: EntityKind::Arc { center: center_id, a: pa, b: pb, ccw: accw }, construction: con });
            // The arc endpoints are anchored to the crossing entities, so a cut point does not drift along the
            // angle on the next solve.
            self.anchor_cut_point(si, pa);
            self.anchor_cut_point(si, pb);
        }
        self.merge_close_points(si, 1e-3); // Stitch coincident cut points so the pieces stay connected.
        self.prune_orphan_sketch_points(si); // Leave no dangling points behind a trim.
        self.solve_sketch(si); // the edit ends at the solver, not at a rebuild: `regen_sketch` recomputes the contours and checks no constraint
        true
    }
    /// Break a circle or an arc at the clicked point: an arc becomes two arcs, a circle becomes two half arcs
    /// (split at the click and at the opposite point). Lines are handled by `break_line`. Returns whether it
    /// succeeded.
    pub fn break_curve(&mut self, si: usize, eid: Id, clickx: f64, clicky: f64) -> bool {
        self.cut_projection(si, eid, |p| p.break_curve_here(si, eid, clickx, clicky))
    }
    pub(crate) fn break_curve_here(&mut self, si: usize, eid: Id, clickx: f64, clicky: f64) -> bool {
        // an ellipse, an arc of one, a spline: cut where it was clicked (`break_piece`)
        if !self.sketches.get(si).and_then(|s| s.entities.iter().find(|e| e.id == eid)).is_some_and(|e| round_entity(s_of(self, si), e).is_some()) {
            return self.break_piece(si, eid, Point2::new(clickx, clicky));
        }
        let Some(s) = self.sketches.get(si) else { return false };
        let Some(e) = s.entities.iter().find(|e| e.id == eid) else { return false };
        let con = e.construction;
        let Some(Round { center_id, cx, cy, r, span }) = round_entity(s, e) else { return false };
        let ca = (clicky - cy).atan2(clickx - cx);
        // the first piece is the curve it was, as a line broken in two (`break_line_here`)
        let mut first = Some(eid);
        let mut mk = |me: &mut Self, g0: f64, g1: f64, ccw: bool| {
            let pa = me.sketch_point_at(si, cx + r * g0.cos(), cy + r * g0.sin(), 1e-6);
            let pb = me.sketch_point_at(si, cx + r * g1.cos(), cy + r * g1.sin(), 1e-6);
            let id = first.take().unwrap_or_else(|| me.alloc_id());
            me.sketches[si].entities.push(SketchEntity { id, kind: EntityKind::Arc { center: center_id, a: pa, b: pb, ccw }, construction: con });
        };
        self.sketches[si].entities.retain(|x| x.id != eid);
        match span {
            None => {
                // A circle becomes two arcs: [click, click + pi] and [click + pi, click].
                let opp = ca + std::f64::consts::PI;
                mk(self, ca, opp, true);
                mk(self, opp, ca, true);
            }
            Some((a0, a1, ccw)) => {
                // An arc becomes two arcs at the clicked point.
                mk(self, a0, ca, ccw);
                mk(self, ca, a1, ccw);
            }
        }
        self.merge_close_points(si, 1e-3); // Stitch coincident cut points so the pieces stay connected.
        self.prune_orphan_sketch_points(si); // Leave no dangling points behind a trim.
        self.solve_sketch(si); // the edit ends at the solver, not at a rebuild: `regen_sketch` recomputes the contours and checks no constraint
        true
    }
    /// WHERE A POINT STANDS in the drawing, or `None` where the sketch has no such point.
    pub fn point_xy(&self, si: usize, id: Id) -> Option<(f64, f64)> {
        let s = self.sketches.get(si)?;
        s.point(id).map(|p| (p.x, p.y))
    }
}
