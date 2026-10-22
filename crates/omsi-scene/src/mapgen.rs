//! The map's picture for the navigator, the city map and the launcher, read off the map
//! itself (`omsi_map::layers`): no package needed.
//!
//! * **Houses**: every object whose model is as tall and as wide as a house. Its footprint is
//!   what its roofs cover seen from above (the model rasterised at half a metre in its own
//!   frame, the ground plates and pavements a house model often brings left out), traced and
//!   simplified, its height the roofs' height. One object type is read once however often it
//!   stands.
//! * **Paved ground as it lies**: the road surfaces of every spline's profile (asphalt and
//!   cobbles as carriageway, pavements apart), the flat road-textured faces of the objects
//!   (junctions, bus bays, car parks) and the painted asphalt and concrete of the ground.
//! * **The ground**: what each `[groundtex]` layer is (grass, field, sand, paved - by the
//!   texture's own `[surface]`, its name, else its colour) where the brush has put it.
//! * **Water** where a tile's water stands over its terrain, and splines textured as water;
//!   **woods** where trees stand close, and the single trees; **rail and tram** tracks.
//!
//! The result is kept in the user's cache (`omsi_map::layers::cache_path`) under a key of the
//! tiles' sizes and dates, so only the first visit of a map pays for the read (some seconds
//! on a big map, on a worker thread).

use glam::{DVec2, Vec2, Vec3};
use hashbrown::HashMap;
use omsi_map::layers::{self, Building, Layers, Line};
use omsi_map::tile_size;
use parking_lot::Mutex;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

/// What one object type contributes, in its own frame.
#[derive(Default)]
struct TypeInfo {
    /// Footprint rings (outer anticlockwise, holes clockwise) and height.
    house: Option<(Vec<Vec<Vec2>>, f32)>,
    /// Flat paved faces: class and triangle.
    paved: Vec<(usize, [Vec2; 3])>,
    /// Crown radius of a tree.
    tree: Option<f32>,
}

/// Cells of the ground's class grid per tile edge (some 4.7 m on a 300 m tile).
const LAND_CELLS: usize = 64;
/// Cell of a house's footprint raster (m).
const HOUSE_CELL: f32 = 0.4;

fn has_any(name: &str, keys: &[&str]) -> bool {
    keys.iter().any(|k| name.contains(k))
}

fn stem(file: &str) -> String {
    file.replace('\\', "/").to_ascii_lowercase()
}

/// Names of objects that are no house however big they are.
fn never_a_house(file: &str) -> bool {
    let n = stem(file);
    has_any(
        &n,
        &[
            "bruecke", "brücke", "bridge", "viadukt", "viaduct", "tunnel", "mauer", "wall", "zaun", "fence", "laerm", "lärm", "noise", "baum", "tree", "hecke", "hedge", "busch", "bush", "gleis", "track", "mast", "lampe", "lamp", "ampel", "signal", "schild", "sign", "dummy", "helper", "invis", "unsicht", "kreuzung", "crossing", "junction", "strasse", "straße", "road", "parkplatz", "parking", "haltestelle", "busstop", "wartehalle", "shelter", "terrain", "boden", "ground", "wiese", "wald", "forest", "water", "wasser", "berg", "felsen", "rock", "pkw", "lkw", "_car", "vehicle", "fahrzeug", "bus_", "/bus", "tram", "strab", "zug", "train", "oberleitung", "catenary", "wire", "kabel", "pole",
        ],
    )
}

fn tree_name(file: &str) -> bool {
    let n = stem(file);
    let base = n.rsplit('/').next().unwrap_or(&n);
    has_any(&n, &["/trees/", "/baeume/", "/bäume/", "/vegetation/", "/pflanzen/"])
        || has_any(base, &["baum", "tree", "busch", "bush", "strauch", "hecke", "hedge", "tanne", "fichte", "kiefer", "birke", "eiche", "linde", "buche", "ahorn", "kastanie", "pappel", "weide", "pine", "spruce", "birch", "oak", "maple", "poplar", "willow", "conifer", "shrub"])
}

/// A texture of road (carriageway), pavement, or neither.
fn paving_of(texture: &str) -> Option<usize> {
    let n = stem(texture);
    let base = n.rsplit('/').next().unwrap_or(&n);
    if has_any(base, &["gruen", "grün", "gras", "rasen", "wiese", "lawn"]) {
        return Some(layers::PAVE_GREEN);
    }
    if has_any(base, &["gehweg", "gw_", "sidewalk", "pavement", "bordst", "kerb", "curb", "fussweg", "fußweg", "footpath", "plaster_gw", "radweg", "cycle", "str_side", "side1", "side2"]) {
        return Some(layers::PAVE_FOOT);
    }
    if crate::open::road_surface_name(texture) || has_any(base, &["asphalt", "asph", "teer", "tarmac", "fahrbahn", "kopfstein", "cobble", "pflaster", "beton", "concrete", "parkpl"]) {
        return Some(layers::PAVE_ROAD);
    }
    None
}

fn water_name(texture: &str) -> bool {
    let n = stem(texture);
    let base = n.rsplit('/').next().unwrap_or(&n);
    has_any(base, &["wasser", "water", "fluss", "river", "bach", "kanal", "canal", "see_", "lake", "teich", "pond", "havel", "spree"]) && !has_any(base, &["rinne", "gully", "puddle", "pfuetze", "drain"])
}

/// The ground class of a `[groundtex]`: its `[surface]` or name, and where that says nothing
/// (an unknown name reads as asphalt) its colour.
fn land_class(texture: &str, dirs: &[&Path]) -> usize {
    let n = stem(texture);
    if water_name(&n) {
        return layers::LAND_WATER;
    }
    if has_any(&n, &["wald", "forest", "nadel", "laubstreu", "moos", "moss"]) && !n.contains("waldweg") {
        return layers::LAND_FOREST;
    }
    let by_name = crate::terrain_paint::surface_by_name(texture);
    let id = crate::terrain_paint::surface_id(texture, dirs);
    // (0 is also what an unknown name gives: then the picture decides)
    if id == 0 && by_name == 0 && !has_any(&n, &["asph", "teer", "tarmac", "road", "str"]) {
        if let Some(c) = mean_color(texture, dirs) {
            return class_by_color(c);
        }
    }
    match id {
        4 | 6 => layers::LAND_GRASS,
        3 => {
            if has_any(&n, &["sand", "strand", "beach"]) {
                layers::LAND_SAND
            } else {
                layers::LAND_FIELD
            }
        }
        5 => layers::LAND_SAND,
        _ => layers::LAND_PAVED,
    }
}

