//! A FIGURE COPIED IS A FIGURE: Copy, Mirror and the patterns lay a rectangle, a polygon or a slot taken whole as one -
//! a record of its own on the copied curves, a rectangle with its centre and its diagonals, held by the constraints the
//! figure holds itself by. Copied as plain curves, a mirrored rectangle fell apart under a dragged corner and a copied
//! polygon had nothing holding its corners on its circle.
use super::*;

/// THE SHAPES A COPY TAKES WHOLE: the rectangles and the figures all of whose curves it takes.
#[derive(Clone, Debug, Default)]
pub(crate) struct Whole {
    rects: Vec<SketchRect>,
    figures: Vec<SketchFigure>,
}

impl Whole {
    /// What a copy of the whole shapes carries besides the curves taken: the diagonals and the centre of a rectangle.
    pub(crate) fn carried(&self) -> impl Iterator<Item = Id> + '_ {
        self.rects.iter().flat_map(|r| r.diagonals.into_iter().flatten().chain([r.centre]))
    }
}

/// HOW A COPY WAS LAID: the old point and curve ids to the new ones, and the map of a direction (the linear part of the
/// map the points went through).
pub(crate) struct CopyMaps<'a> {
    pub(crate) points: &'a std::collections::HashMap<Id, Id>,
    pub(crate) curves: &'a std::collections::HashMap<Id, Id>,
    pub(crate) turn: &'a dyn Fn([f64; 2]) -> [f64; 2],
}

/// A CONSTRAINT AS IT HOLDS THE COPY: on the new points, and a turn of a line taken through the map - a mirror or a
/// turn of the copy turns its sides, and a side held at 30 deg mirrored about the Y axis stands at 150 deg.
pub(crate) fn copied_constraint(c: &Constraint, maps: &CopyMaps) -> Constraint {
    let mut nc = remap_constraint_via(c, maps.points);
    if let Constraint::Orientation { deg, .. } = &mut nc {
        let (s, k) = deg.to_radians().sin_cos();
        let [x, y] = (maps.turn)([k, s]);
        *deg = y.atan2(x).to_degrees();
    }
    nc
}

impl Project {
    /// The rectangles and the figures of sketch `si` all of whose curves are among `eids`.
    pub(crate) fn wholes_among(&self, si: usize, eids: &[Id]) -> Whole {
        let Some(s) = self.sketches.get(si) else { return Whole { rects: Vec::new(), figures: Vec::new() } };
        Whole {
            rects: s.rects.iter().filter(|r| r.sides.iter().all(|e| eids.contains(e))).cloned().collect(),
            figures: s.figures.iter().filter(|f| f.curves.iter().all(|e| eids.contains(e))).cloned().collect(),
        }
    }

    /// THE CONSTRAINTS THE COPIES OF THE SHAPES TAKEN WHOLE HOLD THEMSELVES BY: a copy that carries no constraints of the
    /// set (a mirror, a pattern) still carries these, or it is no figure.
    pub(crate) fn copied_own_constraints(&self, si: usize, whole: &Whole, maps: &CopyMaps) -> Vec<Constraint> {
        let Some(s) = self.sketches.get(si) else { return Vec::new() };
        let rects = whole.rects.iter().flat_map(|r| s.constraints.iter().filter(move |c| r.holds(c)));
        rects.chain(whole.figures.iter().flat_map(|f| f.own.iter())).map(|c| copied_constraint(c, maps)).collect()
    }

    /// LAY THE RECORDS OF THE COPIES of the shapes taken whole, on the copied curves and points.
    pub(crate) fn lay_copied_records(&mut self, si: usize, whole: &Whole, maps: &CopyMaps) {
        let p = |id: Id| maps.points.get(&id).copied().unwrap_or(id);
        let e = |id: Id| maps.curves.get(&id).copied().unwrap_or(id);
        for r in &whole.rects {
            let anchor = match r.anchor {
                RectAnchor::Corner(c) => RectAnchor::Corner(p(c)),
                RectAnchor::Centre => RectAnchor::Centre,
            };
            let id = self.alloc_id();
            let copy = SketchRect { id, corners: r.corners.map(p), sides: r.sides.map(e), centre: p(r.centre), diagonals: r.diagonals.map(|d| d.map(e)), anchor };
            self.sketches[si].rects.push(copy);
        }
        for f in &whole.figures {
            let own: Vec<Constraint> = f.own.iter().map(|c| copied_constraint(c, maps)).collect();
            let id = self.alloc_id();
            self.sketches[si].figures.push(SketchFigure { id, shape: f.shape, curves: f.curves.iter().map(|c| e(*c)).collect(), own });
        }
    }
}
