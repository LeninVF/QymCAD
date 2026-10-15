//! WHERE THE OTHER CURVES OF A SKETCH MEET A CURVE OF IT, and the trim, the break and the extension of a curve of any kind
//! built on that and on the pieces of a curve (`Curve::piece`, `Project::cut_curve`).

use super::*;
use crate::model::curve::{Curve, End};

/// A PLACE WHERE ANOTHER CURVE MEETS A CURVE: the place along it (`Curve` places, 0 to 1), the point, and the curve met.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crossing {
    pub t: f64,
    pub at: Point2,
    pub by: Id,
}

impl Project {
    /// THE PLACES ON `curve` WHERE THE CURVES OF SKETCH `si` MEET IT - construction geometry included, a boundary as
    /// well - but entity `skip`, along it in order.
    pub(crate) fn meetings(&self, si: usize, curve: &Curve, skip: Id) -> Vec<Crossing> {
        let Some(s) = self.sketches.get(si) else { return Vec::new() };
        let mut out: Vec<Crossing> = s
            .entities
            .iter()
            .filter(|e| e.id != skip)
            .filter_map(|e| Curve::of(s, e).map(|c| (e.id, c)))
            .flat_map(|(id, other)| curve.meet(&other).into_iter().map(move |m| Crossing { t: m.ta, at: m.at, by: id }))
            .collect();
        out.sort_by(|a, b| a.t.total_cmp(&b.t));
        out
    }

    /// WHETHER CURVE `eid` OF SKETCH `si` IS A STRAIGHT LINE.
    pub fn is_straight(&self, si: usize, eid: Id) -> bool {
        let Some(s) = self.sketches.get(si) else { return false };
        matches!(s.entities.iter().find(|e| e.id == eid).and_then(|e| Curve::of(s, e)), Some(Curve::Line { .. }))
    }

    /// THE PLACES ON CURVE `eid` OF SKETCH `si` WHERE ITS OTHER CURVES MEET IT, in order along it.
    pub fn crossings_on(&self, si: usize, eid: Id) -> Vec<Crossing> {
        let Some(s) = self.sketches.get(si) else { return Vec::new() };
        let Some(c) = s.entities.iter().find(|e| e.id == eid).and_then(|e| Curve::of(s, e)) else { return Vec::new() };
        self.meetings(si, &c, eid)
    }

    /// TRIM CURVE `eid` OF ANY KIND AT THE CLICK: the piece under the click, between the two crossings on either side of
    /// it - or an end - goes, the rest stays whole: an open curve keeps the pieces before and after, a closed one the one
    /// piece round from the far crossing to the near one. A curve crossed nowhere, or a closed one crossed once, goes
    /// whole.
    pub(crate) fn trim_piece(&mut self, si: usize, eid: Id, click: Point2) -> bool {
        let Some(s) = self.sketches.get(si) else { return false };
        let Some(c) = s.entities.iter().find(|e| e.id == eid).and_then(|e| Curve::of(s, e)) else { return false };
        let mut cuts: Vec<f64> = self.crossings_on(si, eid).into_iter().map(|x| x.t).collect();
        cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
        let tc = c.nearest(click).t;
        if c.closed() {
            if cuts.len() < 2 {
                return self.trim_away_whole(si, eid);
            }
            // the crossings either side of the click, round the start where the click lies past the last or before the first
            let after = cuts.iter().copied().find(|t| *t > tc).unwrap_or(cuts[0] + 1.0);
            let before = cuts.iter().copied().rev().find(|t| *t <= tc).unwrap_or(cuts[cuts.len() - 1] - 1.0);
            let pieces = self.cut_curve(si, eid, &[before.rem_euclid(1.0), after.rem_euclid(1.0)]);
            // the piece holding the click is the one from `before`: pieces come in order of their starts round the curve
            let gone = pieces.iter().copied().find(|id| self.piece_holds(si, *id, click));
            if let Some(g) = gone {
                self.delete_entities(si, &[g]);
            }
        } else {
            cuts.retain(|t| *t > 1e-9 && *t < 1.0 - 1e-9);
            if cuts.is_empty() {
                return self.trim_away_whole(si, eid);
            }
            let lo = cuts.iter().copied().rev().find(|t| *t <= tc);
            let hi = cuts.iter().copied().find(|t| *t > tc);
            let places: Vec<f64> = lo.into_iter().chain(hi).collect();
            let pieces = self.cut_curve(si, eid, &places);
            if let Some(g) = pieces.iter().copied().find(|id| self.piece_holds(si, *id, click)) {
                self.delete_entities(si, &[g]);
            }
        }
        self.merge_close_points(si, 1e-3);
        self.prune_orphan_sketch_points(si);
        self.solve_sketch(si);
        true
    }