fn class_by_color(c: Vec3) -> usize {
    let (r, g, b) = (c.x, c.y, c.z);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let sat = if max > 0.0 { (max - min) / max } else { 0.0 };
    if b > r * 1.15 && b > g * 1.05 && sat > 0.2 {
        layers::LAND_WATER
    } else if g >= r && g >= b && sat > 0.12 {
        if max < 0.28 { layers::LAND_FOREST } else { layers::LAND_GRASS }
    } else if r > b * 1.15 && sat > 0.15 {
        if r > 0.55 && g > 0.45 { layers::LAND_SAND } else { layers::LAND_FIELD }
    } else {
        layers::LAND_PAVED
    }
}

fn mean_color(texture: &str, dirs: &[&Path]) -> Option<Vec3> {
    let found = omsi_texture::find_texture(texture, dirs)?;
    let img = omsi_texture::decode_file(&found).ok()?;
    let n = (img.width * img.height) as usize;
    if n == 0 {
        return None;
    }
    let step = (n / 4096).max(1);
    let mut sum = Vec3::ZERO;
    let mut k = 0.0;
    for px in img.rgba.chunks_exact(4).step_by(step) {
        sum += Vec3::new(px[0] as f32, px[1] as f32, px[2] as f32) / 255.0;
        k += 1.0;
    }
    (k > 0.0).then(|| sum / k)
}

/// What an object type gives the map (see the module's notes).
fn type_info(root: &Path, file: &str) -> Option<TypeInfo> {
    let path = omsi_cfg::resolve_path(root, file);
    let sco = omsi_scenery::sco::SceneryObject::load(&path).ok()?;
    let mut info = TypeInfo::default();
    if sco.only_editor {
        return Some(info);
    }
    let sco_dir = path.parent()?.to_path_buf();
    let (model, model_dir) = match &sco.model_file {
        Some(m) => {
            let mp = omsi_cfg::resolve_path(&sco_dir, m);
            (omsi_model::Model::load(&mp).ok()?, mp.parent()?.to_path_buf())
        }
        None => (sco.model.clone(), sco_dir.clone()),
    };
    let is_tree = sco.tree.is_some() || tree_name(file);
    if let Some(t) = &sco.tree {
        // ([tree]: texture, min and max height, min and max width-to-height)
        let h = (t.1 + t.2) * 0.5;
        let w = (t.3 + t.4) * 0.5;
        info.tree = Some((h * w * 0.5).clamp(1.0, 9.0));
        return Some(info);
    }
    if model.lods.is_empty() {
        return Some(info);
    }
    // (every mesh of the most detailed level: the triangles with their textures)
    let mut tris: Vec<([Vec3; 3], Option<usize>)> = Vec::new();
    for md in model.lod_meshes(0) {
        if md.mouse_event.is_some() || md.is_shadow {
            continue;
        }
        let mp = omsi_cfg::resolve_path(&omsi_cfg::resolve_path(&model_dir, "model"), &md.file);
        let mp = if omsi_cfg::vfs::is_file(&mp) { mp } else { omsi_cfg::resolve_path(&model_dir, &md.file) };
        let Ok(m) = omsi_o3d::load_mesh(&mp) else { continue };
        let swap = |v: Vec3| Vec3::new(v.x, v.z, v.y);
        for t in &m.triangles {
            let p = t.indices.map(|i| m.vertices.get(i as usize).map(|v| swap(v.position)).unwrap_or(Vec3::ZERO));
            let tex = m.materials.get(t.material as usize).map(|mt| mt.texture.as_str()).unwrap_or("");
            tris.push((p, paving_of(tex)));
        }
    }
    if tris.is_empty() {
        return Some(info);
    }
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for (t, _) in &tris {
        for p in t {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
    }
    if !(lo.x.is_finite() && hi.x.is_finite()) || (hi - lo).max_element() > 2000.0 {
        return Some(info);
    }
    if is_tree {
        info.tree = Some(((hi.x - lo.x).max(hi.y - lo.y) * 0.5).clamp(1.0, 9.0));
        return Some(info);
    }
    // the ground's faces as the model lays them, pressed flat (carriageways, pavements, kerbs,
    // islands): the faces seen from above near its foot
    for (t, class) in &tris {
        let Some(class) = class else { continue };
        let n = (t[1] - t[0]).cross(t[2] - t[0]);
        if n.length_squared() < 1e-6 || n.z.abs() < n.length() * 0.5 || t.iter().any(|p| p.z > lo.z + 1.5) {
            continue;
        }
        let class = if *class == layers::PAVE_ROAD && sco.paths.is_empty() { layers::PAVE_AREA } else { *class };
        info.paved.push((class, [t[0].truncate(), t[1].truncate(), t[2].truncate()]));
    }
    if sco.surface || !sco.paths.is_empty() || sco.is_traffic_light || sco.is_bus_stop || never_a_house(file) {
        return Some(info);
    }
    let size = hi - lo;
    if size.z < 2.8 || size.x < 3.0 || size.y < 3.0 {
        return Some(info);
    }
    // the roofs: what stands 2 m or more above the model's ground, seen from above
    let base = lo.z.max(-1.0);
    let top_tris: Vec<[Vec2; 3]> = tris.iter().filter(|(t, _)| t.iter().all(|p| p.z > base + 2.0)).map(|(t, _)| [t[0].truncate(), t[1].truncate(), t[2].truncate()]).collect();
    if top_tris.is_empty() {
        return Some(info);
    }
    let (w, h) = (((size.x / HOUSE_CELL).ceil() as usize + 3).min(1200), ((size.y / HOUSE_CELL).ceil() as usize + 3).min(1200));
    let org = lo.truncate() - Vec2::splat(HOUSE_CELL * 1.5);
    let mut cells = vec![false; w * h];
    for t in &top_tris {
        let a = (t[1] - t[0]).perp_dot(t[2] - t[0]);
        if a.abs() < 1e-4 {
            continue;
        }
        let tlo = t[0].min(t[1]).min(t[2]);
        let thi = t[0].max(t[1]).max(t[2]);
        let (x0, y0) = (((tlo.x - org.x) / HOUSE_CELL).floor().max(0.0) as usize, ((tlo.y - org.y) / HOUSE_CELL).floor().max(0.0) as usize);
        let (x1, y1) = ((((thi.x - org.x) / HOUSE_CELL).ceil() as usize).min(w), (((thi.y - org.y) / HOUSE_CELL).ceil() as usize).min(h));
        for y in y0..y1 {
            for x in x0..x1 {
                let q = org + Vec2::new((x as f32 + 0.5) * HOUSE_CELL, (y as f32 + 0.5) * HOUSE_CELL);
                let s = a.signum();
                let e0 = (t[1] - t[0]).perp_dot(q - t[0]) * s;
                let e1 = (t[2] - t[1]).perp_dot(q - t[1]) * s;
                let e2 = (t[0] - t[2]).perp_dot(q - t[2]) * s;
                if e0 >= -0.05 && e1 >= -0.05 && e2 >= -0.05 {
                    cells[y * w + x] = true;
                }
            }
        }
    }
    // (closed by a cell: the gaps between a roof's faces)
    let grow = |c: &[bool], set: bool| -> Vec<bool> {
        let mut out = c.to_vec();
        for y in 0..h {
            for x in 0..w {
                let n = [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)];
                let any = n.iter().any(|&(i, j)| i < w && j < h && c[j * w + i] == set);
                if c[y * w + x] != set && any {
                    out[y * w + x] = set;
                }
            }
        }
        out
    };
    let cells = grow(&grow(&cells, true), false);
    let covered = cells.iter().filter(|c| **c).count() as f32 * HOUSE_CELL * HOUSE_CELL;
    // (a roof over less than 40 % of its box is a frame, a canopy, a mast - no house)
    if covered < 12.0 || covered < size.x * size.y * 0.25 {
        return Some(info);
    }
    let rings: Vec<Vec<Vec2>> = layers::trace_cells(&cells, w, h)
        .into_iter()
        .map(|r| layers::simplify_ring(&r, 1.3).into_iter().map(|p| org + p * HOUSE_CELL).collect::<Vec<Vec2>>())
        .filter(|r: &Vec<Vec2>| r.len() >= 3 && layers::area2(r).abs() * 0.5 >= 4.0)
        .collect();
    if rings.is_empty() {
        return Some(info);
    }
    // the height: of the roofs, a chimney or an aerial aside
    let mut zs: Vec<f32> = tris.iter().flat_map(|(t, _)| t.iter().map(|p| p.z)).filter(|z| *z > base + 2.0).collect();
    zs.sort_by(|a, b| a.total_cmp(b));
    let top = zs.get(((zs.len() as f32 * 0.97) as usize).min(zs.len().saturating_sub(1))).copied().unwrap_or(hi.z);
    info.house = Some((rings, (top - base).clamp(3.0, 250.0)));
    Some(info)
}

