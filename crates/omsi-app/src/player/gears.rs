//! The automated manual (#713) and the gate a gear lever was put into last (#1832).

use super::Player;

impl Player {
    /// The automated manual (the settings' `auto_shift`, #713; not an OMSI feature): on a
    /// manual gearbox worked through gates, first gear goes in when the throttle is pressed
    /// in neutral at a standstill, the next gear up once the engine turns well above its idle
    /// (later the more throttle), and the next down when it falls back towards it. The
    /// clutch is worked as for a shift by key.
    pub(crate) fn tick_auto_shift(&mut self, dt: f32, throttle: f32, brake: f32) {
        self.auto_shift_wait = (self.auto_shift_wait - dt).max(0.0);
        let read = self.script_gate_gear();
        if self.gates.tick(dt, read) {
            log::info!("gear lever: the script's gear variable did not follow gate {:?} (stayed {read:?}); the gate put in last is taken as the gear", self.gates.last);
        }
        if !self.auto_shift || !self.vehicle.ty.program.manual_gearbox() {
            return;
        }
        let Some(cur) = self.gate_gear() else { return };
        let Some(n) = ["engine_n", "antrieb_eng_n", "engine_rpm", "motor_n", "motor_rpm"].iter().find_map(|v| self.vehicle.var(v)) else { return };
        let kmh = self.vehicle.physics.velocity_kmh().abs();
        // the idle speed, from the engine running free at a standstill
        if n > 300.0 && kmh < 1.0 && throttle < 0.02 && (cur == 0 || self.axes.clutch > 0.9) {
            self.auto_shift_idle = if self.auto_shift_idle > 0.0 { self.auto_shift_idle + (n - self.auto_shift_idle) * (dt / 2.0).min(1.0) } else { n };
        }
        // (not while the clutch is still coming up from the last shift - except in neutral,
        // or rolling to a stop, where the automatic clutch holds it down anyway)
        if n < 300.0 || self.auto_shift_wait > 0.0 || (self.axes.clutch > 0.5 && cur != 0 && kmh >= 5.0) {
            return;
        }
        // (the governor's speed: full throttle, and the engine no longer rising for most of
        // a second)
        if throttle > 0.8 {
            let (peak, flat) = &mut self.auto_shift_climb;
            if n > *peak * 1.01 {
                (*peak, *flat) = (n, 0.0);
            } else {
                *flat += dt;
                if *flat > 0.8 {
                    self.auto_shift_max = self.auto_shift_max.max(*peak);
                }
            }
        } else {
            self.auto_shift_climb = (0.0, 0.0);
        }
        let idle = if self.auto_shift_idle > 300.0 { self.auto_shift_idle } else { 700.0 };
        let top = (1..=12).take_while(|g| self.vehicle.ty.program.trigger(&format!("kw_s_{g}")).is_some()).last().unwrap_or(0);
        let to = auto_shift_gear(cur, top, n / idle, self.auto_shift_max / idle, throttle, brake, kmh);
        if to != cur && self.shift_gate_to(to) {
            self.auto_shift_wait = 1.5;
        }
    }
}

/// The gate a gear lever was put into last. Some mods keep their gear in a variable of
/// their own and also declare the automatics' `antrieb_getr_aktugang`, which then never
/// moves: read as the gear, the automated manual saw neutral after every shift and put
/// first in again and again (an O305 Schaltwagen, a Citybus i280, #1832). A gate put in
/// with the clutch down whose variable has not followed a second later marks the
/// variable deaf, and from then on the gate put in last is the gear.
#[derive(Debug, Clone, Default)]
pub(crate) struct GateMemory {
    pub(crate) last: Option<i32>,
    check: Option<(i32, f32)>,
    deaf: bool,
}

impl GateMemory {
    /// Gate `gate` was put in; `checked`: with the clutch down, so its variable must follow.
    pub(crate) fn engaged(&mut self, gate: i32, checked: bool) {
        self.last = Some(gate);
        if checked && !self.deaf {
            self.check = Some((gate, 1.0));
        }
    }

    /// A frame of `dt` seconds with the script's variable reading `read`: true when it is
    /// found deaf now.
    pub(crate) fn tick(&mut self, dt: f32, read: Option<i32>) -> bool {
        let Some((gate, left)) = &mut self.check else { return false };
        *left -= dt;
        if *left > 0.0 {
            return false;
        }
        let gate = *gate;
        self.check = None;
        self.deaf = read.is_some_and(|r| r != gate);
        self.deaf
    }

