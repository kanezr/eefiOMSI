//! What the people need of the loaded map: the surfaces they walk on, the bus stops and
//! their waiting places, the walls and the parked cars. The world snapshot
//! (`crate::world_data::WorldData`) gives it; the simulation never sees the rest of the
//! game's world (textures, the GPU, the loading of the tiles).

use crate::collision::{CollisionWorld, Obb};
use glam::DVec3;
use std::path::Path;
use std::sync::Arc;

pub trait World {
    /// The game's folder.
    fn root(&self) -> &Path;
    /// The map's folder.
    fn map_dir(&self) -> &Path;
    /// Height for somebody on foot: the top of whatever is here (a pavement, a platform),
    /// else the bare ground; None where nothing is loaded.
    fn walk_height(&self, x: f64, y: f64) -> Option<f64>;
    /// Height for somebody on foot standing at height `near`: the surface under them, not
    /// a roof over them.
    fn walk_height_near(&self, x: f64, y: f64, near: f64) -> Option<f64>;
    /// Whether the ground at world (x, y) is loaded.
    fn has_ground(&self, x: f64, y: f64) -> bool;
    /// How many passengers get off at stop object `id` (0.5 for a stop without strings).
    fn stop_exit_weight(&self, id: i64) -> f32;
    /// Stop object `id`'s (pass_enter_max, pass_enter_min).
    fn stop_enter(&self, id: i64) -> (f32, f32);
    /// The side stop object `id`'s platform lies on: 0 = right, 1 = the other, 2 = both.
    fn stop_side(&self, id: i64) -> f32;
    /// Stop object `id`'s length (30 m when the map says nothing).
    fn stop_length(&self, id: i64) -> f32;
    /// Placed `[busstop]` objects: (map id, world position, heading, name).
    fn bus_stops(&self) -> &[(i64, DVec3, f64, String)];
    /// Where people wait at the stops: (object id, world position, heading in degrees, seat
    /// height - 0 for a standing place).
    fn waiting_places(&self) -> &[(i64, DVec3, f64, f32)];
    /// World position and rotation of every loaded map object by id.
    fn object_positions(&self) -> &crate::world_data::ObjectPlaces;
    /// The scenery's collision boxes and meshes.
    fn collision(&self) -> &Arc<CollisionWorld>;
    /// The boxes of the parked cars of the loaded tiles, which people walk round.
    fn parked_boxes(&self) -> &Arc<Vec<Obb>>;
    /// Counts the changes of the loaded tiles: whoever keeps what it derived from the
    /// stops, the waiting places or the ground looks again.
    fn tiles_generation(&self) -> u64;
}
