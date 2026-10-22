//! The plane the wheels stand on, and the shadow blob drawn on it.

use super::*;

/// How far above the plane its wheels stand on a shadow blob is drawn (m): over the road's
/// crown between the wheels, and clear of it for the surfaces' depth bias.
pub const SHADOW_LIFT: f32 = 0.02;

/// How far over the wheel's own plane the face it stands on may lie for a `[isshadow]`
/// blob's plane (m): a kerb or a ramp, the step the AI's wheels climb
/// (as much as an AI car's wheels used to climb).
pub(super) const SHADOW_STEP_UP: f64 = 0.6;
/// How far under it (m). Loose: the model's origin plane is the contact plane of the
/// *unloaded* springs, so a body at rest stands its ground 10-16 cm below its own plane,
/// and a map may put a vehicle down a little over its road. Farther down is another level -
/// a road under a bridge - and not the face this wheel stands on.
pub(super) const SHADOW_STEP_DOWN: f64 = 3.0;

pub(super) fn is_shadow_mesh(ty: &VehicleType, i: usize) -> bool {
    ty.meshes
        .get(i)
        .is_some_and(|m| ty.model.meshes[m.def_index].is_shadow)
}

/// What one wheel of a body without a rigid body stands on at world `p` (the point on the
/// model's z = 0 plane under it): the drawn road there, within a step of the wheel - the same
/// level-limited probe the AI bodies ask (`ai_motion::AiBody::settle`). The plain height
/// sampler knows only x and y and gives the *highest* face, so a vehicle under a bridge or a
/// canopy had its `[isshadow]` blob laid onto the deck over it (the same sampler lifted the
/// coupled parts onto the bridge, #140). The plain sampler stays the fallback where the
/// tiles put no road face near the wheel.
pub(super) fn wheel_ground(
    contact: Option<&dyn crate::rigid::Ground>,
    ground: Option<&(dyn Fn(f64, f64) -> Option<f64> + Send + Sync)>,
    p: DVec3,
) -> Option<f64> {
    if let Some(c) = contact {
        if let Some(g) = c
            .probe(p.x, p.y, p.z + SHADOW_STEP_UP)
            .below
            .filter(|g| *g >= p.z - SHADOW_STEP_DOWN)
        {
            return Some(g);
        }
    }
    ground.and_then(|g| g(p.x, p.y))
}

/// Body frame → body frame with the plane z = 0 laid onto z = p[0] + p[1]·x + p[2]·y
/// (lifted by `SHADOW_LIFT`).
pub fn onto_plane(p: [f32; 3]) -> Mat4 {
    Mat4::from_cols(
        Vec4::new(1.0, 0.0, p[1], 0.0),
        Vec4::new(0.0, 1.0, p[2], 0.0),
        Vec4::Z,
        Vec4::new(0.0, 0.0, p[0] + SHADOW_LIFT, 1.0),
    )
}

/// The least-squares plane z = p[0] + p[1]·x + p[2]·y through `points`; level through their
/// mean height when they do not span a plane (one axle).
pub fn fit_plane(points: &[Vec3]) -> [f32; 3] {
    let n = points.len().max(1) as f32;
    let mean = points.iter().copied().sum::<Vec3>() / n;
    let (mut sxx, mut sxy, mut syy, mut sxz, mut syz) = (0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for p in points {
        let d = *p - mean;
        sxx += d.x * d.x;
        sxy += d.x * d.y;
        syy += d.y * d.y;
        sxz += d.x * d.z;
        syz += d.y * d.z;
    }
    let det = sxx * syy - sxy * sxy;
    let (b, c) = if det > 1e-3 * (sxx + syy).max(1e-6) {
        ((sxz * syy - syz * sxy) / det, (syz * sxx - sxz * sxy) / det)
    } else {
        (0.0, 0.0)
    };
    [mean.z - b * mean.x - c * mean.y, b, c]
}
