//! A CURVE AS A CONSTRAINT REFERS TO IT (`CurveRef`), and the reference a curve of each kind is taken by.
use super::*;

/// A CURVE AS A CONSTRAINT SEES IT: by the points it stands on, so the solver moves the curve by moving them - a line by
/// its ends, a circle or an arc by its centre (its radius is the variable of that centre), an ellipse by its centre and
/// the ends of its axes, a spline by its nodes with the handles it has.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CurveRef {
    /// a point, where a dimension is measured from a point to a curve
    Point {
        p: Id,
    },
    Line {
        a: Id,
        b: Id,
    },
    Round {
        c: Id,
    },
    /// an arc as it is drawn, between its ends, where a dimension measures to the arc itself and not its circle
    Arc {
        c: Id,
        a: Id,
        b: Id,
        winding: crate::feature::Winding,
    },
    Oval {
        c: Id,
        ma: Id,
        mi: Id,
    },
    Spline {
        nodes: Vec<Id>,
        handles: Vec<Handle>,
        ends: crate::feature::Ends,
    },
}

impl CurveRef {
    /// The points the curve stands on.
    pub fn points(&self) -> Vec<Id> {
        match self {
            CurveRef::Point { p } => vec![*p],
            CurveRef::Line { a, b } => vec![*a, *b],
            CurveRef::Round { c } => vec![*c],
            CurveRef::Arc { c, a, b, .. } => vec![*c, *a, *b],
            CurveRef::Oval { c, ma, mi } => vec![*c, *ma, *mi],
            CurveRef::Spline { nodes, .. } => nodes.clone(),
        }
    }

    /// Every point the curve stands on, handed to `f` to read or change.
    pub fn each_point_mut(&mut self, mut f: impl FnMut(&mut Id)) {
        match self {
            CurveRef::Point { p } => f(p),
            CurveRef::Line { a, b } => {
                f(a);
                f(b);
            }
            CurveRef::Round { c } => f(c),
            CurveRef::Arc { c, a, b, .. } => {
                f(c);
                f(a);
                f(b);
            }
            CurveRef::Oval { c, ma, mi } => {
                f(c);
                f(ma);
                f(mi);
            }
            CurveRef::Spline { nodes, .. } => nodes.iter_mut().for_each(f),
        }
    }
}

/// THE CURVE OF AN ENTITY AS A CONSTRAINT SEES IT: a line by its ends, a circle or an arc by its centre, an ellipse or an
/// arc of one by its frame - whole, as a constraint ties to the whole curve - a spline by its nodes and handles.
pub fn curve_ref_of(e: &SketchEntity) -> CurveRef {
    match &e.kind {
        EntityKind::Line { a, b } => CurveRef::Line { a: *a, b: *b },
        EntityKind::Circle { center, .. } | EntityKind::Arc { center, .. } => CurveRef::Round { c: *center },
        EntityKind::Ellipse { c, ma, mi, .. } => CurveRef::Oval { c: *c, ma: *ma, mi: *mi },
        EntityKind::Spline { nodes, handles, ends } => CurveRef::Spline { nodes: nodes.clone(), handles: handles.clone(), ends: *ends },
    }
}

/// THE CURVE OF AN ENTITY AS IT IS DRAWN, for a dimension measuring to it: an arc between its ends, any other as a
/// constraint sees it (`curve_ref_of`).
pub fn curve_ref_drawn(e: &SketchEntity) -> CurveRef {
    match &e.kind {
        EntityKind::Arc { center, a, b, ccw } => CurveRef::Arc { c: *center, a: *a, b: *b, winding: if *ccw { crate::feature::Winding::Ccw } else { crate::feature::Winding::Cw } },
        _ => curve_ref_of(e),
    }
}