/// A roof's triangles from its rings (outer anticlockwise, holes clockwise): each hole is
/// bridged to the outline, then the one ring is clipped.
pub(crate) fn roof_of(rings: &[Vec<Vec2>]) -> (Vec<Vec2>, Vec<[u16; 3]>) {
    let mut outers: Vec<Vec<Vec2>> = rings.iter().filter(|r| layers::area2(r) > 0.0).cloned().collect();
    if outers.is_empty() {
        return (Vec::new(), Vec::new());
    }
    outers.sort_by(|a, b| layers::area2(b).total_cmp(&layers::area2(a)));
    let mut outline = outers.swap_remove(0);
    let mut holes: Vec<&Vec<Vec2>> = rings.iter().filter(|r| layers::area2(r) < 0.0).collect();
    holes.sort_by(|a, b| {
        let ma = a.iter().map(|p| p.x).fold(f32::MIN, f32::max);
        let mb = b.iter().map(|p| p.x).fold(f32::MIN, f32::max);
        mb.total_cmp(&ma)
    });
    let crosses = |a: Vec2, b: Vec2, ring: &[Vec2]| {
        let n = ring.len();
        (0..n).any(|i| {
            let (c, d) = (ring[i], ring[(i + 1) % n]);
            if c == a || d == a || c == b || d == b {
                return false;
            }
            let o = |p: Vec2, q: Vec2, r: Vec2| (q - p).perp_dot(r - p);
            o(a, b, c) * o(a, b, d) < 0.0 && o(c, d, a) * o(c, d, b) < 0.0
        })
    };
    for hole in holes {
        // (only a hole inside this outline)
        let Some((hi, hp)) = hole.iter().enumerate().max_by(|a, b| a.1.x.total_cmp(&b.1.x)).map(|(i, p)| (i, *p)) else { continue };
        let mut cands: Vec<usize> = (0..outline.len()).collect();
        cands.sort_by(|&a, &b| outline[a].distance_squared(hp).total_cmp(&outline[b].distance_squared(hp)));
        let Some(oi) = cands.into_iter().find(|&i| !crosses(hp, outline[i], &outline) && !crosses(hp, outline[i], hole)) else { continue };
        let mut merged = Vec::with_capacity(outline.len() + hole.len() + 2);
        merged.extend_from_slice(&outline[..=oi]);
        for k in 0..=hole.len() {
            merged.push(hole[(hi + k) % hole.len()]);
        }
        merged.push(outline[oi]);
        merged.extend_from_slice(&outline[oi + 1..]);
        outline = merged;
    }
    let tris = layers::triangulate(&outline);
    (outline, tris)
}

