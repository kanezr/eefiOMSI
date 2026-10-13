//! The launcher's side in this program. The launchers are other programs or other front ends:
//! the desktop launcher (`launcher/`, a Tauri window beside this program, see `launcher_link`),
//! the launcher page in a web view on Android (`android_launcher`) and the Swift launcher on
//! Apple's systems (`apple_bridge`). What they ask of the game is here: the helper service the
//! desktop launcher runs (`openomsi --launcher-service`: the bus preview drawn with the game's
//! own scene in `showroom`, the city map, the controllers), and the program's side of the
//! launcher's model (`AppHost`): the interface's language, machine translation and the city map
//! read the way the navigator reads it.

mod key_actions;
mod pads;
mod service;
pub(crate) mod showroom;
pub(crate) use service::run as run_service;
#[cfg_attr(not(target_os = "android"), allow(unused_imports))]
pub(crate) use service::InProcess;

#[cfg_attr(not(any(target_os = "macos", target_os = "ios")), allow(unused_imports))]
pub(crate) use omsi_launcher_model::mapview;
#[cfg_attr(not(any(target_os = "android", target_os = "ios")), allow(unused_imports))]
pub(crate) use omsi_launcher_model::crash_of;
#[allow(unused_imports)]
pub(crate) use omsi_launcher_model::state;

use omsi_launcher_model::host::{CityMap, Host, MapRoad};
use std::path::{Path, PathBuf};

/// The program's side of the launcher's model, for every model made from here on. Whatever
/// makes a launcher `state::State` (another front end bridging to it) or reads a map through
/// `mapview` calls this first: the state sets the interface language and machine translation
/// through it.
pub(crate) fn install() {
    omsi_launcher_model::host::set(Box::new(AppHost));
}

/// The showroom's renderer: a bus on a floor needs none of the game's costly passes - no
/// ambient occlusion, a small shadow map, 4x MSAA for the edges whatever the game uses -
/// and it draws Vanilla+ whatever the game does (`showroom::lighting_for`): no enhanced
/// pipelines, reflection probe or cloud noise either (`RenderOptions::preview_only`).
pub(crate) fn showroom_options(settings: &crate::settings::Settings) -> omsi_render::RenderOptions {
    omsi_render::RenderOptions { msaa: 4, ssao: false, shadow_size: 1024, render_scale: 1.0, preview_only: true, no_enhanced: true, ray_tracing: false, ..settings.render_options() }
}

struct AppHost;

impl Host for AppHost {
    fn ui_language(&self, code: &str) {
        crate::ui_language(code);
    }
    fn mt_enable(&self, on: bool) {
        crate::mt::enable(on);
    }
    fn mt_protect(&self, texts: &mut dyn Iterator<Item = &str>) {
        crate::mt::protect(texts);
    }
    #[cfg(target_os = "android")]
    fn previous_run_crash(&self) -> Option<(String, String)> {
        crate::android::previous_run_crash()
    }
    #[cfg(target_os = "ios")]
    fn previous_run_crash(&self) -> Option<(String, String)> {
        crate::ios::previous_run_crash()
    }
    fn city_map(&self, root: &Path, tiles: &[(i32, i32, PathBuf)], chrono: &[PathBuf]) -> CityMap {
        let crate::scene::NavigationMap { lanes, road_surfaces, places, signs: _ } = crate::scene::navigation_map_of(root, tiles, chrono);
        let (roads, net) = crate::navigator::city_roads(lanes, &road_surfaces);
        CityMap { roads: roads.into_iter().map(|r| MapRoad { points: r.points, width: r.width, main: r.main }).collect(), net, places: places.into_iter().collect() }
    }
    fn city_map_colors(&self) -> [[f32; 4]; 4] {
        use crate::navigator::{ROAD, ROAD_CASING, ROAD_MAIN, ROUTE};
        [ROAD_CASING.0, ROAD.0, ROAD_MAIN.0, ROUTE.0]
    }
    fn simplify(&self, pts: &[glam::Vec3], tol: f32) -> Vec<glam::Vec3> {
        crate::navigator::simplify(pts, tol)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    /// What the launcher's settings page saves is what the game reads (the launcher's own
    /// checks are its page's; the game's reading of the file is here).
    #[test]
    fn the_game_reads_what_the_settings_page_saves() {
        let mut s = omsi_launcher_lib::settings_from_text(None);
        s["cloud_quality"] = json!("low");
        s["mouse_smooth"] = json!(false);
        let saved = omsi_launcher_lib::settings_to_text(&s, None);
        let game = crate::settings::Settings::from_text(&saved);
        assert_eq!(game.cloud_quality, "low");
        assert!(!game.mouse_smooth);
        assert!(crate::settings::Settings::from_text("").mouse_smooth);
    }
}
