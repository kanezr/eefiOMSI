//! Money on the cash desk: coins a passenger pays with (`[ticket_sale_money_point]`) and the
//! change the driver hands out with `GiveChangeCoin` (`[ticket_sale_change_point]`).

use crate::scene::World;
use glam::{DVec3, Mat4, Vec3};
use hashbrown::HashMap;
use omsi_content::Currency;
use omsi_geometry::mesh_from_o3d;
use omsi_render::{AlphaMode, MaterialId, MeshId, Renderer, Scene};
use omsi_sim::VehicleInstance;
use std::path::{Path, PathBuf};

pub struct Money {
    pub currency: Option<Currency>,
    dir: PathBuf,
    meshes: HashMap<usize, (MeshId, Vec<MaterialId>)>,
    /// (instance, place in the bus frame, coin index, is change)
    placed: Vec<(usize, Mat4, usize, bool)>,
    hidden: Vec<usize>,
    rng: u64,
}

impl Money {
    pub fn new(root: &Path, money_system: &str) -> Money {
        let path = omsi_cfg::resolve_path(root, money_system);
        let currency = Currency::load(&path).map_err(|e| log::warn!("money system {}: {e}", path.display())).ok();
        if let Some(c) = &currency {
            log::info!("money system {}: {} coins, {} bills", c.name, c.coins.len(), c.bills.len());
        }
        Money { currency, dir: path.parent().map(|p| p.to_path_buf()).unwrap_or_default(), meshes: HashMap::new(), placed: Vec::new(), hidden: Vec::new(), rng: 0x5151_7777 }
    }

