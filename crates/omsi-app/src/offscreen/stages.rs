//! The offscreen run's step as a list of stages (`OFFSCREEN_STEP`, 1/30 s each), taken by
//! the same runner as the window's frame (see `frame_stages`).

use super::*;
use crate::frame_stages::{self, Flow, Group, Phase, Profile, Role, StageInfo};

/// The offscreen run's stages, in the order of [`OFFSCREEN_STEP`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OffscreenStage {
    Server,
    Clock,
    Traffic,
    Startup,
    Duty,
    Drive,
    GroundGap,
    Humans,
    Lan,
    Surroundings,
    Snapshot,
    Record,
}

/// One step of the offscreen run: the server's work, the traffic, the player's bus, the
/// people, the LAN, the spray and the snapshots due.
pub(crate) const OFFSCREEN_STEP: &[OffscreenStage] = {
    use OffscreenStage::*;
    &[Server, Clock, Traffic, Startup, Duty, Drive, GroundGap, Humans, Lan, Surroundings, Snapshot, Record]
};

const fn stage(name: &'static str, phase: Phase, role: Role, reads: &'static [Group], writes: &'static [Group]) -> StageInfo {
    StageInfo { name, phase, role, reads, writes, takes: &[], gives: &[], profile: Profile::Key(name) }
}

impl frame_stages::Stage for OffscreenStage {
    fn info(self) -> StageInfo {
        use Group as G;
        use OffscreenStage::*;
        use Phase as P;
        match self {
            Server => stage("server", P::Housekeeping, Role::Own, &[G::Net, G::Clock, G::Traffic, G::People], &[G::Net, G::Clock, G::Traffic, G::Gfx]),
            Clock => stage("clock", P::Sim, Role::Clock, &[G::Clock, G::Weather, G::World], &[G::Clock, G::Player]),
            Traffic => stage("traffic", P::Sim, Role::Traffic, &[G::Traffic, G::Player, G::People, G::World, G::View, G::Clock, G::Weather, G::Net], &[G::Traffic, G::Player, G::Gfx, G::Perf]),
            Startup => stage("startup", P::Sim, Role::Own, &[G::Player], &[G::Player]),
            Duty => stage("duty", P::Sim, Role::Duty, &[G::Duty, G::Player, G::People, G::Clock, G::World], &[G::Duty, G::Player, G::Perf]),
            Drive => stage("drive", P::Sim, Role::Player, &[G::Player, G::Input, G::World, G::Traffic], &[G::Player, G::Traffic, G::Gfx, G::Perf]),
            GroundGap => stage("ground_gap", P::Sim, Role::Own, &[G::World, G::Player, G::Traffic, G::Clock], &[G::Perf]),
            Humans => stage("humans", P::Sim, Role::People, &[G::People, G::Player, G::Traffic, G::Duty, G::Clock, G::World, G::View, G::Net], &[G::People, G::Player, G::Traffic, G::Duty, G::Gfx, G::Perf]),
            Lan => stage("lan", P::Sim, Role::Lan, &[G::Net, G::Player, G::Clock, G::Duty, G::View], &[G::Net, G::Traffic, G::People, G::Gfx, G::Sound]),
            Surroundings => stage("surroundings", P::Present, Role::Surroundings, &[G::Weather, G::Player, G::Traffic, G::Net, G::View, G::People], &[G::Weather]),
            Snapshot => stage("snapshot", P::Output, Role::Picture, &[G::Gfx, G::View, G::Traffic, G::Weather], &[G::Gfx, G::Perf]),
            Record => stage("record", P::Output, Role::Own, &[G::Gfx, G::View, G::Sound], &[G::Gfx, G::Perf]),
        }
    }
}

/// The offscreen run's `i`-th step as the runner takes it.
pub(super) struct Step<'r, 'a> {
    pub(super) run: &'r mut Offscreen<'a>,
    pub(super) i: usize,
}

impl frame_stages::Host<OffscreenStage> for Step<'_, '_> {
    fn run_stage(&mut self, stage: OffscreenStage) -> Result<Flow> {
        use OffscreenStage::*;
        let (run, i) = (&mut *self.run, self.i);
        let t_s = i as f32 * run.dt;
        match stage {
            Server => {
                if run.server && !run.server_step(i) {
                    return Ok(Flow::Stop);
                }
            }
            Clock => run.clock_step(t_s),
            Traffic => run.traffic_step(i, t_s)?,
            Startup => run.startup_step(),
            Duty => run.duty_step(),
            Drive => run.drive_stage(i),
            GroundGap => run.ground_gap_step(t_s),
            Humans => run.humans_step(i),
            Lan => run.lan_step(),
            Surroundings => run.surroundings_step(),
            Snapshot => run.snapshot_step(t_s)?,
            Record => run.record_step(i, t_s)?,
        }
        Ok(Flow::Go)
    }

    fn profile(&mut self) -> &mut std::collections::BTreeMap<&'static str, f64> {
        &mut self.run.profile
    }
}
