//! Where coupled parts join: the body's ends, its declared couplings and joints.

use super::*;

/// A negative `[boogies]` swaps the rail body's ends relative to its path frame. This is
/// independent of the consist's coupling flags.
/// Keep `heading` in the direction of travel and turn the whole body,
/// including its lights and sound sources, rather than just its meshes.
pub fn body_reversed(def: &Vehicle, reversed: bool) -> bool {
    reversed ^ def.boogies.is_some_and(|b| b < 0.0)
}

/// Model heading in the world, from the path heading and the absolute consist flag.
pub fn body_heading(def: &Vehicle, heading: f64, reversed: bool) -> f64 {
    if body_reversed(def, reversed) {
        heading + 180.0
    } else {
        heading
    }
}

pub(super) fn body_rotation(heading: f64, pitch: f32, bank: f32, reversed: bool) -> Mat4 {
    let (h, p, b) = if reversed {
        (heading + 180.0, -pitch, -bank)
    } else {
        (heading, pitch, bank)
    };
    Mat4::from_quat(
        Quat::from_rotation_z((-h).to_radians() as f32)
            * Quat::from_rotation_x(p.to_radians())
            * Quat::from_rotation_y(b.to_radians()),
    )
}

pub(super) fn declared_coupling(def: &Vehicle, front: bool) -> Vec3 {
    let coupling = if front {
        &def.coupling_front
    } else {
        &def.coupling_back
    };
    let p = coupling
        .as_ref()
        .map(|c| Vec3::from(c.pos))
        .unwrap_or(Vec3::new(0.0, if front { 4.0 } else { -4.0 }, 0.3));
    // Rail couplings are declared in the path frame. Express them in the model frame
    // so body_rotation can turn a negative-bogie body without moving its joints.
    if body_reversed(def, false) {
        Vec3::new(-p.x, -p.y, p.z)
    } else {
        p
    }
}

/// Where the two parts meet, in their respective model frames. Bogie-defined trains
/// use declared couplings, as Omsi.exe does, even when a mod's
/// joints do not match its bodies: the CR200J's inset rear coupling causes overlap
/// in vanilla too.
/// Other rail definitions retain body-end placement; road vehicles retain their
/// declared joint.
pub fn coupling_offsets(
    lead: &VehicleType,
    lead_reversed: bool,
    car: &VehicleType,
    car_reversed: bool,
) -> Option<(f32, f32)> {
    if !car.def.is_rail() {
        return None;
    }
    // the part in front joins by the coupling of its own frame that faces the one behind it -
    // its rear coupling, which is its front coupling where it is itself turned round; the
    // one behind joins by its front coupling, which is its rear where it is turned round.
    // The two read opposite ends because the two face opposite ways.
    let lead_rear = if lead.def.boogies.is_some() {
        declared_joint(lead, lead_reversed)
    } else {
        lead.model_box()
            .map(|(lo, hi)| if lead_reversed { hi.y } else { lo.y })?
    };
    let car_front = if car.def.boogies.is_some() {
        declared_joint(car, !car_reversed)
    } else {
        car.model_box()
            .map(|(lo, hi)| if car_reversed { lo.y } else { hi.y })?
    };
    Some((lead_rear, car_front))
}

/// The shortest model that is a car body and not one of its fittings. The bellows, the
/// passenger floor and the gear of a rail car are modelled on their own, a fraction of a
/// metre long, and their declared joints are measured from where they really are: only a
/// body long enough to stand on its own bounds is judged against itself.
pub(super) const MIN_BODY_FOR_JOINT: f32 = 1.0;

/// The joint of a bogie-defined rail car at the `front` or `back` coupling of its body
/// frame (m forward of its origin), from the file where it is there.
///
/// A declared joint is kept even where it leaves an overlap or a gap between two cars -
/// that is what the CR200J's inset rear coupling does in vanilla, and it is deliberate. A
/// joint that stands metres clear of the body is not that car's joint in that frame,
/// though, and taking it for one puts the car on the wrong side of the one it hangs on.
/// The NF6D's two bogie frames declare `[coupling_front] 0 -17.71 1` and
/// `[coupling_back] 0 3.42 1` on 2.2 m models: the first put its bogie seventeen metres ahead
/// of the joint it hangs on - in front of the tram's own cab - and the second put the rear
/// cab car in front of the bogie it belongs behind. A joint may stand clear of the body - a
/// coupler, a drawbar - but not by more than the body's own length, and there the body's
/// own end is used, as a rail car without `[boogies]` is placed anyway. A body too short to
/// be one is not measured against itself: see [`MIN_BODY_FOR_JOINT`].
pub(super) fn declared_joint(ty: &VehicleType, front: bool) -> f32 {
    let declared = declared_coupling(&ty.def, front).y;
    let Some((lo, hi)) = ty
        .model_box()
        .filter(|(lo, hi)| hi.y - lo.y >= MIN_BODY_FOR_JOINT)
    else {
        return declared;
    };
    let slack = hi.y - lo.y;
    if declared < lo.y - slack || declared > hi.y + slack {
        if front {
            hi.y
        } else {
            lo.y
        }
    } else {
        declared
    }
}

/// `(lead's rear, car's front)` in each body's model frame, including default joints.
pub fn coupling_points(
    lead: &VehicleType,
    lead_reversed: bool,
    car: &VehicleType,
    car_reversed: bool,
) -> (Vec3, Vec3) {
    let mut back = declared_coupling(&lead.def, lead_reversed);
    let mut front = declared_coupling(&car.def, !car_reversed);
    if let Some((b, f)) = coupling_offsets(lead, lead_reversed, car, car_reversed) {
        back.y = b;
        front.y = f;
    }
    // Between bogie-defined cars only distance along the track joins them;
    // lateral position and height come from the track, not a road joint.
    if lead.def.boogies.is_some() && car.def.boogies.is_some() {
        back = Vec3::new(0.0, back.y, 0.0);
        front = Vec3::new(0.0, front.y, 0.0);
    }
    (back, front)
}

/// Where a car of a consist stands, for a caller that lays the cars along one heading with
/// no turn of its own (`Traffic::blocked`): the world position and heading of the car whose
/// body front sits at the joint [`coupling_offsets`] named.
///
/// Both `lead_reversed` and `reversed` are absolute body orientations, including the
/// intrinsic turn of negative bogies, and `heading` is the consist's path heading.
/// A car's body sits along the consist's heading turned round when it is itself turned
/// round, so its heading is derived from `reversed` alone and never from the car in front
/// of it: two cars turned round in a row would otherwise come out turned twice and lie on
/// top of each other.
///
/// `back` is the leading car's body end in the leading car's frame and `front` this car's
/// body front in its own, so the joint lies `back` along the lead's heading (turned round
/// when the *lead* is) and the car's origin steps back from it by `front` along the car's
/// own heading. A caller that turns the part around itself (`TrailerPart::body_rotation`)
/// needs none of this.
pub fn coupling_placement(
    lead_origin: DVec3,
    heading: f64,
    lead_reversed: bool,
    back: f32,
    reversed: bool,
    front: f32,
) -> (DVec3, f64) {
    let turned = |base: f64, rev: bool| if rev { base + 180.0 } else { base };
    let lead_h = turned(heading, lead_reversed);
    let lh = lead_h.to_radians();
    let joint = lead_origin + DVec3::new(lh.sin(), lh.cos(), 0.0) * back as f64;
    let car_h = turned(heading, reversed);
    let ch = car_h.to_radians();
    let origin = joint - DVec3::new(ch.sin(), ch.cos(), 0.0) * front as f64;
    (origin, car_h)
}
