//! `global.cfg` (unit `mc_mapclass`, `TMap.loadGlobalFile`).

use omsi_cfg::CfgFile;
use std::path::{Path, PathBuf};

/// One `[groundtex]` of `global.cfg`: a ground texture the map can be painted with.
///
/// The three numbers are the editor's settings for the layer: the size of the painting mask
/// as a power of two (`8` = 256×256 alpha texels over the tile - the stock masks in
/// `texture/map/tile_x_y.map.<layer>.dds` are exactly that size), how often the texture
/// repeats across the tile, and how often the detail texture does. The first entry is the
/// ground everything starts as and has no mask of its own.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GroundTex {
    pub texture: String,
    pub detail_texture: String,
    pub params: [f32; 3],
}

impl GroundTex {
    /// Edge length of this layer's painting mask in texels (`2^params[0]`).
    pub fn mask_size(&self) -> u32 {
        let p = self.params[0].round() as i32;
        if (1..=13).contains(&p) {
            1 << p
        } else {
            256
        }
    }

    /// How often the texture repeats across one tile.
    pub fn repeats(&self) -> f32 {
        if self.params[1] > 0.0 {
            self.params[1]
        } else {
            1.0
        }
    }

    /// How often the detail texture repeats across one tile.
    pub fn detail_repeats(&self) -> f32 {
        if self.params[2] > 0.0 {
            self.params[2]
        } else {
            self.repeats()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct MapTileRef {
    pub x: i32,
    pub y: i32,
    pub file: String,
    /// Its place in global.cfg's `[map]` list (0-based, every entry counted): the number
    /// entry points, repeaters and timetable tracks name a tile by.
    pub index: usize,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Season {
    pub kind: i32,
    pub start_day: i32,
    pub end_day: i32,
}

impl Season {
    /// Texture subfolder of a season kind: 1 spring, 2 autumn, 3 winter, 4 winter with snow,
    /// 5 dry summer (`SummerDry`, the editor's "Summer (dry)": Omsi.exe 0x7f93a6).
    pub fn folder(kind: i32) -> Option<&'static str> {
        match kind {
            1 => Some("spring"),
            2 => Some("fall"),
            3 => Some("Winter"),
            4 => Some("WinterSnow"),
            5 => Some("SummerDry"),
            _ => None,
        }
    }
}

/// Value of an hourly curve `[(hour, value)]` at `hour` (linear between points, first/last
/// value outside).
pub fn curve_at(curve: &[(f32, f32)], hour: f32) -> f32 {
    if curve.is_empty() {
        return 1.0;
    }
    let mut pts: Vec<(f32, f32)> = curve.to_vec();
    pts.sort_by(|a, b| a.0.total_cmp(&b.0));
    if hour <= pts[0].0 {
        return pts[0].1;
    }
    for w in pts.windows(2) {
        if hour >= w[0].0 && hour <= w[1].0 {
            let t = if w[1].0 > w[0].0 { (hour - w[0].0) / (w[1].0 - w[0].0) } else { 0.0 };
            return w[0].1 + (w[1].1 - w[0].1) * t;
        }
    }
    pts[pts.len() - 1].1
}

/// `[entrypoints]` record: the entry point is a scenery object (`[entrypoint]`) placed on the
/// map; the position is stored in the object's own frame order (x, height, y) and the
/// orientation as a quaternion.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EntryPoint {
    pub index: i32,
    pub object_id: i64,
    pub unknown: i32,
    pub pos: [f64; 3],
    pub quat: [f64; 4],
    pub group: i32,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct GlobalCfg {
    pub path: PathBuf,
    pub name: String,
    pub friendly_name: String,
    pub description: String,
    pub version: i32,
    pub next_id_code: i64,
    pub world_coordinates: bool,
    pub dyn_helper_active: bool,
    pub real_rail: bool,
    pub left_hand_traffic: bool,
    pub background_image: Vec<String>,
    pub map_cam: Vec<f64>,
    pub money_system: String,
    pub ticket_pack: String,
    pub repair_time_min: f32,
    pub years: (i32, i32),
    pub real_year_offset: i32,
    pub standard_depot: String,
    pub ground_textures: Vec<GroundTex>,
    pub seasons: Vec<Season>,
    pub traffic_density_road: Vec<(f32, f32)>,
    pub traffic_density_passenger: Vec<(f32, f32)>,
    pub entry_points: Vec<EntryPoint>,
    pub spline_obj_types: Vec<String>,
    pub scen_obj_list: Vec<String>,
    pub tiles: Vec<MapTileRef>,
    /// Every `[map]` entry's tile in file order, an entry listed twice too: a tile index of
    /// the map's files counts those (Westcountry 3 lists 33 tiles twice, and every index
    /// after the first of them named the wrong tile).
    pub raw_tiles: Vec<(i32, i32)>,
    pub unknown_keywords: Vec<(String, usize)>,
}

impl GlobalCfg {
    /// Season kind for a day of the year (0 = summer / none).
    pub fn season_kind(&self, day_of_year: i32) -> i32 {
        self.seasons.iter().find(|s| day_of_year >= s.start_day && day_of_year < s.end_day).map(|s| s.kind).unwrap_or(0)
    }

    /// Road traffic density factor at an hour of the day (1 without a curve).
    pub fn road_density(&self, hour: f32) -> f32 {
        curve_at(&self.traffic_density_road, hour)
    }

    /// Passenger density factor at an hour of the day.
    pub fn passenger_density(&self, hour: f32) -> f32 {
        curve_at(&self.traffic_density_passenger, hour)
    }

    pub fn load(path: &Path) -> Result<GlobalCfg, omsi_cfg::CfgError> {
        let f = CfgFile::read(path)?;
        Ok(Self::parse(&f))
    }

    pub fn dir(&self) -> &Path {
        self.path.parent().unwrap_or(Path::new(""))
    }

    pub fn parse(file: &CfgFile) -> GlobalCfg {
        let mut g = GlobalCfg { path: file.path.clone(), repair_time_min: 10.0, ..Default::default() };
        let mut r = file.reader();
        while let Some(k) = r.next_keyword() {
            match k.as_str() {
                "name" => g.name = r.str().to_string(),
                "friendlyname" => g.friendly_name = r.str().to_string(),
                "description" => g.description = r.until("[end]").join("\n"),
                "version" => g.version = r.i32(),
                "nextidcode" => g.next_id_code = r.i64(),
                "worldcoordinates" => g.world_coordinates = true,
                "dynhelperactive" => g.dyn_helper_active = true,
                "realrail" => g.real_rail = true,
                "lht" => g.left_hand_traffic = true,
                "backgroundimage" => g.background_image = (0..5).map(|_| r.str().to_string()).collect(),
                "mapcam" => g.map_cam = (0..8).map(|_| r.f64()).collect(),
                "moneysystem" => g.money_system = r.str().to_string(),
                "ticketpack" => g.ticket_pack = r.str().to_string(),
                "repair_time_min" => g.repair_time_min = r.f32(),
                "years" => {
                    let a = r.i32();
                    let b = r.i32();
                    g.years = (a, b);
                }
                "realyearoffset" => g.real_year_offset = r.i32(),
                "standarddepot" => g.standard_depot = r.str().to_string(),
                "groundtex" => {
                    let texture = r.str().to_string();
                    let detail_texture = r.str().to_string();
                    let params = r.f32s::<3>();
                    g.ground_textures.push(GroundTex { texture, detail_texture, params });
                }
                "addseason" => {
                    let kind = r.i32();
                    let start_day = r.i32();
                    let end_day = r.i32();
                    g.seasons.push(Season { kind, start_day, end_day });
                }
                "trafficdensity_road" => {
                    let t = r.f32();
                    let d = r.f32();
                    g.traffic_density_road.push((t, d));
                }
                "trafficdensity_passenger" => {
                    let t = r.f32();
                    let d = r.f32();
                    g.traffic_density_passenger.push((t, d));
                }
                "entrypoints" => {
                    let n = r.usize();
                    for _ in 0..n {
                        let index = r.i32();
                        let object_id = r.i64();
                        let unknown = r.i32();
                        let x = r.f64();
                        let z = r.f64();
                        let y = r.f64();
                        let quat = r.f64s::<4>();
                        let group = r.i32();
                        let name = r.str().to_string();
                        g.entry_points.push(EntryPoint { index, object_id, unknown, pos: [x, y, z], quat, group, name });
                    }
                }
                "splineobjtypes" => g.spline_obj_types = r.rest_of_block().into_iter().map(|s| s.to_string()).collect(),
                "scenobjlist" => g.scen_obj_list = r.rest_of_block().into_iter().map(|s| s.to_string()).collect(),
                "map" => {
                    let x = r.i32();
                    let y = r.i32();
                    let file = r.str().to_string();
                    // (a map listing a tile twice drew every object of it twice in one
                    // place: fences, houses, all of it flickering; the first entry counts)
                    let index = g.raw_tiles.len();
                    g.raw_tiles.push((x, y));
                    if !g.tiles.iter().any(|t| (t.x == x && t.y == y) || t.file.eq_ignore_ascii_case(&file)) {
                        g.tiles.push(MapTileRef { x, y, file, index });
                    }
                }
                _ => g.unknown_keywords.push((k, r.block_line())),
            }
        }
        g
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tile listed twice counts in the numbering the map's files use (an entry point on
    /// the tile after the repeat names index 2, which is tile (5, 5)).
    #[test]
    fn a_repeated_tile_keeps_the_numbering() {
        let text = "[map]\n0\n0\ntile_0_0.map\n\n[map]\n0\n0\ntile_0_0.map\n\n[map]\n5\n5\ntile_5_5.map\n";
        let g = GlobalCfg::parse(&omsi_cfg::CfgFile::from_str("global.cfg", text));
        assert_eq!(g.tiles.len(), 2);
        assert_eq!(g.raw_tiles, vec![(0, 0), (0, 0), (5, 5)]);
        assert_eq!(g.tiles[1].index, 2);
        assert_eq!(g.raw_tiles.get(2), Some(&(5, 5)));
    }
}
