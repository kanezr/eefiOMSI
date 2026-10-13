//! Rays into a vehicle: the `[mouseevent]` meshes, the HTML pages and the trailers' switches.

use super::*;

pub(crate) fn pick_in(vehicle: &omsi_sim::VehicleInstance, origin: DVec3, dir: Vec3, spread: f32) -> Option<usize> {
    let o = (origin - vehicle.position).as_vec3();
    let right = Vec3::new(-dir.y, dir.x, 0.0).normalize_or_zero();
    let up = dir.cross(right).normalize_or_zero();
    let mut rings: Vec<Vec<Vec3>> = vec![vec![dir]];
    if spread > 0.0 {
        for ring in 1..=2 {
            let r = spread * ring as f32;
            let n = 8 * ring;
            rings.push(
                (0..n)
                    .map(|k| {
                        let a = k as f32 / n as f32 * std::f32::consts::TAU;
                        (dir + right * (a.cos() * r) + up * (a.sin() * r)).normalize()
                    })
                    .collect(),
            );
        }
    }
    // The same broad-phase applies to every ring. A large cockpit can contain
    // hundreds of meshes; calculating their posed transforms three times made
    // hovering over its controls needlessly expensive.
    let candidates: Vec<(usize, glam::Mat4, Vec<u32>)> = vehicle.ty.meshes.iter().enumerate().filter_map(|(i, vm)| {
        if vehicle.ty.model.meshes[vm.def_index].mouse_event.is_none() || !vehicle.mesh_props[i].visible {
            return None;
        }
        let xf = vehicle.mesh_local_transform(i);
        if !ray_may_hit(&vehicle.ty, i, &xf, o, dir, spread * 2.2) {
            return None;
        }
        let tris = omsi_geometry::cone_triangles(o, dir, spread * 2.0 + 1e-4, &vm.data, &xf);
        (!tris.is_empty()).then_some((i, xf, tris))
    }).collect();
    for dirs in &rings {
        // Collect every hit in this ring, then among surfaces within a depth band of the
        // nearest prefer the mesh whose centre the ray aims at. Aachen ibox D10/D11/D12 are
        // tilted, overlap by ~6 mm, and sit ~16 mm apart along Z - a 2 mm depth-only pick
        // let backspace (D10) steal digit 0 (D11) whenever both boxes were hit.
        let mut hits: Vec<(f32, f32, usize)> = Vec::new();
        for (i, xf, tris) in &candidates {
            let (i, xf) = (*i, *xf);
            let vm = &vehicle.ty.meshes[i];
            let lateral = vehicle
                .ty
                .mesh_bounds
                .get(i)
                .map(|&(c, _)| {
                    let p = xf.transform_point3(c) - o;
                    let along = p.dot(dir);
                    (p - dir * along).length_squared()
                })
                .unwrap_or(0.0);
            for d in dirs {
                if let Some(t) = omsi_geometry::ray_triangles(o, *d, &vm.data, &xf, tris) {
                    hits.push((t, lateral, i));
                }
            }
        }
        if let Some(i) = best_mouseevent_hit(&hits) {
            return Some(i);
        }
    }
    None
}

/// Depth band (m) inside which overlapping cockpit `[mouseevent]` meshes are disambiguated
/// by aim (lateral distance to the mesh centre) rather than by which face is a hair closer.
/// Sized for the Aachen ibox digit row (D10/D11/D12 centres ~16 mm apart on a tilted pad).
pub(crate) const MOUSEEVENT_PICK_DEPTH_BAND: f32 = 0.03;

