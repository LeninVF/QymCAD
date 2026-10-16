//! THE GUI TESTS, lifted out of `gui.rs`.
//!
//! This is a CHILD module of `gui` rather than a neighbouring file: `App`'s private fields are visible to the
//! owning module and its descendants, so the headless flows keep working with the internal state directly.
// DO NOT REMOVE: rustc calls this import unused, yet without it the file does not compile - it is what lets the
// nested test modules see the `gui` names by the `super::` path (a private import is available to descendants).
#[allow(unused_imports)]
use super::*;

#[cfg(test)]
mod command_flow_tests;

#[cfg(test)]
mod section_clip_tests;

#[cfg(test)]
mod gizmo_ring_sign_tests;

#[cfg(test)]
mod section_drag_tests;

#[cfg(test)]
mod pick_depth_tests;

#[cfg(test)]
mod gizmo_math_tests;

#[cfg(test)]
mod sketch_conflict_ui_tests;

#[cfg(test)]
mod brep_warmup_tests;

#[cfg(test)]
mod sketch_plane_pick_frames_tests;

#[cfg(test)]
mod faces_part_in_light;

#[cfg(test)]
mod the_floor_fades_round_the_view;

#[cfg(test)]
mod parts_far_apart_draw_clean;

#[cfg(test)]
mod the_camera_holds_after_a_far_view;

#[cfg(test)]
pub(in crate::gui) mod a_quick_rebuild_does_not_blink;

#[cfg(test)]
mod nothing_changed_rebuilds_nothing;

#[cfg(test)]
mod a_hinge_follows_its_parameter;

#[cfg(test)]
mod a_cut_through_both_ways_reopens_both_ways;

#[cfg(test)]
mod a_cut_on_a_datum_follows_its_parameter_in_the_window;

#[cfg(test)]
mod a_reference_found_as_the_other_piece_is_shown;

#[cfg(test)]
mod a_reference_lost_by_an_edit_is_shown;

#[cfg(test)]
mod every_tool_takes_every_curve;

#[cfg(test)]
mod two_fillets_meet_on_a_side;

#[cfg(test)]
mod a_figure_is_carried_whole;

#[cfg(test)]
mod a_figure_opens_its_size;

#[cfg(test)]
mod a_figure_shows_no_badge_of_its_own;

#[cfg(test)]
mod a_projected_ellipse_follows_its_body;

#[cfg(test)]
mod a_circle_shows_its_centre_lines;

#[cfg(test)]
mod a_trim_preview_shows_any_curve;

#[cfg(test)]
mod a_curve_under_the_pointer_is_the_one_cut;

#[cfg(test)]
mod a_point_drawn_on_a_curve_is_held_on_it;

#[cfg(test)]
mod a_picked_curve_of_any_kind_is_lit;
