//! Timetable buses as traffic. A timetable bus is one of the town's AI cars (`AiCar`,
//! created by the same `Traffic::create_car` as every other car, driven by the same
//! following, lights, junctions, lane changes and passing) that carries a `BusService`:
//! the stops of its trip, the pull into the bay, the doors at each stop, the wait for the
//! departure time, the blinker and the pull back out, the end of the trip, and the people
//! aboard. Nothing of the player's bus is involved: no driver inputs, no throttle and brake
//! for the script to turn into motion - the car moves, the script only animates it.

use super::berths::{BerthKey, BerthScene, Berths, QUEUE_STANDOFF, SERVE_BEHIND_REACH};
use crate::traffic::{AiState, LaneKind, Network};
use crate::VehicleInstance;
use std::collections::VecDeque;

/// Script callbacks describe the whole trip, including stations on unloaded tiles.
/// The service's geometric queue only describes the part the bus can drive now.
pub struct AiTimetable {
    pub line: String,
    pub terminus: String,
    pub stops: Vec<(i64, String, f32, f32)>,
}

impl AiTimetable {
    pub fn install(&self, host: &mut crate::VehicleHost, next: Option<&Stop>) {
        host.schedule_active = 1.0;
        host.tt_line = self.line.clone();
        host.tt_stops = self.stops.iter().map(|s| (s.1.clone(), s.2, s.3)).collect();
        host.tt_stop_ids = self.stops.iter().map(|s| s.0).collect();
        host.tt_terminus_index = host
            .hof
            .as_ref()
            .and_then(|hof| {
                hof.termini
                    .iter()
                    .position(|t| t.texture_id == self.terminus)
            })
            .map_or(-1, |i| i as i32);
        host.tt_busstop_index = timetable_stop_index(host, next).unwrap_or(0) as i32;
        host.tt_delay = 0.0;
    }
}

fn timetable_stop_index(host: &crate::VehicleHost, next: Option<&Stop>) -> Option<usize> {
    let stop = next?;
    // A circular trip may visit the same object twice. Its departure time identifies
    // the visit, rather than a lookup by object id alone. f32 times lose subsecond
    // precision on later days, so compare after casting to the callback's type.
    host.tt_stop_ids
        .iter()
        .zip(&host.tt_stops)
        .position(|(id, data)| *id == stop.id && data.2 == stop.depart as f32)
}

/// One stop of the trip, on the car's route.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stop {
    /// Index into the car's route of the lane the stop is on.
    pub ri: usize,
    /// Where the front of the bus comes to rest, along that lane (m).
    pub s: f32,
    /// How far right of the lane's middle the stop's bay lies (m).
    pub bay: f32,
    /// Timetable departure (seconds of the day).
    pub depart: f64,
    /// The stop's map object (its `[busstop]` strings weigh who gets off there).
    pub id: i64,
    /// The side the platform lies on (see `tiles::stop_side`): 0 = right, 1 = the other,
    /// 2 = both. A bus whose doors are on both sides opens only these (it reads the value
    /// as `AI_Scheduled_AtStation_Side`).
    pub side: f32,
}

