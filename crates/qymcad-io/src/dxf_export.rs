//! Exporting a sketch to DXF: an R2000 drawing with an ENTITIES section of LINE, CIRCLE, ARC, ELLIPSE and SPLINE, each
//! curve the entity it is - R2000 is the first version a reader of the drawing takes an ELLIPSE and a SPLINE from as
//! plainly as a LINE.
//!
//! DXF runs Y upwards as CAD does, so no inversion is needed. Arcs and ellipses in DXF run counter-clockwise from their
//! start to their end, so a clockwise edge has its ends swapped.
use dxf::entities::{Arc, Circle, Ellipse, Entity, EntityType, Line, Spline};
use dxf::enums::AcadVersion;
use dxf::{Drawing, Point, Vector};
use qymcad_core::geom::{Point2, ProfEdge};

/// Write the curves of a sketch to DXF. An error means there was nothing to export, or the write failed.
pub fn export_dxf(edges: &[ProfEdge], path: &str) -> Result<(), String> {
    if edges.is_empty() {
        return Err("io-dxf-empty-sketch".into());
    }
    let mut drawing = Drawing::new();
    drawing.header.version = AcadVersion::R2000;
    let at = |p: Point2| Point::new(p.x, p.y, 0.0);
    let mut k = 0;
    while k < edges.len() {
        let specific = match edges[k] {
            ProfEdge::Line { a, b } => EntityType::Line(Line::new(at(a), at(b))),
            ProfEdge::Circle { center, r } => EntityType::Circle(Circle::new(at(center), r)),
            ProfEdge::Arc { a, b, center, ccw } => {
                let deg = |p: Point2| (p.y - center.y).atan2(p.x - center.x).to_degrees().rem_euclid(360.0);
                // DXF draws counter-clockwise from start to end; for a clockwise edge the same arc results from swapping them
                let (start, end) = if ccw { (deg(a), deg(b)) } else { (deg(b), deg(a)) };
                EntityType::Arc(Arc::new(at(center), center.dist(a), start, end))
            }
            ProfEdge::Oval { centre, major, minor, from, sweep } => EntityType::Ellipse(ellipse(OvalEdge { centre, major, minor, from, sweep })),
            ProfEdge::Cubic { .. } => {
                // the spans of one spline, joined end to end, are one SPLINE
                let run = edges[k..].iter().zip(edges[k..].iter().skip(1)).take_while(|(x, y)| matches!((x.ends(), y), (Some([_, b]), ProfEdge::Cubic { a, .. }) if b.dist(*a) < 1e-9)).count() + 1;
                let spline = spline(&edges[k..k + run]);
                k += run - 1;
                EntityType::Spline(spline)
            }
        };
        drawing.add_entity(Entity::new(specific));
        k += 1;
    }
    drawing.save_file(path).map_err(|e| format!("io-dxf-write-failed#{e}"))
}

/// AN ELLIPSE EDGE as a sketch gives it: its centre, its semi-axes as vectors, and the run of its angle.
struct OvalEdge {
    centre: Point2,
    major: Point2,
    minor: Point2,
    from: f64,
    sweep: f64,
}

/// The ELLIPSE of an ellipse edge. DXF holds the greater semi-axis as a vector, the lesser as its ratio to it turned a
/// quarter counter-clockwise, and the angles from the start to the end counter-clockwise: an edge whose lesser axis is
/// the greater, or runs a quarter clockwise, or which runs clockwise is turned round to that.
fn ellipse(e: OvalEdge) -> Ellipse {
    let OvalEdge { centre, mut major, mut minor, mut from, mut sweep } = e;
    let len = |v: Point2| v.x.hypot(v.y);
    // the lesser the greater: c + M cos t + N sin t = c + N cos u + (-M) sin u, u = t - pi/2
    if len(minor) > len(major) {
        (major, minor) = (minor, Point2::new(-major.x, -major.y));
        from -= std::f64::consts::FRAC_PI_2;
    }
    // the lesser a quarter clockwise of the greater: c + M cos t + N sin t = c + M cos(-t) + (-N) sin(-t)
    if major.x * minor.y - major.y * minor.x < 0.0 {
        minor = Point2::new(-minor.x, -minor.y);
        (from, sweep) = (-from, -sweep);
    }
    if sweep < 0.0 {
        (from, sweep) = (from + sweep, -sweep);
    }
    let start = from.rem_euclid(std::f64::consts::TAU);
    let whole = sweep >= std::f64::consts::TAU - 1e-12;
    Ellipse {
        center: Point::new(centre.x, centre.y, 0.0),
        major_axis: Vector::new(major.x, major.y, 0.0),
        minor_axis_ratio: len(minor) / len(major),
        start_parameter: if whole { 0.0 } else { start },
        end_parameter: if whole { std::f64::consts::TAU } else { start + sweep },
        ..Default::default()
    }
}

/// The SPLINE of spans of a spline joined end to end: a cubic B-spline whose knots repeat three times between spans, so
/// each span is the Bezier of its four points.
fn spline(spans: &[ProfEdge]) -> Spline {
    let mut control_points = Vec::new();
    for (k, span) in spans.iter().enumerate() {
        let ProfEdge::Cubic { a, c1, c2, b } = *span else { continue };
        let lead = if k == 0 { vec![a] } else { Vec::new() };
        control_points.extend(lead.into_iter().chain([c1, c2, b]).map(|p| Point::new(p.x, p.y, 0.0)));
    }
    let n = spans.len();
    let knot_values = std::iter::repeat_n(0.0, 4).chain((1..n).flat_map(|k| std::iter::repeat_n(k as f64, 3))).chain(std::iter::repeat_n(n as f64, 4)).collect();
    let mut out = Spline { degree_of_curve: 3, control_points, knot_values, ..Default::default() };
    out.set_is_planar(true);
    out
}
