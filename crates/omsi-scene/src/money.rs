//! Money on the cash desk, drawn: the coins a passenger pays with and the change the driver
//! hands out (which coins, and where each lies, is omsi-sim's `people::money`).

use crate::World;
use glam::{DVec3, Mat4, Vec3};
use omsi_geometry::mesh_from_o3d;
use omsi_render::{AlphaMode, MaterialId, MeshId, Renderer, Scene};
use omsi_sim::VehicleInstance;
use std::path::{Path, PathBuf};

pub use omsi_sim::people::Money;

/// The mesh and materials of a coin or note, made the first time it is put down.
fn mesh(m: &mut Money, world: &World, renderer: &Renderer, scene: &mut Scene, coin: usize) -> Option<(MeshId, Vec<MaterialId>)> {
    if let Some(c) = m.meshes.get(&coin) {
        return Some(c.clone());
    }
    let c = m.currency.as_ref()?;
    let n = c.coins.len();
    let file = if coin < n { c.coins.get(coin)?.0.clone() } else { c.bills.get(coin - n)?.0.clone() };
    let o3d = omsi_o3d::load_mesh(&omsi_cfg::resolve_path(&m.dir, &file)).map_err(|e| log::warn!("{e}")).ok()?;
    let dirs = [m.dir.clone(), omsi_cfg::resolve_path(&world.root, "Texture")];
    let dirs_ref: Vec<&Path> = dirs.iter().map(|p| p.as_path()).collect();
    let mats: Vec<MaterialId> = o3d
        .materials
        .iter()
        .map(|mat| {
            let tex = omsi_texture::find_texture(&mat.texture, &dirs_ref).and_then(|p| world.textures.get_gpu_fast(&p)).map(|(img, _)| renderer.add_texture_data(scene, &img));
            renderer.add_material(scene, tex, AlphaMode::Opaque, [1.0; 4], false)
        })
        .collect();
    let id = renderer.add_mesh(scene, &mesh_from_o3d(&o3d));
    m.meshes.insert(coin, (id, mats.clone()));
    Some((id, mats))
}

/// Put coins on a point of the cabin as Omsi.exe does (sub_7e7e08): each one flat at the
/// point's height, somewhere in the variation rectangle and turned at random about the
/// vertical. Nothing is stacked: the coins had been raised 3 mm per coin already lying
/// there, so a driver clicking change built an endless tower on the tray.
/// A point of `[ticket_sale_money_point_2]` / `[ticket_sale_change_point_2]` names the
/// mesh (`parent`, its `[mesh_ident]`) the money lies on: it moves with that mesh - the
/// change tray in a cash desk swinging with the driver's door (#1468).
#[allow(clippy::too_many_arguments)]
pub fn place(m: &mut Money, world: &World, renderer: &Renderer, scene: &mut Scene, coins: &[usize], point: Vec3, var: [f32; 2], change: bool, parent: Option<&str>) {
    for coin in coins {
        let Some((id, mats)) = mesh(m, world, renderer, scene, *coin) else { continue };
        let local = Money::coin_place(point, var, [m.rand_f(), m.rand_f(), m.rand_f()]);
        let inst = renderer.add_instance(scene, id, DVec3::ZERO, Mat4::IDENTITY, mats);
        m.placed.push((inst, local, *coin, change, parent.map(str::to_string)));
    }
}

/// Show the coins lying in the bus where they lie now; the ones taken away hidden.
pub fn sync(m: &mut Money, renderer: &Renderer, scene: &mut Scene, bus: &VehicleInstance) {
    for inst in m.hidden.drain(..) {
        renderer.set_params(scene, inst, &[], false, &[]);
    }
    let rot = bus.body_rotation();
    let mesh_of = |name: &str| -> Mat4 {
        let defs = &bus.ty.model.meshes;
        bus.ty
            .meshes
            .iter()
            .rposition(|m| defs.get(m.def_index).and_then(|d| d.mesh_ident.as_deref()).is_some_and(|n| n.trim().eq_ignore_ascii_case(name.trim())))
            .and_then(|i| bus.mesh_transforms.get(i).copied())
            .unwrap_or(Mat4::IDENTITY)
    };
    for (inst, local, _, _, parent) in &m.placed {
        let on = parent.as_deref().map(mesh_of).unwrap_or(Mat4::IDENTITY);
        renderer.set_transform(scene, *inst, bus.position, rot * on * *local);
    }
}

