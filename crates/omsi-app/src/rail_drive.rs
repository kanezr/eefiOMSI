//! Driving a rail vehicle (a tram, a train): the player's vehicle is bound to the track.
//! Its own physics give its speed along the rails; where it stands and which way it faces
//! come from the rail lanes of the map's paths, as the AI trains' do. At a fork it takes
//! the branch its indicator points to (Berlin's trams set their points so), else the
//! branch the switch is set to, else the straightest - and throws the points it runs over
//! (`World::set_switches`), so they move with it.
//!
//! A vehicle is rail-bound when its file says so: `[rail_body_osc]`, a `[contact_shoe]` or
//! `[boogies]`.

use crate::player::Player;
use crate::scene::World;
use glam::DVec3;
use omsi_sim::traffic::{LaneKind, Network};

/// Where on the track the vehicle is.
#[derive(Debug, Clone)]
pub(crate) struct RailDrive {
    pub lane: usize,
    pub s: f32,
    /// The vehicle faces the lane's direction (else it drives it backwards).
    pub along: bool,
    /// The track the vehicle has come along: (distance travelled, the origin's position),
    /// oldest first - where its coupled parts are placed (see `VehicleInstance::retrail`).
    trail: std::collections::VecDeque<(f64, DVec3)>,
    /// Distance travelled along the track (forward positive).
    u: f64,
}

/// How much trail is kept behind the vehicle (m): a long train's length.
const TRAIL: f64 = 400.0;

/// Whether the vehicle the arguments name is bound to rails (it then needs the rail
/// network the traffic builds, with or without cars).
pub(crate) fn args_rail(args: &crate::Args) -> bool {
    args.bus
        .as_deref()
        .map(|b| omsi_cfg::resolve_path(&args.root, b))
        .and_then(|p| omsi_vehicle::Vehicle::load(&p).ok())
        .is_some_and(|d| is_rail(&d))
}

/// Whether the vehicle `def` is bound to rails.
pub(crate) fn is_rail(def: &omsi_vehicle::Vehicle) -> bool {
    def.is_rail()
}

/// Put the vehicle on the rail lane nearest it (within `reach` m), facing the way that
/// lane runs nearest its own heading.
pub(crate) fn attach(p: &mut Player, net: &Network, reach: f64) -> Option<RailDrive> {
    let pos = p.vehicle.position;
    let (lane, s, dist) = net.nearest_lane(pos, LaneKind::Rail).or_else(|| nearest_anywhere(net, pos))?;
    if dist > reach {
        return None;
    }
    let (_, h) = net.lanes[lane].at(s);
    let diff = angle_diff(h as f64, p.vehicle.heading);
    let mut r = RailDrive { lane, s, along: diff.abs() <= 90.0, trail: Default::default(), u: 0.0 };
    // the track behind it, for its coupled parts: walked backwards from where it stands
    let mut back = r.clone();
    let mut points = Vec::new();
    for k in 1..=60 {
        back.step(net, None, -2.0, 0);
        let (pos, _) = net.lanes[back.lane].at(back.s);
        points.push((-2.0 * k as f64, pos));
    }
    let (here, _) = net.lanes[lane].at(s);
    r.trail = points.into_iter().rev().chain(std::iter::once((0.0, here))).collect();
    log::info!("rail: {} stands on rail lane {lane} at {s:.1} m ({dist:.1} m away) at ({:.1}, {:.1})", p.vehicle.ty.def.path.display(), net.lanes[lane].at(s).0.x, net.lanes[lane].at(s).0.y);
    Some(r)
}

/// The nearest rail lane of the whole network (the grid only looks nearby).
fn nearest_anywhere(net: &Network, p: DVec3) -> Option<(usize, f32, f64)> {
    let mut best: Option<(usize, f32, f64)> = None;
    for (i, l) in net.lanes.iter().enumerate().filter(|(_, l)| l.kind == LaneKind::Rail) {
        if let Some((s, d)) = l.nearest_point(p) {
            if best.map(|b| d < b.2).unwrap_or(true) {
                best = Some((i, s, d));
            }
        }
    }
    best
}

