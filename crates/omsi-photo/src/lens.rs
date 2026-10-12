//! The photo camera's lens: its focal length as a field of view (a full-frame camera's, 24 mm
//! high), and the pictures one photo is made of - each taken from another point of the
//! aperture, all aimed at the same plane of focus (what is on it stays sharp, the rest
//! blurs as much as a real lens of that focal length and f-number blurs it), a little
//! turned within the pixel (the edges smoothed), at another moment of the shutter.

use glam::{DVec3, Vec3};
use omsi_render::Camera;

/// A full-frame sensor's height (mm): the field of view and the focal length go together
/// through it, as a 35 mm camera's lens is named.
const SENSOR_H_MM: f32 = 24.0;

/// The focal length (mm) of a vertical field of view (degrees).
pub fn focal_mm(fov_deg: f32) -> f32 {
    SENSOR_H_MM * 0.5 / (fov_deg.to_radians() * 0.5).tan().max(1e-4)
}

/// The vertical field of view (degrees) of a focal length (mm).
pub fn fov_for(focal_mm: f32) -> f32 {
    (2.0 * (SENSOR_H_MM * 0.5 / focal_mm.max(1.0)).atan()).to_degrees()
}

/// The `i`th number of the Halton sequence of `base` (0..1): points that fill the aperture
/// and the shutter evenly however many are taken.
pub fn halton(mut i: u32, base: u32) -> f32 {
    let mut f = 1.0f32;
    let mut r = 0.0f32;
    i += 1;
    while i > 0 {
        f /= base as f32;
        r += f * (i % base) as f32;
        i /= base;
    }
    r
}

/// A point of the aperture (unit radius) for the square's point (u, v): the disc evenly
/// (Shirley's concentric mapping), or with `blades` (5 and more) the polygon the blades
/// close to - the shape every out-of-focus light takes.
pub fn aperture_point(u: f32, v: f32, blades: u32) -> (f32, f32) {
    let (a, b) = (2.0 * u - 1.0, 2.0 * v - 1.0);
    if a == 0.0 && b == 0.0 {
        return (0.0, 0.0);
    }
    let (r, phi) = if a.abs() > b.abs() { (a, std::f32::consts::FRAC_PI_4 * (b / a)) } else { (b, std::f32::consts::FRAC_PI_2 - std::f32::consts::FRAC_PI_4 * (a / b)) };
    let mut r = r;
    if blades >= 5 {
        // the polygon's edge at this angle, as a share of its corners' radius
        let seg = std::f32::consts::TAU / blades as f32;
        let a = (phi + std::f32::consts::FRAC_PI_2).rem_euclid(seg) - seg * 0.5;
        r *= (seg * 0.5).cos() / a.cos();
    }
    (r * phi.cos(), r * phi.sin())
}

/// How the photo is taken.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shot {
    /// Depth of field: the aperture opens to `fstop`, focused at `focus` metres.
    pub dof: bool,
    pub fstop: f32,
    pub focus: f32,
    /// 0: a round aperture; 5 and more: that many blades.
    pub blades: u32,
    /// The bus's velocity (m/s, world) and how long the shutter is open (s): the bus and the
    /// camera travel together while the world passes (a rolling shot).
    pub motion: Option<(Vec3, f32)>,
}

/// The `i`th picture of a photo: the camera moved over the aperture and aimed back at the
/// plane of focus, turned within the pixel (`height_px`: the picture's height), and the
/// moment of the shutter (seconds from its middle) it is taken at.
pub fn sample(base: &Camera, i: u32, shot: &Shot, height_px: f32) -> (Camera, f32) {
    let mut cam = *base;
    let f = base.forward();
    let (right, up) = (base.right(), base.up());
    let mut dir = f;
    if shot.dof && i > 0 {
        let radius = focal_mm(base.fov_deg) / 1000.0 / (2.0 * shot.fstop.max(0.7));
        let (ax, ay) = aperture_point(halton(i, 2), halton(i, 3), shot.blades);
        let off = right * (ax * radius) + up * (ay * radius);
        let focus = f * shot.focus.max(0.05);
        cam.position = base.position + off.as_dvec3();
        dir = (focus - off).normalize_or(f);
    }
    // within the pixel (the first picture straight)
    if i > 0 {
        let k = 2.0 * (base.fov_deg.to_radians() * 0.5).tan() / height_px.max(1.0);
        // (half a pixel across: the edges smoothed, the detail kept - a whole pixel's box
        // softened the photo next to the live picture)
        let (jx, jy) = ((halton(i, 5) - 0.5) * 0.5, (halton(i, 7) - 0.5) * 0.5);
        dir = (dir + right * (jx * k) + up * (jy * k)).normalize_or(dir);
    }
    cam.yaw = dir.x.atan2(dir.y).to_degrees();
    cam.pitch = dir.z.clamp(-1.0, 1.0).asin().to_degrees();
    let t = match shot.motion {
        // (the golden ratio's sequence: the moments evenly over the shutter's time, apart
        // from the aperture's and the pixel's points)
        Some((_, shutter)) if i > 0 => ((i as f32 * 0.618_034).fract() - 0.5) * shutter,
        _ => 0.0,
    };
    (cam, t)
}

