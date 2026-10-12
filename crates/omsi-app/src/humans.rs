//! People on foot: passengers and pedestrians as agents with a goal.
//!
//! The simulation is omsi-sim's `people` (`PeopleSim`, which knows nothing of the GPU);
//! here it gets the loaded world, the traffic and the player's duty, and the people's
//! pictures (`crate::view_sync::people`): their meshes, posing and skinning, and the coins
//! on the cash desk.

use crate::ambience;
use crate::scene::World;
use crate::traffic::Traffic;
use glam::DVec3;
use omsi_render::{Camera, Renderer, Scene};
use omsi_sim::human::HumanType;
use omsi_sim::people::pax::{StopPlan, TripPlan};
use omsi_sim::people::{DutyTrip, PeopleSim};
use omsi_sim::VehicleInstance;
use std::path::Path;
use std::sync::Arc;

// The renderer's side of the people.
use crate::view_sync::people::PeopleView;

// The module's API, at the paths it always had (some only returned, never named outside).
#[allow(unused_imports)]
pub use omsi_sim::people::{AvatarCmd, SeatSpot};
#[allow(unused_imports)]
pub use omsi_sim::people::{placed_bus_id, remote_bus_id, remote_bus_player, LanPerson, MirrorPose};
pub use omsi_sim::people::{BusId, DoorWants};
#[allow(unused_imports)]
pub use omsi_sim::people::Person;
#[allow(unused_imports)]
pub use omsi_sim::people::VoiceLine;

/// The map's traffic keeps left (its stops are on the left): see the doors of `Cabin`.

/// The people as the game drives them. Everything of the simulation reads through it
/// (`Deref` to `PeopleSim`). Their pictures are the view sync's (`PeopleView`, in
/// `view_sync::SimView`): every call that makes or removes people takes it and shows them
/// before it returns.
pub struct Humans {
    pub sim: PeopleSim,
}

impl std::ops::Deref for Humans {
    type Target = PeopleSim;
    fn deref(&self) -> &PeopleSim {
        &self.sim
    }
}

impl std::ops::DerefMut for Humans {
    fn deref_mut(&mut self) -> &mut PeopleSim {
        &mut self.sim
    }
}

/// Where the player looks from (`omsi_sim::people::Eye`), made from the camera.
pub struct Eye;

impl Eye {
    pub fn of(cam: &Camera, aspect: f32) -> omsi_sim::people::Eye {
        omsi_sim::people::Eye::looking(cam.position, cam.forward().as_dvec3(), cam.fov_deg as f64, aspect)
    }
}

impl Humans {
    /// The people, with `view` started afresh for them.
    pub fn new(root: &Path, view: &mut PeopleView) -> Humans {
        let configured_people = crate::settings::Settings::load().ai_max_humans as usize;
        let sim = PeopleSim::new(root, configured_people);
        *view = PeopleView::new();
        Humans { sim }
    }

    /// Advance everybody (see `PeopleSim::tick_inner`), and show the renderer what the step
    /// did. `bus`: the player's vehicle; `traffic`: the timetable buses, the traffic lights
    /// and the cars pedestrians wait for. Returns true when a passenger took the printed
    /// ticket (the caller resets `GivenTicket`).
    #[allow(clippy::too_many_arguments)]
    pub fn tick(
        &mut self,
        view: &mut PeopleView,
        dt: f32,
        world: &World,
        bus: Option<&VehicleInstance>,
        traffic: Option<&Traffic>,
        renderer: &Renderer,
        scene: &mut Scene,
    ) -> bool {
        let started = std::time::Instant::now();
        self.sim.tick_stages.clear();
        let took = self.sim.tick_inner(dt, world, bus, traffic.map(|t| &t.sim));
        // what the step did, drawn (people who came and went, coins on the desk)
        let mark = std::time::Instant::now();
        self.show_bodies(view, world, renderer, scene);
        self.sim.tick_stages.push(("bodies", mark.elapsed().as_secs_f64() * 1000.0));
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        self.sim.tick_done(dt, world, ms);
        took
    }

    /// Put people at the bus stops near `center` (see `PeopleSim::populate`).
    pub fn populate(&mut self, view: &mut PeopleView, world: &World, renderer: &Renderer, scene: &mut Scene, center: DVec3) {
        self.sim.populate(world, center);
        self.show_bodies(view, world, renderer, scene);
    }

