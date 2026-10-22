//! LAN play (see omsi-app's `lan_world`): a client runs the host's light clocks instead of
//! its own simulation.

use super::*;

impl TrafficSim {
    pub fn is_mirror(&self) -> bool {
        self.mirror
    }

    /// A client's frame: interpolate the host's light clocks, without re-evaluating its
    /// stop and jump points from the client's incomplete traffic requests.
    pub fn mirror_tick(&mut self, dt: f32) {
        self.time += dt;
        self.day_time += dt as f64 * self.time_scale;
        self.last_dt = dt;
        let day_time = self.day_time;
        for c in self.lights.iter_mut() {
            mirror_light_tick(c, dt, day_time);
        }
        self.log_lights();
    }
}