/// Marching squares over a grid of `nx` x `ny` node values (row-major, row 0 at y = 0): the
/// triangles of where the value is `iso` or more, `cell` metres a cell from `org`. Whole
/// cells in a row are joined into one rectangle.
pub(crate) fn iso_fill(v: &[f32], nx: usize, ny: usize, cell: f32, org: Vec2, iso: f32, out: &mut Vec<Vec2>) {
    let at = |x: usize, y: usize| v[y * nx + x];
    for y in 0..ny.saturating_sub(1) {
        let mut run: Option<usize> = None;
        let flush = |run: &mut Option<usize>, x_end: usize, out: &mut Vec<Vec2>| {
            if let Some(x0) = run.take() {
                let a = org + Vec2::new(x0 as f32, y as f32) * cell;
                let b = org + Vec2::new(x_end as f32, (y + 1) as f32) * cell;
                out.extend([a, Vec2::new(b.x, a.y), b, a, b, Vec2::new(a.x, b.y)]);
            }
        };
        for x in 0..nx.saturating_sub(1) {
            let c = [at(x, y), at(x + 1, y), at(x + 1, y + 1), at(x, y + 1)];
            let inside = c.map(|k| k >= iso);
            if inside.iter().all(|i| *i) {
                if run.is_none() {
                    run = Some(x);
                }
                continue;
            }
            flush(&mut run, x, out);
            if !inside.iter().any(|i| *i) {
                continue;
            }
            let corners = [Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(1.0, 1.0), Vec2::new(0.0, 1.0)];
            let mut poly: Vec<Vec2> = Vec::with_capacity(6);
            for k in 0..4 {
                let j = (k + 1) % 4;
                if inside[k] {
                    poly.push(corners[k]);
                }
                if inside[k] != inside[j] {
                    let t = ((iso - c[k]) / (c[j] - c[k])).clamp(0.0, 1.0);
                    poly.push(corners[k].lerp(corners[j], t));
                }
            }
            let base = org + Vec2::new(x as f32, y as f32) * cell;
            for k in 1..poly.len().saturating_sub(1) {
                out.extend([base + poly[0] * cell, base + poly[k] * cell, base + poly[k + 1] * cell]);
            }
        }
        flush(&mut run, nx - 1, out);
    }
}

/// A placed piece of a type: where (world) and which way (compass heading, degrees).
fn place(local: Vec2, at: DVec2, heading: f64, origin: DVec2) -> Vec2 {
    let a = (-heading).to_radians();
    let (s, c) = (a.sin(), a.cos());
    let (x, y) = (local.x as f64, local.y as f64);
    let w = DVec2::new(at.x + x * c - y * s, at.y + x * s + y * c);
    (w - origin).as_vec2()
}

/// What one tile gives.
#[derive(Default)]
struct Part {
    tile: [f32; 4],
    land: Vec<Vec<Vec2>>,
    paved: Vec<Vec<Vec2>>,
    buildings: Vec<Building>,
    trees: Vec<[f32; 3]>,
    lines: Vec<Line>,
}

