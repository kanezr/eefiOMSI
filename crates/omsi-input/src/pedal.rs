//! The pedals as the settings shape them.

/// A pedal's last few per cent of travel are its end: a wheel's pedal on the floor reads
/// 0.93..0.99, and the scripts ask for the ends exactly - the LiAZ/PAZ gearboxes put a gear
/// in only at `(L.L.clutch) 1 =` and part the engine from the wheels only above 0.95, so a
/// clutch held down to the floor still dragged and the engine died at every stop.
pub fn pedal_ends(v: f32) -> f32 {
    if v >= 0.96 {
        1.0
    } else if v <= 0.02 {
        0.0
    } else {
        v
    }
}

/// A pedal as the settings shape it: `v` 0..1 through the response curve of `strength`.
pub fn pedal_curve(v: f32, strength: f32) -> f32 {
    let g = strength.clamp(0.25, 4.0);
    v.clamp(0.0, 1.0).powf(1.0 / g)
}
