//! The scripts of the traffic light lamps (`ampel1.osc` & co): they turn the state of their
//! crossing's light into the `Red`/`Yellow`/`Green`/`Left`/`Light` variables the lamp's
//! meshes and coronas show, and move what they animate (a barrier arm). The traffic's view
//! sync runs them with the light programs; they are owned here, one per placed lamp, by
//! the key the tile loader gave it (see [`super::SceneryChange::Lamp`]).

use super::SceneryInstance;

/// A placed lamp's script.
pub struct LampScript {
    pub key: u64,
    /// The tile it belongs to (it goes when the tile is unloaded).
    pub tile: (i32, i32),
    pub inst: SceneryInstance,
}

/// The lamps' scripts, by key.
#[derive(Default)]
pub struct LampScripts {
    /// In key order.
    list: Vec<LampScript>,
}

impl LampScripts {
    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    fn index(&self, key: u64) -> Option<usize> {
        self.list.binary_search_by_key(&key, |l| l.key).ok()
    }

    pub fn get(&self, key: u64) -> Option<&SceneryInstance> {
        self.index(key).map(|i| &self.list[i].inst)
    }

    pub fn get_mut(&mut self, key: u64) -> Option<&mut SceneryInstance> {
        self.index(key).map(|i| &mut self.list[i].inst)
    }

    pub fn push(&mut self, lamp: LampScript) {
        match self.list.last() {
            Some(l) if l.key > lamp.key => {
                let i = self.list.partition_point(|l| l.key < lamp.key);
                self.list.insert(i, lamp);
            }
            _ => self.list.push(lamp),
        }
    }

    /// The lamps of tile `key` go.
    pub fn drop_tile(&mut self, key: (i32, i32)) {
        self.list.retain(|l| l.tile != key);
    }
}
