//! AN ELLIPSE AND A SPLINE ARE FACES OF THEIR OWN: a profile of a sketch goes to the kernel as the curves it is made of,
//! an ellipse one edge, an arc of one one edge, a spline a cubic a span, and the body built from it has one side face for
//! each, of the volume the curves close.
//!
//! Reported behaviour: a profile of an ellipse or a spline went to the kernel as the segments of its points, and the
//! body had a face for every segment instead of one.
use qymcad_core::feature::{Ends, Purpose, Winding};
use qymcad_core::geom::Point2;
use qymcad_core::model::curve::Curve;
use qymcad_core::model::{EllipseArcSpec, Id, Project};

/// THE BODY OF A SKETCH drawn by `draw`, extruded 10: its faces and its volume, and the area of the profile, read off
/// the curves of the sketch laid out finely.
struct Built {
    faces: u32,
    volume: f64,
    area: f64,
}

fn built(draw: fn(&mut Project, usize)) -> Built {
    let mut p = Project::default();
    p.new_document();
    let c = p.add_part("A");
    p.set_active_component(Some(c));
    let si = p.new_sketch("s");
    let sid = p.sketches[si].id;
    p.add_sketch_node(sid, "s");
    draw(&mut p, si);
    p.regen_sketch(si);
    let s = &p.sketches[si];
    // the area of the region the curves close, by the points of the curves laid out to 0.005 mm, in their order
    let path: Vec<Point2> = s.entities.iter().filter_map(|e| Curve::of(s, e)).flat_map(|c| c.polyline()).collect();
    let area = 0.5 * path.iter().zip(path.iter().cycle().skip(1)).map(|(a, b)| a.x * b.y - b.x * a.y).sum::<f64>().abs();
    let body: Id = p.add_extrude(sid, 10.0);
    p.finish_base_body(body, 1);
    let (report, shapes) = qymcad_testkit::regenerate(&mut p);
    assert!(report.errors.is_empty(), "the body did not build: {:?}", report.errors);
    let shape = shapes.get(&body).expect("the body has a shape");
    Built { faces: shape.face_kinds().map_or(0, |k| k.iter().sum()), volume: shape.volume(), area }
}

#[test]
fn an_ellipse_extruded_is_three_faces() {
    let b = built(|p, si| {
        p.add_ellipse_entity(si, Point2::new(0.0, 0.0), 10.0, 5.0, 0.3, Purpose::Real);
    });
    let want = std::f64::consts::PI * 10.0 * 5.0 * 10.0;
    assert_eq!(b.faces, 3, "an ellipse extruded has {} faces, not its side and two ends", b.faces);
    assert!((b.volume - want).abs() < 1e-3 * want, "the volume is {:.3}, pi x 10 x 5 x 10 is {want:.3}", b.volume);
}

#[test]
fn a_closed_spline_of_four_nodes_extruded_is_a_face_a_span() {
    let b = built(|p, si| {
        let nodes = [(12.0, 0.0), (0.0, 7.0), (-10.0, 0.0), (0.0, -6.0)].map(|(x, y)| Point2::new(x, y));
        p.add_spline(si, nodes.to_vec(), Ends::Closed, Purpose::Real);
    });
    assert_eq!(b.faces, 6, "a closed spline of four spans extruded has {} faces, not four and two ends", b.faces);
    assert!((b.volume - b.area * 10.0).abs() < 2e-3 * b.volume, "the volume is {:.3}, the area {:.3} x 10 is {:.3}", b.volume, b.area, b.area * 10.0);
}

#[test]
fn half_an_ellipse_closed_by_a_line_is_four_faces() {
    let b = built(|p, si| {
        let arc = EllipseArcSpec { centre: Point2::new(0.0, 0.0), radii: Point2::new(10.0, 5.0), rot: 0.0, angles: [0.0, std::f64::consts::PI], winding: Winding::Ccw };
        p.add_ellipse_arc_entity(si, arc, Purpose::Real);
        p.add_line_entity(si, -10.0, 0.0, 10.0, 0.0, Purpose::Real);
    });
    let want = std::f64::consts::PI * 10.0 * 5.0 / 2.0 * 10.0;
    assert_eq!(b.faces, 4, "half an ellipse closed by a line extruded has {} faces, not the arc, the line and two ends", b.faces);
    assert!((b.volume - want).abs() < 1e-3 * want, "the volume is {:.3}, half of pi x 10 x 5, x 10, is {want:.3}", b.volume);
}

/// THE WALL BETWEEN AN ELLIPSE AND ITS OFFSET: the ellipse 10 by 5 offset 3 outwards, the ring between them extruded - one
/// face for the ellipse, one for each span of the spline the offset is laid as (`Curve::offset`), and the two ends; the
/// volume the ring between the two curves closes. Laid as the segments of its points, the offset gave a face a segment.
#[test]
fn the_wall_between_an_ellipse_and_its_offset_is_a_face_a_curve_span() {
    let mut spans = 0;
    let mut p = Project::default();
    p.new_document();
    let c = p.add_part("A");
    p.set_active_component(Some(c));
    let si = p.new_sketch("s");
    let sid = p.sketches[si].id;
    p.add_sketch_node(sid, "s");
    p.add_ellipse_entity(si, Point2::new(0.0, 0.0), 10.0, 5.0, 0.3, Purpose::Real);
    let ellipse = p.sketches[si].entities.iter().find(|e| e.kind.as_ellipse().is_some()).map(|e| e.id).expect("the ellipse");
    assert!(p.offset_entities(si, &[ellipse], 3.0) > 0, "setup: the ellipse is not offset");
    p.solve_sketch(si);
    let s = &p.sketches[si];
    for e in &s.entities {
        if let Some(sp) = e.kind.as_spline() {
            spans += sp.nodes.len();
        }
    }
    // the ring: the area inside the offset less the ellipse's
    let area = |c: &Curve| {
        let path = c.polyline();
        0.5 * path.iter().zip(path.iter().cycle().skip(1)).map(|(a, b)| a.x * b.y - b.x * a.y).sum::<f64>().abs()
    };
    let curves: Vec<Curve> = s.entities.iter().filter_map(|e| Curve::of(s, e)).collect();
    let ring = curves.iter().map(area).fold(0.0, f64::max) - curves.iter().map(area).fold(f64::MAX, f64::min);
    // the outer loop, the offset's: its region is it less the ellipse inside it - the ring
    let loops: Vec<Id> = p.sketches[si].contour_ids.clone();
    let outer = loops.iter().copied().max_by(|a, b| {
        let size = |c: Id| p.contour_profile_xy(c).map_or(0.0, |xy| xy.chunks(2).map(|q| q[0].hypot(q[1])).fold(0.0, f64::max));
        size(*a).total_cmp(&size(*b))
    });
    let body: Id = p.add_extrude_on(sid, outer.expect("the loops"), 10.0, qymcad_core::feature::Reach::Forward, 0.0);
    p.finish_base_body(body, 1);
    let (report, shapes) = qymcad_testkit::regenerate(&mut p);
    assert!(report.errors.is_empty(), "the body did not build: {:?}", report.errors);
    let shape = shapes.get(&body).expect("the body has a shape");
    let faces: u32 = shape.face_kinds().map_or(0, |k| k.iter().sum());
    let want = 1 + spans as u32 + 2;
    assert_eq!(faces, want, "the wall has {faces} faces, not the ellipse, the {spans} spans of its offset and two ends");
    assert!((shape.volume() - ring * 10.0).abs() < 2e-3 * ring * 10.0, "the volume is {:.3}, the ring {ring:.3} x 10 is {:.3}", shape.volume(), ring * 10.0);
}