/// The map's layers, read now (see the module's notes). `tiles`: (x, y, path) of every tile.
pub fn generate(root: &Path, map_dir: &Path, ground_textures: &[omsi_map::global::GroundTex], tiles: &[(i32, i32, PathBuf)], chrono_dirs: &[PathBuf]) -> Layers {
    use rayon::prelude::*;
    let t0 = std::time::Instant::now();
    let ts = tile_size();
    let origin = tiles.first().map(|t| DVec2::new(t.0 as f64 * ts, t.1 as f64 * ts)).unwrap_or(DVec2::ZERO);
    let dirs = [root];
    let ground: Vec<usize> = ground_textures.iter().map(|g| land_class(&g.texture, &dirs)).collect();
    let base_class = ground.first().copied().unwrap_or(layers::LAND_GRASS);
    type Once<T> = Arc<OnceLock<Option<Arc<T>>>>;
    let types: Mutex<HashMap<String, Once<TypeInfo>>> = Mutex::new(HashMap::new());
    let type_of = |file: &str| -> Option<Arc<TypeInfo>> {
        let key = stem(file.trim());
        let cell = types.lock().entry(key).or_default().clone();
        cell.get_or_init(|| type_info(root, file).map(Arc::new)).clone()
    };
    let slis: Mutex<HashMap<String, Once<omsi_scenery::sli::Spline>>> = Mutex::new(HashMap::new());
    let sli_of = |file: &str| -> Option<Arc<omsi_scenery::sli::Spline>> {
        let key = stem(file.trim());
        let cell = slis.lock().entry(key).or_default().clone();
        cell.get_or_init(|| omsi_scenery::sli::Spline::load(&omsi_cfg::resolve_path(root, file)).ok().map(Arc::new)).clone()
    };
    let parts: Vec<Part> = tiles
        .par_iter()
        .map(|(tx, ty, path)| {
            let mut part = Part { land: vec![Vec::new(); layers::LAND_CLASSES], paved: vec![Vec::new(); layers::PAVE_CLASSES], ..Default::default() };
            let o2 = DVec2::new(*tx as f64 * ts, *ty as f64 * ts);
            let lo = (o2 - origin).as_vec2();
            part.tile = [lo.x, lo.y, lo.x + ts as f32, lo.y + ts as f32];
            let Some(tile) = omsi_world::map_index::read_tile(path, chrono_dirs) else { return part };
            let terrain = omsi_map::terrain::Terrain::load(&omsi_world::map_index::terrain_file(&tile, path)).unwrap_or_else(|_| omsi_map::terrain::Terrain::flat());
            ground_of(&mut part, map_dir, path, &ground, base_class, lo, ts as f32);
            if tile.has_water {
                water_of(&mut part, &tile, path, &terrain, lo, ts as f32);
            }
            // splines: paved profiles, water, rail and tram
            for sp in tile.splines.iter().filter(|s| !s.deleted && !s.file.trim().is_empty()) {
                let Some(def) = sli_of(&sp.file) else { continue };
                if def.only_editor || sp.length < 0.5 {
                    continue;
                }
                spline_of(&mut part, sp, &def, o2, origin);
            }
            // objects and the rows along splines
            let mut placed: Vec<(String, DVec2, f64)> = Vec::new();
            for o in &tile.objects {
                if !o.file.trim().is_empty() {
                    placed.push((o.file.clone(), o2 + DVec2::new(o.pos[0], o.pos[1]), o.rot[0]));
                }
            }
            for att in &tile.spline_attachments {
                for (_, ro) in omsi_world::map_index::tile_row_objects(att, &tile.splines, o2, None) {
                    placed.push((att.file.clone(), ro.pose.pos.truncate(), ro.pose.heading()));
                }
            }
            for (file, at, heading) in placed {
                let Some(info) = type_of(&file) else { continue };
                if let Some(r) = info.tree {
                    let p = (at - origin).as_vec2();
                    part.trees.push([p.x, p.y, r]);
                }
                for (class, t) in &info.paved {
                    part.paved[*class].extend(t.iter().map(|q| place(*q, at, heading, origin)));
                }
                if let Some((rings, height)) = &info.house {
                    let rings: Vec<Vec<Vec2>> = rings.iter().map(|r| r.iter().map(|q| place(*q, at, heading, origin)).collect()).collect();
                    let (outline, roof) = roof_of(&rings);
                    if roof.is_empty() {
                        continue;
                    }
                    part.buildings.push(Building { outline, roof, height: *height });
                }
            }
            woods_of(&mut part, lo, ts as f32);
            part
        })
        .collect();
    let mut out = Layers::empty(origin);
    for p in parts {
        out.tiles.push(p.tile);
        for (k, l) in p.land.into_iter().enumerate() {
            out.land[k].extend(l);
        }
        for (k, l) in p.paved.into_iter().enumerate() {
            out.paved[k].extend(l);
        }
        out.buildings.extend(p.buildings);
        out.trees.extend(p.trees);
        out.lines.extend(p.lines);
    }
    dedupe_buildings(&mut out.buildings);
    out.paved[layers::PAVE_INNER] = inner_streets(&out);
    log::info!(
        "map layers: {} tiles, {} houses, {} trees, {} paved triangles, {} land triangles, {} lines, {} object types, {:.1} s",
        out.tiles.len(),
        out.buildings.len(),
        out.trees.len(),
        out.paved.iter().map(|p| p.len() / 3).sum::<usize>(),
        out.land.iter().map(|p| p.len() / 3).sum::<usize>(),
        out.lines.len(),
        types.lock().len(),
        t0.elapsed().as_secs_f64()
    );
    out
}

/// How wide the streets' dark rim is (m): a pavement's breadth.
const RIM: f32 = 2.5;
/// Cell of the streets' distance field (m).
const RIM_CELL: f32 = 0.25;

/// Where the streets (carriageways and pavements, all pieces together) lie more than `RIM`
/// from their edge: the bright middle drawn over the dark street, so that the street shows a
/// dark rim. The distance is to the edge of all the paving at once - a spline's end against a
/// junction's model is no edge, and where their outlines differ the rim runs on unbroken; the
/// streets' own faces are left as they are.
fn inner_streets(l: &Layers) -> Vec<Vec2> {
    use rayon::prelude::*;
    const B: f32 = 20.0;
    let key = |p: Vec2| ((p.x / B).floor() as i32, (p.y / B).floor() as i32);
    let tris: Vec<[Vec2; 3]> = [layers::PAVE_ROAD, layers::PAVE_FOOT].iter().flat_map(|&c| l.paved[c].chunks_exact(3).map(|t| [t[0], t[1], t[2]])).filter(|t| (t[1] - t[0]).perp_dot(t[2] - t[0]).abs() > 1e-4).collect();
    let mut grid: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
    for (i, t) in tris.iter().enumerate() {
        let (a, b) = (key(t[0].min(t[1]).min(t[2])), key(t[0].max(t[1]).max(t[2])));
        if (b.0 as i64 - a.0 as i64 + 1) * (b.1 as i64 - a.1 as i64 + 1) > 2500 {
            continue;
        }
        for y in a.1..=b.1 {
            for x in a.0..=b.0 {
                grid.entry((x, y)).or_default().push(i as u32);
            }
        }
    }
    let margin = ((RIM + 1.0) / RIM_CELL).ceil() as usize;
    l.tiles
        .par_iter()
        .flat_map_iter(|t| {
            let (tlo, thi) = (Vec2::new(t[0], t[1]), Vec2::new(t[2], t[3]));
            let core = ((thi.x - tlo.x) / RIM_CELL).round() as usize;
            let n = core + 1 + 2 * margin;
            let org = tlo - Vec2::splat(margin as f32 * RIM_CELL);
            // the nodes on the streets (a little generous: pieces meet without a hairline)
            let mut inside = vec![false; n * n];
            let mut seen = hashbrown::HashSet::new();
            let (ka, kb) = (key(org), key(org + Vec2::splat(n as f32 * RIM_CELL)));
            for gy in ka.1..=kb.1 {
                for gx in ka.0..=kb.0 {
                    for &i in grid.get(&(gx, gy)).map(|v| v.as_slice()).unwrap_or(&[]) {
                        if !seen.insert(i) {
                            continue;
                        }
                        let t = &tris[i as usize];
                        let sg = (t[1] - t[0]).perp_dot(t[2] - t[0]).signum();
                        let (lo, hi) = (t[0].min(t[1]).min(t[2]) - org, t[0].max(t[1]).max(t[2]) - org);
                        let x0 = ((lo.x / RIM_CELL).floor().max(0.0) as usize).min(n);
                        let y0 = ((lo.y / RIM_CELL).floor().max(0.0) as usize).min(n);
                        let x1 = ((hi.x / RIM_CELL).ceil().max(0.0) as usize).min(n - 1);
                        let y1 = ((hi.y / RIM_CELL).ceil().max(0.0) as usize).min(n - 1);
                        for y in y0..=y1 {
                            for x in x0..=x1 {
                                let q = org + Vec2::new(x as f32, y as f32) * RIM_CELL;
                                let e = |u: Vec2, v: Vec2| (v - u).perp_dot(q - u) * sg / (v - u).length().max(1e-4) >= -0.15;
                                if e(t[0], t[1]) && e(t[1], t[2]) && e(t[2], t[0]) {
                                    inside[y * n + x] = true;
                                }
                            }
                        }
                    }
                }
            }
            if !inside.iter().any(|b| *b) {
                return Vec::new().into_iter();
            }
            let d = distance_to_outside(&inside, n);
            // (the tile's own nodes: the neighbours' parts are theirs)
            let m = core + 1;
            let mut v = vec![0.0f32; m * m];
            for y in 0..m {
                for x in 0..m {
                    v[y * m + x] = d[(y + margin) * n + x + margin];
                }
            }
            let mut out = Vec::new();
            iso_fill(&v, m, m, RIM_CELL, tlo, RIM, &mut out);
            out.into_iter()
        })
        .collect()
}

