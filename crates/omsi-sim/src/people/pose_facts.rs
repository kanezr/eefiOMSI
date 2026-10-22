//! What the enhanced (procedural) pose of a person reads of the simulation's state: whether
//! they sit, walk, pay or stand, where the seat and the hand reaching out are, how far the
//! seat lifts the model, whether they are steadied against the sway. Taken with the person
//! into the view's snapshot (`crate::view_events::PersonFrame`), so that the posing never
//! reads the simulation itself.

use super::*;
use crate::human::Activity;
use crate::people::pax::Task;

/// See the module.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PoseFacts {
    pub activity: Activity,
    /// The pelvis on the seat, in the person's frame (sitting).
    pub seat: Option<Vec3>,
    /// Where the hand reaches (paying in a bus), in the person's frame.
    pub reach: Option<Vec3>,
    /// The model's `[seatheight]` over the physical seat height (sitting), m.
    pub floor_lift: f32,
    /// 1 for a standing rider, steadied against the sway.
    pub hold: f32,
    /// A seated person's anchor over the floor their feet are sought at: `[seatheight]`
    /// minus the seat's height (m); None: the feet are at the anchor.
    pub seated_level: Option<f64>,
}

/// Only riders standing still at their standing place are steadied against the sway: queued,
/// walking and paying passengers are not.
pub fn grip_for_standing_rider(inside: bool, assigned_standing: bool, walking: bool, reaching: bool) -> f32 {
    if inside && assigned_standing && !walking && !reaching { 1.0 } else { 0.0 }
}

/// A passenger is anchored at a model's [seatheight] below the seat's hip point.
/// Its actual floor is at the cabin [passpos] height below that point, which may differ.
/// Re-anchor the procedural floor and keep the pelvis on the seat's [passpos], not below it.
pub fn seated_pose(model_height: f32, place_height: f32, seat_front: f32, seat_lift: f32) -> (Vec3, f32) {
    let lift = model_height - place_height;
    (Vec3::new(0.0, seat_front - 0.03, place_height - seat_lift), lift)
}

impl Person {
    /// What the enhanced pose reads of this person's state (see [`PoseFacts`]).
    pub fn pose_facts(&self) -> PoseFacts {
        let (activity, seat, reach, floor_lift, hold) = match &self.state {
            State::Pax(x) => {
                let kind = x.pax_state.round().clamp(0.0, 2.0) as u8;
                let activity = match kind {
                    2 => Activity::Sit,
                    1 => Activity::Walk,
                    _ if x.reach => Activity::Pay,
                    _ => Activity::Stand,
                };
                let (seat, floor_lift) = if kind == 2 {
                    let (seat, lift) = seated_pose(self.ty.def.seat_height, x.seat_h, self.ty.rig.seat_front(), self.ty.rig.seat_lift);
                    (Some(seat), lift)
                } else {
                    (None, 0.0)
                };
                let reach = (x.reach && x.inside.is_some()).then(|| {
                    let d = x.reach_at.as_dvec3() - x.pos;
                    let (s, c) = x.yaw.sin_cos();
                    Vec3::new((d.x * c - d.y * s) as f32, (d.x * s + d.y * c) as f32, d.z as f32)
                });
                let hold = grip_for_standing_rider(
                    x.inside.is_some(),
                    x.task == Task::SittingInBus && kind == 0,
                    kind == 1,
                    x.reach,
                );
                (activity, seat, reach, floor_lift, hold)
            }
            _ => (self.activity, None, None, 0.0, 0.0),
        };
        // A seated renderer anchor is [passpos] - human [seatheight], not the floor.
        let seated_level = match &self.state {
            State::Pax(x) if x.pax_state.round() >= 1.5 => Some((self.ty.def.seat_height - x.seat_h) as f64),
            _ => None,
        };
        PoseFacts { activity, seat, reach, floor_lift, hold, seated_level }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seated_floor_and_pelvis_use_different_seat_heights() {
        // Test the geometry used by the renderer: an OMSI model with [seatheight]
        // 0.82 m placed at a 0.45 m physical seat height.
        let model = 0.82;
        let physical = 0.45;
        let seat_front = 0.34;
        let seat_lift = 0.10;
        // The same calculation as seated_pose, with rig measurements supplied.
        let (seat, floor_lift) = seated_pose(model, physical, seat_front, seat_lift);
        // The pelvis is still precisely at [passpos] (the simulation origin + 0.82).
        assert!((floor_lift + seat.z + seat_lift - model).abs() < 1e-5);
        // The hip aligns with [passpos] instead of being behind the seat.
        assert!((seat.y - seat_front + 0.03).abs() < 1e-5);
        // The soles follow the real floor of the seat, not model [seatheight].
        assert!((floor_lift - (model - physical)).abs() < 1e-5);
    }

    #[test]
    fn only_stationary_standing_riders_are_steadied() {
        assert_eq!(grip_for_standing_rider(true, true, false, false), 1.0);
        assert_eq!(grip_for_standing_rider(true, false, false, false), 0.0);
        assert_eq!(grip_for_standing_rider(true, true, true, false), 0.0);
        assert_eq!(grip_for_standing_rider(true, true, false, true), 0.0);
        assert_eq!(grip_for_standing_rider(false, true, false, false), 0.0);
    }
}
