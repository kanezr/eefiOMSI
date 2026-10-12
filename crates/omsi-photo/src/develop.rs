//! Developing a photo: the pictures of one shot added up in linear light (each one taken
//! through another point of the lens and at another moment of the shutter - that is what
//! makes the depth of field and the motion blur), then the darkroom: exposure, white
//! balance, tone, colour, a film look, vignetting, grain, the lens's colour fringes and
//! sharpening, into the 8-bit sRGB picture shown and saved.

use rayon::prelude::*;

/// The film looks of the photo mode (the "Filter" list).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Filter {
    #[default]
    None,
    Vivid,
    Warm,
    Cool,
    Vintage,
    Sepia,
    Mono,
    Noir,
    BleachBypass,
    TealOrange,
    Faded,
}

impl Filter {
    pub const ALL: [Filter; 11] = [Filter::None, Filter::Vivid, Filter::Warm, Filter::Cool, Filter::Vintage, Filter::Sepia, Filter::Mono, Filter::Noir, Filter::BleachBypass, Filter::TealOrange, Filter::Faded];

    pub fn name(self) -> &'static str {
        match self {
            Filter::None => "None",
            Filter::Vivid => "Vivid",
            Filter::Warm => "Warm",
            Filter::Cool => "Cool",
            Filter::Vintage => "Vintage",
            Filter::Sepia => "Sepia",
            Filter::Mono => "Black and white",
            Filter::Noir => "Noir",
            Filter::BleachBypass => "Bleach bypass",
            Filter::TealOrange => "Teal and orange",
            Filter::Faded => "Faded film",
        }
    }
}

/// The darkroom's settings. Every value is 0 when it leaves the picture as it is; the
/// signed ones run from -1 to 1, the others from 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Grade {
    /// Stops of light (-3 .. 3).
    pub exposure: f32,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub saturation: f32,
    pub vibrance: f32,
    /// Warmer (positive) or cooler.
    pub temperature: f32,
    /// Magenta (positive) or green.
    pub tint: f32,
    pub filter: Filter,
    pub filter_strength: f32,
    pub vignette: f32,
    pub grain: f32,
    pub aberration: f32,
    pub sharpen: f32,
}

impl Grade {
    /// Settings that leave the picture as it was taken.
    pub fn neutral() -> Grade {
        Grade { filter_strength: 1.0, ..Default::default() }
    }
}

/// The pictures of one shot added up, in linear light.
pub struct Accum {
    pub w: u32,
    pub h: u32,
    sum: Vec<f32>,
    pub n: u32,
}

/// sRGB byte to linear light.
fn lut() -> &'static [f32; 256] {
    static LUT: std::sync::OnceLock<[f32; 256]> = std::sync::OnceLock::new();
    LUT.get_or_init(|| {
        let mut t = [0.0f32; 256];
        for (i, v) in t.iter_mut().enumerate() {
            let c = i as f32 / 255.0;
            *v = if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
        }
        t
    })
}

/// Linear light (0..1) to sRGB (0..1).
fn to_srgb(l: f32) -> f32 {
    let l = l.clamp(0.0, 1.0);
    if l <= 0.003_130_8 {
        l * 12.92
    } else {
        1.055 * l.powf(1.0 / 2.4) - 0.055
    }
}

impl Accum {
    pub fn new(w: u32, h: u32) -> Accum {
        Accum { w, h, sum: vec![0.0; (w * h * 3) as usize], n: 0 }
    }

    /// One more picture of the shot (RGBA, sRGB, `w` x `h`).
    pub fn add(&mut self, rgba: &[u8]) {
        let lut = lut();
        if rgba.len() < (self.w * self.h * 4) as usize {
            return;
        }
        self.sum.par_chunks_mut(self.w as usize * 3).zip(rgba.par_chunks(self.w as usize * 4)).for_each(|(s, px)| {
            for (o, p) in s.chunks_exact_mut(3).zip(px.chunks_exact(4)) {
                o[0] += lut[p[0] as usize];
                o[1] += lut[p[1] as usize];
                o[2] += lut[p[2] as usize];
            }
        });
        self.n += 1;
    }