impl Stop {
    #[allow(clippy::type_complexity)]
    pub fn from_tuple(t: (usize, f32, f32, f64, i64, f32)) -> Stop {
        Stop { ri: t.0, s: t.1, bay: t.2, depart: t.3, id: t.4, side: t.5 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// On the way to the next stop (or with none left, to the end of the route).
    Running,
    /// At the stop with the doors open: people get off and on.
    Boarding,
    /// At the stop with the doors shut, waiting for the departure time (a layover at the
    /// first stop, or a bus that is early).
    Waiting,
    /// The doors close and the stop brake comes off, the blinker goes on: about to pull out.
    Closing,
    /// At the end of the trip: it stands until the timetable hands it the tour's next trip
    /// or lets it go.
    TripDone,
}

/// Something went wrong at a stop that the bus could not mend by itself (it is kept for the
/// diagnostics; the bus goes on as best it can).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceFault {
    /// The doors are open and the script never says they are shut: the bus stands.
    DoorsOpenUnanswered,
    /// The script never said the doors are shut, and nobody knows whether they are: the
    /// bus left after `CLOSE_MAX`.
    DoorsUnanswered,
}

#[derive(Debug, Clone)]
pub struct BusService {
    pub stops: VecDeque<Stop>,
    pub phase: Phase,
    /// Seconds in the current phase.
    pub phase_t: f32,
    /// Seconds of boarding left (the people at the doors hold it open: `hold`).
    pub boarding: f32,
    /// Seconds the people at the doors have held it past its departure at this stop: after
    /// `HOLD_MAX` they hold it no longer (see `hold`).
    pub held_over: f32,
    /// When it may leave the stop (seconds of the day).
    pub leave_at: f64,
    /// When it came to the stop it stands at (seconds of the day).
    pub arrived_at: f64,
    /// The doors have been open at this stop already (a layover bus opens them only for
    /// the last minute before its departure).
    pub boarded: bool,
    /// Seconds behind (positive) or ahead of the timetable, as of the last stop.
    pub delay: f64,
    /// Put out before its departure: it waits for it at its first stop.
    pub layover: bool,
    /// The timetable still carries the route on as tiles bring their lanes: at the end of
    /// what it has, it waits for more.
    pub route_open: bool,
    /// The terminus of its trip, the name the waiting people read off it (Omsi.exe's bus
    /// +0x7bc) to see whether it goes their way.
    pub terminus: String,
    /// How far the front stop was last frame (m; infinite when not measured yet).
    near_d: f32,
    /// Whether it serves the front stop, once that is settled (`SKIP_DECIDE`).
    serve: Option<bool>,
    /// The trip's last station: always served.
    pub last_stop: Option<i64>,
    /// Stops the timetable has it serve in any case, and those it serves when it would be
    /// more than `EARLY_STOP_SHORT` early (`schedule::TripTimes::kinds`).
    pub always: Vec<i64>,
    pub serve_early: Vec<i64>,
    /// The stops whose time the map wrote itself (`schedule::TripTimes::holds`): the only
    /// ordinary stops the bus waits at for its departure. At every other stop the timetable's
    /// time is just the running time shared out by distance, so an early bus serves it and
    /// drives on instead of standing there until its time (`waits_here`).
    pub holds: Vec<i64>,
    /// The berth it holds (`berths`), with the stop's place on its route (route index and
    /// distance along that lane): kept from when it is handed the berth until its rear has
    /// left it after pulling out.
    held: Option<(BerthKey, usize, f32)>,
    /// It serves (or served) its front stop from behind the bus holding the berth: it holds
    /// no berth, and on a layover it moves up once the berth is free (`move_up`).
    pub behind: bool,
    /// Moving up onto the berth from behind, doors shut.
    moving_up: bool,
    /// Seconds it has been at rest on its way to the front stop (`DOCK_SPEED`).
    rest_t: f32,
    /// The stops it could not serve (overshot, or could not get to): their ids in order.
    pub missed: Vec<i64>,
    pub fault: Option<ServiceFault>,
}

/// Docked: the origin within `DOCK_LONG` of its stop point along the lane, the body within
/// `DOCK_LAT` of where it moves across to, at rest (m, m, m/s).
const DOCK_LONG: f32 = 0.5;
const DOCK_LAT: f32 = 0.25;
const DOCK_SPEED: f32 = 0.1;
/// At rest this near its point (along, across) for `SERVE_HERE_AFTER` seconds, a bus serves
/// the stop from where it stands: people reach the doors all the same, and creeping on to
/// the exact point would keep the buses behind waiting.
const SERVE_HERE_LONG: f32 = 2.5;
const SERVE_HERE_LAT: f32 = 0.8;
const SERVE_HERE_AFTER: f32 = 1.5;
/// At rest this long at its point but where it cannot dock from (across), the stop is
/// missed: it drives on rather than back up or stand there for good. Held short of its point
/// (within `UNREACHABLE_NEAR` m) by something that came to stand in the berth, it serves the
/// stop from there after `berths::BLOCKED_LONG`.
const UNREACHABLE_AFTER: f32 = 8.0;
const UNREACHABLE_NEAR: f32 = 12.0;
/// Pulling out of a bay is held for traffic only when the bus stands this far or more off
/// the lane's middle (m): one already in line with the lane just drives.
const ALIGNED: f32 = 0.3;
/// Where the trip's track runs on more than this past its last stop (m, to a place to stand
/// on the layover), the trip ends at the end of the track, not at the stop.
const TRACK_ON: f32 = 30.0;

/// Whether the vehicle's script answers `AI_Scheduled_AtStation` (writes it back to 0 once
/// its doors are shut, as the stock `door_X10_AI.osc` does): only then is there an answer to
/// wait for.
fn script_answers(vehicle: &VehicleInstance) -> bool {
    let p = &vehicle.ty.program;
    p.var("AI_Scheduled_AtStation").is_some_and(|v| p.stores(v))
}

/// Whether the script's doors stand open: None when the script has no door variables the
/// traffic knows (`door_0` .. `door_3`, the stock buses' door positions, 0 shut .. 1 open).
fn doors_open(vehicle: &VehicleInstance) -> Option<bool> {
    let mut known = false;
    let mut open = false;
    for name in ["door_0", "door_1", "door_2", "door_3"] {
        if vehicle.has_script_var(name) || vehicle.ty.program.var(name).is_some_and(|v| vehicle.ty.program.stores(v)) {
            known = true;
            open |= vehicle.var(name).is_some_and(|v| v > 0.1);
        }
    }
    known.then_some(open)
}

/// How far the rear of a bus with state `st` has got past point `s` of its route's lane
/// `ri` (m, negative before it).
fn past_by(st: &AiState, net: &Network, ri: usize, s: f32) -> f32 {
    if st.route.get(ri).is_none() {
        return f32::INFINITY;
    }
    let origin = if ri >= st.route_index {
        -st.route_distance(net, ri, s)
    } else {
        // the point carried on to the lane the bus is on, as `AiState::route_distance` goes
        // the other way: a lane's length on to the next, the offset to a lane beside (a route
        // that moves over to the lane beside at a stop)
        let mut x = s;
        for k in ri..st.route_index.min(st.route.len().saturating_sub(1)) {
            let (a, b) = (st.route[k], st.route[k + 1]);
            if net.parallel(a, b) {
                x += net.beside_delta(a, b);
            } else {
                x -= net.lanes[a].length();
            }
        }
        st.s - x
    };
    origin - st.rear
}

/// The way left on a bus's route from its origin (m).
fn route_left(st: &AiState, net: &Network) -> f32 {
    st.route.iter().skip(st.route_index).map(|&l| net.lanes[l].length()).sum::<f32>() - st.s
}

/// The side the bus pulls out towards: left (1) from a bay on the right, else right (2).
fn out_signal(bay: f32) -> i32 {
    if bay < -0.1 {
        2
    } else {
        1
    }
}

/// `AI_Engine` at a stop (`BusService::engine_running`): off this long after arriving when
/// the departure is more than `ENGINE_OFF_WAIT` away, on again `ENGINE_ON_BEFORE` before it.
const ENGINE_OFF_AFTER: f64 = 6.0;
const ENGINE_OFF_WAIT: f64 = 60.0;
const ENGINE_ON_BEFORE: f64 = 20.0;

/// Seconds the doors stay open at a stop without anyone holding them.
fn boarding_time(id: u64) -> f32 {
    7.0 + (id % 5) as f32
}

/// An early bus waits at its stop until this long before its departure, a train at its
/// station until `EARLY_LEAVE_RAIL` before (Omsi.exe 0x7d9bdc: it stands while it is more
/// than 20 s, a train 120 s, early). Capped at 40 s, the buses no longer waited for their
/// times at the stops where a timetable holds them all for a connection (#1012).
/// Only applied where the timetable gives the stop a time of its own, and at the trip's
/// ends and a layover - see `BusService::waits_here`.
const EARLY_LEAVE: f64 = 20.0;
const EARLY_LEAVE_RAIL: f64 = 120.0;
/// A layover waits for the departure however long (a tour's bus in on its previous trip).
const LAYOVER_WAIT: f64 = 1800.0;
/// An untimed intermediate stop has only an interpolated departure. Holding to that
/// estimate for many minutes can block the stop and the lane behind it on mod maps.
const INTERPOLATED_STOP_WAIT: f64 = 120.0;

/// How long a bus arriving at `now` stands at a stop it is to leave at `depart`.
fn early_wait(depart: f64, now: f64, layover: bool, rail: bool) -> f64 {
    let lead = if layover {
        0.0
    } else if rail {
        EARLY_LEAVE_RAIL
    } else {
        EARLY_LEAVE
    };
    (depart - lead - now).clamp(0.0, LAYOVER_WAIT)
}
/// The longest the people at the doors hold a bus past its departure (s). Omsi.exe's AI
/// buses do not stand at a stop for ever either.
const HOLD_MAX: f32 = 60.0;

/// On a layover, the doors open this long before the departure.
const LAYOVER_BOARDING: f64 = 45.0;
/// Pull into the bay over this distance before the stop: the stop's docking distance,
/// 30 m unless its object strings say otherwise (Omsi.exe 0x620058, string 4; the bus
/// moves over once it is that near, 0x7dac5e).
const BAY_REACH: f32 = 30.0;
/// Pulling out: at least this long after the doors were told to close (s), at most this
/// long waiting for the script to say they are shut.
const CLOSE_MIN: f32 = 1.5;
const CLOSE_MAX: f32 = 12.0;
/// Brake for the stop from this far.
const STOP_REACH: f32 = 80.0;
/// This near its stop a timetable bus settles whether it stops there at all (Omsi.exe
/// 0x7da5b5: 50 m): not when nobody aboard wants to get off and nobody waits there,
/// unless it is the trip's first or last stop or the bus is more than `EARLY_STOP` early.
const SKIP_DECIDE: f32 = 50.0;
const EARLY_STOP: f64 = 120.0;
const EARLY_STOP_SHORT: f64 = 20.0;

/// What the service needs of the world this frame.
pub struct Ctx<'a> {
    pub net: &'a Network,
    /// The car's way ahead: (lane, distance from its origin to the lane's start).
    pub way: &'a [(usize, f32)],
    pub day_time: f64,
    pub dt: f32,
    pub id: u64,
    /// Seconds it has stood still without a stop of its own.
    pub stopped: f32,
    /// Passing something (the passing manoeuvre owns the lateral position).
    pub passing: bool,
    /// Where it would swerve to round a car parked at the kerb.
    pub kerb_swerve: Option<f32>,
    /// Somebody aboard wants to get off at the front stop or somebody waits there (None:
    /// nobody knows - no passengers run - and every stop is served).
    pub wanted: Option<bool>,
    pub debug: bool,
    /// `TrafficSim::timed_waits_only`: an early bus waits only where `waits_here` says so
    /// (off: at every stop it serves, as Omsi.exe does).
    pub timed_waits_only: bool,
    /// The traffic's time (s), what the berths are timed by.
    pub time: f32,
    /// The front stop's berth as the traffic stands (`TrafficSim::berth_scene`).
    pub berth: BerthScene,
}

impl BusService {
    /// Keep callbacks current before the next AI script frame. No trip-sized buffers
    /// are rebuilt here; tile streaming only changes the geometric stop queue.
    pub fn feed_timetable(&self, vehicle: &mut VehicleInstance, day_time: f64) {
        if vehicle.host.schedule_active < 0.5 {
            return;
        }
        if let Some(index) = timetable_stop_index(&vehicle.host, self.stops.front()) {
            vehicle.host.tt_busstop_index = index as i32;
        } else if self.stops.is_empty() && !self.route_open {
            vehicle.host.tt_busstop_index = vehicle.host.tt_stops.len().saturating_sub(1) as i32;
        }
        vehicle.host.tt_delay = if self.at_stop() {
            self.stops
                .front()
                .map(|s| day_time - s.depart)
                .unwrap_or(self.delay)
        } else {
            vehicle
                .host
                .tt_stops
                .get(vehicle.host.tt_busstop_index.max(0) as usize)
                .map(|s| self.delay.max(day_time - s.1 as f64))
                .unwrap_or(self.delay)
        } as f32;
        vehicle.set_var("schedule_active", vehicle.host.schedule_active);
    }

