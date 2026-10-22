//! The simulation's questions to the map (`people::World`, `TimetableWorld`) answered by
//! the world snapshot.

use super::{ObjectPlaces, WorldData};
use crate::collision::{CollisionWorld, Obb};
use crate::timetable_run::TimetableWorld;
use glam::DVec3;
use std::path::{Path, PathBuf};
use std::sync::Arc;

impl crate::people::World for WorldData {
    fn root(&self) -> &Path {
        &self.map.root
    }
    fn map_dir(&self) -> &Path {
        &self.map.map_dir
    }
    fn walk_height(&self, x: f64, y: f64) -> Option<f64> {
        WorldData::walk_height(self, x, y)
    }
    fn walk_height_near(&self, x: f64, y: f64, near: f64) -> Option<f64> {
        WorldData::walk_height_near(self, x, y, near)
    }
    fn has_ground(&self, x: f64, y: f64) -> bool {
        WorldData::has_ground(self, x, y)
    }
    fn stop_exit_weight(&self, id: i64) -> f32 {
        WorldData::stop_exit_weight(self, id)
    }
    fn stop_enter(&self, id: i64) -> (f32, f32) {
        WorldData::stop_enter(self, id)
    }
    fn stop_side(&self, id: i64) -> f32 {
        WorldData::stop_side(self, id)
    }
    fn stop_length(&self, id: i64) -> f32 {
        WorldData::stop_length(self, id)
    }
    fn bus_stops(&self) -> &[(i64, DVec3, f64, String)] {
        &self.lists.bus_stops
    }
    fn waiting_places(&self) -> &[(i64, DVec3, f64, f32)] {
        &self.lists.waiting_places
    }
    fn object_positions(&self) -> &ObjectPlaces {
        &self.objects
    }
    fn collision(&self) -> &Arc<CollisionWorld> {
        &self.lists.collision
    }
    fn parked_boxes(&self) -> &Arc<Vec<Obb>> {
        &self.lists.parked_boxes
    }
    fn tiles_generation(&self) -> u64 {
        self.lists.generation
    }
}

impl TimetableWorld for WorldData {
    fn root(&self) -> &Path {
        &self.map.root
    }
    fn map_dir(&self) -> &Path {
        &self.map.map_dir
    }
    fn chrono_dirs(&self) -> Vec<PathBuf> {
        self.map.chrono_dirs.clone()
    }
    fn ailists(&self) -> &omsi_map::AiLists {
        &self.map.ailists
    }
    fn raw_tiles(&self) -> Vec<(i32, i32)> {
        self.map.raw_tiles.clone()
    }
    fn has_tile(&self, key: (i32, i32)) -> bool {
        WorldData::has_tile(self, key)
    }
    fn date(&self) -> i32 {
        self.map.date
    }
    fn object_positions(&self) -> &ObjectPlaces {
        &self.objects
    }
    fn stop_side(&self, id: i64) -> f32 {
        WorldData::stop_side(self, id)
    }
}