/// The trail's point at travelled distance `u` (between its samples; None beyond its ends).
pub(crate) fn point_at(trail: &std::collections::VecDeque<(f64, DVec3)>, u: f64) -> Option<DVec3> {
    let i = trail.iter().position(|(v, _)| *v >= u)?;
    if i == 0 {
        return (trail[0].0 - u < 0.01).then_some(trail[0].1);
    }
    let (a, b) = (trail[i - 1], trail[i]);
    let t = ((u - a.0) / (b.0 - a.0).max(1e-6)).clamp(0.0, 1.0);
    Some(a.1 + (b.1 - a.1) * t)
}

fn angle_diff(a: f64, b: f64) -> f64 {
    (a - b + 540.0).rem_euclid(360.0) - 180.0
}

impl RailDrive {
    /// Move `ds` metres (forward along the vehicle, negative backwards) and put the
    /// vehicle there. `blinker`: 1 left, 2 right (the branch at the next fork).
    pub(crate) fn advance(&mut self, p: &mut Player, net: &Network, world: &World, ds: f32, blinker: u8) {
        if !self.step(net, Some(world), ds, blinker) {
            p.vehicle.set_speed(0.0);
        }
        let (pos, h) = net.lanes[self.lane].at(self.s);
        let heading = if self.along { h as f64 } else { (h as f64 + 180.0).rem_euclid(360.0) };
        let v = &mut p.vehicle;
        v.position = pos;
        v.heading = heading;
        // the track steers: the wheel stays straight
        let mut c = v.physics.controls;
        c.steering = 0.0;
        v.set_controls(c);
        // the trail, and the coupled parts on it
        self.u += ds as f64;
        if self.trail.back().map(|b| (self.u - b.0).abs() > 0.5).unwrap_or(true) {
            // (backing up takes the trail back with it)
            while self.trail.back().is_some_and(|b| b.0 > self.u) {
                self.trail.pop_back();
            }
            self.trail.push_back((self.u, pos));
            while self.trail.front().is_some_and(|f| self.u - f.0 > TRAIL) {
                self.trail.pop_front();
            }
        }
        if !v.trailers.is_empty() {
            let trail = &self.trail;
            let u = self.u;
            v.retrail(0.0, &|d| point_at(trail, u - d));
        }
    }

    /// Move along the rails by `ds` (the vehicle's forward); false at the end of the track.
    /// `world`: the switches (read and thrown); None looks without touching them.
    fn step(&mut self, net: &Network, world: Option<&World>, ds: f32, blinker: u8) -> bool {
        // along the lane: the vehicle's forward is the lane's if it faces it
        let mut d = if self.along { ds } else { -ds };
        let mut guard = 0;
        while guard < 16 {
            guard += 1;
            let len = net.lanes[self.lane].length();
            let t = self.s + d;
            if t > len {
                let facing = self.along;
                match self.pick(net, world, &net.lanes[self.lane].next, true, blinker, facing) {
                    Some(n) => {
                        d = t - len;
                        self.lane = n;
                        self.s = 0.0;
                        continue;
                    }
                    None => {
                        // the end of the track: the vehicle stops at its buffer
                        self.s = len;
                        return false;
                    }
                }
            } else if t < 0.0 {
                let prev = net.prev.get(self.lane).cloned().unwrap_or_default();
                match self.pick(net, world, &prev, false, blinker, !self.along) {
                    Some(n) => {
                        d = t;
                        self.lane = n;
                        self.s = net.lanes[n].length();
                        continue;
                    }
                    None => {
                        self.s = 0.0;
                        return false;
                    }
                }
            } else {
                self.s = t;
                break;
            }
        }
        true
    }