    pub fn new(stops: Vec<Stop>) -> BusService {
        BusService {
            stops: stops.into(),
            phase: Phase::Running,
            phase_t: 0.0,
            boarding: 0.0,
            held_over: 0.0,
            leave_at: 0.0,
            arrived_at: 0.0,
            boarded: false,
            delay: 0.0,
            layover: false,
            route_open: false,
            terminus: String::new(),
            near_d: f32::INFINITY,
            serve: None,
            last_stop: None,
            always: Vec::new(),
            serve_early: Vec::new(),
            holds: Vec::new(),
            held: None,
            behind: false,
            moving_up: false,
            rest_t: 0.0,
            missed: Vec::new(),
            fault: None,
        }
    }

    /// The berth it holds, if any.
    pub fn held_berth(&self) -> Option<BerthKey> {
        self.held.map(|h| h.0)
    }

    /// The network changed and every berth was given up (`Berths::begin_step`).
    pub fn forget_berth(&mut self) {
        self.held = None;
        self.moving_up = false;
    }

    /// Doors open for people (`AI_Scheduled_AtStation`).
    pub fn at_station(&self) -> bool {
        self.phase == Phase::Boarding
    }

    /// The side's doors the bus opens at the stop it is at (`AI_Scheduled_AtStation_Side`):
    /// the front stop's while it boards, else 0 (nobody at a stop, nothing to open).
    pub fn at_station_side(&self) -> f32 {
        if self.phase == Phase::Boarding {
            self.stops.front().map(|s| s.side).unwrap_or(0.0)
        } else {
            0.0
        }
    }

    pub fn trip_done(&self) -> bool {
        self.phase == Phase::TripDone
    }

    /// Standing at a stop (boarding, waiting or pulling out).
    /// Whether its engine runs (`AI_Engine`): Omsi.exe (0x7d9128) switches a timetable
    /// bus's engine off six seconds after it came to a stop it is to leave more than a
    /// minute later, and on again twenty seconds before it leaves. The buses took their
    /// breaks at the termini with the engines running (#1404).
    pub fn engine_running(&self, day_time: f64) -> bool {
        let standing = matches!(self.phase, Phase::Boarding | Phase::Waiting) && !self.moving_up;
        !(standing
            && self.leave_at - self.arrived_at > ENGINE_OFF_WAIT
            && day_time - self.arrived_at > ENGINE_OFF_AFTER
            && self.leave_at - day_time > ENGINE_ON_BEFORE)
    }

    pub fn at_stop(&self) -> bool {
        matches!(self.phase, Phase::Boarding | Phase::Waiting | Phase::Closing)
    }

    /// Seconds it expects to stand where it is yet (for the traffic behind: worth going
    /// round, or worth waiting for).
    pub fn standing_for(&self, day_time: f64) -> f32 {
        let wait = (self.leave_at - day_time).max(0.0) as f32;
        match self.phase {
            Phase::Running => 0.0,
            Phase::Boarding => self.boarding.max(0.0) + 2.0 + wait,
            Phase::Waiting => wait + 2.0,
            // (doors open that the script never shuts: it stands until somebody sees to it)
            Phase::Closing if self.fault == Some(ServiceFault::DoorsOpenUnanswered) => 600.0,
            Phase::Closing => 1.0,
            Phase::TripDone => 600.0,
        }
    }

    /// Somebody is still at the doors: keep them open for `secs` more - for somebody
    /// coming from stop `stop` only while the bus serves that stop (Omsi.exe 0x7d9f1d: the
    /// person's stop is the bus's), not a stop it stands next to.
    pub fn hold(&mut self, stop: Option<i64>, secs: f32) {
        let here = stop.is_none_or(|s| self.stops.front().is_some_and(|f| f.id == s));
        // (not for ever: somebody who never gets in - waiting at a door the bus does not
        // open, the other side's on a bus with doors on both, or stuck on the way - held it
        // at the stop for good, and the buses behind with it, #1801 #1697 #1544)
        if self.phase == Phase::Boarding && here && self.held_over < HOLD_MAX {
            self.boarding = self.boarding.max(secs);
        }
    }

    /// A new trip (the tour's next, or the rest of a trip).
    pub fn restart(&mut self, stops: Vec<Stop>, layover: bool) {
        self.near_d = f32::INFINITY;
        self.serve = None;
        self.stops = stops.into();
        self.phase = Phase::Running;
        self.phase_t = 0.0;
        self.boarding = 0.0;
        self.held_over = 0.0;
        self.boarded = false;
        self.layover = layover;
        // (the berths it held are given back by `TrafficSim::reroute`)
        self.held = None;
        self.behind = false;
        self.moving_up = false;
        self.rest_t = 0.0;
        self.fault = None;
    }

    fn set_phase(&mut self, p: Phase) {
        self.phase = p;
        self.phase_t = 0.0;
    }

    /// A stop it serves whoever wants it or not: the trip's first (a layover) and last, the
    /// ones its timetable says it always serves (`[profile_otherstopping]` 1 or 4 - the
    /// editor's "stop" setting on its own), and any stop it would reach more than
    /// `EARLY_STOP` early (`EARLY_STOP_SHORT` at a stop marked for it).
    fn must_serve(&self, stop: &Stop, day_time: f64) -> bool {
        let last = (self.stops.len() == 1 && !self.route_open) || self.last_stop == Some(stop.id);
        let early = stop.depart - day_time;
        last || self.layover
            || early > EARLY_STOP
            || self.always.contains(&stop.id)
            || (early > EARLY_STOP_SHORT && self.serve_early.contains(&stop.id))
    }

    /// Whether the bus waits at its front stop for the departure time. True at the trip's
    /// first stop (a layover), at its last, at a station on a railway (a train keeps to its
    /// times), at a stop the timetable wrote a time for (`schedule::TripTimes::holds`), and
    /// at a stop marked to be served when early (`[profile_otherstopping]` 3, `early`
    /// seconds before its departure). Everywhere else - an ordinary on-demand stop, even one
    /// marked 1/4 to be served whoever wants it - the timetable's time is only the running
    /// time shared out by distance: the bus serves the stop and drives on, rather than
    /// standing there until its time and holding up the traffic behind it (a map like London
    /// writes such times for nearly none of its stops, and the buses stood at every one).
    ///
    /// So the two knobs the OMSI timetable editor offers per station decide this between
    /// them: the departure-time box (`Edits_dep`/`CheckBoxes_dep`) and the stop setting
    /// (`ComboBoxes_stopping`). Stopping and waiting are not the same thing.
    fn waits_here(&self, layover: bool, rail: bool, early: f64) -> bool {
        let last = self.stops.len() == 1 && !self.route_open;
        let id = self.stops.front().map(|s| s.id);
        layover
            || rail
            || last
            || id.is_some_and(|id| {
                self.last_stop == Some(id)
                    || self.holds.contains(&id)
                    || (early > EARLY_STOP_SHORT && self.serve_early.contains(&id))
            })
    }

    /// Whether an early bus stands at its front stop until its departure: at every stop, as
    /// Omsi.exe does, unless `timed_only` (the `ai_wait_timed_stops_only` setting) has it wait
    /// only where `waits_here` says so.
    fn holds_for_departure(&self, timed_only: bool, layover: bool, rail: bool, early: f64) -> bool {
        !timed_only || self.waits_here(layover, rail, early)
    }

    fn stop_wait(&self, depart: f64, now: f64, timed_only: bool, layover: bool, rail: bool) -> f64 {
        let timed_stop = self.waits_here(layover, rail, depart - now);
        if !self.holds_for_departure(timed_only, layover, rail, depart - now) {
            return 0.0;
        }
        let scheduled = early_wait(depart, now, layover, rail);
        if timed_stop { scheduled } else { scheduled.min(INTERPOLATED_STOP_WAIT) }
    }