    fn rand_f(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Every coin and note of the currency as (index, value): the coins first, then the
    /// notes (index `coins.len() + k`). The passengers had only ever paid in coins - a
    /// Novi Sad fare of 65 dinars was always 20 + 20 + 20 + 5, never a 100 note.
    fn denominations(&self) -> Vec<(usize, f32)> {
        let Some(c) = &self.currency else { return Vec::new() };
        let n = c.coins.len();
        let mut all: Vec<(usize, f32)> = c.coins.iter().enumerate().map(|(i, (_, v))| (i, *v)).chain(c.bills.iter().enumerate().map(|(k, (_, v))| (n + k, *v))).filter(|(_, v)| *v > 0.0).collect();
        all.sort_by(|a, b| b.1.total_cmp(&a.1));
        all
    }

    /// `value` in as few pieces as the currency allows, largest first; None when it cannot
    /// be paid exactly with them (a fraction smaller than the smallest coin).
    fn exact_pieces(all: &[(usize, f32)], value: f32) -> Option<Vec<usize>> {
        let mut out = Vec::new();
        let mut left = value;
        for (i, v) in all {
            while left >= *v - 0.001 && out.len() < 40 {
                out.push(*i);
                left -= v;
            }
        }
        (left.abs() <= 0.001).then_some(out)
    }

    /// The fare exactly (no change due): coins and notes. (Capped at twelve coins and topped
    /// up with the smallest when that ran out, "exact" was not always exact.)
    pub fn exact_coins_for(&mut self, value: f32) -> Vec<usize> {
        let all = self.denominations();
        if let Some(out) = Self::exact_pieces(&all, value) {
            return out;
        }
        // not payable to the last cent: the nearest amount over it
        let mut out = Vec::new();
        let mut left = value;
        for (i, v) in &all {
            while left >= *v - 0.001 && out.len() < 40 {
                out.push(*i);
                left -= v;
            }
        }
        if left > 0.001 {
            if let Some((i, _)) = all.iter().rev().find(|(_, v)| *v >= left) {
                out.push(*i);
            }
        }
        out
    }


    /// What a passenger puts on the desk for `price`, as Omsi.exe does it (sub_7e8254):
    /// coins drawn at random until they cover the price, then every coin that is not needed
    /// (the rest still covers the price less half the smallest coin) taken back again.
    pub fn omsi_coins_for(&mut self, price: f32) -> Vec<usize> {
        let values: Vec<f32> = match &self.currency {
            Some(c) => c.coins.iter().map(|(_, v)| *v).collect(),
            None => return Vec::new(),
        };
        if values.is_empty() || values.iter().all(|v| *v <= 0.0) {
            return self.exact_coins_for(price);
        }
        let half_smallest = self.smallest_value() / 2.0;
        let mut out: Vec<usize> = Vec::new();
        let mut sum = 0.0f32;
        while sum < price && out.len() < 200 {
            let k = ((self.rand_f() * values.len() as f32) as usize).min(values.len() - 1);
            sum += values[k];
            out.push(k);
        }
        let mut again = true;
        while again {
            again = false;
            for j in 0..out.len() {
                if price - half_smallest <= sum - values[out[j]] {
                    sum -= values[out[j]];
                    out.remove(j);
                    again = true;
                    break;
                }
            }
        }
        out
    }

    /// The value of the smallest coin (the tolerance of the change is half of it).
    pub fn smallest_value(&self) -> f32 {
        self.currency.as_ref().and_then(|c| c.coins.iter().map(|(_, v)| *v).filter(|v| *v > 0.0).reduce(f32::min)).unwrap_or(0.01)
    }

    /// How many coins lie on the change tray.
    pub fn change_count(&self) -> usize {
        self.placed.iter().filter(|p| p.3).count()
    }

    pub fn value_of(&self, coins: &[usize]) -> f32 {
        let Some(c) = &self.currency else { return 0.0 };
        let n = c.coins.len();
        coins.iter().filter_map(|&i| if i < n { c.coins.get(i) } else { c.bills.get(i - n) }).map(|(_, v)| *v).sum()
    }

    fn mesh(&mut self, world: &World, renderer: &Renderer, scene: &mut Scene, coin: usize) -> Option<(MeshId, Vec<MaterialId>)> {
        if let Some(m) = self.meshes.get(&coin) {
            return Some(m.clone());
        }
        let c = self.currency.as_ref()?;
        let n = c.coins.len();
        let file = if coin < n { c.coins.get(coin)?.0.clone() } else { c.bills.get(coin - n)?.0.clone() };
        let m = omsi_o3d::load_mesh(&omsi_cfg::resolve_path(&self.dir, &file)).map_err(|e| log::warn!("{e}")).ok()?;
        let dirs = [self.dir.clone(), omsi_cfg::resolve_path(&world.root, "Texture")];
        let dirs_ref: Vec<&Path> = dirs.iter().map(|p| p.as_path()).collect();
        let mats: Vec<MaterialId> = m
            .materials
            .iter()
            .map(|mat| {
                let tex = omsi_texture::find_texture(&mat.texture, &dirs_ref).and_then(|p| world.textures.get_gpu_fast(&p)).map(|(img, _)| renderer.add_texture_data(scene, &img));
                renderer.add_material(scene, tex, AlphaMode::Opaque, [1.0; 4], false)
            })
            .collect();
        let id = renderer.add_mesh(scene, &mesh_from_o3d(&m));
        self.meshes.insert(coin, (id, mats.clone()));
        Some((id, mats))
    }

    /// Put coins on a point of the cabin as Omsi.exe does (sub_7e7e08): each one flat at the
    /// point's height, somewhere in the variation rectangle and turned at random about the
    /// vertical. Nothing is stacked: the coins had been raised 3 mm per coin already lying
    /// there, so a driver clicking change built an endless tower on the tray.
    pub fn place(&mut self, world: &World, renderer: &Renderer, scene: &mut Scene, coins: &[usize], point: Vec3, var: [f32; 2], change: bool) {
        for coin in coins {
            let Some((id, mats)) = self.mesh(world, renderer, scene, *coin) else { continue };
            let local = Self::coin_place(point, var, [self.rand_f(), self.rand_f(), self.rand_f()]);
            let inst = renderer.add_instance(scene, id, DVec3::ZERO, Mat4::IDENTITY, mats);
            self.placed.push((inst, local, *coin, change));
        }
    }

    /// One coin's place on a point from three random numbers in 0..1: x and y (forward) in the
    /// variation, the height of the point itself, a turn of 2*pi*r about the vertical.
    fn coin_place(point: Vec3, var: [f32; 2], r: [f32; 3]) -> Mat4 {
        let local = point + Vec3::new((r[0] - 0.5) * var[0], (r[1] - 0.5) * var[1], 0.0);
        Mat4::from_translation(local) * Mat4::from_rotation_z(r[2] * std::f32::consts::TAU)
    }

    /// Remove the payment (driver takes it) or the change (passenger takes it).
    pub fn clear(&mut self, change: bool) {
        let (gone, keep): (Vec<_>, Vec<_>) = self.placed.drain(..).partition(|p| p.3 == change);
        self.hidden.extend(gone.into_iter().map(|p| p.0));
        self.placed = keep;
    }

    pub fn change_value(&self) -> f32 {
        let coins: Vec<usize> = self.placed.iter().filter(|p| p.3).map(|p| p.2).collect();
        self.value_of(&coins)
    }

    pub fn sync(&mut self, renderer: &Renderer, scene: &mut Scene, bus: &VehicleInstance) {
        for inst in self.hidden.drain(..) {
            renderer.set_params(scene, inst, &[], false, &[]);
        }
        let rot = bus.body_rotation();
        for (inst, local, _, _) in &self.placed {
            renderer.set_transform(scene, *inst, bus.position, rot * *local);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coins_lie_flat_on_the_point_not_stacked() {
        let point = Vec3::new(0.4, 5.0, 1.1);
        let mut rng = Money { currency: None, dir: PathBuf::new(), meshes: HashMap::new(), placed: Vec::new(), hidden: Vec::new(), rng: 7 };
        let mut top = f32::MIN;
        for _ in 0..80 {
            let r = [rng.rand_f(), rng.rand_f(), rng.rand_f()];
            let m = Money::coin_place(point, [0.1, 0.06], r);
            let p = m.transform_point3(Vec3::ZERO);
            assert!((p.x - point.x).abs() <= 0.05 + 1e-6 && (p.y - point.y).abs() <= 0.03 + 1e-6);
            top = top.max(p.z);
        }
        // 80 coins: all at the point's height (they were 80 * 3 mm = 24 cm high before)
        assert!((top - point.z).abs() < 1e-6, "{top}");
    }
}