/// Among ray hits `(t, lateral², id)`, pick the aimed mesh: nearest surface first, then the
/// smallest lateral distance among hits within [`MOUSEEVENT_PICK_DEPTH_BAND`] of that nearest.
pub(crate) fn best_mouseevent_hit(hits: &[(f32, f32, usize)]) -> Option<usize> {
    let min_t = hits.iter().map(|h| h.0).fold(f32::INFINITY, f32::min);
    if !min_t.is_finite() {
        return None;
    }
    hits.iter()
        .filter(|h| h.0 <= min_t + MOUSEEVENT_PICK_DEPTH_BAND)
        .min_by(|a, b| {
            a.1.partial_cmp(&b.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
        })
        .map(|h| h.2)
}

#[cfg(test)]
mod mouseevent_pick_tests {
    use super::{best_mouseevent_hit, MOUSEEVENT_PICK_DEPTH_BAND};

    #[test]
    fn aachen_ibox_d11_wins_when_d10_is_closer_but_off_aim() {
        // Realistic AC O530 digit row: D10 (backspace) ~12 mm closer along the ray than
        // D11 (digit 0), but the cursor aims at D11's centre (lateral ≈ 0).
        let d10 = 0usize;
        let d11 = 1usize;
        let d12 = 2usize;
        let hits = [
            (0.488, 0.012_f32.powi(2), d10),
            (0.500, 0.001_f32.powi(2), d11),
            (0.512, 0.015_f32.powi(2), d12),
        ];
        assert!(
            (hits[1].0 - hits[0].0) > 0.002,
            "fixture must exceed the old 2 mm tie window"
        );
        assert!((hits[1].0 - hits[0].0) < MOUSEEVENT_PICK_DEPTH_BAND);
        assert_eq!(best_mouseevent_hit(&hits), Some(d11));
    }

    #[test]
    fn clearly_nearer_surface_still_wins_outside_the_band() {
        let hits = [(0.40, 0.02_f32.powi(2), 0usize), (0.50, 0.0, 1usize)];
        assert_eq!(best_mouseevent_hit(&hits), Some(0));
    }

    #[test]
    fn empty_hits_yield_none() {
        assert_eq!(best_mouseevent_hit(&[]), None);
    }
}

/// The page of an `[htmltexture]` a ray lands on: its script texture index and the place on it
/// (`u`/`v` from 0 to 1, `v` down from the top), for the nearest such surface. The nearest
/// triangle of a mesh decides: a bezel of the same mesh in front of the screen takes the click
/// away from it. `also`: script textures clicked like pages (those `.opl` plugins draw).
pub(crate) fn pick_html_in(vehicle: &omsi_sim::VehicleInstance, origin: DVec3, dir: Vec3, also: &[usize]) -> Option<(usize, f32, f32)> {
    let pages: Vec<usize> = vehicle.html_textures.iter().map(|t| t.script_index).chain(also.iter().copied()).collect();
    if pages.is_empty() {
        return None;
    }
    let o = (origin - vehicle.position).as_vec3();
    let mut best: Option<(f32, usize, f32, f32)> = None;
    for (i, vm) in vehicle.ty.meshes.iter().enumerate() {
        if !vehicle.mesh_props[i].visible {
            continue;
        }
        let def = &vehicle.ty.model.meshes[vm.def_index];
        let shows_page = |n: Option<i32>| n.is_some_and(|n| pages.contains(&(n.max(0) as usize)));
        if !def.materials.iter().any(|m| shows_page(m.use_script_texture)) {
            continue;
        }
        let xf = vehicle.mesh_local_transform(i);
        if !ray_may_hit(&vehicle.ty, i, &xf, o, dir, 0.0) {
            continue;
        }
        let Some(hit) = omsi_geometry::ray_mesh_hit(o, dir, &vm.data, &xf) else {
            continue;
        };
        // the page the hit material slot shows (a slot that shows none is in the way)
        let slot = vm.data.slot_of(hit.index) as usize;
        let page = def
            .materials
            .iter()
            .filter(|m| omsi_sim::vehicle::override_slot(&vm.materials, m) == Some(slot))
            .find_map(|m| m.use_script_texture)
            .map(|n| n.max(0) as usize)
            .filter(|n| pages.contains(n));
        let Some(page) = page else { continue };
        if best.is_none_or(|b| hit.t < b.0) {
            best = Some((hit.t, page, hit.uv.x.clamp(0.0, 1.0), hit.uv.y.clamp(0.0, 1.0)));
        }
    }
    best.map(|(_, page, u, v)| (page, u, v))
}

/// The same forgiving pick as `pick`, but for the coupled sections of an articulated
/// bus.  The old picker only searched the lead vehicle, so GN92's rear door opener and
/// every other button in the second section could never receive a click.
pub(crate) fn pick_trailer_in(vehicle: &omsi_sim::VehicleInstance, origin: DVec3, dir: Vec3, spread: f32) -> Option<(usize, usize)> {
    let right = Vec3::new(-dir.y, dir.x, 0.0).normalize_or_zero();
    let up = dir.cross(right).normalize_or_zero();
    let mut rings: Vec<Vec<Vec3>> = vec![vec![dir]];
    if spread > 0.0 {
        for ring in 1..=2 {
            let r = spread * ring as f32;
            let n = 8 * ring;
            rings.push(
                (0..n)
                    .map(|k| {
                        let a = k as f32 / n as f32 * std::f32::consts::TAU;
                        (dir + right * (a.cos() * r) + up * (a.sin() * r)).normalize()
                    })
                    .collect(),
            );
        }
    }
    let candidates: Vec<(usize, usize, glam::Mat4, Vec<u32>)> = vehicle.trailers.iter().enumerate().flat_map(|(ti, trailer)| {
        let o = (origin - trailer.position).as_vec3();
        trailer.ty.meshes.iter().enumerate().filter_map(move |(i, vm)| {
            if trailer.ty.model.meshes[vm.def_index].mouse_event.is_none() || !trailer.mesh_props[i].visible {
                return None;
            }
            let xf = trailer.mesh_local_transform(i);
            if !ray_may_hit(&trailer.ty, i, &xf, o, dir, spread * 2.2) {
                return None;
            }
            let tris = omsi_geometry::cone_triangles(o, dir, spread * 2.0 + 1e-4, &vm.data, &xf);
            (!tris.is_empty()).then_some((ti, i, xf, tris))
        })
    }).collect();
    for dirs in &rings {
        let mut hits: Vec<(f32, f32, usize)> = Vec::new();
        let mut id_of: Vec<(usize, usize)> = Vec::new();
        for (ti, i, xf, tris) in &candidates {
            let (ti, i, xf) = (*ti, *i, *xf);
            let trailer = &vehicle.trailers[ti];
            let o = (origin - trailer.position).as_vec3();
            let vm = &trailer.ty.meshes[i];
            let lateral = trailer
                .ty
                .mesh_bounds
                .get(i)
                .map(|&(c, _)| {
                    let p = xf.transform_point3(c) - o;
                    let along = p.dot(dir);
                    (p - dir * along).length_squared()
                })
                .unwrap_or(0.0);
            for d in dirs {
                if let Some(t) = omsi_geometry::ray_triangles(o, *d, &vm.data, &xf, tris) {
                    let id = id_of.len();
                    id_of.push((ti, i));
                    hits.push((t, lateral, id));
                }
            }
        }
        if let Some(id) = best_mouseevent_hit(&hits) {
            return Some(id_of[id]);
        }
    }
    None
}
