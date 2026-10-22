//! What the simulation did that the renderer has to follow.
//!
//! The simulation does not touch the renderer. What it does that the renderer has to
//! follow - somebody appearing, somebody going, coins put on the desk - it writes down as
//! [`BodyOp`]s, and omsi-app's view replays them in the order they happened, right after
//! the call that made them (`view_sync::apply_people`): the renderer sees the very calls,
//! in the very order, it saw when the simulation made them itself. (The traffic's are
//! `crate::view_events::TrafficEvent`s.)

use super::*;

/// Something the simulation did that the renderer has to follow.
pub enum BodyOp {
    /// Somebody appeared: the meshes of their type and clothes, at `position`.
    Spawn { id: u32, ty: Arc<HumanType>, variant: usize, position: DVec3 },
    /// Somebody went: hidden, and their meshes kept for the next person of the type
    /// (`tkey`: the type's key, `Arc::as_ptr` of it; `variant`: their clothes).
    Retire { id: u32, tkey: usize, variant: usize },
    /// Somebody already about is called `to` from now on (a LAN client's own people,
    /// numbered anew when the host's come): the view keeps their meshes under the new id.
    Renamed { from: u32, to: u32 },
    /// Coins on a point of the player's cabin (`Money::place`).
    Coins { coins: Vec<usize>, point: Vec3, var: [f32; 2], change: bool, parent: Option<String> },
}

impl BodyOp {
    /// Whether replaying it needs the world (textures, the money's meshes).
    pub fn needs_world(&self) -> bool {
        !matches!(self, BodyOp::Retire { .. } | BodyOp::Renamed { .. })
    }
}

/// The simulation's side of the people's bodies: what the view has to catch up with.
#[derive(Default)]
pub struct BodyOps {
    /// What the simulation did since the view last caught up, oldest first.
    pub ops: Vec<BodyOp>,
}

impl BodyOps {
    /// Somebody not yet drawn is now called `to` (a LAN host's person takes the host's id).
    pub fn renamed(&mut self, from: u32, to: u32) {
        for op in self.ops.iter_mut() {
            if let BodyOp::Spawn { id, .. } = op {
                if *id == from {
                    *id = to;
                }
            }
        }
    }
}

impl PeopleSim {
    /// Somebody has gone: hidden, and their meshes kept for the next person of the type.
    pub fn retire(&mut self, p: &Person) {
        let tkey = Arc::as_ptr(&p.ty) as usize;
        self.bodies.ops.push(BodyOp::Retire { id: p.id, tkey, variant: p.variant });
    }
}