    /// The gear engaged, from the script's variable reading `read`.
    pub(crate) fn gear(&self, read: i32) -> i32 {
        if self.deaf { self.last.unwrap_or(read) } else { read }
    }
}

/// The automated manual's choice (#713) from gear `cur` (of `top`), the engine speed as a
/// multiple of its idle, the highest seen so far (0 when none yet), the pedals and the road
/// speed.
pub(crate) fn auto_shift_gear(cur: i32, top: i32, n_over_idle: f32, max_over_idle: f32, throttle: f32, brake: f32, kmh: f32) -> i32 {
    if cur == 0 {
        return if throttle > 0.1 && brake < 0.05 && kmh < 3.0 && top >= 1 { 1 } else { 0 };
    }
    if cur < 1 {
        return cur;
    }
    // stopping: back to first, ready to pull away
    if cur > 1 && kmh < 5.0 {
        return 1;
    }
    // (and below what the governor lets the engine turn to: a diesel turning at most three
    // and a half times its idle - an O305's, a Citybus i280's - never reached 3.4 under full
    // throttle, and stayed in first, #1832)
    let mut up = 2.2 + 1.2 * throttle.clamp(0.0, 1.0);
    if max_over_idle > 2.0 {
        up = up.min(0.88 * max_over_idle);
    }
    if cur < top && throttle > 0.05 && n_over_idle > up {
        cur + 1
    } else if cur > 1 && n_over_idle < 1.35 {
        cur - 1
    } else {
        cur
    }
}

#[cfg(test)]
mod auto_shift_tests {
    use super::auto_shift_gear as g;
    use super::GateMemory;

    #[test]
    fn a_gear_variable_that_does_not_follow_the_gates_gives_way_to_the_gate() {
        // the variable follows: it stays the gear, a refused gate included
        let mut m = GateMemory::default();
        m.engaged(1, true);
        assert!(!m.tick(0.5, Some(0)));
        assert!(!m.tick(0.6, Some(1)));
        assert_eq!(m.gear(1), 1);
        m.engaged(2, false);
        assert_eq!(m.gear(1), 1);
        // it stays at neutral after first went in with the clutch down (#1832)
        let mut m = GateMemory::default();
        m.engaged(1, true);
        assert!(m.tick(1.1, Some(0)));
        assert_eq!(m.gear(0), 1);
        m.engaged(2, true);
        assert!(!m.tick(1.1, Some(0)), "found deaf once");
        assert_eq!(m.gear(0), 2);
        m.engaged(-1, false);
        assert_eq!(m.gear(0), -1);
    }

    #[test]
    fn the_automated_manual_shifts_by_the_engine_speed() {
        // pulling away: first gear in from neutral, never reverse
        assert_eq!(g(0, 5, 1.0, 0.0, 0.5, 0.0, 0.0), 1);
        assert_eq!(g(0, 5, 1.0, 0.0, 0.0, 0.0, 0.0), 0);
        assert_eq!(g(-1, 5, 3.0, 0.0, 1.0, 0.0, 5.0), -1);
        // up early with a light foot, late with a heavy one
        assert_eq!(g(2, 5, 2.8, 0.0, 0.3, 0.0, 30.0), 3);
        assert_eq!(g(2, 5, 2.8, 0.0, 1.0, 0.0, 30.0), 2);
        assert_eq!(g(2, 5, 3.7, 0.0, 1.0, 0.0, 30.0), 3);
        // never past the top gear, down near the idle, not below first
        assert_eq!(g(5, 5, 4.0, 0.0, 1.0, 0.0, 90.0), 5);
        assert_eq!(g(3, 5, 1.2, 0.0, 0.0, 0.5, 20.0), 2);
        assert_eq!(g(1, 5, 1.0, 0.0, 0.0, 1.0, 2.0), 1);
        assert_eq!(g(4, 5, 1.0, 0.0, 0.0, 1.0, 3.0), 1);
        // an engine the governor holds at 3.6 x its idle: up under full throttle all the same
        assert_eq!(g(1, 5, 3.4, 3.6, 1.0, 0.0, 15.0), 2);
        assert_eq!(g(1, 5, 3.4, 0.0, 1.0, 0.0, 15.0), 1);
    }
}
