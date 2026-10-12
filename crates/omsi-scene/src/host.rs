//! What the game hands the scene: the settings it reads while it loads (the graphics' kind,
//! the scenery's volume, the mirrors' size, how many cars may park), and the decoders the
//! simulation draws its pictures with. The game sets these at its start; a world a test or
//! a tool opens takes the defaults.

// The settings below are set by the game at every session start and read while worlds and
// vehicles load, where no game state is at hand.

/// Enhanced graphics wanted (from the settings, `--enhanced`, or OMSI_ENHANCED=1).
pub static ENHANCED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Vanilla graphics: the picture as OMSI 2 draws it (no Vanilla+ extras, see
/// `omsi_render::Lighting::classic`).
pub static CLASSIC: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Volume of the scenery's own sounds (OMSI's `sound_scenery`), as the bits of an f32.
pub static SOUND_SCENERY: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x3f80_0000);

/// Edge of a mirror's picture (the `mirror_size` setting, OMSI's reflTexSize).
pub static MIRROR_SIZE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(256);

pub fn sound_gain(which: &std::sync::atomic::AtomicU32) -> f32 {
    f32::from_bits(which.load(std::sync::atomic::Ordering::Relaxed))
}

/// The settings a world reads when it opens.
#[derive(Clone, Copy, Debug, Default)]
pub struct OpenSettings {
    /// Parked AI cars at most (`ai_max_parked`: 0 no limit, -1 none).
    pub max_parked: i64,
}

// (an install-once hook: the game's settings file is the process's, read when a world opens)
static OPEN_SETTINGS: std::sync::OnceLock<fn() -> OpenSettings> = std::sync::OnceLock::new();

/// Where a world opening reads the game's settings from (the game's settings file; set once,
/// a second call changes nothing).
pub fn install_open_settings(read: fn() -> OpenSettings) {
    let _ = OPEN_SETTINGS.set(read);
}

/// The game's settings as a world opening now reads them (the defaults without the game).
pub fn open_settings() -> OpenSettings {
    OPEN_SETTINGS.get().map(|read| read()).unwrap_or_default()
}

/// Register what the simulation draws and reads its pictures with, which omsi-sim does not
/// depend on: the picture decoder (`STLoadTex`, the text textures' font sheets, the HTML
/// pages' pictures; see `omsi_sim::image_decode`), the presentation's texture loading, and
/// the `[htmltexture]` pages' engine (omsi-htmlengine, see `omsi_sim::htmltex::set_backend`).
/// Every entry point calls it before anything is loaded (a second call changes nothing).
pub fn install_sim_decoder() {
    omsi_sim::image_decode::install(|path| {
        omsi_texture::decode_file(path)
            .map(|i| omsi_sim::image_decode::DecodedImage { width: i.width, height: i.height, rgba: i.rgba, has_alpha: i.has_alpha })
            .map_err(|e| e.to_string())
    });
    omsi_sim::htmltex::set_backend(|width, height, html, api| Box::new(omsi_htmlengine::EngineRenderer::with_api(width, height, html, api)));
}