/// The tear-off ticket blocks of a bus without a ticket printer: the ticket pack's
/// `Ticket_<n>_block.o3d` hung on the bus's `[new_attachment]` point n (the stock SD's
/// "ticket block attach points"), moving with the bus. A click on block n tears off a
/// ticket of type n - `GivenTicket` n, as a printer's script hands one over (#1413).
pub struct TicketBlocks {
    /// The bus type and the pack they were made for.
    pub made_for: (PathBuf, PathBuf),
    /// (instance, place in the bus frame, ticket type, the mesh for clicks)
    blocks: Vec<(usize, Mat4, usize, omsi_geometry::MeshData)>,
}

impl TicketBlocks {
    pub fn new(world: &World, renderer: &Renderer, scene: &mut Scene, bus: &VehicleInstance, pack: &Path) -> TicketBlocks {
        let dir = pack.parent().map(Path::to_path_buf).unwrap_or_default();
        let mut blocks = Vec::new();
        for (n, a) in bus.ty.def.attachments.iter().enumerate() {
            let file = dir.join(format!("Ticket_{n}_block.o3d"));
            if !omsi_cfg::vfs::is_file(&file) {
                continue;
            }
            let Ok(m) = omsi_o3d::load_mesh(&file).map_err(|e| log::warn!("{e}")) else { continue };
            let dirs = [dir.clone(), omsi_cfg::resolve_path(&world.root, "Texture")];
            let dirs_ref: Vec<&Path> = dirs.iter().map(|p| p.as_path()).collect();
            let mats: Vec<MaterialId> = m
                .materials
                .iter()
                .map(|mat| {
                    let tex = omsi_texture::find_texture(&mat.texture, &dirs_ref).and_then(|p| world.textures.get_gpu_fast(&p)).map(|(img, _)| renderer.add_texture_data(scene, &img));
                    renderer.add_material(scene, tex, AlphaMode::Test, [1.0; 4], false)
                })
                .collect();
            let data = mesh_from_o3d(&m);
            let id = renderer.add_mesh(scene, &data);
            let inst = renderer.add_instance(scene, id, DVec3::ZERO, Mat4::IDENTITY, mats);
            let local = omsi_world::map_index::attachment_matrix(&omsi_scenery::sco::Attachment { ops: a.ops.clone() });
            blocks.push((inst, local, n, data));
        }
        if !blocks.is_empty() {
            log::info!("ticket blocks: {} of {} on the bus", blocks.len(), dir.display());
        }
        TicketBlocks { made_for: (bus.ty.def.path.clone(), pack.to_path_buf()), blocks }
    }

    pub fn sync(&self, renderer: &Renderer, scene: &mut Scene, bus: &VehicleInstance) {
        let rot = bus.body_rotation();
        for (inst, local, _, _) in &self.blocks {
            renderer.set_transform(scene, *inst, bus.position, rot * *local);
        }
    }

    /// The ticket type of the block a ray from `origin` along `dir` hits first.
    pub fn hit(&self, origin: DVec3, dir: Vec3, bus: &VehicleInstance) -> Option<usize> {
        let o = (origin - bus.position).as_vec3();
        let rot = bus.body_rotation();
        self.blocks
            .iter()
            .filter_map(|(_, local, n, data)| omsi_geometry::ray_mesh(o, dir, data, &(rot * *local)).map(|t| (t, *n)))
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|x| x.1)
    }
}