    /// Arrived at the front stop: what now.
    fn arrive(&mut self, ctx: &Ctx, depart: f64, at: (usize, f32)) {
        if omsi_cfg::flags::OMSI_DEBUG_STOPS.is_set() {
            log::info!("t={:.1}: timetable bus {} serves its stop {:?}", ctx.day_time, ctx.id, self.stops.front().map(|s| s.id));
        }
        let layover = std::mem::take(&mut self.layover);
        let rail = ctx.net.lanes.get(at.0).is_some_and(|l| l.kind == LaneKind::Rail);
        // with the `ai_wait_timed_stops_only` setting only a stop the timetable actually puts
        // a time on holds the bus for it; elsewhere its time is the running time shared out,
        // and an early bus serves and drives on. Without it (the default) the bus waits at
        // every stop it serves, as in Omsi.exe
        let wait = self.stop_wait(depart, ctx.day_time, ctx.timed_waits_only, layover, rail);
        self.leave_at = ctx.day_time + wait;
        self.arrived_at = ctx.day_time;
        self.boarding = boarding_time(ctx.id);
        self.held_over = 0.0;
        self.boarded = false;
        self.rest_t = 0.0;
        // a layover opens the doors for the last minute only; anywhere else people get off
        // straight away
        if layover && wait > LAYOVER_BOARDING + 10.0 {
            self.set_phase(Phase::Waiting);
        } else {
            self.set_phase(Phase::Boarding);
            self.boarded = true;
        }
        self.delay = (ctx.day_time + (self.boarding as f64).max(wait)) - depart;
        if ctx.debug {
            log::info!(
                "t={:.1}: timetable bus {} at its stop, {:.0} s to its departure, waiting {:.0} s ({:?}, timed {}) at {:?}",
                ctx.day_time,
                ctx.id,
                depart - ctx.day_time,
                wait,
                self.phase,
                self.waits_here(layover, rail, depart - ctx.day_time),
                ctx.net.lanes.get(at.0).map(|l| { let p = l.at(at.1).0; (p.x.round(), p.y.round()) })
            );
        }
    }

    /// Off from the stop: the next one is the front.
    fn depart(&mut self, st: &mut AiState, vehicle: &mut VehicleInstance, ctx: &Ctx) {
        let bay = self.stops.front().map(|s| s.bay).unwrap_or(0.0);
        if let Some(stop) = self.stops.front() {
            self.delay = ctx.day_time - stop.depart;
        }
        self.stops.pop_front();
        self.near_d = f32::INFINITY;
        self.serve = None;
        self.rest_t = 0.0;
        self.behind = false;
        super::model::ibis_to_next_stop(vehicle, self.stops.len());
        let air = ctx.net.lanes[st.lane].kind == LaneKind::Air;
        // the end of the trip: at its last stop, unless the timetable's track runs on past it
        // (to where the bus stands out its layover) - then at the end of the track, where
        // `Traffic::end_of_way` stops it and ends the trip
        let track_on = route_left(st, ctx.net) > TRACK_ON + st.front;
        if self.stops.is_empty() && !self.route_open && !air && !track_on {
            self.set_phase(Phase::TripDone);
            st.signal = 0;
            st.signal_time = 0.0;
            return;
        }
        self.set_phase(Phase::Running);
        // the blinker stays on while it pulls out
        st.signal = out_signal(bay);
        st.signal_time = 2.5;
        // out of the bay, and back in the lane before the next junction
        let lat = st.lateral;
        if lat.abs() > 0.05 && !ctx.passing {
            let junction = ctx
                .way
                .iter()
                .find(|&&(l, dl)| dl > 0.0 && !ctx.net.crossings[l].is_empty())
                .map(|w| (w.1 - st.front - 1.0).max(4.0));
            let len = (lat.abs() * 8.0).clamp(8.0, 30.0).min(junction.unwrap_or(f32::MAX));
            st.lateral_target = 0.0;
            st.lateral_ramp = (lat, 0.0, st.odometer, len);
        }
        if ctx.debug {
            log::info!(
                "t={:.1}: timetable bus {} pulls away, IBIS stop {:?}",
                ctx.day_time,
                ctx.id,
                vehicle.var("IBIS_busstop")
            );
        }
    }

    /// Whether the doors are shut for pulling out (`Phase::Closing`), as the script tells
    /// it: a script that answers (writes `AI_Scheduled_AtStation` back to 0) after
    /// `CLOSE_MIN`; one that never writes the variable, which has nothing to answer with,
    /// after `CLOSE_MIN` as well. A script that has the variable and never answers is waited for
    /// `CLOSE_MAX`: then, with its doors open, the bus stands (it does not drive off with
    /// people in the doorway), else it leaves - both kept as a fault.
    fn doors_shut(&mut self, vehicle: &VehicleInstance, ctx: &Ctx) -> bool {
        if !script_answers(vehicle) {
            return self.phase_t >= CLOSE_MIN;
        }
        let answered = vehicle.station_released() && vehicle.var("AI_Scheduled_AtStation").map(|v| v.abs() < 0.5).unwrap_or(true);
        if answered {
            return self.phase_t >= CLOSE_MIN;
        }
        if self.phase_t < CLOSE_MAX {
            return false;
        }
        let open = doors_open(vehicle) == Some(true);
        let fault = if open { ServiceFault::DoorsOpenUnanswered } else { ServiceFault::DoorsUnanswered };
        if self.fault != Some(fault) {
            self.fault = Some(fault);
            log::warn!("t={:.1}: timetable bus {}: its script never said the doors are shut ({fault:?})", ctx.day_time, ctx.id);
        }
        !open
    }

    /// One frame of the service: where the bus has to stop (distance from its origin along
    /// its way), if anywhere; it also sets its blinker and its place across the lane.
    /// `berths`: the stops' berths, who holds and who waits for which.
    pub fn step(&mut self, st: &mut AiState, vehicle: &mut VehicleInstance, ctx: &Ctx, berths: &mut Berths) -> Option<f32> {
        self.phase_t += ctx.dt;
        let here = Some(st.front);
        self.keep_berth(st, ctx, berths);
        match self.phase {
            Phase::TripDone => here,
            Phase::Waiting => {
                let up = self.move_up(st, ctx, berths);
                let board_at = self.leave_at - LAYOVER_BOARDING;
                if up.is_some() {
                    // (doors shut while it moves)
                } else if !self.boarded && ctx.day_time >= board_at {
                    self.boarded = true;
                    self.boarding = self.boarding.max((self.leave_at - ctx.day_time) as f32 - 3.0);
                    self.set_phase(Phase::Boarding);
                } else if self.boarded && ctx.day_time >= self.leave_at {
                    self.set_phase(Phase::Closing);
                }
                up.or(here)
            }
            Phase::Boarding => {
                self.boarding -= ctx.dt;
                if self.boarding > 0.0 && ctx.day_time >= self.leave_at {
                    self.held_over += ctx.dt;
                }
                // an early bus waits for its departure with the doors open, as drivers do
                // (shut, it stood at the stop for half a minute for no reason anyone could
                // see); the doors close when it is time to go
                if self.boarding <= 0.0 && ctx.day_time >= self.leave_at {
                    self.set_phase(Phase::Closing);
                }
                here
            }
            Phase::Closing => {
                // blinker on towards the traffic while the doors close and the stop brake
                // comes off. The bus leaves when its script says the doors are shut (it
                // writes AI_Scheduled_AtStation back to 0), as OMSI's buses do; leaving
                // after a fixed 3.5 s drove buses off with their doors still closing, and
                // each type at its own moment (`doors_shut`). Out of a bay it waits for a
                // gap: not in front of something coming up close behind, nor into
                // something standing just ahead.
                let bay = self.stops.front().map(|s| s.bay).unwrap_or(0.0);
                st.signal = out_signal(bay);
                st.signal_time = st.signal_time.max(1.0);
                let in_bay = st.lateral.abs() > ALIGNED;
                if self.doors_shut(vehicle, ctx) && !(in_bay && ctx.berth.pull_out_blocked) {
                    self.depart(st, vehicle, ctx);
                    if self.phase == Phase::TripDone {
                        return here;
                    }
                    return None;
                }
                here
            }
            Phase::Running => self.approach(st, vehicle, ctx, berths),
        }
    }

