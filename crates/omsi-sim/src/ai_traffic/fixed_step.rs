//! The traffic's fixed step: every decision and every body moves on in steps of `SIM_DT`,
//! whatever the frame rate. With one step per frame, how a car decided depended on the frame
//! rate (a car at 144 frames a second looked at the junction again before one at 30 had
//! moved), and two runs of the same session at different frame rates drove differently.
//! The frame's time is collected in `sim_accum` and spent in whole steps; what is left over
//! says how far the picture is between the last two steps (`frame_alpha`), and the cars are
//! drawn there (`show_between`), so that they glide at any frame rate.

use super::*;

/// One step of the traffic (s).
pub const SIM_DT: f32 = 0.02;
/// At most this many steps in one frame: after a long stall (a hitch, a breakpoint) the
/// traffic does not try to make up for all of it at once, which made the next frame as long
/// again.
pub const MAX_SIM_STEPS: u32 = 8;
/// A part that moved further than this between two steps was put somewhere (placed, taken
/// over, recovered): it is drawn where it is, not swept there.
const JUMP: f64 = 10.0;

/// Add a frame of `frame_dt` seconds to `accum` and take out the whole steps of `dt` it
/// holds: how many to run now. At most `max_steps`, the time beyond them dropped. The steps
/// depend only on the time that went by, not on how it was split into frames (as long as
/// no frame needs more than `max_steps`).
pub fn advance_fixed_clock(accum: &mut f64, frame_dt: f32, dt: f32, max_steps: u32) -> u32 {
    if !(frame_dt > 0.0) || !(dt > 0.0) {
        return 0;
    }
    let dt = dt as f64;
    *accum += frame_dt as f64;
    // (a hair under a whole step counts as one: 0.02 s summed fifty times is not quite 1 s)
    let n = ((*accum + 1e-9) / dt).floor().max(0.0) as u64;
    let n = n.min(max_steps as u64);
    *accum = (*accum - n as f64 * dt).max(0.0);
    if *accum >= dt {
        // more than `max_steps` behind: the rest of the debt is dropped, its fraction kept
        *accum %= dt;
    }
    n as u32
}

/// Where one part of a car stood after a step: position, heading, pitch and bank.
pub type PartPose = (DVec3, f64, f32, f32);

impl TrafficSim {
    /// Move the traffic on by a frame of `frame_dt` seconds: as many fixed steps as the time
    /// collected holds (see `advance_fixed_clock`). Returns how many ran. A LAN client's
    /// traffic is the host's and only follows it, frame by frame.
    pub fn advance(&mut self, frame_dt: f32, player: Option<PlayerBox>) -> u32 {
        if self.mirror {
            self.tick(frame_dt, player);
            self.frame_alpha = 1.0;
            return 1;
        }
        let steps = advance_fixed_clock(&mut self.sim_accum, frame_dt, SIM_DT, MAX_SIM_STEPS);
        for _ in 0..steps {
            self.remember_poses();
            self.tick(SIM_DT, player);
        }
        self.frame_alpha = ((self.sim_accum / SIM_DT as f64) as f32).clamp(0.0, 1.0);
        steps
    }

    /// Every car's parts as they stand before a step, to draw them between it and the next.
    fn remember_poses(&mut self) {
        let mut before = std::mem::take(&mut self.pose_before);
        before.clear();
        for c in &self.cars {
            let mut parts = Vec::with_capacity(1 + c.vehicle.trailers.len());
            parts.push((c.vehicle.position, c.vehicle.heading, c.vehicle.pitch, c.vehicle.bank));
            parts.extend(c.vehicle.trailers.iter().map(|t| (t.position, t.heading, t.pitch, t.bank)));
            before.insert(c.id, parts);
        }
        self.pose_before = before;
    }