    /// The mean of the pictures at pixel (x, y), linear RGB.
    fn at(&self, x: i64, y: i64) -> [f32; 3] {
        let x = x.clamp(0, self.w as i64 - 1) as usize;
        let y = y.clamp(0, self.h as i64 - 1) as usize;
        let k = (y * self.w as usize + x) * 3;
        let inv = 1.0 / self.n.max(1) as f32;
        [self.sum[k] * inv, self.sum[k + 1] * inv, self.sum[k + 2] * inv]
    }

    /// The mean between pixels (bilinear), at pixel coordinates `fx`, `fy`.
    fn sample(&self, fx: f32, fy: f32) -> [f32; 3] {
        let (x0, y0) = (fx.floor(), fy.floor());
        let (tx, ty) = (fx - x0, fy - y0);
        let (x0, y0) = (x0 as i64, y0 as i64);
        let a = self.at(x0, y0);
        let b = self.at(x0 + 1, y0);
        let c = self.at(x0, y0 + 1);
        let d = self.at(x0 + 1, y0 + 1);
        let mut o = [0.0; 3];
        for i in 0..3 {
            let top = a[i] + (b[i] - a[i]) * tx;
            let bot = c[i] + (d[i] - c[i]) * tx;
            o[i] = top + (bot - top) * ty;
        }
        o
    }
}

/// The part of a `w` x `h` picture kept by the aspect ratio `aspect` (width / height; None:
/// the whole): x, y, width, height.
pub fn crop_rect(w: u32, h: u32, aspect: Option<f32>) -> (u32, u32, u32, u32) {
    let Some(a) = aspect.filter(|a| *a > 0.0) else { return (0, 0, w, h) };
    let (fw, fh) = (w as f32, h as f32);
    if fw / fh > a {
        let cw = (fh * a).round().clamp(1.0, fw) as u32;
        ((w - cw) / 2, 0, cw, h)
    } else {
        let ch = (fw / a).round().clamp(1.0, fh) as u32;
        (0, (h - ch) / 2, w, ch)
    }
}

