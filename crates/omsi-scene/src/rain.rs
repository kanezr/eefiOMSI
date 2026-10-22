//! Precipitation: the rain falling round the camera - what it is (how hard, how big its
//! drops) and how the eye moves through it; the renderer works every drop out on the GPU
//! (rain.wgsl) - and the picture the film on the glass wears in a snow weather.

use glam::{DVec3, Vec3};
use omsi_render::{Rainfall, Scene};

/// The rain rate (mm/h) of OMSI's precipitation strength (0..1 of its 255): a drizzle at
/// the bottom, a cloudburst (60 mm/h) at the top.
pub fn rain_rate_mm_h(rate: f32) -> f32 {
    0.3 + 60.0 * rate.clamp(0.0, 1.0).powi(2)
}

/// The median drop diameter (mm) of rain falling at `r` mm/h: Marshall and Palmer's drop
/// sizes (1948), `N(D) = 8000 e^(-ΛD)` with `Λ = 4.1 R^-0.21` per mm, whose median volume
/// diameter is 3.67 / Λ.
pub fn median_drop_mm(r: f32) -> f32 {
    3.67 / (4.1 * r.max(0.01).powf(-0.21))
}

/// The speed (m/s) a raindrop of `d` mm falls at in still air at sea level (Atlas, Srivastava
/// and Sekhon 1973): a drizzle drifts down at 2-4 m/s, the big drops of a cloudburst at 7-8.
/// (rain.wgsl gives every drop its own.)
pub fn terminal_speed(d: f32) -> f32 {
    (9.65 - 10.3 * (-0.6 * d.max(0.0)).exp()).max(0.5)
}

/// The rain's quality of a `rain_quality` setting (`high`, `medium`, `low`): 2 high, 1
/// medium, 0 low.
pub fn quality_of(q: &str) -> u8 {
    match q {
        "low" => 0,
        "medium" => 1,
        _ => 2,
    }
}

pub fn weather_wind(weather: &omsi_content::weather::Weather) -> Vec3 {
    let heading = weather.wind.0.to_radians();
    Vec3::new(heading.sin(), heading.cos(), 0.0) * weather.wind.1
}

/// How much of the ground and of what faces up the snow covers (0..1) after `secs` more of
/// this weather (`kind` and `rate` as `Rain::set` takes them, `temp` °C), from `start`. A
/// heavy snowfall lays a few centimetres an hour - a white cover in a quarter of an hour, a
/// light one in an hour or two; flakes landing above +2 °C melt where they land. Without
/// snow falling it lies on below zero, thaws in the warmth (half an hour at +5 °C) and
/// faster in the rain.
pub fn snow_cover(kind: i32, rate: f32, temp: f32, secs: f32, start: f32) -> f32 {
    let rate = rate.clamp(0.0, 1.0);
    let change = if kind == 2 && rate > 0.0 && temp < 2.0 {
        (0.05 + rate) / 900.0
    } else {
        -(temp.max(0.0) / 9000.0 + if kind == 1 { rate / 600.0 } else { 0.0 })
    };
    (start + change * secs).clamp(0.0, 1.0)
}

pub struct Rain {
    kind: i32,
    rate: f32,
    quality: u8,
    /// How thick the drops are (mm, their median volume diameter), from the rain's rate.
    drop_mm: f32,
    /// The eye's velocity (m/s), smoothed over a few frames.
    eye: Vec3,
    /// The weather's temperature (°C) and the snow lying from the snowfalls so far (see
    /// [`snow_cover`]; `None` before the first weather).
    temp: f32,
    cover: Option<f32>,
}

/// The boxes a vehicle keeps the weather out of: its own `[boundingbox]` and each coupled
/// part's (an articulated bus's rear section is a part of its own, with its own box:
/// without it, it rained in the rear saloon, #777).
pub fn vehicle_boxes(v: &omsi_sim::VehicleInstance) -> Vec<(DVec3, f64, [f32; 6])> {
    let front = v.ty.def.bounding_box.map(|bb| (v.position, v.heading, bb));
    let parts = v.trailers.iter().filter_map(|t| {
        // a part coupled the other way round stands turned about its own origin
        let heading = if t.reversed { t.heading + 180.0 } else { t.heading };
        t.ty.def.bounding_box.map(|bb| (t.position, heading, bb))
    });
    front.into_iter().chain(parts).collect()
}

