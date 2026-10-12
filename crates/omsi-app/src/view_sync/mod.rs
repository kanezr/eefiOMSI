//! View sync: the one place that turns the simulation's state into renderer instances (see
//! `docs/ARCHITECTURE.md`, "Target layering"). The AI traffic and the people simulate in
//! omsi-sim and know nothing of the GPU; what they look like is here:
//!
//! * `traffic`: the AI vehicles' renders, their drivers and the traffic light lamps;
//! * `traffic_audio`: the AI vehicles' sounds;
//! * `people`: the people's meshes, posing and skinning, the coins and the ticket blocks;
//! * `vehicle`, `vehicle_damage`: what any vehicle's simulation changed this frame (the
//!   player's, an AI car's, their coupled parts'): skinned meshes, switched materials, free
//!   textures, interior lamps, text and script texture uploads, glass cracks and dents.
//!
//! What the simulation hands over is data, never pixels it sends or sounds it plays: the
//! cars it took off the road (`TrafficSim::retired`, `TrafficSim::take_parked`), the
//! pictures its scripts and pages drew (`ScriptTexture::take_upload`,
//! `TextTextureState::pending`), the sounds its scripts triggered (`VehicleHost::fired_*`)
//! and the posts it knocked over (`VehicleInstance::knocked_now`). The pictures it reads
//! itself (fonts, `STLoadTex`) come through the decoder registered here
//! ([`install_sim_decoder`]).
//!
//! [`sync`] is the frame's view sync. The window runs it twice a frame, each time where the
//! part it brings up to date used to be synced: the traffic at the end of its step (before
//! the player and the people move: parking cars leave the traffic's list there, which the
//! people's step reads), the people at the end of theirs. The offscreen run calls it at the
//! same points, and before each picture it takes.
//!
//! The view's state is [`SimView`], kept apart from the simulation wrappers (`Traffic`,
//! `Humans` hold only the simulation's side): the window has it in `GfxState::sim_view`, the
//! offscreen run in its own struct. Two kinds of renderer change still happen inside the
//! wrappers' calls, at the moment the simulation makes them, so that the renderer sees the
//! same calls in the same order: a car's renders are made or let go as the traffic puts it
//! on the road or takes it off, and the people who appeared or went are shown at the end of
//! every `Humans` call that made them (`Humans::show_bodies`). Those calls take the part of
//! the `SimView` they change (`&mut TrafficView`, `&mut PeopleView`) as a parameter.

pub(crate) mod people;
pub(crate) mod traffic;
pub(crate) mod traffic_audio;
pub(crate) mod vehicle;
pub(crate) mod vehicle_damage;

use crate::humans::Humans;
use crate::scene::World;
use crate::traffic::Traffic;
use glam::DVec3;
use omsi_render::{Renderer, Scene};
use omsi_sim::VehicleInstance;
use people::PeopleView;
use traffic::TrafficView;

// (registered by the scene, whose worlds read pictures as they open)
pub(crate) use crate::scene::host::install_sim_decoder;

/// What the renderer shows of the simulation, kept for the next view sync: the AI traffic's
/// renders and drivers, and the people's meshes, posing and ticket blocks. Each part starts
/// afresh with the simulation it shows (`Traffic` put in place, `Humans::new`).
#[derive(Default)]
pub(crate) struct SimView {
    pub(crate) traffic: TrafficView,
    pub(crate) people: PeopleView,
}

/// The people to bring up to date in a view sync.
pub(crate) struct People<'a> {
    pub(crate) humans: &'a mut Humans,
    /// The player's bus: its coins on the cash desk and its ticket blocks (None: no bus, or
    /// those are not wanted at this point).
    pub(crate) bus: Option<&'a VehicleInstance>,
    /// Where the people are seen from when they have no eye (`PeopleSim::eye`): the nearer,
    /// the more often one is posed.
    pub(crate) camera: DVec3,
}

/// What a view sync brings up to date: the traffic, the people, or both (in that order).
#[derive(Default)]
pub(crate) struct ViewSync<'a> {
    pub(crate) traffic: Option<&'a mut Traffic>,
    pub(crate) people: Option<People<'a>>,
}

impl<'a> ViewSync<'a> {
    /// The traffic alone.
    pub(crate) fn traffic(traffic: &'a mut Traffic) -> ViewSync<'a> {
        ViewSync { traffic: Some(traffic), people: None }
    }

    /// The people alone (see [`People`]).
    pub(crate) fn people(humans: &'a mut Humans, bus: Option<&'a VehicleInstance>, camera: DVec3) -> ViewSync<'a> {
        ViewSync { traffic: None, people: Some(People { humans, bus, camera }) }
    }
}

/// Bring the renderer up to the simulation's state: the traffic (cars that have parked
/// become parked objects, renders of cars gone go back to the world, the drivers, the
/// traffic lamps, the cars' transforms, materials and script textures), then the people
/// (the coins and ticket blocks of the player's bus, those who went since, the posing and
/// skinning and their transforms).
pub(crate) fn sync(what: ViewSync<'_>, view: &mut SimView, world: &World, renderer: &Renderer, scene: &mut Scene) {
    if let Some(t) = what.traffic {
        t.sync(&mut view.traffic, world, renderer, scene);
    }
    if let Some(People { humans, bus, camera }) = what.people {
        if let Some(bus) = bus {
            humans.sync_money(&mut view.people, world, renderer, scene, bus);
        }
        humans.sync(&mut view.people, world, renderer, scene, camera);
    }
}