/// Where the bus is moved for a picture taken `t` seconds from the shutter's middle.
pub fn travel(shot: &Shot, t: f32) -> DVec3 {
    shot.motion.map(|(v, _)| (v * t).as_dvec3()).unwrap_or(DVec3::ZERO)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cam() -> Camera {
        Camera { position: DVec3::new(10.0, 20.0, 2.0), yaw: 30.0, pitch: -5.0, roll: 0.0, fov_deg: 40.0, near: 0.1, far: 1000.0 }
    }

    #[test]
    fn focal_length_and_field_of_view_go_together() {
        assert!((focal_mm(fov_for(50.0)) - 50.0).abs() < 1e-3);
        // a 50 mm lens on full frame: 27 degrees high
        assert!((fov_for(50.0) - 26.99).abs() < 0.05, "{}", fov_for(50.0));
    }

    #[test]
    fn the_aperture_points_stay_inside_its_shape() {
        for blades in [0, 5, 6, 8] {
            for i in 0..256 {
                let (x, y) = aperture_point(halton(i, 2), halton(i, 3), blades);
                assert!((x * x + y * y).sqrt() <= 1.0 + 1e-4, "{blades}: {x} {y}");
            }
        }
        // a hexagon reaches its corners but not the circle between them
        let edge = (0..4096).map(|i| { let (x, y) = aperture_point(halton(i, 2), halton(i, 3), 6); (x * x + y * y).sqrt() }).fold(0.0f32, f32::max);
        assert!(edge > 0.97, "{edge}");
    }

    #[test]
    fn every_picture_aims_at_the_same_point_of_the_focus_plane() {
        let base = cam();
        let shot = Shot { dof: true, fstop: 1.4, focus: 8.0, blades: 0, motion: None };
        let target = base.position + (base.forward() * 8.0).as_dvec3();
        let mut moved = 0.0f64;
        for i in 1..32 {
            let (c, _) = sample(&base, i, &shot, 100_000.0);
            moved = moved.max((c.position - base.position).length());
            // (the ray from the moved camera passes the focus point)
            let to = (target - c.position).as_vec3().normalize();
            assert!(to.dot(c.forward()) > 0.999_99, "{i}");
        }
        // a 50-odd mm lens at f/1.4: the aperture is a couple of centimetres across
        assert!(moved > 0.005 && moved < 0.03, "{moved}");
    }

    #[test]
    fn without_depth_of_field_the_camera_stays_put() {
        let base = cam();
        let shot = Shot { dof: false, fstop: 2.8, focus: 5.0, blades: 0, motion: None };
        for i in 0..8 {
            let (c, t) = sample(&base, i, &shot, 1080.0);
            assert_eq!(c.position, base.position);
            assert_eq!(t, 0.0);
            assert!((c.yaw - base.yaw).abs() < 0.05 && (c.pitch - base.pitch).abs() < 0.05);
        }
    }

    #[test]
    fn the_shutter_spreads_the_pictures_over_its_time() {
        let shot = Shot { dof: false, fstop: 2.8, focus: 5.0, blades: 0, motion: Some((Vec3::new(10.0, 0.0, 0.0), 1.0 / 30.0)) };
        let ts: Vec<f32> = (1..64).map(|i| sample(&cam(), i, &shot, 1080.0).1).collect();
        let (lo, hi) = ts.iter().fold((1.0f32, -1.0f32), |(a, b), t| (a.min(*t), b.max(*t)));
        assert!(lo < -0.015 && hi > 0.015 && lo >= -0.0168 && hi <= 0.0168, "{lo} {hi}");
        assert!((travel(&shot, hi).x - 10.0 * hi as f64).abs() < 1e-6);
    }
}