    /// The berth it holds: the holder tells how long it will stand there yet, and gives it
    /// back once its rear has left it after pulling out. At its stop a bus that lost the
    /// berth with a change of the network takes it again (it stands in it).
    fn keep_berth(&mut self, st: &AiState, ctx: &Ctx, berths: &mut Berths) {
        if let Some((key, ri, s)) = self.held {
            if self.at_stop() {
                berths.note_standing(key, ctx.id, self.standing_for(ctx.day_time));
            } else if self.phase == Phase::Running && past_by(st, ctx.net, ri, s) > super::berths::CLEAR_PAST {
                berths.release(key, ctx.id);
                self.held = None;
            }
        } else if self.at_stop() && !self.behind && ctx.berth.applies {
            if let Some(stop) = self.stops.front() {
                let key = BerthKey { stop: stop.id, lane: st.route.get(stop.ri).copied().unwrap_or(st.lane) };
                berths.ask(key, ctx.id, true, 0.0);
                if berths.grant(key, ctx.id, ctx.berth.free) {
                    self.held = Some((key, stop.ri, stop.s));
                }
            }
        }
    }

    /// A bus that stands out its layover behind the berth (it served the stop from there)
    /// moves up onto the berth once that is free, doors shut: its tail held the lanes
    /// behind it the whole layover otherwise. Where to stop, while it moves.
    fn move_up(&mut self, st: &mut AiState, ctx: &Ctx, berths: &mut Berths) -> Option<f32> {
        self.moving_up = false;
        if !self.behind || !ctx.berth.applies || self.boarded {
            return None;
        }
        let stop = *self.stops.front()?;
        let key = BerthKey { stop: stop.id, lane: st.route.get(stop.ri).copied().unwrap_or(st.lane) };
        let d = st.route_distance(ctx.net, stop.ri, stop.s);
        berths.ask(key, ctx.id, ctx.berth.there, d);
        if !berths.grant(key, ctx.id, ctx.berth.free) {
            return None;
        }
        self.held = Some((key, stop.ri, stop.s));
        if !ctx.passing && d < BAY_REACH {
            st.lateral_target = stop.bay;
        }
        self.rest_t = if st.speed < DOCK_SPEED { self.rest_t + ctx.dt } else { 0.0 };
        let lat_err = (st.lateral - st.lateral_target).abs();
        let docked = d.abs() < DOCK_LONG && lat_err < DOCK_LAT && st.speed < DOCK_SPEED;
        if docked || (d.abs() < SERVE_HERE_LONG && lat_err < SERVE_HERE_LAT && self.rest_t >= SERVE_HERE_AFTER) {
            self.behind = false;
            self.rest_t = 0.0;
            return None;
        }
        self.moving_up = true;
        Some(d.max(0.0) + st.front - 0.3)
    }

    /// The front stop is missed (`why`): no doors there, the berth given back, on to the next.
    fn miss(&mut self, vehicle: &mut VehicleInstance, ctx: &Ctx, berths: &mut Berths, key: Option<BerthKey>, why: &str) {
        if let Some(stop) = self.stops.pop_front() {
            if ctx.debug || omsi_cfg::flags::OMSI_DEBUG_STOPS.is_set() {
                log::info!("t={:.1}: timetable bus {} missed its stop {} ({why})", ctx.day_time, ctx.id, stop.id);
            }
            self.missed.push(stop.id);
        }
        if let Some(k) = key {
            berths.release(k, ctx.id);
            if self.held.is_some_and(|h| h.0 == k) {
                self.held = None;
            }
        }
        self.near_d = f32::INFINITY;
        self.serve = None;
        self.rest_t = 0.0;
        super::model::ibis_to_next_stop(vehicle, self.stops.len());
    }

    /// Driving: brake for the next stop, pull into its bay.
    fn approach(&mut self, st: &mut AiState, vehicle: &mut VehicleInstance, ctx: &Ctx, berths: &mut Berths) -> Option<f32> {
        loop {
            let Some(stop) = self.stops.front().copied() else {
                if !ctx.passing {
                    st.lateral_target = ctx.kerb_swerve.unwrap_or(0.0);
                }
                return None;
            };
            let speed = st.speed;
            // a road stop has a berth; trains and aircraft keep to their stations
            let key = ctx.berth.applies.then(|| BerthKey { stop: stop.id, lane: st.route.get(stop.ri).copied().unwrap_or(st.lane) });
            // A bus creeping the last metres to its stop point is at its stop, even when the
            // point lies past the end of the stop's lane and the lane ahead takes over first:
            // measured from the new lane the stop was suddenly 20-30 m behind, and the bus -
            // blinker on, pulled into the bay - drove on without opening its doors.
            let crept_past = self.near_d < 12.0 && speed < 4.0;
            let holds = key.is_none_or(|k| berths.holder(k) == Some(ctx.id));
            if stop.ri < st.route_index {
                if crept_past && holds {
                    self.near_d = f32::INFINITY;
                    self.arrive(ctx, stop.depart, (st.lane, st.s));
                    return Some(st.front);
                }
                // behind it already (the route was cut short)
                if key.is_some() {
                    self.miss(vehicle, ctx, berths, key, "behind it");
                } else {
                    self.stops.pop_front();
                    self.near_d = f32::INFINITY;
                    self.serve = None;
                }
                continue;
            }
            let d = st.route_distance(ctx.net, stop.ri, stop.s);
            // near enough to see whether anybody wants it (a train keeps to its stations)
            if self.serve.is_none() && d < SKIP_DECIDE {
                let rail = ctx.net.lanes.get(st.lane).is_some_and(|l| l.kind == LaneKind::Rail);
                self.serve = Some(rail || self.must_serve(&stop, ctx.day_time) || ctx.wanted.unwrap_or(true));
            }
            if self.serve == Some(false) {
                if ctx.debug || omsi_cfg::flags::OMSI_DEBUG_STOPS.is_set() {
                    log::info!("t={:.1}: timetable bus {} passes its stop {}: nobody gets off or on", ctx.day_time, ctx.id, stop.id);
                }
                if let Some(k) = key {
                    berths.withdraw(k, ctx.id);
                }
                self.stops.pop_front();
                self.near_d = f32::INFINITY;
                self.serve = None;
                super::model::ibis_to_next_stop(vehicle, self.stops.len());
                if !ctx.passing {
                    st.lateral_target = ctx.kerb_swerve.unwrap_or(0.0);
                }
                continue;
            }
            // into the bay over the last metres, but only once no junction lies between
            // the bus and its stop: the meeting places of a junction are laid out for
            // vehicles in the middle of their lane. Not where the stop lies too close
            // behind the junction for the S-curve into the bay (`AiState::lateral`):
            // the bus came to rest in the middle of its lane, too far from the pole for
            // anybody to get on, and stood there for good (DBC_Map, Grand and Peshtigo)
            if !ctx.passing {
                let junction_end = ctx
                    .way
                    .iter()
                    .filter(|&&(l, dl)| dl < d && !ctx.net.crossings[l].is_empty())
                    .map(|&(l, dl)| dl + ctx.net.lanes[l].length())
                    .reduce(f32::max);
                let ramp = ((stop.bay - st.lateral).abs() * 8.0).clamp(8.0, 30.0);
                let junction_first = junction_end.is_some_and(|e| d - e >= ramp);
                st.lateral_target = if d < BAY_REACH && d > -25.0 && !junction_first {
                    stop.bay
                } else {
                    ctx.kerb_swerve.unwrap_or(0.0)
                };
            }
            let Some(key) = key else {
                // a station of a train or an aircraft: queued behind something standing at
                // it (another train), after a while it serves it where it stands
                let queued = speed < 0.3 && ctx.stopped > 6.0 && (2.0..45.0).contains(&d);
                if (d < 2.0 && speed < 0.3) || queued || (d < -2.0 && crept_past) {
                    self.near_d = f32::INFINITY;
                    self.arrive(ctx, stop.depart, (st.lane, st.s));
                    return Some(st.front);
                }
                self.near_d = d;
                if d < -2.0 {
                    if ctx.debug || omsi_cfg::flags::OMSI_DEBUG_STOPS.is_set() {
                        log::info!("t={:.1}: timetable bus {} missed its stop ({:.1} m past, {:.1} m/s, stood {:.1} s, passing {})", ctx.day_time, ctx.id, -d, speed, ctx.stopped, ctx.passing);
                    }
                    self.stops.pop_front();
                    self.near_d = f32::INFINITY;
                    self.serve = None;
                    continue;
                }
                return (d < STOP_REACH).then(|| d.max(0.0) + st.front - 0.3);
            };
            self.rest_t = if speed < DOCK_SPEED { self.rest_t + ctx.dt } else { 0.0 };
            if self.serve.is_none() {
                // not settled yet whether it stops there: braking for it all the same
                self.near_d = d;
                return (d < STOP_REACH).then(|| d.max(0.0) + st.front - 0.3);
            }
            berths.ask(key, ctx.id, ctx.berth.there, d);
            if !berths.grant(key, ctx.id, ctx.berth.free) {
                if let Some(at) = self.wait_for_berth(st, ctx, berths, key, &stop, d) {
                    return at;
                }
                self.miss(vehicle, ctx, berths, Some(key), "past it while waiting for the berth");
                continue;
            }
            // the berth is its own: dock
            self.held = Some((key, stop.ri, stop.s));
            let lat_err = (st.lateral - st.lateral_target).abs();
            let docked = d.abs() < DOCK_LONG && lat_err < DOCK_LAT && speed < DOCK_SPEED;
            let near = d.abs() < SERVE_HERE_LONG && lat_err < SERVE_HERE_LAT && self.rest_t >= SERVE_HERE_AFTER;
            // (the stop point measured from the next lane all at once metres behind: see
            // `crept_past`; one it crept past by itself is overshot)
            let jumped = crept_past && d < -2.0 && self.near_d - d > 5.0;
            if docked || near || jumped {
                self.near_d = f32::INFINITY;
                self.arrive(ctx, stop.depart, (st.lane, st.s));
                return Some(st.front);
            }
            if d < -SERVE_HERE_LONG {
                self.miss(vehicle, ctx, berths, Some(key), "overshot");
                continue;
            }
            if self.rest_t >= UNREACHABLE_AFTER && d.abs() < SERVE_HERE_LONG {
                self.miss(vehicle, ctx, berths, Some(key), "cannot get to it");
                continue;
            }
            // something came to stand in the berth after it was handed it (a queue at the
            // lights beyond the stop, a car pulled in): the first bus serves the stop where it
            // stands, short of its point, once it has waited for long - nobody waits ahead
            if self.rest_t >= super::berths::BLOCKED_LONG && !ctx.berth.free && (0.0..UNREACHABLE_NEAR).contains(&d) {
                self.near_d = f32::INFINITY;
                self.arrive(ctx, stop.depart, (st.lane, st.s));
                return Some(st.front);
            }
            self.near_d = d;
            return (d < STOP_REACH).then(|| d.max(0.0) + st.front - 0.3);
        }
    }

