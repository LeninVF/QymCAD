//! THE CONSTRAINTS of a sketch: laid, refused, offered on the bar.

use qymcad_ui_state::Notice;
use super::*;

/// THE ENTITIES A CONSTRAINT TOUCHES, derived from its points.
///
/// The list of constraints used to name only THE KIND: horizontal, horizontal, vertical, vertical - four
/// identical rows. On a sketch with thirty constraints such a list is useless: it shows that constraints
/// exist but gives no way to find the one wanted. Highlighting on hover already existed, but it answers
/// "where is this one" rather than "which one do I need".
pub fn constraint_parts(dc: &qymcad_ui_state::DrawCtx, si: usize, c: &qymcad_core::model::Constraint) -> Vec<String> {
    let Some(s) = dc.project.sketches.get(si) else { return Vec::new() };
    let owner = |pid: Id| -> Option<Id> { s.entities.iter().find(|e| qymcad_core::model::entity_points(e).contains(&pid)).map(|e| e.id) };
    let mut out: Vec<String> = Vec::new();
    for pid in c.points() {
        if let Some(eid) = owner(pid) {
            let name = sketch_entity_name(dc.project, si, eid);
            if !name.is_empty() && !out.contains(&name) {
                out.push(name);
            }
        }
    }
    out
}