    /// Put every car where the picture of this frame shows it, between the last two steps,
    /// and return where they really stand, for `show_actual` to put them back after drawing.
    /// Nothing moves when the frame falls on a step.
    pub fn show_between(&mut self) -> Vec<(usize, Vec<PartPose>)> {
        let a = self.frame_alpha;
        let mut actual = Vec::new();
        if a >= 1.0 || self.mirror {
            return actual;
        }
        for (i, c) in self.cars.iter_mut().enumerate() {
            let Some(before) = self.pose_before.get(&c.id) else { continue };
            if before.len() != 1 + c.vehicle.trailers.len() {
                continue;
            }
            let now = (c.vehicle.position, c.vehicle.heading, c.vehicle.pitch, c.vehicle.bank);
            if (now.0 - before[0].0).length() > JUMP {
                continue;
            }
            let mut saved = Vec::with_capacity(before.len());
            saved.push(now);
            let shown = between(before[0], now, a);
            (c.vehicle.position, c.vehicle.heading, c.vehicle.pitch, c.vehicle.bank) = shown;
            for (t, b) in c.vehicle.trailers.iter_mut().zip(&before[1..]) {
                let now = (t.position, t.heading, t.pitch, t.bank);
                saved.push(now);
                (t.position, t.heading, t.pitch, t.bank) = between(*b, now, a);
            }
            actual.push((i, saved));
        }
        actual
    }

    /// Back where they really stand after the picture was drawn (see `show_between`).
    pub fn show_actual(&mut self, actual: Vec<(usize, Vec<PartPose>)>) {
        for (i, saved) in actual {
            let Some(c) = self.cars.get_mut(i) else { continue };
            if let Some(&p) = saved.first() {
                (c.vehicle.position, c.vehicle.heading, c.vehicle.pitch, c.vehicle.bank) = p;
            }
            for (t, &p) in c.vehicle.trailers.iter_mut().zip(saved.iter().skip(1)) {
                (t.position, t.heading, t.pitch, t.bank) = p;
            }
        }
    }
}

/// `a` of the way from `from` to `to`, the angles the short way round.
fn between(from: PartPose, to: PartPose, a: f32) -> PartPose {
    let turn = (to.1 - from.1 + 540.0).rem_euclid(360.0) - 180.0;
    (
        from.0.lerp(to.0, a as f64),
        from.1 + turn * a as f64,
        from.2 + (to.2 - from.2) * a,
        from.3 + (to.3 - from.3) * a,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The same seconds give the same steps whether they come as 15, 30, 60 or 144 frames
    /// a second.
    #[test]
    fn the_steps_depend_on_the_time_not_the_frame_rate() {
        for fps in [15.0f32, 30.0, 60.0, 144.0] {
            let mut accum = 0.0;
            let frames = (10.0 * fps).round() as u32;
            let steps: u32 = (0..frames)
                .map(|_| advance_fixed_clock(&mut accum, 1.0 / fps, SIM_DT, MAX_SIM_STEPS))
                .sum();
            assert_eq!(steps, 500, "{fps} fps");
        }
    }

    #[test]
    fn a_long_stall_runs_at_most_the_cap_and_drops_the_rest() {
        let mut accum = 0.0;
        assert_eq!(advance_fixed_clock(&mut accum, 1.0, SIM_DT, MAX_SIM_STEPS), MAX_SIM_STEPS);
        assert!(accum < SIM_DT as f64);
        // and the next frame is ordinary again
        assert_eq!(advance_fixed_clock(&mut accum, 0.021, SIM_DT, MAX_SIM_STEPS), 1);
    }

    #[test]
    fn nothing_runs_for_no_time() {
        let mut accum = 0.0;
        assert_eq!(advance_fixed_clock(&mut accum, 0.0, SIM_DT, MAX_SIM_STEPS), 0);
        assert_eq!(advance_fixed_clock(&mut accum, f32::NAN, SIM_DT, MAX_SIM_STEPS), 0);
        assert_eq!(accum, 0.0);
    }

    #[test]
    fn a_heading_is_turned_the_short_way_round() {
        let p = |h: f64| (DVec3::ZERO, h, 0.0, 0.0);
        let mid = between(p(350.0), p(10.0), 0.5);
        assert!((mid.1.rem_euclid(360.0) - 0.0).abs() < 1e-9 || (mid.1 - 360.0).abs() < 1e-9);
        let q = between(p(10.0), p(350.0), 0.25);
        assert!((q.1 - 5.0).abs() < 1e-9);
    }
}