/// Every node's distance (m, `RIM_CELL` a node) to the nearest node off the streets (exact,
/// Felzenszwalb and Huttenlocher's two passes of lower parabola envelopes).
fn distance_to_outside(inside: &[bool], n: usize) -> Vec<f32> {
    const INF: f32 = 1e12;
    let mut f: Vec<f32> = inside.iter().map(|&i| if i { INF } else { 0.0 }).collect();
    let pass = |line: &mut [f32]| {
        let len = line.len();
        let mut v = vec![0usize; len];
        let mut z = vec![0.0f32; len + 1];
        let mut k = 0usize;
        z[0] = f32::MIN;
        z[1] = f32::MAX;
        for q in 1..len {
            loop {
                let p = v[k];
                let s = ((line[q] + (q * q) as f32) - (line[p] + (p * p) as f32)) / (2.0 * (q - p) as f32);
                if s <= z[k] && k > 0 {
                    k -= 1;
                    continue;
                }
                if s <= z[k] {
                    v[0] = q;
                    z[0] = f32::MIN;
                    z[1] = f32::MAX;
                    break;
                }
                k += 1;
                v[k] = q;
                z[k] = s;
                z[k + 1] = f32::MAX;
                break;
            }
        }
        let src = line.to_vec();
        k = 0;
        for q in 0..len {
            while z[k + 1] < q as f32 {
                k += 1;
            }
            let p = v[k];
            line[q] = (q as f32 - p as f32).powi(2) + src[p];
        }
    };
    for y in 0..n {
        pass(&mut f[y * n..(y + 1) * n]);
    }
    let mut col = vec![0.0f32; n];
    for x in 0..n {
        for y in 0..n {
            col[y] = f[y * n + x];
        }
        pass(&mut col);
        for y in 0..n {
            f[y * n + x] = col[y];
        }
    }
    f.into_iter().map(|d| d.min(INF).sqrt() * RIM_CELL).collect()
}

/// Two copies of one house in one place (a map that placed it twice, or a chrono patch): one.
fn dedupe_buildings(b: &mut Vec<Building>) {
    let mut seen: hashbrown::HashSet<(i32, i32, i32)> = hashbrown::HashSet::new();
    b.retain(|h| {
        let c = h.outline.iter().fold(Vec2::ZERO, |a, p| a + *p) / h.outline.len().max(1) as f32;
        seen.insert(((c.x * 2.0).round() as i32, (c.y * 2.0).round() as i32, h.outline.len() as i32))
    });
}

/// The painted ground of a tile: its base class everywhere, and each painted layer's class
/// where the brush put it (the later layer over the earlier).
fn ground_of(part: &mut Part, map_dir: &Path, tile_path: &Path, ground: &[usize], base: usize, lo: Vec2, ts: f32) {
    let n = LAND_CELLS;
    // per cell: the class on top
    let mut top = vec![base; n * n];
    let mut painted_any = false;
    if let Some(name) = tile_path.file_name().map(|n| n.to_string_lossy().to_string()) {
        for (layer, &class) in ground.iter().enumerate().skip(1) {
            let p = omsi_cfg::resolve_path(map_dir, &format!("texture/map/{name}.{layer}.dds"));
            if !omsi_cfg::vfs::is_file(&p) {
                continue;
            }
            let Ok(img) = omsi_texture::decode_file(&p) else { continue };
            let (w, h) = (img.width as usize, img.height as usize);
            if w == 0 || h == 0 {
                continue;
            }
            for j in 0..n {
                for i in 0..n {
                    // (the mask's first row is north; the cell's row 0 south)
                    let (u, v) = ((i as f32 + 0.5) / n as f32, (j as f32 + 0.5) / n as f32);
                    let (px, py) = (((u * w as f32) as usize).min(w - 1), (((1.0 - v) * h as f32) as usize).min(h - 1));
                    if img.rgba[(py * w + px) * 4 + 3] > 127 {
                        top[j * n + i] = class;
                        painted_any = true;
                    }
                }
            }
        }
    }
    let cell = ts / n as f32;
    // the base class under the whole tile (water as land: the tile's own water says where it is)
    let base = if base == layers::LAND_WATER { layers::LAND_GRASS } else { base };
    part.land[base].extend([lo, lo + Vec2::new(ts, 0.0), lo + Vec2::new(ts, ts), lo, lo + Vec2::new(ts, ts), lo + Vec2::new(0.0, ts)]);
    if !painted_any {
        return;
    }
    for class in 0..layers::LAND_CLASSES {
        if class == base || !top.contains(&class) {
            continue;
        }
        // node values: the share of the four cells round a node that are this class
        let nn = n + 1;
        let mut v = vec![0.0f32; nn * nn];
        for j in 0..nn {
            for i in 0..nn {
                let mut s = 0.0;
                let mut k = 0.0;
                for (di, dj) in [(0i64, 0i64), (-1, 0), (0, -1), (-1, -1)] {
                    let (ci, cj) = (i as i64 + di, j as i64 + dj);
                    if ci >= 0 && cj >= 0 && (ci as usize) < n && (cj as usize) < n {
                        k += 1.0;
                        if top[cj as usize * n + ci as usize] == class {
                            s += 1.0;
                        }
                    }
                }
                v[j * nn + i] = if k > 0.0 { s / k } else { 0.0 };
            }
        }
        iso_fill(&v, nn, nn, cell, lo, 0.5, &mut part.land[class]);
    }
}