    /// Waiting for the berth of its front stop (`d` m ahead): where it stops, `QUEUE_STANDOFF`
    /// before a bus docked there - or Some(Some(front)) when it serves the stop from where it
    /// stands because the berth is taken for long (`berths::HELD_LONG`). None: it has got
    /// past the stop, which it has missed.
    fn wait_for_berth(&mut self, st: &AiState, ctx: &Ctx, berths: &mut Berths, key: BerthKey, stop: &Stop, d: f32) -> Option<Option<f32>> {
        berths.note_blocked(key, !ctx.berth.free, ctx.time);
        self.near_d = d;
        if d < -SERVE_HERE_LONG {
            return None;
        }
        if self.rest_t >= SERVE_HERE_AFTER && (0.0..SERVE_BEHIND_REACH).contains(&d) && berths.held_long(key, ctx.time) {
            if ctx.debug || omsi_cfg::flags::OMSI_DEBUG_STOPS.is_set() {
                log::info!("t={:.1}: timetable bus {} serves its stop {} from {d:.0} m behind: the berth is taken for long", ctx.day_time, ctx.id, stop.id);
            }
            berths.withdraw(key, ctx.id);
            self.behind = true;
            self.near_d = f32::INFINITY;
            self.arrive(ctx, stop.depart, (st.lane, st.s));
            return Some(Some(st.front));
        }
        Some((d < STOP_REACH).then(|| (d - st.rear - QUEUE_STANDOFF).max(st.front)))
    }
}

/// How far before a station's point a vehicle stops with its origin, as Omsi.exe measures
/// the way to the station (0x7da4e3): from the origin of the vehicle that leads, less half
/// its length for a train (its front comes to rest at the station, not its middle - the
/// S-Bahn stopped with half a car past the end of the platform), plus the holding point
/// offset of its `[ai_brakeperformance]` ("to correct unprecise braking").
pub fn stop_shift(ty: &crate::VehicleType, rail: bool) -> f32 {
    let hold = ty.def.ai_brake_performance.map(|b| b[4]).unwrap_or(0.0);
    let half = if rail { ty.half_length().unwrap_or(0.0) } else { 0.0 };
    half - hold
}

#[cfg(test)]
mod tests {
    /// A bus with a long wait at its stop switches its engine off after six seconds and on
    /// twenty seconds before it leaves; a short stop keeps it running (#1404).
    #[test]
    fn the_engine_rests_through_a_long_wait() {
        let mut b = super::BusService::new(Vec::new());
        b.phase = super::Phase::Waiting;
        b.arrived_at = 1000.0;
        b.leave_at = 1300.0;
        assert!(b.engine_running(1003.0), "just arrived");
        assert!(!b.engine_running(1010.0), "resting");
        assert!(b.engine_running(1285.0), "about to leave");
        b.leave_at = 1040.0;
        assert!(b.engine_running(1010.0), "a short stop");
        b.leave_at = 1300.0;
        b.phase = super::Phase::Running;
        assert!(b.engine_running(1010.0), "driving");
    }

    use super::*;

    /// Somebody coming from another stop than the one the bus serves does not keep it there
    /// (#767); somebody on the way out does, wherever.
    #[test]
    fn a_hold_counts_at_the_stop_served() {
        let stop = Stop { ri: 0, s: 0.0, bay: 0.0, depart: 0.0, id: 42, side: 0.0 };
        let mut s = BusService::new(vec![stop]);
        s.phase = Phase::Boarding;
        s.boarding = 0.0;
        s.hold(Some(41), 2.5);
        assert_eq!(s.boarding, 0.0);
        s.hold(Some(42), 2.5);
        assert_eq!(s.boarding, 2.5);
        s.boarding = 0.0;
        s.hold(None, 2.5);
        assert_eq!(s.boarding, 2.5);
    }

    #[test]
    fn an_early_bus_waits_for_its_time() {
        // five minutes early: until 20 s before its departure (a train: 2 min)
        assert_eq!(early_wait(400.0, 100.0, false, false), 280.0);
        assert_eq!(early_wait(400.0, 100.0, false, true), 180.0);
        // late, or nearly on time: off at once
        assert_eq!(early_wait(400.0, 390.0, false, false), 0.0);
        assert_eq!(early_wait(400.0, 500.0, false, false), 0.0);
        // a layover: to the departure itself
        assert_eq!(early_wait(400.0, 100.0, true, false), 300.0);
    }

    #[test]
    fn standing_time() {
        let mut s = BusService::new(vec![]);
        assert_eq!(s.standing_for(0.0), 0.0);
        s.phase = Phase::Waiting;
        s.leave_at = 100.0;
        assert!((s.standing_for(40.0) - 62.0).abs() < 1e-3);
        s.phase = Phase::TripDone;
        assert!(s.standing_for(0.0) > 100.0);
    }

    #[test]
    fn only_the_ends_of_the_trip_and_an_early_bus_stop_for_nobody() {
        let stop = |id: i64, depart: f64| Stop::from_tuple((0, 0.0, 0.0, depart, id, 0.0));
        let mut s = BusService::new(vec![stop(1, 100.0), stop(2, 200.0), stop(3, 300.0)]);
        s.last_stop = Some(3);
        // on time at a stop in the middle: only if somebody wants it
        assert!(!s.must_serve(&stop(2, 200.0), 150.0));
        // over two minutes early: it stops and waits
        assert!(s.must_serve(&stop(2, 200.0), 70.0));
        // the trip's terminus, and the last stop it knows of with the route complete
        assert!(s.must_serve(&stop(3, 300.0), 300.0));
        s.stops = vec![stop(2, 200.0)].into();
        assert!(s.must_serve(&stop(2, 200.0), 200.0));
        s.route_open = true;
        assert!(!s.must_serve(&stop(2, 200.0), 200.0));
        // its first stop, where it stands out its layover
        s.layover = true;
        assert!(s.must_serve(&stop(2, 200.0), 200.0));
        s.layover = false;
        // `[profile_otherstopping]` 1/4 (always) and 3 (when early)
        s.always = vec![2];
        assert!(s.must_serve(&stop(2, 200.0), 200.0));
        s.always.clear();
        s.serve_early = vec![2];
        assert!(!s.must_serve(&stop(2, 200.0), 190.0));
        assert!(s.must_serve(&stop(2, 200.0), 170.0));
    }