    /// The next (or previous) rail lane among `candidates`: the indicator's branch, else
    /// the one the switch is set to, else the straightest. The points it takes are thrown.
    fn pick(&self, net: &Network, world: Option<&World>, candidates: &[usize], forward: bool, blinker: u8, _facing: bool) -> Option<usize> {
        let here = &net.lanes[self.lane];
        let h0 = if forward { here.at(here.length()).1 } else { here.at(0.0).1 } as f64;
        let mut rails: Vec<(usize, f64)> = candidates
            .iter()
            .copied()
            .filter(|&i| net.lanes.get(i).map(|l| l.kind == LaneKind::Rail).unwrap_or(false))
            .map(|i| {
                let l = &net.lanes[i];
                // how the branch turns over its first 15 m (positive: right)
                let h1 = if forward { l.at(15.0f32.min(l.length())).1 } else { l.at((l.length() - 15.0).max(0.0)).1 } as f64;
                let turn = angle_diff(h1, h0) * if forward { 1.0 } else { -1.0 };
                (i, turn)
            })
            .collect();
        if rails.len() <= 1 {
            return rails.first().map(|r| r.0);
        }
        rails.sort_by(|a, b| a.1.total_cmp(&b.1));
        let chosen = match blinker {
            1 => rails.first().map(|r| r.0),
            2 => rails.last().map(|r| r.0),
            _ => rails
                .iter()
                .find(|(i, _)| net.lanes[*i].key.and_then(|k| world.and_then(|w| w.switch_set_to(k.id, k.path))) == Some(true))
                .or_else(|| rails.iter().min_by(|a, b| a.1.abs().total_cmp(&b.1.abs())))
                .map(|r| r.0),
        }?;
        if let (Some(k), Some(w)) = (net.lanes[chosen].key, world) {
            w.set_switches(&[(k.id, k.path)]);
        }
        Some(chosen)
    }
}

/// The indicator the driver has set (1 left, 2 right, else 0), from the script's switch or
/// its lamps.
fn blinker_of(v: &omsi_sim::VehicleInstance) -> u8 {
    let on = |n: &str| v.var(n).map(|x| x > 0.5).unwrap_or(false);
    match v.var("lights_sw_blinker") {
        Some(s) if (0.5..2.5).contains(&s) => s.round() as u8,
        _ => match (on("lights_blinker_l"), on("lights_blinker_r")) {
            (true, false) => 1,
            (false, true) => 2,
            _ => 0,
        },
    }
}

/// One frame of a rail-bound player vehicle after its own update: onto the track (first),
/// then along it by the distance its speed covered.
pub(crate) fn frame(p: &mut Player, net: Option<&Network>, world: &World, dt: f32) {
    if !p.rail_bound {
        return;
    }
    let Some(net) = net else { return };
    if p.rail.is_none() {
        if let Some(mut r) = attach(p, net, 3000.0) {
            r.advance(p, net, world, 0.0, 0);
            p.rail = Some(r);
        }
        return;
    }
    // A vehicle whose scripts give no drive to a driver (the stock trains are scripted for
    // the AI only) is driven by a plain traction and brake of its own: a locomotive's
    // pull (up to 300 kN, 4 MW) and 1.2 m/s² of brake at full pedal.
    let c = p.vehicle.physics.controls;
    let scripted = p.vehicle.var("M_Wheel").is_some_and(|m| m.abs() > 1.0) || p.vehicle.var("Brakeforce").is_some_and(|b| b > 1.0);
    if !scripted {
        let m = p.vehicle.physics.mass_kg.max(1000.0);
        let v = p.vehicle.physics.speed;
        let reverse = p.vehicle.var("rail_reverse").is_some_and(|r| r > 0.5);
        let pull = (c.throttle.clamp(0.0, 1.0) * 300_000.0).min(4.0e6 / v.abs().max(1.0)).min(0.15 * m * 9.81) * if reverse { -1.0 } else { 1.0 };
        let brake = c.brake.clamp(0.0, 1.0) * 1.2 * m;
        let resist = 0.002 * m * 9.81 + 5.0 * v * v;
        let mut dv = (pull / m) * dt;
        let stop = ((brake + resist) / m) * dt;
        let nv = v + dv;
        dv = if nv.abs() <= stop && c.throttle < 0.05 { -v } else { dv - stop * nv.signum() };
        p.vehicle.set_speed(v + dv);
    }
    let ds = p.vehicle.physics.speed * dt;
    let blinker = blinker_of(&p.vehicle);
    if let Some(mut r) = p.rail.take() {
        r.advance(p, net, world, ds, blinker);
        p.rail = Some(r);
    }
}