    /// Whether piece `eid` is the one of its curve nearest to `click` - the piece a click there trims.
    fn piece_holds(&self, si: usize, eid: Id, click: Point2) -> bool {
        let Some(s) = self.sketches.get(si) else { return false };
        let off = |e: &SketchEntity| Curve::of(s, e).map_or(f64::MAX, |c| c.nearest(click).off);
        let Some(me) = s.entities.iter().find(|e| e.id == eid) else { return false };
        let mine = off(me);
        // among the curves of the sketch, none nearer to the click
        s.entities.iter().all(|e| e.id == eid || off(e) >= mine)
    }

    /// BREAK CURVE `eid` OF ANY KIND AT THE CLICK: an open curve into two at the place nearest the click, a closed one
    /// into two halves - at the click and opposite it.
    pub(crate) fn break_piece(&mut self, si: usize, eid: Id, click: Point2) -> bool {
        let Some(s) = self.sketches.get(si) else { return false };
        let Some(c) = s.entities.iter().find(|e| e.id == eid).and_then(|e| Curve::of(s, e)) else { return false };
        let t = c.nearest(click).t;
        let places = if c.closed() { vec![t, (t + 0.5).rem_euclid(1.0)] } else { vec![t] };
        if self.cut_curve(si, eid, &places).is_empty() {
            return false;
        }
        self.merge_close_points(si, 1e-3);
        self.prune_orphan_sketch_points(si);
        self.solve_sketch(si);
        true
    }

    /// EXTEND CURVE `eid` OF ANY KIND from its end nearer the click to the nearest curve on its way: an arc of an ellipse
    /// round its ellipse, a spline by a straight span along the tangent of its end. The end of an arc moves along its
    /// curve; a spline gains a node. `false` for a closed curve or when its way meets nothing.
    pub(crate) fn extend_piece(&mut self, si: usize, eid: Id, click: Point2) -> bool {
        let Some(s) = self.sketches.get(si) else { return false };
        let Some(e) = s.entities.iter().find(|e| e.id == eid) else { return false };
        let Some(c) = Curve::of(s, e) else { return false };
        if c.closed() {
            return false;
        }
        let [start, finish] = c.ends();
        let end = if click.dist(start) < click.dist(finish) { End::Start } else { End::Finish };
        // the longest the curve could get is its stretch to the nearest curve met on its way
        let longer = s.entities.iter().filter(|o| o.id != eid).filter_map(|o| Curve::of(s, o)).filter_map(|o| c.stretched(end, &o)).min_by(|x, y| x.length().total_cmp(&y.length()));
        let Some(longer) = longer else { return false };
        let reached = if end == End::Start { longer.at(0.0) } else { longer.at(1.0) };
        let kind = e.kind.clone();
        match kind {
            EntityKind::Spline { mut nodes, handles: _, ends } => {
                let Curve::Spline { tangents, .. } = &longer else { return false };
                let id = self.alloc_id();
                self.sketches[si].points.push(SketchPoint { id, x: reached.x, y: reached.y });
                if end == End::Start {
                    nodes.insert(0, id);
                } else {
                    nodes.push(id);
                }
                let handles = tangents.iter().map(|t| Handle { arrive: Some([t.arrive.x, t.arrive.y]), leave: Some([t.leave.x, t.leave.y]) }).collect();
                if let Some(x) = self.sketches[si].entities.iter_mut().find(|x| x.id == eid) {
                    x.kind = EntityKind::Spline { nodes, handles, ends };
                }
            }
            _ => {
                // the end point of the curve - its first or its last point along it - moves to where it reaches
                let Some(end_id) = self.end_point_of(si, eid, end) else { return false };
                if let Some(q) = self.sketches[si].points.iter_mut().find(|q| q.id == end_id) {
                    q.x = reached.x;
                    q.y = reached.y;
                }
            }
        }
        self.solve_sketch(si);
        true
    }

    /// THE POINT AT `end` OF CURVE `eid`: the end of the curve standing there, among the points it stands on.
    fn end_point_of(&self, si: usize, eid: Id, end: End) -> Option<Id> {
        let s = self.sketches.get(si)?;
        let e = s.entities.iter().find(|e| e.id == eid)?;
        let c = Curve::of(s, e)?;
        let at = if end == End::Start { c.at(0.0) } else { c.at(1.0) };
        entity_points(e).into_iter().filter_map(|id| s.point(id).map(|p| (id, Point2::new(p.x, p.y)))).find(|(_, p)| p.dist(at) < 1e-9).map(|(id, _)| id)
    }
}