pub fn try_constraint_inner(
    project: &mut Project,
    regen: &mut qymcad_ui_state::Rebuilding,
    sel: qymcad_ui_state::Sel,
    sel_sk: &mut qymcad_ui_state::SketchSelection,
    status: &mut qymcad_ui_state::Notice,
    code: u8,
) -> bool {
    use qymcad_core::model::Constraint;
    let qymcad_ui_state::Sel::Sketch(si) = sel else { return false };
    middles_for(project, si, sel_sk, code);
    let pts = qymcad_ui_state::sel_point_ids(sel_sk);
    let mut lines = qymcad_ui_state::sel_line_pts(project, sel_sk, si);
    // the lines drawn, before the axes join them: Vertical and Horizontal go on these, an axis already stands so
    let drawn = lines.clone();
    // coordinate axes picked as lines (kind 3): their straight lines are materialised and used as lines
    let axes: Vec<u64> = sel_sk.items.iter().filter(|(k, _)| *k == 3).map(|(_, id)| *id).collect();
    for w in axes {
        let (o, d) = project.ensure_axis(si, w as usize);
        lines.push((o, d));
    }
    // THE CURVES PICKED, read as what a constraint ties: the centres of the round ones and of the ellipses, the frames of
    // the ellipses, the major axes as the straights of an ellipse - horizontal, vertical, parallel and square go on them
    let picked: Vec<qymcad_core::model::SketchEntity> = project.sketches[si].entities.iter().filter(|e| sel_sk.items.iter().any(|&(k, id)| k == 1 && id == e.id)).cloned().collect();
    let centres: Vec<Id> = picked.iter().filter_map(qymcad_core::model::entity_centre).collect();
    let frames: Vec<[Id; 3]> = picked.iter().filter_map(qymcad_core::model::ellipse_frame).collect();
    let axes: Vec<[Id; 2]> = frames.iter().map(|&[c, ma, _]| [c, ma]).collect();
    let straights: Vec<[Id; 2]> = lines.iter().map(|&(a, b)| [a, b]).chain(axes.iter().copied()).collect();
    let mut new: Vec<Constraint> = Vec::new();
    match code {
        0 if pts.len() >= 2 => new.push(Constraint::Coincident { a: pts[0], b: pts[1] }),
        // one point coincident with an entity: a circle or an arc gives point-on-circle (which takes
        // priority), otherwise a line or an axis gives point-on-line
        0 if pts.len() == 1 => {
            let cs = qymcad_ui_state::sel_circle_centers(project, sel_sk, si);
            if !cs.is_empty() {
                new.push(Constraint::PointOnCircle { p: pts[0], c: cs[0] });
            } else if let Some(&[c, ma, mi]) = frames.first() {
                new.push(Constraint::PointOnEllipse { p: pts[0], c, ma, mi });
            } else if !lines.is_empty() {
                new.push(Constraint::PointOnLine { p: pts[0], a: lines[0].0, b: lines[0].1 });
            } else if let Some(e) = picked.first() {
                // a spline, which has no equation of its own a point satisfies
                new.push(Constraint::PointOnCurve { p: pts[0], on: qymcad_core::model::curve_ref_of(e) });
            }
        }
        // EVERY SELECTED LINE GETS IT, not the first alone; with no line selected, every point lines up with the first.
        // Reported behaviour (issue #34): with three lines selected Vertical turned one of them.
        1 if !drawn.is_empty() => new.extend(drawn.iter().map(|&(a, b)| Constraint::Horizontal { a, b })),
        1 if !axes.is_empty() => new.extend(axes.iter().map(|&[a, b]| Constraint::Horizontal { a, b })),
        1 if pts.len() >= 2 => new.extend(pts[1..].iter().map(|&b| Constraint::Horizontal { a: pts[0], b })),
        2 if !drawn.is_empty() => new.extend(drawn.iter().map(|&(a, b)| Constraint::Vertical { a, b })),
        2 if !axes.is_empty() => new.extend(axes.iter().map(|&[a, b]| Constraint::Vertical { a, b })),
        2 if pts.len() >= 2 => new.extend(pts[1..].iter().map(|&b| Constraint::Vertical { a: pts[0], b })),
        // between lines: every further line is tied to the first one picked
        3 if straights.len() >= 2 => new.extend(straights[1..].iter().map(|&[c, d]| Constraint::Parallel { a: straights[0][0], b: straights[0][1], c, d })),
        4 if straights.len() >= 2 => new.push(Constraint::Perpendicular { a: straights[0][0], b: straights[0][1], c: straights[1][0], d: straights[1][1] }),
        5 if lines.len() >= 2 => new.extend(lines[1..].iter().map(|&(c, d)| Constraint::Equal { a: lines[0].0, b: lines[0].1, c, d })),
        5 => {
            // equal radii of two circles
            let cs = qymcad_ui_state::sel_circle_centers(project, sel_sk, si);
            if cs.len() >= 2 {
                new.push(Constraint::EqualRadius { c1: cs[0], c2: cs[1] });
            }
        }
        6 if !pts.is_empty() => new.extend(pts.iter().map(|p| Constraint::Fixed { p: *p })),
        7 if lines.len() >= 2 => new.extend(lines[1..].iter().map(|&(c, d)| Constraint::Collinear { a: lines[0].0, b: lines[0].1, c, d })),
        8 => {
            // concentricity is A REAL kind with a glyph of its own, not a fake made of coincident centres; an ellipse has a
            // centre as a circle has
            if centres.len() >= 2 {
                new.push(Constraint::Concentric { c1: centres[0], c2: centres[1] });
            }
        }
        9 => {
            // tangency: a line plus a circle, the line touching it at a point of its own (`touch_point`)
            let rounds = qymcad_ui_state::sel_circle_centers(project, sel_sk, si);
            if let (Some((la, lb)), Some((cc, rr))) = (lines.first().copied(), qymcad_ui_state::sel_circle_cr(project, sel_sk, si)) {
                new.push(Constraint::Tangent { a: la, b: lb, c: cc, r: rr });
                let toward = qymcad_ui_state::sketch_pt(project, si, cc);
                if let Some(p) = toward.and_then(|at| touch_point(project, si, sel_sk, (la, lb), at)) {
                    new.push(Constraint::PointOnCircle { p, c: cc });
                }
            } else if let [c1, c2, ..] = rounds[..] {
                // two circles or arcs touch from outside or from inside, whichever they stand nearer to
                new.push(Constraint::CircleTangent { c1, c2, external: touches_outside(project, si, [c1, c2]) });
            } else if let [one, other, ..] = &picked[..] {
                // any other two curves: an ellipse, a spline, with a line or a circle or one another - touching on
                // themselves, an end put on the other curve where the touching would come past it
                new.extend(project.tangency(si, [one.id, other.id]));
            }
        }
        10 => {
            // symmetry: two points about an axis (the picked line)
            if pts.len() >= 2 {
                if let Some((la, lb)) = lines.first().copied() {
                    new.push(Constraint::Symmetric { a: pts[0], b: pts[1], la, lb });
                }
            }
        }
        11 if pts.len() == 1 && !lines.is_empty() => {
            // midpoint: the point at the middle of the picked line
            new.push(Constraint::Midpoint { p: pts[0], a: lines[0].0, b: lines[0].1 });
        }
        // the middle of an arc: on its circle and as far from both its ends
        11 if pts.len() == 1 => {
            let arc = picked.iter().find_map(|e| e.kind.as_arc()).map(|k| [k.center, k.a, k.b]);
            if let Some([c, a, b]) = arc {
                new.push(Constraint::PointOnCircle { p: pts[0], c });
                new.push(Constraint::Equal { a: pts[0], b: a, c: pts[0], d: b });
                // the two hold at the middle of the arc and at the middle of the rest of its circle: the point is put at
                // the middle of the arc first, and the solve keeps it there
                let middle = picked.iter().find(|e| qymcad_core::model::entity_centre(e) == Some(c)).and_then(|e| qymcad_core::model::curve::Curve::of(&project.sketches[si], e)).map(|k| k.at(0.5));
                if let (Some(m), Some(q)) = (middle, project.sketches[si].points.iter_mut().find(|q| q.id == pts[0])) {
                    (q.x, q.y) = (m.x, m.y);
                }
            }
        }
        _ => {}
    }
    if new.is_empty() {
        return false;
    }
    // WHAT IS HELD ALREADY IS NOT LAID AGAIN: a second Vertical on a line that has one turned both redundant (found
    // checking issue #34). With nothing left to add, the sketch says so instead of "The constraint is added".
    let new = project.independent_of(si, new);
    if new.is_empty() {
        sel_sk.clear();
        *status = Notice::info(qymcad_i18n::tr("sk-constraint-already"));
        return true;
    }
    // the points of what was selected: the relations among them that the new ones make redundant are lifted below
    let among: std::collections::HashSet<Id> = pts.iter().copied().chain(lines.iter().flat_map(|&(a, b)| [a, b])).collect();
    let (had, old_pts): (usize, Vec<(f64, f64)>) = (project.sketches[si].constraints.len(), project.sketches[si].points.iter().map(|p| (p.x, p.y)).collect());
    // A LINE MADE HORIZONTAL OR VERTICAL IS TURNED TO IT FIRST, about its middle (about an end that is held), keeping its
    // length: a line given a direction is turned to it, not squashed onto it. The solve takes the least movement of the points, and bringing both
    // ends to one height is less than turning a steep line: a line at 80 deg given Horizontal came out 5 mm long of 30,
    // and a vertical one, its Vertical deleted, was refused as pulled into a point. Taken back with the rest if the
    // constraint is not kept.
    for c in &new {
        let (line, level) = match *c {
            Constraint::Horizontal { a, b } if drawn.contains(&(a, b)) => ((a, b), Level::Horizontal),
            Constraint::Vertical { a, b } if drawn.contains(&(a, b)) => ((a, b), Level::Vertical),
            _ => continue,
        };
        turn_to_level(project, si, line, level);
    }
    // AN ELLIPSE MADE HORIZONTAL OR VERTICAL IS TURNED TO IT FIRST, about its centre, whole: its axis turned alone would
    // bend it out of its shape
    for c in &new {
        let (axis, level) = match *c {
            Constraint::Horizontal { a, b } if axes.contains(&[a, b]) => ([a, b], Level::Horizontal),
            Constraint::Vertical { a, b } if axes.contains(&[a, b]) => ([a, b], Level::Vertical),
            _ => continue,
        };
        if let Some(e) = picked.iter().find(|e| qymcad_core::model::ellipse_frame(e).is_some_and(|[c, ma, _]| [c, ma] == axis)) {
            turn_ellipse_to_level(project, si, e, level);
        }
    }
    // EQUAL ON TWO CIRCLES THAT EACH CARRY THEIR RADIUS: the second one's radius becomes a reference and follows the
    // first - one radius, not a contradiction. Reported behaviour: the circle tool gives each circle its dimension, and
    // Equal on two of them left the sketch over-defined (residual 2.89).
    let referenced: Vec<usize> = new
        .iter()
        .filter_map(|c| match c {
            Constraint::EqualRadius { c1, c2 } => {
                let driving = |cc: Id| project.sketches[si].constraints.iter().position(|k| matches!(k, Constraint::Diameter { c, driven: false, .. } if *c == cc));
                driving(*c1).and(driving(*c2))
            }
            _ => None,
        })
        .collect();
    for &k in &referenced {
        if let Constraint::Diameter { driven, .. } = &mut project.sketches[si].constraints[k] {
            *driven = true;
        }
    }
    let sizes_before = shape_sizes(project, si);
    project.sketches[si].constraints.extend(new);
    let resid = project.solve_sketch(si);
    // A CONSTRAINT MET BY SHRINKING A SHAPE TO NOTHING IS NOT MET. Horizontal over Vertical on one line is satisfied
    // exactly by a line of no length - residual 0 - and both stood green while the line was gone. Reported behaviour
    // (found checking issue #34): three lines made Vertical, then Horizontal, shrank to points.
    let shrunk = shape_sizes(project, si).iter().zip(&sizes_before).any(|(after, before)| *before > 1e-6 && *after < 1e-6);
    sel_sk.clear(); // the selection and whatever was waiting for it
    qymcad_ui_state::invalidate(regen);
    // A CONSTRAINT THE SKETCH CANNOT MEET IS NOT KEPT, as a dimension that does not fit is not: parallel on two lines an
    // angle of 45 deg holds apart left both in the sketch, unsolved (residual 0.79), with nothing marked. It is taken
    // back, the points return where they were, and the status line says why, at the threshold the dimension editor rolls
    // back at (`SKETCH_FITS`).
    if resid > qymcad_core::model::SKETCH_FITS || shrunk {
        project.sketches[si].constraints.truncate(had);
        for &k in &referenced {
            if let Constraint::Diameter { driven, .. } = &mut project.sketches[si].constraints[k] {
                *driven = false; // taken back with the constraint
            }
        }
        for (p, (x, y)) in project.sketches[si].points.iter_mut().zip(old_pts) {
            p.x = x;
            p.y = y;
        }
        project.solve_sketch(si);
        *status = Notice::refusal(if shrunk { qymcad_i18n::tr("sk-constraint-shrinks") } else { qymcad_i18n::tr1("sk-constraint-conflict", "r", &qymcad_i18n::num(resid, 2)) });
        return true;
    }
    let lifted = project.drop_implied_relations(si, had, &among);
    *status = if resid >= 1e-3 {
        Notice::info(qymcad_i18n::tr1("sk-constraint-added-resid", "r", &qymcad_i18n::num(resid, 2)))
    } else {
        Notice::info(if lifted > 0 { qymcad_i18n::tr1("sk-constraint-added-implied", "n", &lifted.to_string()) } else { qymcad_i18n::tr("sk-constraint-added") })
    };
    true
}