/// Where a tile's water stands over its ground (the plane through its four corner heights).
fn water_of(part: &mut Part, tile: &omsi_map::Tile, path: &Path, terrain: &omsi_map::terrain::Terrain, lo: Vec2, ts: f32) {
    let w = match omsi_cfg::vfs::read(&omsi_world::map_index::water_file(tile, path)).ok().map(|b| omsi_map::terrain::Water::parse(&b)) {
        Some(w) if w.count == 1 && w.values.len() >= 4 => [w.values[0], w.values[1], w.values[2], w.values[3]],
        _ => return,
    };
    let level = (w[0] + w[1] + w[2] + w[3]) * 0.25;
    let n = 61;
    let cell = ts / (n - 1) as f32;
    let v: Vec<f32> = (0..n * n).map(|k| level - terrain.sample_bilinear((k % n) as f32 * cell, (k / n) as f32 * cell)).collect();
    if v.iter().all(|x| *x < 0.0) {
        return;
    }
    iso_fill(&v, n, n, cell, lo, 0.15, &mut part.land[layers::LAND_WATER]);
}

/// A spline's paved profile sections, water and tracks.
fn spline_of(part: &mut Part, sp: &omsi_map::tile::MapSpline, def: &omsi_scenery::sli::Spline, o2: DVec2, origin: DVec2) {
    // (the cross-sections the game builds the spline with, shifted by its skew: the map's road
    // is the road's own mesh, pressed flat)
    let raw = omsi_geometry::SplineCurve::from_map(sp, o2);
    let n = omsi_geometry::spline_station_count(def, &raw).max(1);
    let curve = raw.with_sli(def);
    let side = if sp.mirror { -1.0 } else { 1.0 };
    let at = |s: f64, x: f64| -> Vec2 { (omsi_geometry::skewed_point(&curve, s, side * x, 0.0).0.truncate() - origin).as_vec2() };
    let has_cars = def.paths.iter().any(|p| p.kind == 0);
    let rails: Vec<f32> = def.paths.iter().filter(|p| p.kind == 2).map(|p| p.start[0]).collect();
    let file = stem(&def.path.to_string_lossy());
    for profile in &def.profiles {
        let tex = def.textures.get(profile.texture).map(|t| t.file.as_str()).unwrap_or("");
        let class = if water_name(tex) {
            Some(usize::MAX)
        } else {
            paving_of(tex).map(|c| if c == layers::PAVE_ROAD && !has_cars && rails.is_empty() && !crate::open::road_surface_name(&file) { layers::PAVE_AREA } else { c })
        };
        let Some(class) = class else { continue };
        for pair in profile.points.windows(2) {
            let (a, b) = (&pair[0], &pair[1]);
            let (x0, x1) = (a.x.min(b.x) as f64, a.x.max(b.x) as f64);
            // (the whole profile pressed flat: only what has no width from above - a kerb's
            // face, a fence, a wall - and what stands high over the ground drop out)
            if x1 - x0 < 0.02 || a.z.max(b.z) > 1.0 {
                continue;
            }
            let target = if class == usize::MAX { &mut part.land[layers::LAND_WATER] } else { &mut part.paved[class] };
            for k in 0..n {
                let (s0, s1) = (curve.length * k as f64 / n as f64, curve.length * (k + 1) as f64 / n as f64);
                let (p0, p1, p2, p3) = (at(s0, x0), at(s0, x1), at(s1, x1), at(s1, x0));
                target.extend([p0, p1, p2, p0, p2, p3]);
            }
        }
    }
    if !rails.is_empty() {
        let mid = rails.iter().sum::<f32>() / rails.len() as f32;
        let span = rails.iter().fold(0.0f32, |m, r| m.max((r - mid).abs())) * 2.0;
        let tram = has_cars || has_any(&file, &["tram", "strab", "stadtbahn", "lightrail"]);
        let m = ((curve.length / 4.0).ceil() as usize).clamp(1, 500);
        let pts: Vec<Vec2> = (0..=m).map(|k| at(curve.length * k as f64 / m as f64, mid as f64)).collect();
        part.lines.push(Line { class: if tram { layers::LINE_TRAM } else { layers::LINE_RAIL }, width: span + 1.5, points: pts });
    }
}

/// Woods where trees stand close: a field of the crowns, its dense parts.
fn woods_of(part: &mut Part, lo: Vec2, ts: f32) {
    if part.trees.len() < 8 {
        return;
    }
    let n = 49;
    let cell = ts / (n - 1) as f32;
    let mut v = vec![0.0f32; n * n];
    for t in &part.trees {
        let (p, r) = (Vec2::new(t[0], t[1]) - lo, t[2]);
        let sigma = r + 4.0;
        let reach = (sigma * 2.5 / cell).ceil() as i64;
        let (ci, cj) = ((p.x / cell).round() as i64, (p.y / cell).round() as i64);
        for j in (cj - reach).max(0)..=(cj + reach).min(n as i64 - 1) {
            for i in (ci - reach).max(0)..=(ci + reach).min(n as i64 - 1) {
                let q = Vec2::new(i as f32 * cell, j as f32 * cell);
                let d2 = q.distance_squared(p);
                v[j as usize * n + i as usize] += 0.34 * (-d2 / (2.0 * sigma * sigma)).exp();
            }
        }
    }
    iso_fill(&v, n, n, cell, lo, 0.5, &mut part.land[layers::LAND_FOREST]);
}

