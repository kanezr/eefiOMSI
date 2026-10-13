//! What the people's step reads of the player's bus, as owned values made once a frame
//! before the step (`PeopleSim::player_bus_view`). The step does not hold the vehicle: it can
//! then run on another thread while the bus stays where the picture and the player's
//! controls are (see `crate::sim_thread`). Every value is read from the vehicle exactly as the
//! step read it, so a step on the view is the step on the vehicle.

use super::*;

/// The player's bus as the people see it this frame.
#[derive(Debug, Clone)]
pub struct PlayerBusView {
    pub position: DVec3,
    pub heading: f64,
    /// `VehicleInstance::body_rotation`.
    pub rot: Mat4,
    /// `physics.speed` (m/s) and `physics.velocity_kmh()`.
    pub speed: f32,
    pub speed_kmh: f32,
    /// `physics.a_trans`: the floor's acceleration the riders feel.
    pub a_trans: Vec3,
    /// The vehicle type's `[boundingbox]`.
    pub bounding_box: Option<[f32; 6]>,
    /// The saloon light (`interior_light`) and the cabin air (`CabinAir::of`).
    pub interior: f32,
    pub air: CabinAir,
    /// `GivenTicket`: the ticket the driver handed over (none: -1 or no variable).
    pub given_ticket: Option<f32>,
    /// The terminus its destination display shows: `None` when its scripts keep no
    /// destination index (the duty's terminus is taken then), `Some(None)` for none.
    pub terminus: Option<Option<String>>,
    /// Its timetable's current stop (`tt_busstop_index`: the stop's object and name) and
    /// `act_busstop`, the stop its scripts name.
    pub tt_stop: Option<(i64, Option<String>)>,
    pub act_busstop: String,
    /// What depends on the passenger cabin (none without one: nobody rides it).
    pub cabin: Option<CabinView>,
}

/// The parts of [`PlayerBusView`] that go with the bus's passenger cabin.
#[derive(Debug, Clone)]
pub struct CabinView {
    /// Its entries' and exits' doors open (`PeopleSim::doors_open`).
    pub entry_open: Vec<bool>,
    pub exit_open: Vec<bool>,
    /// Its rear sections (`part_frames`) and the places switched off (`places_off`).
    pub trailers: Vec<PartFrame>,
    pub places_off: Vec<bool>,
}

impl PeopleSim {
    /// The player's bus `v` as this frame's step reads it (with the cabin the people know for
    /// it, `set_cabin`).
    pub fn player_bus_view(&self, v: &VehicleInstance) -> PlayerBusView {
        let cabin = self.player_cabin.as_ref().map(|cabin| {
            let (entry_open, exit_open) = Self::doors_open(v, cabin.entries.len(), cabin.exits.len());
            CabinView { entry_open, exit_open, trailers: part_frames(v, cabin), places_off: places_off(v, cabin) }
        });
        let terminus = match (v.var("target_index_int"), v.host.hof.as_ref()) {
            (Some(i), Some(hof)) if i.is_finite() && i >= 0.0 => {
                Some(hof.termini.get(i.round() as usize).filter(|t| !t.all_exit).map(|t| t.texture_id.trim().to_string()))
            }
            (None, _) => None,
            _ => Some(None),
        };
        let tt_stop = usize::try_from(v.host.tt_busstop_index).ok().and_then(|index| {
            v.host.tt_stop_ids.get(index).map(|id| (*id, v.host.tt_stops.get(index).map(|stop| stop.0.clone())))
        });
        PlayerBusView {
            position: v.position,
            heading: v.heading,
            rot: v.body_rotation(),
            speed: v.physics.speed,
            speed_kmh: v.physics.velocity_kmh(),
            a_trans: v.physics.a_trans,
            bounding_box: v.ty.def.bounding_box,
            interior: v.interior_light(),
            air: CabinAir::of(v),
            given_ticket: v.var("GivenTicket"),
            terminus,
            tt_stop,
            act_busstop: v.str_var("act_busstop").to_string(),
            cabin,
        }
    }
}