/// WHETHER TWO CIRCLES OR ARCS, by their centres, stand nearer to touching from outside than from inside.
fn touches_outside(project: &Project, si: usize, centres: [Id; 2]) -> bool {
    let s = &project.sketches[si];
    let at = |id: Id| s.point(id).map(|q| Point2::new(q.x, q.y));
    let radius = |c: Id| {
        s.entities
            .iter()
            .filter(|e| qymcad_core::model::entity_centre(e) == Some(c))
            .find_map(|e| qymcad_core::model::curve::Curve::of(s, e))
            .map(|k| k.at(0.0).dist(at(c).unwrap_or(k.at(0.0))))
            .unwrap_or(0.0)
    };
    let [c1, c2] = centres;
    let (Some(p1), Some(p2)) = (at(c1), at(c2)) else { return true };
    let (d, r1, r2) = (p1.dist(p2), radius(c1), radius(c2));
    ((r1 + r2) - d).abs() <= ((r1 - r2).abs() - d).abs()
}

/// TURN ELLIPSE `e` WHOLE ABOUT ITS CENTRE until its major axis lies `level`, the nearer way round.
fn turn_ellipse_to_level(project: &mut Project, si: usize, e: &qymcad_core::model::SketchEntity, level: Level) {
    let Some([c, ma, _]) = qymcad_core::model::ellipse_frame(e) else { return };
    let s = &mut project.sketches[si];
    let at = |s: &qymcad_core::model::Sketch, id: Id| s.point(id).map(|q| Point2::new(q.x, q.y));
    let (Some(pc), Some(pa)) = (at(s, c), at(s, ma)) else { return };
    let now = (pa.y - pc.y).atan2(pa.x - pc.x);
    let quarter = std::f64::consts::FRAC_PI_2;
    let want = match level {
        Level::Horizontal => (now / (2.0 * quarter)).round() * 2.0 * quarter,
        Level::Vertical => ((now - quarter) / (2.0 * quarter)).round() * 2.0 * quarter + quarter,
    };
    let (sin, cos) = (want - now).sin_cos();
    let turned: Vec<Id> = qymcad_core::model::entity_points(e).into_iter().filter(|id| *id != c).collect();
    for q in s.points.iter_mut().filter(|q| turned.contains(&q.id)) {
        let (dx, dy) = (q.x - pc.x, q.y - pc.y);
        (q.x, q.y) = (pc.x + dx * cos - dy * sin, pc.y + dx * sin + dy * cos);
    }
}

