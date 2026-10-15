//! THE CURVE UNDER THE POINTER IS THE ONE CUT: Trim takes the curve nearest the pointer, of whatever kind - a spline under
//! the pointer is trimmed, though a line runs within the grab of a curve beside it.
//!
//! Reported behaviour: a line within the grab radius was taken before a spline under the pointer, and the click cut the
//! line nobody pointed at.
use crate::gui::hand::Hand;
use crate::gui::App;
use qymcad_core::feature::SketchPlane;
use qymcad_core::model::EntityKind;

#[test]
fn trim_takes_the_spline_under_the_pointer_and_not_the_line_beside_it() {
    let mut app = App::default();
    let si = app.create_sketch_on(SketchPlane::default());
    app.enter_sketch_edit(si);
    let mut hand = Hand::new(&mut app);
    // a spline over (30, 31), and a line along y = 30 a millimetre - 6 px - below it, within the grab of a curve (8 px)
    hand.sk_tool(9).click2d(20.0, 32.0).click2d(30.0, 31.0).double_click2d(40.0, 32.0);
    hand.key(egui::Key::Escape).key(egui::Key::Escape);
    hand.sk_tool(1).click2d(10.0, 30.0).double_click2d(50.0, 30.0);
    hand.key(egui::Key::Escape).key(egui::Key::Escape);
    assert!(hand.press_hint(&qymcad_i18n::tr("tb-trim-hint")), "no trim button");
    hand.close_window();
    hand.click2d(30.0, 31.0);
    let kinds: Vec<&str> = hand.app.project.sketches[si]
        .entities
        .iter()
        .map(|e| match e.kind {
            EntityKind::Line { .. } => "line",
            EntityKind::Spline { .. } => "spline",
            _ => "other",
        })
        .collect();
    let status = hand.app.status.clone();
    assert_eq!(kinds, ["line"], "the sketch holds {kinds:?} after the trim under the pointer on the spline; the status line says {status:?}");
}