impl Rain {
    pub fn new() -> Rain {
        Rain { kind: 0, rate: 0.0, quality: 2, drop_mm: 2.0, eye: Vec3::ZERO, temp: 10.0, cover: None }
    }

    /// `kind`: 0 none, 1 rain, 2 snow; `rate` 0..1; `quality` the rain's (see [`quality_of`]).
    pub fn set(&mut self, kind: i32, rate: f32, quality: u8) {
        self.kind = kind;
        self.rate = rate.clamp(0.0, 1.0);
        self.quality = quality;
        // the drops of this rain: small and slow in a drizzle, big and fast in a downpour
        // (they all looked the same whatever the rain, #1412)
        self.drop_mm = median_drop_mm(rain_rate_mm_h(self.rate));
    }

    /// The weather's temperature (°C), for the snow lying.
    pub fn set_temperature(&mut self, temp: f32) {
        self.temp = temp;
    }

    /// How much of what faces up the snow covers now (0..1, see [`snow_cover`]).
    pub fn snow_cover(&self) -> f32 {
        self.cover.unwrap_or(0.0)
    }

    /// Hand the renderer this frame's rain (`Scene::rainfall`; snow is drawn by the renderer
    /// from the weather: render snow.wgsl, `Lighting::snowfall`). `carried`: the velocity of
    /// the vehicle the camera rides with (m/s; zero for a free camera or on foot). The
    /// streaks slant with that alone - not with the camera's own way round the bus or a turn
    /// of the head: taken from the camera's motion, the whole rain swung and stretched with
    /// every move of the mouse, which nothing else in the picture does.
    pub fn tick(&mut self, dt: f32, carried: Vec3, wind: Vec3, scene: &mut Scene) {
        if dt > 0.0 {
            self.eye += (carried - self.eye) * (dt / 0.1).min(1.0);
        }
        let start = self.cover.unwrap_or(0.0);
        self.cover = Some(snow_cover(self.kind, self.rate, self.temp, dt, start));
        let raining = self.kind == 1 && self.rate > 0.0;
        scene.rainfall = Rainfall {
            rate: if raining { self.rate } else { 0.0 },
            drop_mm: self.drop_mm,
            eye_velocity: self.eye,
            wind,
            quality: self.quality,
        };
    }
}

/// The picture the windscreen and window layers wear while it snows.
///
/// Every bus carries the same rain film: a mesh in front of the glass textured with
/// `regen.tga` (running drops) and faded in by `[alphascale] Rain_Window_*_Wetness`, which
/// `rain.osc` fills from `PrecipRate` - and `rain.osc` never asks what is falling, so in a
/// snowstorm the original shows raindrops on every pane, in the cab and along the saloon.
/// OMSI's own way out is the seasonal texture folder (`texture\WinterSnow\`), which no stock
/// vehicle fills in, so openOMSI builds the winter picture itself: crystals settled on
/// the glass, thickest along the rim where a pane collects them, made from the game's own
/// `Texture\snowflake.tga` where it is there and from soft specks of its own where it is not.
pub fn snow_on_glass(root: &std::path::Path) -> omsi_texture::Image {
    // the same grain as the rain film it stands in for (`regen.tga`, 1024 x 1024 with
    // drops a dozen pixels across and a mean alpha of 7 %)
    const SIDE: usize = 512;
    // white where it is clear as well, so that the smaller mip levels stay white specks
    // and do not grey towards the black of transparent texels
    let mut rgba = [255u8, 255, 255, 0].repeat(SIDE * SIDE);
    let flake =
        omsi_texture::decode_file(&omsi_cfg::resolve_path(root, "Texture\\snowflake.tga")).ok();
    let mut rng = 0x9E37_79B9_7F4A_7C15u64;
    let mut rand = move || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        (rng >> 40) as f32 / (1u64 << 24) as f32
    };
    // a pane holds more snow at its rim than in the middle, where the airstream and the
    // wipers take it away
    let edge = |x: f32, y: f32| {
        let d = (x - 0.5).abs().max((y - 0.5).abs()) * 2.0;
        0.35 + 0.65 * d * d
    };
    for _ in 0..1800 {
        let (cx, cy) = (rand() * SIDE as f32, rand() * SIDE as f32);
        if rand() > edge(cx / SIDE as f32, cy / SIDE as f32) {
            continue;
        }
        let r = 1.5 + rand() * 3.5;
        let strength = 0.35 + 0.65 * rand();
        let lo = |v: f32| (v - r).max(0.0) as usize;
        let hi = |v: f32| ((v + r) as usize).min(SIDE - 1);
        for y in lo(cy)..=hi(cy) {
            for x in lo(cx)..=hi(cx) {
                let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                let d = (dx * dx + dy * dy).sqrt() / r;
                if d > 1.0 {
                    continue;
                }
                // the flake's own picture where the game has one, else a soft round speck
                let a = match &flake {
                    Some(f) => {
                        let u = ((dx / r * 0.5 + 0.5) * (f.width - 1) as f32) as usize;
                        let v = ((dy / r * 0.5 + 0.5) * (f.height - 1) as f32) as usize;
                        let p = (v * f.width as usize + u) * 4;
                        // the stock flake is white on black; its alpha channel, or its
                        // brightness where it has none
                        (f.rgba[p + 3] as f32 / 255.0).max(f.rgba[p] as f32 / 255.0)
                    }
                    None => (1.0 - d * d).powf(1.5),
                };
                let a = (a * strength * 255.0) as u32;
                let p = (y * SIDE + x) * 4;
                rgba[p] = 255;
                rgba[p + 1] = 255;
                rgba[p + 2] = 255;
                rgba[p + 3] = rgba[p + 3].max(a.min(255) as u8);
            }
        }
    }
    omsi_texture::Image {
        width: SIDE as u32,
        height: SIDE as u32,
        rgba,
        has_alpha: true,
    }
}