/// Apply a geometric constraint to the sketch selection. Returns true when it was applied.
/// `code`: 0 coincident, 1 horizontal, 2 vertical, 3 parallel, 4 perpendicular, 5 equal, 6 fixed,
/// 7 collinear, 8 concentric.
pub fn try_constraint(sk: &mut qymcad_ui_state::SketchCtx, code: u8) -> bool {
    // THE BOUNDARY OF AN OPERATION: placing a constraint is a deliberate act and makes one undo step.
    qymcad_ui_state::begin_edit(sk.edits, sk.project, qymcad_i18n::tr("sk-constraint"));
    let ok = try_constraint_inner(sk.project, sk.regen, *sk.sel, sk.sel_sk, sk.status, code);
    if ok {
        qymcad_ui_state::commit_edit(&mut sk.rebuild());
    } else {
        qymcad_ui_state::abort_edit(&mut sk.rebuild()); // the constraint did not take, so it leaves no trace
    }
    ok
}

/// The constraint button: if the selection is enough, apply it; otherwise go into waiting mode (the
/// button lights up, the elements are picked, Esc cancels).
pub fn constraint_button(sk: &mut qymcad_ui_state::SketchCtx, code: u8) {
    qymcad_ui_state::exit_draw_tools(&mut qymcad_ui_state::tools_in!(sk));
    *sk.armed = qymcad_ui_state::Armed::None;
    sk.sel_sk.constraint = None;
    sk.sel_sk.modify = None;
    let picked = !sk.sel_sk.items.is_empty();
    if try_constraint(sk, code) {
        sk.sel_sk.constraint = None;
    } else {
        // it could not be applied to the current selection, so the picking starts AFRESH, without any
        // elements left stuck: otherwise they would go into the constraint as `pts[0]` or `lines[0]` and
        // tie the wrong things together.
        sk.sel_sk.clear(); // the selection and whatever was waiting for it
        sk.sel_sk.constraint = Some(code);
        // WHAT WAS PICKED DOES NOT TAKE IT, and that is said: two points are not parallel, two lines have no centre.
        // Asking for the geometry again as if nothing had been picked left the person guessing what was wrong.
        *sk.status = if picked {
            Notice::refusal(qymcad_i18n::tr1("sk-constraint-not-for-pick", "takes", &qymcad_i18n::tr(constraint_hint_key(code))))
        } else {
            Notice::info(qymcad_i18n::tr("sk-pick-for-constraint"))
        };
    }
}

