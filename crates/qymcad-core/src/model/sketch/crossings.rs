//! WHERE THE OTHER CURVES OF A SKETCH MEET A CURVE OF IT, and the trim, the break and the extension of a curve of any kind
//! built on that and on the pieces of a curve (`Curve::piece`, `Project::cut_curve`).

use super::*;
use crate::model::curve::{Curve, End};

/// HOW A CURVE IS STRETCHED: the end it grows from, and the curve it becomes.
struct Stretch {
    end: End,
    curve: Curve,
}

/// WHAT TRIM DOES AT A CLICK: the curve, and the run of it that goes - `None` when the whole of it goes.
struct TrimCut {
    curve: Curve,
    run: Option<[f64; 2]>,
}

/// ONE CURVE OF A CROSSING TO CUT: the entity, its curve, the curve it crosses, and the crossing.
struct CutThrough<'a> {
    eid: Id,
    me: &'a Curve,
    them: &'a Curve,
    at: Point2,
}

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

    /// THE CROSSING OF TWO CURVES NEAREST `at`, within `reach` of it: two curves of the drawing (not construction) going
    /// through each other where neither has a point the other ends at - or one ending on the middle of the other.
    /// Where both end there, they already share a corner and there is no crossing to find. Only the curves passing
    /// within `reach` of `at` are asked where they meet.
    pub fn crossing_near(&self, si: usize, at: Point2, reach: f64) -> Option<LinesCrossing> {
        let s = self.sketches.get(si)?;
        let near: Vec<Id> = s.entities.iter().filter(|e| !e.construction).filter(|e| Curve::of(s, e).is_some_and(|c| c.nearest(at).off <= reach)).map(|e| e.id).collect();
        near.iter()
            .enumerate()
            .flat_map(|(i, &one)| near[i + 1..].iter().filter_map(move |&other| self.crossing_of(si, [one, other], at)))
            .filter(|c| c.at.dist(at) <= reach)
            .min_by(|x, y| x.at.dist(at).total_cmp(&y.at.dist(at)))
    }

    /// WHERE CURVES `one` AND `other` CROSS, the crossing nearest `near` of those they have: through each other, or one
    /// ending on the middle of the other. Curves that both end there make a corner, not a crossing.
    pub fn crossing_of(&self, si: usize, [one, other]: [Id; 2], near: Point2) -> Option<LinesCrossing> {
        let s = self.sketches.get(si)?;
        let curve = |id: Id| s.entities.iter().find(|e| e.id == id).and_then(|e| Curve::of(s, e));
        let (a, b) = (curve(one)?, curve(other)?);
        let inside = |c: &Curve, t: f64| c.closed() || (t > 1e-9 && t < 1.0 - 1e-9);
        a.meet(&b).into_iter().filter(|m| inside(&a, m.ta) || inside(&b, m.tb)).min_by(|x, y| x.at.dist(near).total_cmp(&y.at.dist(near))).map(|m| LinesCrossing { one, other, at: m.at })
    }

    /// THE TWO CURVES OF `crossing` CUT WHERE THEY CROSS, each that goes on past it: a line in two (`break_line`), any
    /// other open curve into its two pieces there, a closed one at every place it meets the other curve - cut at the one
    /// place alone it would be a single piece both of whose ends stand there. The halves share one point at the crossing:
    /// the corners of a crossing are then corners of the drawing, rounded and cut as any are. Answers that point.
    pub fn cut_at_crossing(&mut self, si: usize, crossing: LinesCrossing) -> Option<Id> {
        let LinesCrossing { one, other, at } = crossing;
        let s = self.sketches.get(si)?;
        let curve = |id: Id| s.entities.iter().find(|e| e.id == id).and_then(|e| Curve::of(s, e));
        let (a, b) = (curve(one)?, curve(other)?);
        if !(self.cut_through(si, CutThrough { eid: one, me: &a, them: &b, at }) && self.cut_through(si, CutThrough { eid: other, me: &b, them: &a, at })) {
            return None;
        }
        self.merge_close_points(si, 1e-9);
        let s = self.sketches.get(si)?;
        // the point the halves end at: a point of the origin may stand on the same place, and is not stitched to them
        let half_end = |p: Id| s.entities.iter().any(|e| !e.construction && entity_ends(e).is_some_and(|ends| ends.contains(&p)));
        s.points.iter().filter(|q| (q.x - at.x).hypot(q.y - at.y) < 1e-6).map(|q| q.id).find(|&p| half_end(p))
    }

    /// ONE CURVE OF A CROSSING CUT THERE, where it goes on past it: a line in two, any other open curve into its two
    /// pieces, a closed one at every place it meets the other. Answers false where a closed curve meets the other at one
    /// place alone.
    fn cut_through(&mut self, si: usize, cut: CutThrough) -> bool {
        let CutThrough { eid, me, them, at } = cut;
        if !me.closed() && me.ends().iter().any(|p| p.dist(at) < 1e-9) {
            return true;
        }
        if matches!(me, Curve::Line { .. }) {
            self.break_line(si, eid, at.x, at.y);
        } else if me.closed() {
            let places: Vec<f64> = me.meet(them).iter().map(|m| m.ta).collect();
            if places.len() < 2 {
                return false;
            }
            self.cut_curve(si, eid, &places);
        } else {
            self.cut_curve(si, eid, &[me.nearest(at).t]);
        }
        true
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

    /// WHAT TRIM DOES AT A CLICK ON CURVE `eid`: the curve, and the run of it under the click between the two crossings on
    /// either side of it - or an end - which goes; `None` for the run when the whole curve goes, crossed nowhere, or a
    /// closed one crossed once. The run of a closed curve may pass its start: its end is then past 1.
    fn trim_cut(&self, si: usize, eid: Id, click: Point2) -> Option<TrimCut> {
        let s = self.sketches.get(si)?;
        let curve = s.entities.iter().find(|e| e.id == eid).and_then(|e| Curve::of(s, e))?;
        let mut cuts: Vec<f64> = self.crossings_on(si, eid).into_iter().map(|x| x.t).collect();
        cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
        let tc = curve.nearest(click).t;
        let run = if curve.closed() {
            (cuts.len() >= 2).then(|| {
                // the crossings either side of the click, round the start where the click lies past the last or before the
                // first
                let after = cuts.iter().copied().find(|t| *t > tc).unwrap_or(cuts[0] + 1.0);
                let before = cuts.iter().copied().rev().find(|t| *t <= tc).unwrap_or(cuts[cuts.len() - 1] - 1.0);
                let from = before.rem_euclid(1.0);
                [from, from + (after - before)]
            })
        } else {
            cuts.retain(|t| *t > 1e-9 && *t < 1.0 - 1e-9);
            (!cuts.is_empty()).then(|| [cuts.iter().copied().rev().find(|t| *t <= tc).unwrap_or(0.0), cuts.iter().copied().find(|t| *t > tc).unwrap_or(1.0)])
        };
        Some(TrimCut { curve, run })
    }

    /// THE PIECE OF CURVE `eid` A TRIM AT THE CLICK TAKES AWAY - the whole curve when it goes whole. What the preview of
    /// the trim draws, from the places the trim cuts at.
    pub fn trim_span(&self, si: usize, eid: Id, click: Point2) -> Option<Curve> {
        let TrimCut { curve, run } = self.trim_cut(si, eid, click)?;
        Some(match run {
            Some(run) => curve.piece(run),
            None => curve,
        })
    }

    /// TRIM CURVE `eid` OF ANY KIND AT THE CLICK: the piece under the click, between the two crossings on either side of
    /// it - or an end - goes, the rest stays whole: an open curve keeps the pieces before and after, a closed one the one
    /// piece round from the far crossing to the near one. A curve crossed nowhere, or a closed one crossed once, goes
    /// whole.
    pub(crate) fn trim_piece(&mut self, si: usize, eid: Id, click: Point2) -> bool {
        let Some(TrimCut { curve, run }) = self.trim_cut(si, eid, click) else { return false };
        let Some([from, to]) = run else { return self.trim_away_whole(si, eid) };
        // an open curve is cut where the run starts and ends inside it; a closed one at both, round its start
        let places: Vec<f64> = if curve.closed() { vec![from, to.rem_euclid(1.0)] } else { [from, to].into_iter().filter(|t| *t > 1e-9 && *t < 1.0 - 1e-9).collect() };
        let pieces = self.cut_curve(si, eid, &places);
        // the piece holding the click is the one that goes
        if let Some(g) = pieces.iter().copied().find(|id| self.piece_holds(si, *id, click)) {
            self.delete_entities(si, &[g]);
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

    /// WHERE A BREAK AT THE CLICK CUTS CURVE `eid`: an open curve at the place nearest the click - a line no nearer its
    /// ends than a twentieth of it - a closed one there and opposite it, into two halves. What the preview of the break
    /// draws, and where the break cuts.
    pub fn break_at(&self, si: usize, eid: Id, click: Point2) -> Vec<f64> {
        let Some(s) = self.sketches.get(si) else { return Vec::new() };
        let Some(c) = s.entities.iter().find(|e| e.id == eid).and_then(|e| Curve::of(s, e)) else { return Vec::new() };
        let t = c.nearest(click).t;
        match c {
            Curve::Line { .. } => vec![t.clamp(0.05, 0.95)],
            _ if c.closed() => vec![t, (t + 0.5).rem_euclid(1.0)],
            _ => vec![t],
        }
    }

    /// BREAK CURVE `eid` OF ANY KIND AT THE CLICK, where `break_at` says.
    pub(crate) fn break_piece(&mut self, si: usize, eid: Id, click: Point2) -> bool {
        let places = self.break_at(si, eid, click);
        if places.is_empty() || self.cut_curve(si, eid, &places).is_empty() {
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
    /// HOW AN EXTENSION AT THE CLICK STRETCHES CURVE `eid`: its end nearer the click, and the curve stretched from it to
    /// the nearest curve on its way. `None` for a closed curve or when its way meets nothing.
    fn stretch_at(&self, si: usize, eid: Id, click: Point2) -> Option<Stretch> {
        let s = self.sketches.get(si)?;
        let c = Curve::of(s, s.entity(eid)?)?;
        if c.closed() {
            return None;
        }
        let [start, finish] = c.ends();
        let end = if click.dist(start) < click.dist(finish) { End::Start } else { End::Finish };
        // the longest the curve could get is its stretch to the nearest curve met on its way
        let curve = s.entities.iter().filter(|o| o.id != eid).filter_map(|o| Curve::of(s, o)).filter_map(|o| c.stretched(end, &o)).min_by(|x, y| x.length().total_cmp(&y.length()))?;
        Some(Stretch { end, curve })
    }

    /// CURVE `eid` AS AN EXTENSION AT THE CLICK LEAVES IT, stretched from its end nearer the click to the nearest curve on
    /// its way. What the preview of the extension draws.
    pub fn extended(&self, si: usize, eid: Id, click: Point2) -> Option<Curve> {
        self.stretch_at(si, eid, click).map(|s| s.curve)
    }

    pub(crate) fn extend_piece(&mut self, si: usize, eid: Id, click: Point2) -> bool {
        let Some(Stretch { end, curve: longer }) = self.stretch_at(si, eid, click) else { return false };
        let Some(e) = self.sketches.get(si).and_then(|s| s.entity(eid)) else { return false };
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