    #[test]
    fn only_a_stop_the_timetable_times_holds_the_bus() {
        let stop = |id: i64, depart: f64| Stop::from_tuple((0, 0.0, 0.0, depart, id, 0.0));
        // an ordinary stop whose time is only the running time shared out: served and left
        let mut s = BusService::new(vec![stop(2, 200.0), stop(3, 300.0)]);
        s.last_stop = Some(3);
        assert!(!s.waits_here(false, false, 100.0));
        // a stop the map gave a time of its own: the bus waits for it
        s.holds = vec![2];
        assert!(s.waits_here(false, false, 100.0));
        // a stop marked to be served when early (kind 3) also holds the bus when it is early
        s.holds.clear();
        s.serve_early = vec![2];
        assert!(!s.waits_here(false, false, 5.0));
        assert!(s.waits_here(false, false, 100.0));
        s.serve_early.clear();
        // a stop the timetable only says it stops at in any case (kind 1/4) is served, not
        // waited at: stopping and waiting are different settings of the editor
        s.always = vec![2];
        assert!(!s.waits_here(false, false, 100.0));
        s.always.clear();
        // the trip's last stop always waits, and so do a layover and a railway station
        s.last_stop = Some(2);
        assert!(s.waits_here(false, false, 100.0));
        assert!(s.waits_here(true, false, 100.0));
        assert!(s.waits_here(false, true, 100.0));
    }

    /// The `ai_wait_timed_stops_only` setting off (the default): an early bus waits for its
    /// departure at every stop it serves, as Omsi.exe does, timed by the map or not.
    #[test]
    fn without_the_setting_an_early_bus_waits_at_every_stop_as_in_omsi() {
        let stop = |id: i64, depart: f64| Stop::from_tuple((0, 0.0, 0.0, depart, id, 0.0));
        let mut s = BusService::new(vec![stop(2, 200.0), stop(3, 300.0)]);
        s.last_stop = Some(3);
        assert!(s.holds_for_departure(false, false, false, 100.0));
        s.always = vec![2];
        assert!(s.holds_for_departure(false, false, false, 100.0));
        // and with it only where the timetable times the stop
        assert!(!s.holds_for_departure(true, false, false, 100.0));
        s.holds = vec![2];
        assert!(s.holds_for_departure(true, false, false, 100.0));
        // the wait itself is OMSI's: until 20 s before the departure
        assert_eq!(early_wait(200.0, 100.0, false, false), 80.0);
    }

    #[test]
    fn interpolated_intermediate_stop_cannot_hold_a_bus_for_many_minutes() {
        let stop = |id: i64| Stop::from_tuple((0, 0.0, 0.0, 1200.0, id, 0.0));
        let mut service = BusService::new(vec![stop(2), stop(3)]);
        service.last_stop = Some(3);
        assert!(!service.waits_here(false, false, 1200.0));
        assert_eq!(service.stop_wait(1200.0, 0.0, false, false, false), 120.0);
        assert_eq!(service.stop_wait(1200.0, 0.0, true, false, false), 0.0);
        service.holds.push(2);
        assert!(service.waits_here(false, false, 1200.0));
        assert_eq!(service.stop_wait(1200.0, 0.0, false, false, false), 1180.0);
        service.holds.clear();
        service.stops.pop_front();
        assert!(service.waits_here(false, false, 1200.0), "the terminus still keeps its layover");
        assert_eq!(service.stop_wait(1200.0, 0.0, false, false, false), 1180.0);
    }

    #[test]
    fn station_side_comes_from_the_stop_it_boards_at() {
        let stop = |side: f32| Stop::from_tuple((0, 0.0, 0.0, 0.0, 1, side));
        let mut s = BusService::new(vec![stop(1.0)]);
        // off a stop: nothing to open
        s.phase = Phase::Running;
        assert_eq!(s.at_station_side(), 0.0);
        // boarding: the front stop's side
        s.phase = Phase::Boarding;
        assert_eq!(s.at_station_side(), 1.0);
        // waiting to pull out (doors shut): the side is not asked for any more
        s.phase = Phase::Waiting;
        assert_eq!(s.at_station_side(), 0.0);
        // an empty queue answers 0, not a panic
        s.phase = Phase::Boarding;
        s.stops.clear();
        assert_eq!(s.at_station_side(), 0.0);
    }