fn luma(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The white balance's gains for a warmer/cooler and a magenta/green setting: a shift along
/// the daylight locus (blue-amber) and across it (green-magenta), the brightness kept.
fn white_balance(temperature: f32, tint: f32) -> [f32; 3] {
    let t = temperature * 0.30;
    let g = tint * 0.20;
    let m = [1.0 + t, 1.0 - g, 1.0 - t];
    let l = luma(m);
    [m[0] / l, m[1] / l, m[2] / l]
}

/// A film look, in display values (0..1).
fn film(c: [f32; 3], f: Filter) -> [f32; 3] {
    let l = luma(c);
    let sat = |c: [f32; 3], s: f32| mix3([luma(c); 3], c, s);
    let curve = |x: f32, k: f32| {
        // an S around the middle grey, k = how steep
        let s = x - 0.5;
        (0.5 + s * (1.0 + k) / (1.0 + k * 2.0 * s.abs())).clamp(0.0, 1.0)
    };
    match f {
        Filter::None => c,
        Filter::Vivid => {
            let c = sat(c, 1.35);
            [curve(c[0], 0.25), curve(c[1], 0.25), curve(c[2], 0.25)]
        }
        Filter::Warm => sat([c[0] * 1.07, c[1] * 1.01, c[2] * 0.86], 1.05),
        Filter::Cool => sat([c[0] * 0.90, c[1] * 0.99, c[2] * 1.10], 0.95),
        Filter::Vintage => {
            let c = sat(c, 0.72);
            [0.07 + c[0] * 0.90 * 1.04, 0.06 + c[1] * 0.88, 0.05 + c[2] * 0.80]
        }
        Filter::Sepia => [l * 1.07 + 0.03, l * 0.92 + 0.02, l * 0.72],
        Filter::Mono => {
            // (a black-and-white film sees red and green brighter than the eye's luma)
            let m = 0.30 * c[0] + 0.59 * c[1] + 0.11 * c[2];
            [m; 3]
        }
        Filter::Noir => {
            let m = curve(0.30 * c[0] + 0.59 * c[1] + 0.11 * c[2], 0.9);
            [m; 3]
        }
        Filter::BleachBypass => {
            // the silver left in: the luma laid over the colour, half the colour gone
            let over = |a: f32, b: f32| if a < 0.5 { 2.0 * a * b } else { 1.0 - 2.0 * (1.0 - a) * (1.0 - b) };
            let o = [over(l, c[0]), over(l, c[1]), over(l, c[2])];
            sat(mix3(c, o, 0.75), 0.55)
        }
        Filter::TealOrange => {
            // the shadows to teal, the light (skin, lamps, sunlit paint) to orange
            let w = smoothstep(0.15, 0.85, l);
            let shade = mix3(c, [c[0] * 0.80, c[1] * 1.02, c[2] * 1.12], 1.0 - w);
            let lit = mix3(shade, [shade[0] * 1.12, shade[1] * 1.00, shade[2] * 0.84], w);
            sat(lit, 1.1)
        }
        Filter::Faded => {
            let c = sat(c, 0.82);
            [0.08 + c[0] * 0.84, 0.08 + c[1] * 0.84, 0.10 + c[2] * 0.82]
        }
    }
}

/// A grain value at a pixel: noise from -0.5 to 0.5, the same picture after picture.
fn grain_at(x: u32, y: u32) -> f32 {
    let mut h = x.wrapping_mul(0x8da6_b343) ^ y.wrapping_mul(0xd816_3841) ^ 0x9e37_79b9;
    h ^= h >> 13;
    h = h.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 16;
    (h & 0xffff) as f32 / 65535.0 - 0.5
}

/// The developed picture (RGBA, sRGB, `acc.w` x `acc.h`); outside `crop` (x, y, w, h) the
/// bars of the frame.
pub fn develop(acc: &Accum, g: &Grade, crop: Option<(u32, u32, u32, u32)>) -> Vec<u8> {
    let (w, h) = (acc.w as usize, acc.h as usize);
    let mut out = vec![0u8; w * h * 4];
    if acc.n == 0 || w == 0 || h == 0 {
        return out;
    }
    let gain = 2f32.powf(g.exposure);
    let wb = white_balance(g.temperature, g.tint);
    let (cx, cy) = (w as f32 * 0.5, h as f32 * 0.5);
    let diag = (cx * cx + cy * cy).sqrt().max(1.0);
    let ab = g.aberration * 0.006;
    let (kx, ky, kw, kh) = crop.unwrap_or((0, 0, w as u32, h as u32));
    out.par_chunks_mut(w * 4).enumerate().for_each(|(y, row)| {
        for x in 0..w {
            let px = &mut row[x * 4..x * 4 + 4];
            px[3] = 255;
            if (x as u32) < kx || (x as u32) >= kx + kw || (y as u32) < ky || (y as u32) >= ky + kh {
                px[0] = 0;
                px[1] = 0;
                px[2] = 0;
                continue;
            }
            let (dx, dy) = (x as f32 - cx, y as f32 - cy);
            let r = (dx * dx + dy * dy).sqrt() / diag;
            // the lens: red drawn out a little, blue drawn in, towards the corners
            let mut c = if ab > 0.0 {
                let k = ab * r * r;
                let red = acc.sample(cx + dx * (1.0 + k), cy + dy * (1.0 + k))[0];
                let blue = acc.sample(cx + dx * (1.0 - k), cy + dy * (1.0 - k))[2];
                let mid = acc.at(x as i64, y as i64);
                [red, mid[1], blue]
            } else {
                acc.at(x as i64, y as i64)
            };
            if g.sharpen > 0.0 {
                let (xi, yi) = (x as i64, y as i64);
                let n = [acc.at(xi - 1, yi), acc.at(xi + 1, yi), acc.at(xi, yi - 1), acc.at(xi, yi + 1)];
                for i in 0..3 {
                    let avg = (n[0][i] + n[1][i] + n[2][i] + n[3][i]) * 0.25;
                    c[i] = (c[i] + (c[i] - avg) * g.sharpen * 1.5).max(0.0);
                }
            }
            // exposure and white balance in linear light; pushed over the white, a soft
            // shoulder rolls it off instead of cutting it (a picture left as it was keeps its
            // values)
            for i in 0..3 {
                let v = c[i] * gain * wb[i];
                c[i] = if v <= 0.8 || gain * wb[i] <= 1.0 { v } else { 0.8 + 0.2 * (1.0 - (-(v - 0.8) / 0.2).exp()) / (1.0 - (-(gain * wb[i] - 0.8) / 0.2).exp()) };
            }
            let mut d = [to_srgb(c[0]), to_srgb(c[1]), to_srgb(c[2])];
            // shadows and highlights: the brightness moved where it is dark or light
            let l = luma(d);
            if g.shadows != 0.0 || g.highlights != 0.0 {
                let lift = g.shadows * 0.35 * (1.0 - l) * (1.0 - l) * (1.0 - smoothstep(0.0, 0.6, l) * 0.5);
                let roll = g.highlights * 0.30 * l * l;
                let to = (l + lift + roll).clamp(0.0, 1.0);
                let k = if l > 1e-4 { to / l } else { 1.0 };
                d = [d[0] * k, d[1] * k, d[2] * k];
            }
            // contrast round the middle grey
            if g.contrast != 0.0 {
                let k = g.contrast;
                for v in d.iter_mut() {
                    let s = *v - 0.5;
                    *v = if k > 0.0 { 0.5 + s * (1.0 + k) / (1.0 + k * 2.0 * s.abs()) } else { 0.5 + s * (1.0 + k * 0.7) };
                }
            }
            // saturation, and vibrance (more where the colour is weak)
            if g.saturation != 0.0 || g.vibrance != 0.0 {
                let l = luma(d);
                let mx = d[0].max(d[1]).max(d[2]);
                let mn = d[0].min(d[1]).min(d[2]);
                let s_now = if mx > 1e-4 { (mx - mn) / mx } else { 0.0 };
                let s = 1.0 + g.saturation + g.vibrance * (1.0 - s_now) * 0.8;
                d = mix3([l; 3], d, s.max(0.0));
            }
            if g.filter != Filter::None && g.filter_strength > 0.0 {
                d = mix3(d, film(d, g.filter), g.filter_strength);
            }
            if g.vignette > 0.0 {
                let v = 1.0 - g.vignette * 0.85 * smoothstep(0.30, 1.05, r);
                d = [d[0] * v, d[1] * v, d[2] * v];
            }
            if g.grain > 0.0 {
                let l = luma(d).clamp(0.0, 1.0);
                // (strongest in the mid-tones, as film's)
                let n = grain_at(x as u32, y as u32) * g.grain * 0.16 * (1.0 - (2.0 * l - 1.0).abs() * 0.6);
                d = [d[0] + n, d[1] + n, d[2] + n];
            }
            px[0] = (d[0].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            px[1] = (d[1].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            px[2] = (d[2].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        }
    });
    out
}

/// The developed picture cut to its frame (RGBA).
pub fn cropped(rgba: &[u8], w: u32, crop: (u32, u32, u32, u32)) -> Vec<u8> {
    let (x, y, cw, ch) = crop;
    let mut out = Vec::with_capacity((cw * ch * 4) as usize);
    for row in y..y + ch {
        let a = ((row * w + x) * 4) as usize;
        out.extend_from_slice(&rgba[a..a + (cw * 4) as usize]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(w: u32, h: u32, c: [u8; 3]) -> Vec<u8> {
        (0..w * h).flat_map(|_| [c[0], c[1], c[2], 255]).collect()
    }

    #[test]
    fn a_neutral_development_gives_the_picture_back() {
        let mut a = Accum::new(4, 3);
        let px: Vec<u8> = (0..12u32).flat_map(|i| [(i * 20) as u8, (255 - i * 20) as u8, 128, 255]).collect();
        a.add(&px);
        let out = develop(&a, &Grade::neutral(), None);
        for (o, p) in out.chunks(4).zip(px.chunks(4)) {
            for i in 0..3 {
                assert!((o[i] as i32 - p[i] as i32).abs() <= 1, "{o:?} vs {p:?}");
            }
        }
    }

    #[test]
    fn the_pictures_of_a_shot_average_in_linear_light() {
        // black and white averaged make the linear half: sRGB 188, not 128
        let mut a = Accum::new(2, 2);
        a.add(&flat(2, 2, [0, 0, 0]));
        a.add(&flat(2, 2, [255, 255, 255]));
        let out = develop(&a, &Grade::neutral(), None);
        assert!((out[0] as i32 - 188).abs() <= 1, "{}", out[0]);
    }

    #[test]
    fn a_stop_more_doubles_the_light() {
        let mut a = Accum::new(1, 1);
        a.add(&flat(1, 1, [100, 100, 100]));
        let base = lut()[100];
        let out = develop(&a, &Grade { exposure: 1.0, ..Grade::neutral() }, None);
        let back = lut()[out[0] as usize];
        assert!((back - base * 2.0).abs() < 0.01, "{back} vs {}", base * 2.0);
    }

    #[test]
    fn black_and_white_has_no_colour_left() {
        let mut a = Accum::new(1, 1);
        a.add(&flat(1, 1, [200, 60, 30]));
        for f in [Filter::Mono, Filter::Noir] {
            let out = develop(&a, &Grade { filter: f, ..Grade::neutral() }, None);
            assert!(out[0] == out[1] && out[1] == out[2], "{f:?}: {:?}", &out[..3]);
        }
    }

    #[test]
    fn the_vignette_darkens_the_corners_and_leaves_the_middle() {
        let mut a = Accum::new(64, 40);
        a.add(&flat(64, 40, [180, 180, 180]));
        let out = develop(&a, &Grade { vignette: 1.0, ..Grade::neutral() }, None);
        let at = |x: usize, y: usize| out[(y * 64 + x) * 4];
        assert!((at(32, 20) as i32 - 180).abs() <= 1);
        assert!(at(0, 0) < 90, "{}", at(0, 0));
    }

    #[test]
    fn warmer_is_redder_and_less_blue() {
        let mut a = Accum::new(1, 1);
        a.add(&flat(1, 1, [128, 128, 128]));
        let out = develop(&a, &Grade { temperature: 1.0, ..Grade::neutral() }, None);
        assert!(out[0] > 128 && out[2] < 128, "{:?}", &out[..3]);
    }

    #[test]
    fn a_frame_crops_to_its_aspect_ratio() {
        assert_eq!(crop_rect(1920, 1080, None), (0, 0, 1920, 1080));
        assert_eq!(crop_rect(1920, 1080, Some(1.0)), (420, 0, 1080, 1080));
        let (x, y, w, h) = crop_rect(1920, 1080, Some(21.0 / 9.0));
        assert_eq!((x, w), (0, 1920));
        assert_eq!(h, 823);
        assert_eq!(y, (1080 - 823) / 2);
        let px = develop(&{ let mut a = Accum::new(4, 2); a.add(&flat(4, 2, [255, 255, 255])); a }, &Grade::neutral(), Some((1, 0, 2, 2)));
        assert_eq!(px[0], 0, "outside the frame: the bar");
        assert_eq!(px[4], 255);
        assert_eq!(cropped(&px, 4, (1, 0, 2, 2)).len(), 2 * 2 * 4);
    }
}
