//! Refresh physical support when streaming or a frame transition changes the floor.
use super::*;

impl Pose {
    pub(super) fn refresh_support(&mut self, input: &PoseInput) {
        // Flat-floor callers without a sampler retain their previous animation behavior.
        // Do not drag a planted foot up a stair just because the body's origin rose:
        // sample at that foot's own x/y, in the frame of the floor.
        if input.floor.is_none() {
            return;
        }
        for f in &mut self.feet {
            if f.planted {
                // (only a floor that moved a little under the foot - the bus's floor settling:
                // a floor found a stair's height away is another one, sought from the body's
                // level as it climbs, and the foot popped up onto it in a frame; it stays
                // and steps up instead)
                let z = Self::sample_floor(input, f.pos.truncate(), f.pos.z);
                if (z - f.pos.z).abs() < 0.2 {
                    f.pos.z = z;
                }
            } else {
                // A landing at the same x/y needs resampling after a vertical change too.
                f.to_sampled = DVec2::splat(f64::MAX);
            }
        }
    }
}