    fn service_context(net: &Network, day_time: f64, dt: f32) -> Ctx<'_> {
        Ctx { net, way: &[], day_time, dt, id: 1, stopped: 0.0, passing: false,
            kerb_swerve: None, wanted: Some(false), debug: false, timed_waits_only: false,
            time: day_time as f32, berth: BerthScene::default() }
    }

    fn service_network() -> Network {
        Network { lanes: vec![crate::traffic::LaneBuilder::arc(
            glam::DVec3::ZERO, 0.0, 100.0, 0.0, 0.0, LaneKind::Street, 3.0)],
            ..Default::default() }
    }

    fn service_vehicle() -> VehicleInstance {
        crate::timetable_run::tests::script_test_vehicle("{frame_ai}\n{end}\n", "schedule_active\n", "")
    }

    #[test]
    fn three_stop_service_closes_each_stop_and_completes_only_at_the_end() {
        let net = service_network();
        let mut vehicle = service_vehicle();
        let mut state = AiState::new(0, 0.0, 1);
        let stops: Vec<_> = (1..=3).map(|id| Stop::from_tuple((0, 10.0, 0.0, id as f64 * 100.0, id, 0.0))).collect();
        let mut service = BusService::new(stops);
        for visit in 1..=3 {
            let depart = visit as f64 * 100.0;
            service.arrive(&service_context(&net, depart, 0.0), depart, (0, 10.0));
            assert_eq!(service.phase, Phase::Boarding);
            service.step(&mut state, &mut vehicle, &service_context(&net, depart + 8.0, 8.0), &mut Berths::default());
            assert_eq!(service.phase, Phase::Closing);
            service.step(&mut state, &mut vehicle, &service_context(&net, depart + 9.5, CLOSE_MIN), &mut Berths::default());
            assert_eq!(service.stops.len(), 3 - visit as usize);
            assert_eq!(service.trip_done(), visit == 3);
            if visit != 3 {
                assert_eq!(service.phase, Phase::Running);
            }
        }
        assert_eq!(state.signal, 0);
        // A next trip must not inherit a previous passenger hold or closing phase.
        service.boarding = 999.0;
        service.restart(vec![Stop::from_tuple((0, 10.0, 0.0, 400.0, 1, 1.0))], true);
        assert_eq!(service.phase, Phase::Running);
        assert_eq!(service.boarding, 0.0);
        assert_eq!(service.phase_t, 0.0);
        assert!(service.layover && !service.boarded);
    }

    #[test]
    fn early_late_and_layover_boundaries_keep_the_omsi_departure_rule() {
        assert_eq!(early_wait(400.0, 379.0, false, false), 1.0);
        assert_eq!(early_wait(400.0, 380.0, false, false), 0.0);
        assert_eq!(early_wait(400.0, 381.0, false, false), 0.0);
        assert_eq!(early_wait(400.0, 401.0, false, false), 0.0);
        assert_eq!(early_wait(400.0, 380.0, true, false), 20.0);
        let net = service_network();
        let mut vehicle = service_vehicle();
        let mut state = AiState::new(0, 0.0, 1);
        let mut service = BusService::new(vec![Stop::from_tuple((0, 10.0, 0.0, 400.0, 1, 0.0))]);
        service.layover = true;
        service.arrive(&service_context(&net, 100.0, 0.0), 400.0, (0, 10.0));
        assert_eq!(service.phase, Phase::Waiting);
        assert!(!service.at_station());
        service.step(&mut state, &mut vehicle, &service_context(&net, 354.0, 254.0), &mut Berths::default());
        assert_eq!(service.phase, Phase::Waiting);
        service.step(&mut state, &mut vehicle, &service_context(&net, 355.0, 1.0), &mut Berths::default());
        assert_eq!(service.phase, Phase::Boarding);
        service.step(&mut state, &mut vehicle, &service_context(&net, 396.0, 41.0), &mut Berths::default());
        assert_eq!(service.phase, Phase::Boarding);
        service.step(&mut state, &mut vehicle, &service_context(&net, 400.0, 4.0), &mut Berths::default());
        assert_eq!(service.phase, Phase::Closing);
    }

    /// Somebody at a door the bus never opens holds it a minute past its departure at
    /// most, then it leaves (#1801: buses stood at their stops for good).
    #[test]
    fn people_at_the_doors_hold_a_bus_a_minute_past_its_time_at_most() {
        let net = service_network();
        let mut vehicle = service_vehicle();
        let mut state = AiState::new(0, 0.0, 1);
        let mut service = BusService::new(vec![Stop::from_tuple((0, 10.0, 0.0, 100.0, 1, 0.0))]);
        service.arrive(&service_context(&net, 100.0, 0.0), 100.0, (0, 10.0));
        let mut t = 100.0;
        while t < 400.0 && service.phase == Phase::Boarding {
            service.hold(Some(1), 2.5);
            t += 0.5;
            service.step(&mut state, &mut vehicle, &service_context(&net, t, 0.5), &mut Berths::default());
        }
        assert_ne!(service.phase, Phase::Boarding, "still boarding at {t}");
        assert!(t < 100.0 + 90.0, "held until {t}");
    }

    #[test]
    fn streamed_route_tail_is_not_trip_completion_and_platform_side_survives_restart() {
        let net = service_network();
        let mut vehicle = service_vehicle();
        let mut state = AiState::new(0, 0.0, 1);
        for side in [0.0, 1.0, 2.0] {
            let mut service = BusService::new(vec![Stop::from_tuple((0, 10.0, 0.0, 100.0, 42, side))]);
            service.route_open = true;
            service.phase = Phase::Boarding;
            assert_eq!(service.at_station_side(), side);
            service.boarding = 1.0;
            service.hold(Some(999), 10.0);
            assert_eq!(service.boarding, 1.0);
            service.hold(Some(42), 10.0);
            assert_eq!(service.boarding, 10.0);
            service.depart(&mut state, &mut vehicle, &service_context(&net, 110.0, 0.0));
            assert_eq!(service.phase, Phase::Running);
            assert!(!service.trip_done());
            // Geometry returns later: preserve the new visit's side and departure.
            service.restart(vec![Stop::from_tuple((0, 10.0, 0.0, 200.0, 42, side))], false);
            service.arrive(&service_context(&net, 200.0, 0.0), 200.0, (0, 10.0));
            assert_eq!(service.at_station_side(), side);
            assert_eq!(service.stops.front().unwrap().depart, 200.0);
        }
    }
    /// The doors closing at a stop: how long until the bus pulls out (None: still there
    /// after a minute), and the fault it notes.
    fn close_doors(vehicle: &mut VehicleInstance, at_station: f32, door: Option<f32>) -> (Option<f32>, Option<ServiceFault>) {
        let net = service_network();
        let mut state = AiState::new(0, 0.0, 1);
        let stop = |id: i64| Stop::from_tuple((0, 10.0 * id as f32, 0.0, 100.0, id, 0.0));
        let mut service = BusService::new(vec![stop(1), stop(2)]);
        service.set_phase(Phase::Closing);
        vehicle.set_var("AI_Scheduled_AtStation", at_station);
        if let Some(d) = door {
            vehicle.set_var("door_0", d);
        }
        let mut t = 0.0;
        while t < 60.0 {
            t += 0.1;
            service.step(&mut state, vehicle, &service_context(&net, 100.0 + t as f64, 0.1), &mut Berths::default());
            if service.phase != Phase::Closing {
                return (Some(t), service.fault);
            }
        }
        (None, service.fault)
    }

    #[test]
    fn the_bus_pulls_out_when_its_script_says_the_doors_are_shut() {
        let answering = "{frame_ai}\n0 (S.L.AI_Scheduled_AtStation)\n{end}\n";
        let vars = "AI_Scheduled_AtStation\ndoor_0\n";
        // the script has answered: after the short close
        let mut v = crate::timetable_run::tests::script_test_vehicle(answering, vars, "");
        let (left, fault) = close_doors(&mut v, 0.0, Some(0.0));
        assert!(left.is_some_and(|t| (CLOSE_MIN..CLOSE_MIN + 0.3).contains(&t)), "{left:?}");
        assert_eq!(fault, None);
        // it has not, and its doors stand open: the bus does not drive off
        let mut v = crate::timetable_run::tests::script_test_vehicle(answering, vars, "");
        let (left, fault) = close_doors(&mut v, -1.0, Some(1.0));
        assert_eq!(left, None);
        assert_eq!(fault, Some(ServiceFault::DoorsOpenUnanswered));
        // it has not, and nobody knows about the doors: off after the longest wait
        let mut v = crate::timetable_run::tests::script_test_vehicle(answering, "AI_Scheduled_AtStation\n", "");
        let (left, fault) = close_doors(&mut v, -1.0, None);
        assert!(left.is_some_and(|t| (CLOSE_MAX..CLOSE_MAX + 0.3).contains(&t)), "{left:?}");
        assert_eq!(fault, Some(ServiceFault::DoorsUnanswered));
        // a script that never answers at all has nothing to wait for: the short close
        let mut v = crate::timetable_run::tests::script_test_vehicle("{frame_ai}\n{end}\n", "AI_Scheduled_AtStation\n", "");
        let (left, fault) = close_doors(&mut v, -1.0, None);
        assert!(left.is_some_and(|t| (CLOSE_MIN..CLOSE_MIN + 0.3).contains(&t)), "{left:?}");
        assert_eq!(fault, None);
    }

    /// A trip whose track runs on past its last stop ends at the end of the track: the bus
    /// drives on from the stop instead of ending its trip there.
    #[test]
    fn the_trip_ends_where_the_track_ends() {
        let net = service_network();
        let mut vehicle = service_vehicle();
        for (route_on, done) in [(false, true), (true, false)] {
            let mut state = AiState::new(0, 10.0, 1);
            if route_on {
                state.route = vec![0];
            }
            let mut service = BusService::new(vec![Stop::from_tuple((0, 10.0, 0.0, 100.0, 1, 0.0))]);
            service.set_phase(Phase::Closing);
            service.phase_t = CLOSE_MIN;
            service.step(&mut state, &mut vehicle, &service_context(&net, 100.0, 0.1), &mut Berths::default());
            assert_eq!(service.trip_done(), done, "the track runs on: {route_on}");
            assert!(service.stops.is_empty());
        }
    }
    /// A route that moves over to the lane beside at its stop (a stop lane of a junction
    /// object): the bus on the lane beside is as far past the stop as it is along, not a
    /// whole lane further - it keeps its berth until its rear has left it.
    #[test]
    fn the_way_past_a_stop_counts_a_lane_beside_as_beside() {
        let key = |path: u16| Some(crate::traffic::LaneKey { tile: (0, 0), id: 9, path });
        let mut a = crate::traffic::LaneBuilder::arc(glam::DVec3::ZERO, 0.0, 100.0, 0.0, 0.0, LaneKind::Street, 3.0);
        let mut b = crate::traffic::LaneBuilder::arc(glam::DVec3::new(3.5, 0.0, 0.0), 0.0, 100.0, 0.0, 0.0, LaneKind::Street, 3.0);
        a.key = key(0);
        b.key = key(1);
        let net = Network { lanes: vec![a, b], ..Default::default() };
        assert!(net.parallel(0, 1));
        let mut st = AiState::new(1, 21.5, 1);
        st.route = vec![0, 1];
        st.route_index = 1;
        let past = past_by(&st, &net, 0, 21.4);
        assert!((past - (0.1 - st.rear)).abs() < 1e-3, "{past}");
        st.s = 21.4 + st.rear + 2.0;
        assert!(past_by(&st, &net, 0, 21.4) > super::super::berths::CLEAR_PAST);
    }
}
