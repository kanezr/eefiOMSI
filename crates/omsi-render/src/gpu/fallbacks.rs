//! Building with less on devices that fail a pipeline: the optional pipelines, and the file
//! that remembers per adapter what had to be left out.

use crate::*;

/// The renderer is built without the pipelines that are not needed to draw a picture (the
/// snowfall, the lamps in the fog, the street lamps' shadow maps): a driver whose shader
/// compiler fails on one of them (it answers "out of memory" or an unknown error - phones'
/// drivers do) takes the whole device with it, and every frame after that came out black
/// (see `Renderer::new_on`). Once set, it stays for the rest of the run. OMSI_BASIC_PIPELINES=1
/// asks for it.
pub(crate) static BASIC_PIPELINES: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub fn basic_pipelines() -> bool {
    BASIC_PIPELINES.load(std::sync::atomic::Ordering::Relaxed) || omsi_cfg::flags::OMSI_BASIC_PIPELINES.is_set()
}

/// One of the pipelines a picture can do without (the snowfall, the lamps in the fog, the
/// street lamps' shadow maps), made in an error scope of its own: where the driver fails on
/// it, that one is left out and the rest of the renderer stands. Caught by the whole
/// build's scope instead, the renderer was made again without all three - one failing on
/// DirectX 12 took the snowfall with it, and the adapter was remembered so for good
/// (`fallback_load`). A device lost on it is not caught here (see `Renderer::new_on`).
pub(crate) fn optional_pipeline<T>(device: &wgpu::Device, what: &str, make: impl FnOnce() -> T) -> Option<T> {
    let internal = device.push_error_scope(wgpu::ErrorFilter::Internal);
    let memory = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let made = make();
    let errors = pollster::block_on(async { [validation.pop().await, memory.pop().await, internal.pop().await] });
    match errors.iter().flatten().next() {
        None => Some(made),
        Some(e) => {
            log::warn!("renderer: {what} left out - the driver failed on it: {}", gpu_error_text(e));
            None
        }
    }
}
/// The file that remembers, per graphics adapter, the reduced renderer that worked there
/// (`~/.openomsi/gpu-fallback.cfg`, lines `adapter|msaa|basic`; basic `1@<version>`: the
/// game's version that found the full set failing, see `build_with_fallbacks`).
pub(crate) fn fallback_path() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(std::path::PathBuf::from(home).join(".openomsi").join("gpu-fallback.cfg"))
}

pub(crate) fn parse_fallbacks(text: &str) -> Vec<(String, u32, bool, String)> {
    text.lines()
        .filter_map(|l| {
            let mut f = l.rsplitn(3, '|');
            let last = f.next()?.trim();
            let (basic, version) = last.split_once('@').map_or((last == "1", String::new()), |(b, v)| (b == "1", v.to_string()));
            let msaa = f.next()?.trim().parse().ok()?;
            Some((f.next()?.to_string(), msaa, basic, version))
        })
        .collect()
}

/// The game's version, for the remembered fallbacks.
pub(crate) fn game_version() -> &'static str {
    option_env!("OPENOMSI_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))
}

/// The reduced renderer (MSAA, basic pipelines) that worked on adapter `name` before, and
/// whether the basic one was found by this version of the game.
pub(crate) fn fallback_load(name: &str) -> Option<(u32, bool, bool)> {
    let text = std::fs::read_to_string(fallback_path()?).ok()?;
    parse_fallbacks(&text).into_iter().find(|e| e.0 == name).map(|e| (e.1, e.2, e.3 == game_version()))
}

/// Remember (`Some`) or forget (`None`) the reduced renderer for adapter `name`.
pub(crate) fn fallback_store(name: &str, what: Option<(u32, bool)>) {
    let Some(path) = fallback_path() else { return };
    let mut all = std::fs::read_to_string(&path).map(|t| parse_fallbacks(&t)).unwrap_or_default();
    all.retain(|e| e.0 != name);
    if let Some((m, b)) = what {
        all.push((name.to_string(), m, b, if b { game_version().to_string() } else { String::new() }));
    }
    let text: String = all.iter().map(|(n, m, b, v)| if *b && !v.is_empty() { format!("{n}|{m}|1@{v}\n") } else { format!("{n}|{m}|{}\n", *b as u8) }).collect();
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(path, text);
}

#[cfg(test)]
pub(crate) mod fallback_tests {
    #[test]
    fn a_remembered_fallback_is_read_back() {
        let e = super::parse_fallbacks("Adreno (TM) 830 (Gl)|1|1\nNVIDIA | odd (Vulkan)|4|0\nRadeon (Dx12)|4|1@0.2.27\nbroken\n");
        assert_eq!(e, vec![
            ("Adreno (TM) 830 (Gl)".to_string(), 1, true, String::new()),
            ("NVIDIA | odd (Vulkan)".to_string(), 4, false, String::new()),
            ("Radeon (Dx12)".to_string(), 4, true, "0.2.27".to_string()),
        ]);
    }
}