/// The key of a map's layers: the generator's version, and every tile's and painting's size
/// and date (a map changed in the editor is read again).
fn key_of(map_dir: &Path, tiles: &[(i32, i32, PathBuf)]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    layers::VERSION.hash(&mut h);
    let stamp = |p: &Path, h: &mut std::collections::hash_map::DefaultHasher| {
        if let Ok(m) = std::fs::metadata(p) {
            m.len().hash(h);
            m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).hash(h);
        }
    };
    stamp(&map_dir.join("global.cfg"), &mut h);
    for (x, y, p) in tiles {
        (x, y).hash(&mut h);
        p.hash(&mut h);
        stamp(p, &mut h);
    }
    stamp(&map_dir.join("texture").join("map"), &mut h);
    h.finish()
}

/// The map's layers from the cache, else read now and kept there.
pub fn cached(root: &Path, map_dir: &Path, ground_textures: &[omsi_map::global::GroundTex], tiles: &[(i32, i32, PathBuf)], chrono_dirs: &[PathBuf]) -> Layers {
    let key = key_of(map_dir, tiles);
    let name = map_dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "map".into());
    let path = layers::cache_path(&name).filter(|_| !omsi_cfg::flags::OMSI_NO_MAP_CACHE.is_set());
    if let Some(l) = path.as_ref().and_then(|p| std::fs::read(p).ok()).and_then(|b| Layers::from_bytes(&b, key)) {
        log::info!("map layers: {} houses from the cache", l.buildings.len());
        return l;
    }
    let l = generate(root, map_dir, ground_textures, tiles, chrono_dirs);
    if let Some(p) = path {
        let _ = std::fs::create_dir_all(p.parent().unwrap_or(Path::new(".")));
        let tmp = p.with_extension(format!("{}.tmp", std::process::id()));
        if std::fs::write(&tmp, l.to_bytes(key)).is_ok() {
            let _ = std::fs::rename(&tmp, &p);
        }
    }
    l
}

impl crate::World {
    /// The map's layers (`mapgen`): from the cache or read now - call it off the main thread.
    pub fn map_layers(&self) -> Layers {
        let tiles: Vec<(i32, i32, PathBuf)> = self.map_tiles().into_iter().map(|(_, x, y, p)| (x, y, p)).collect();
        cached(&self.root, &self.map_dir, &self.global.ground_textures, &tiles, &self.chrono_dirs())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_courtyard_house_keeps_its_yard() {
        let outer = vec![Vec2::new(0.0, 0.0), Vec2::new(20.0, 0.0), Vec2::new(20.0, 20.0), Vec2::new(0.0, 20.0)];
        let hole = vec![Vec2::new(6.0, 6.0), Vec2::new(6.0, 14.0), Vec2::new(14.0, 14.0), Vec2::new(14.0, 6.0)];
        let (outline, tris) = roof_of(&[outer, hole]);
        let area: f32 = tris.iter().map(|t| (outline[t[1] as usize] - outline[t[0] as usize]).perp_dot(outline[t[2] as usize] - outline[t[0] as usize]) * 0.5).sum();
        assert!((area - (400.0 - 64.0)).abs() < 0.5, "{area}");
    }

    #[test]
    fn marching_squares_fill_what_is_over_the_level() {
        // a 3 x 3 grid of nodes, only the middle node high: a diamond of half-cells
        let v = [0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0];
        let mut out = Vec::new();
        iso_fill(&v, 3, 3, 1.0, Vec2::ZERO, 0.5, &mut out);
        let area: f32 = out.chunks_exact(3).map(|t| (t[1] - t[0]).perp_dot(t[2] - t[0]).abs() * 0.5).sum();
        assert!((area - 0.5).abs() < 1e-4, "{area}");
        // all over: two rows of one rectangle each
        let mut out = Vec::new();
        iso_fill(&[1.0; 9], 3, 3, 2.0, Vec2::ZERO, 0.5, &mut out);
        assert_eq!(out.len(), 12);
    }

    #[test]
    fn street_names_are_told_from_stations_and_crossings() {
        for n in ["Hoher Steinweg", "Moritzstr", "Falkenseer Chaussee", "Hauptstraße", "Station Road", "ul. Piotrkowska", "Rue de Rivoli", "Westerwaldstraße", "улица Ленина"] {
            assert!(crate::street_name_of(n).is_some(), "{n}");
        }
        assert_eq!(crate::clean_sign_text("F_Hamburger Str"), "Hamburger Str");
        assert_eq!(crate::street_name_of("B_Moritzstr").as_deref(), Some("Moritzstr"));
        for n in ["U Rathaus Spandau", "Heerstr/Wilhelmstr", "S+U Bf. Zoo", "Am Omnibushof", "Kino", "Maxim-Kolbe"] {
            assert!(crate::street_name_of(n).is_none(), "{n}");
        }
    }

    #[test]
    fn ground_textures_are_told_by_colour_where_the_name_says_nothing() {
        assert_eq!(class_by_color(Vec3::new(0.35, 0.5, 0.2)), layers::LAND_GRASS);
    }

    #[test]
    fn the_distance_to_the_outside_is_exact() {
        // a 9 x 9 node square on the streets in a 13 x 13 grid
        let n = 13;
        let inside: Vec<bool> = (0..n * n).map(|k| (2..=10).contains(&(k % n)) && (2..=10).contains(&(k / n))).collect();
        let d = distance_to_outside(&inside, n);
        assert_eq!(d[6 * n + 6], 5.0 * RIM_CELL);
        assert_eq!(d[6 * n + 2], RIM_CELL);
        assert_eq!(d[0], 0.0);
        assert_eq!(class_by_color(Vec3::new(0.5, 0.5, 0.5)), layers::LAND_PAVED);
        assert_eq!(class_by_color(Vec3::new(0.45, 0.33, 0.2)), layers::LAND_FIELD);
    }
}