/// WHAT A PICKED ITEM IS TO A CONSTRAINT: a point, a straight (a line or an axis), a round curve (a circle or an arc),
/// an ellipse or an arc of one, or a curve of no other kind (a spline).
#[derive(Clone, Copy, PartialEq, Eq)]
enum PickedAs {
    Point,
    Straight,
    Round,
    Oval,
    Other,
}

/// WHETHER WHAT IS PICKED FOR CONSTRAINT `code` COULD TAKE IT, given more picks: every item of a kind the constraint
/// ties. A level or an upright takes a straight, an ellipse or points; a parallel, a square or a collinear straights
/// (and the axis of an ellipse); an equal curves of one class; a concentric round curves, ellipses or their centres; a
/// tangency curves; a fix, a symmetry or a middle points and the line or curve they go on. A level spline can never take
/// one, and the click is answered at once: waiting for more picks said nothing and left the person guessing.
pub fn pick_could_take(project: &Project, si: usize, sel_sk: &qymcad_ui_state::SketchSelection, code: u8) -> bool {
    let Some(s) = project.sketches.get(si) else { return true };
    let kinds: Vec<PickedAs> = sel_sk
        .items
        .iter()
        .map(|&(k, id)| match k {
            0 => PickedAs::Point,
            3 => PickedAs::Straight,
            _ => match s.entity(id).map(|e| &e.kind) {
                Some(kind) if kind.as_line().is_some() => PickedAs::Straight,
                Some(kind) if kind.as_circle().is_some() || kind.as_arc().is_some() => PickedAs::Round,
                Some(kind) if kind.as_ellipse().is_some() => PickedAs::Oval,
                _ => PickedAs::Other,
            },
        })
        .collect();
    let all = |ok: &[PickedAs]| kinds.iter().all(|k| ok.contains(k));
    match code {
        1 | 2 => all(&[PickedAs::Point, PickedAs::Straight, PickedAs::Oval]),
        3 | 4 => all(&[PickedAs::Straight, PickedAs::Oval]),
        5 => {
            let curves: Vec<&PickedAs> = kinds.iter().filter(|k| **k != PickedAs::Point).collect();
            all(&[PickedAs::Straight, PickedAs::Round, PickedAs::Oval]) && curves.windows(2).all(|w| w[0] == w[1])
        }
        6 => all(&[PickedAs::Point]),
        7 => all(&[PickedAs::Straight]),
        8 => all(&[PickedAs::Point, PickedAs::Round, PickedAs::Oval]),
        9 => all(&[PickedAs::Straight, PickedAs::Round, PickedAs::Oval, PickedAs::Other]),
        10 => all(&[PickedAs::Point, PickedAs::Straight]),
        _ => true,
    }
}

/// The words of a constraint's button - its name and what it ties - by its code.
pub fn constraint_hint_key(code: u8) -> &'static str {
    match code {
        0 => "con-coincident-hint",
        1 => "con-horizontal-hint",
        2 => "con-vertical-hint",
        3 => "con-parallel-hint",
        4 => "con-perpendicular-hint",
        5 => "con-equal",
        6 => "con-fix",
        7 => "con-collinear-hint",
        8 => "con-concentric-hint",
        9 => "con-tangent-hint",
        10 => "con-symmetric-hint",
        _ => "con-midpoint-hint",
    }
}