    /// Put avatar `key` where `cmd` says (see `PeopleSim::avatar`).
    #[allow(clippy::too_many_arguments)]
    pub fn avatar(&mut self, view: &mut PeopleView, key: u32, world: &World, renderer: &Renderer, scene: &mut Scene, cmd: AvatarCmd, kind: u64) {
        self.sim.avatar(key, world, cmd, kind);
        self.show_bodies(view, world, renderer, scene);
    }

    /// One of the host's people appears here (client; see `PeopleSim::mirror_add`).
    #[allow(clippy::too_many_arguments)]
    pub fn mirror_add(&mut self, view: &mut PeopleView, world: &World, renderer: &Renderer, scene: &mut Scene, id: u32, ty: usize, pose: &MirrorPose) -> bool {
        let added = self.sim.mirror_add(world, id, ty, pose);
        self.show_bodies(view, world, renderer, scene);
        added
    }

    /// `--riders n` (see `PeopleSim::seed_riders`).
    pub fn seed_riders(&mut self, view: &mut PeopleView, n: usize, bus: &VehicleInstance, world: &World, renderer: &Renderer, scene: &mut Scene) {
        self.sim.seed_riders(n, bus, world);
        self.show_bodies(view, world, renderer, scene);
    }

    /// OMSI's `change_take`: the driver takes back the coins lying on the change tray.
    pub fn take_change_tray(&mut self) {
        if let Some(m) = self.sim.money.as_mut() {
            m.clear(true);
        }
    }

    /// Bus `bus` is gone (the player removed it; see `PeopleSim::evict`).
    pub fn evict(&mut self, bus: BusId, world: &World) {
        self.sim.evict(bus, world);
    }

    /// The stop the player's duty is due at next (see `PeopleSim::set_player_next_stop`).
    pub fn set_player_next_stop(&mut self, stop: Option<&crate::schedule::PlannedStop>) {
        self.sim.set_player_next_stop(stop.map(|s| (s.object_id, s.name.as_str(), s.position)));
    }

    /// The player's duty this frame; None in free drive, where the bus takes whom its terminus
    /// shown takes, as a timetable bus. (Set after `stop_names`: the trip's stops are named by it.)
    pub fn set_duty(&mut self, duty: Option<&crate::schedule::PlayerDuty>) {
        let Some(d) = duty else {
            self.sim.duty = None;
            return;
        };
        // the trip the IBIS is given: on a works trip from the depot the next one with a line
        let (t, next) = d.trip_for_ibis();
        let done = std::ptr::eq(t, d.trip()) && d.trip_done();
        let trip = match self.sim.duty.take() {
            Some((trip, ..)) if trip.name == t.name && trip.departure == t.departure => trip,
            _ => Arc::new(DutyTrip::of(&trip_plan(t), self.sim.stop_names.as_ref())),
        };
        self.sim.duty = Some((trip, next, done));
    }

    /// The footsteps taken since the last call, for the environment sounds (see
    /// `PeopleSim::take_footfalls`).
    pub fn take_footfalls(&mut self) -> Vec<ambience::Footfall> {
        self.sim
            .take_footfalls()
            .into_iter()
            .map(|f| ambience::Footfall { position: f.position, inside: f.inside, own_bus: f.own_bus, pack: f.pack })
            .collect()
    }

    /// Count of people per state, for logs, and the time posing them took (`view`).
    pub fn summary(&self, view: &PeopleView) -> String {
        let mut out = self.sim.summary();
        let (frames, posed, ms, up) = view.pose_stats;
        if frames > 0 {
            out.push_str(&format!(
                "; posing {:.2} ms a frame ({:.1} people, {:.2} ms of it uploading and placing)",
                ms / frames as f64,
                posed as f64 / frames as f64,
                up / frames as f64
            ));
        }
        out
    }

    /// The file of a human type relative to its content root (`Humans/…/x.hum`).
    pub fn type_file(ty: &HumanType) -> String {
        PeopleSim::type_file(ty)
    }

    /// Hand a bus's scripts what the passengers want of its doors and places (see
    /// `PeopleSim::write_door_requests`).
    pub(crate) fn write_door_requests(b: &mut VehicleInstance, doors: &DoorWants) {
        PeopleSim::write_door_requests(b, doors)
    }
}

/// A trip of the timetable as the passengers need it.
fn trip_plan(trip: &crate::schedule::PlannedTrip) -> TripPlan {
    TripPlan {
        name: trip.name.clone(),
        line: trip.line.clone(),
        terminus: trip.terminus.clone(),
        departure: trip.departure,
        stops: trip.stops.iter().map(|s| StopPlan { object_id: s.object_id, name: s.name.clone(), stops: s.stops }).collect(),
    }
}