/// The snow-on-glass picture (`snow_on_glass`) on the GPU, with its mip chain: one level
/// alone, its specks of a pixel or two were point-sampled across a whole windscreen and
/// glittered with every move of the head (#946).
pub fn add_snow_on_glass(
    renderer: &omsi_render::Renderer,
    scene: &mut omsi_render::Scene,
    root: &std::path::Path,
) -> omsi_render::TextureId {
    renderer.add_texture(scene, &snow_on_glass(root), true)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The snow on the glass goes up with its mip levels, and it is white throughout (a
    /// level made smaller stays white, only less opaque).
    #[test]
    fn snow_on_glass_has_mip_levels_and_stays_white() {
        let img = super::snow_on_glass(std::path::Path::new("/nonexistent"));
        assert!(img.rgba.chunks_exact(4).all(|p| p[..3] == [255, 255, 255]));
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        descriptor.backends = wgpu::Backends::NOOP;
        descriptor.backend_options.noop = wgpu::NoopBackendOptions { enable: true };
        let instance = wgpu::Instance::new(descriptor);
        let renderer = pollster::block_on(omsi_render::Renderer::new_with(
            &instance,
            None,
            Some(wgpu::TextureFormat::Rgba8UnormSrgb),
            omsi_render::RenderOptions { msaa: 1, shadow_size: 1024, ..Default::default() },
        ))
        .expect("noop renderer");
        let mut scene = renderer.new_scene();
        let id = super::add_snow_on_glass(&renderer, &mut scene, std::path::Path::new("/nonexistent"));
        assert_eq!(renderer.texture_levels(&scene, id), Some((512, 512, 10)));
    }

    /// The renderer is handed the eye's way through the rain (the streaks are the drops' way
    /// against it), no rain in a dry or a snow weather, and no speed of a jump of the camera.
    #[test]
    fn the_rainfall_follows_the_eye_and_the_weather() {
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        descriptor.backends = wgpu::Backends::NOOP;
        descriptor.backend_options.noop = wgpu::NoopBackendOptions { enable: true };
        let instance = wgpu::Instance::new(descriptor);
        let renderer = pollster::block_on(omsi_render::Renderer::new_with(
            &instance,
            None,
            Some(wgpu::TextureFormat::Rgba8UnormSrgb),
            omsi_render::RenderOptions { msaa: 1, shadow_size: 1024, ..Default::default() },
        ))
        .expect("noop renderer");
        let mut scene = renderer.new_scene();
        let mut rain = Rain::new();
        rain.set(1, 1.0, 2);
        for _ in 0..60 {
            rain.tick(0.05, Vec3::X * 10.0, Vec3::X * 3.0, &mut scene);
        }
        let r = scene.rainfall;
        assert_eq!(r.rate, 1.0);
        assert!((r.eye_velocity - Vec3::X * 10.0).length() < 0.1, "{:?}", r.eye_velocity);
        assert_eq!(r.wind, Vec3::X * 3.0);
        rain.set(2, 1.0, 2);
        rain.tick(0.05, Vec3::ZERO, Vec3::ZERO, &mut scene);
        assert_eq!(scene.rainfall.rate, 0.0);
        rain.set(0, 1.0, 2);
        rain.tick(0.05, Vec3::ZERO, Vec3::ZERO, &mut scene);
        assert_eq!(scene.rainfall.rate, 0.0);
    }

    /// A drizzle falls slower and in smaller drops than a downpour (#1412), as measured.
    #[test]
    fn a_downpour_falls_faster_in_bigger_drops_than_a_drizzle() {
        let drizzle = median_drop_mm(rain_rate_mm_h(0.05));
        let downpour = median_drop_mm(rain_rate_mm_h(1.0));
        assert!(drizzle > 0.5 && drizzle < 1.0, "{drizzle}");
        assert!(downpour > 2.0 && downpour < 2.5, "{downpour}");
        assert!(terminal_speed(drizzle) > 2.0 && terminal_speed(drizzle) < 4.5);
        assert!(terminal_speed(downpour) > 6.5 && terminal_speed(downpour) < 8.0);
        let mut rain = Rain::new();
        rain.set(1, 0.05, 2);
        let small = rain.drop_mm;
        rain.set(1, 1.0, 2);
        assert!(rain.drop_mm > small + 1.0);
    }

    /// Snow lies on in the cold, builds up in a snowfall (faster in a heavy one), melts as
    /// it lands in the warmth and thaws without a snowfall above zero.
    #[test]
    fn the_snow_cover_builds_up_and_thaws() {
        let heavy = snow_cover(2, 1.0, -3.0, 600.0, 0.0);
        let light = snow_cover(2, 0.1, -3.0, 600.0, 0.0);
        assert!(heavy > 0.6 && heavy < 0.8 && light < heavy / 3.0, "{heavy} {light}");
        assert_eq!(snow_cover(2, 1.0, -3.0, 2000.0, 0.0), 1.0);
        assert_eq!(snow_cover(2, 1.0, 5.0, 600.0, 0.0), 0.0);
        assert_eq!(snow_cover(0, 0.0, -5.0, 3600.0, 0.8), 0.8);
        let thaw = snow_cover(0, 0.0, 5.0, 900.0, 1.0);
        assert!(thaw > 0.4 && thaw < 0.6, "{thaw}");
        assert!(snow_cover(1, 0.5, 5.0, 900.0, 1.0) < thaw);
    }

    #[test]
    fn snow_on_glass_is_mostly_clear_and_thickest_at_the_rim() {
        // no content root here: the procedural specks
        let img = super::snow_on_glass(std::path::Path::new("/nonexistent"));
        assert_eq!((img.width, img.height), (512, 512));
        let alpha_of = |x: usize, y: usize| img.rgba[(y * 512 + x) * 4 + 3] as u32;
        let (mut middle, mut rim, mut n_middle, mut n_rim) = (0u32, 0u32, 0u32, 0u32);
        for y in 0..512 {
            for x in 0..512 {
                let d = ((x as f32 - 255.5).abs()).max((y as f32 - 255.5).abs()) / 255.5;
                if d < 0.35 {
                    middle += alpha_of(x, y);
                    n_middle += 1;
                } else if d > 0.8 {
                    rim += alpha_of(x, y);
                    n_rim += 1;
                }
            }
        }
        let mean = img.rgba.chunks_exact(4).map(|p| p[3] as u32).sum::<u32>() / (512 * 512);
        assert!(mean < 60, "the glass would be opaque (mean alpha {mean})");
        let (a, b) = (rim as f32 / n_rim as f32, middle as f32 / n_middle as f32);
        assert!(
            a > b * 1.3,
            "the rim should carry more snow than the middle ({a:.1} vs {b:.1})"
        );
    }
}
