//! The photo mode's camera and darkroom, apart from the game: `lens` turns the photo camera's
//! settings into the many pictures one photo is made of, and `develop` adds them up in linear
//! light and develops the sum into the picture shown and saved. The app drives them (the
//! panel, the renderer's pictures, saving); nothing here knows the simulation or the window.

pub mod develop;
pub mod lens;

/// `px` (`w` x `h`) stretched to `tw` x `th` (the nearest pixel), for the input script's
/// pictures of the window.
pub fn stretch(w: u32, h: u32, px: &[u8], tw: u32, th: u32) -> Vec<u8> {
    let mut out = vec![0u8; (tw * th * 4) as usize];
    for y in 0..th {
        let sy = (y as u64 * h as u64 / th.max(1) as u64) as u32;
        for x in 0..tw {
            let sx = (x as u64 * w as u64 / tw.max(1) as u64) as u32;
            let a = ((sy * w + sx) * 4) as usize;
            let b = ((y * tw + x) * 4) as usize;
            out[b..b + 4].copy_from_slice(&px[a..a + 4]);
        }
    }
    out
}
