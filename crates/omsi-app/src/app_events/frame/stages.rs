//! The window's frame as a list of stages (`WINDOW_FRAME`), and what takes them: the `App`
//! with the frame's event loop and the values one stage hands to the next (see
//! `frame_stages`).

use super::*;
use crate::frame_stages::{self, Carry, Flow, Group, Phase, Profile, Role, StageInfo};

/// The window's stages, in the order of [`WINDOW_FRAME`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WindowStage {
    Timing,
    ShellActions,
    Placement,
    Menus,
    Streaming,
    Traffic,
    Controllers,
    MouseDrive,
    PadActions,
    Player,
    Audio,
    Lan,
    OnFoot,
    Humans,
    AfterHumans,
    Duty,
    Integrations,
    ViewKeys,
    Weather,
    Lights,
    Scripted,
    WheelSurface,
    Photo,
    Ui,
    Lighting,
    TraceLook,
    Render,
}

/// One frame of the window, stage by stage.
pub(crate) const WINDOW_FRAME: &[WindowStage] = {
    use WindowStage::*;
    &[
        // the session's housekeeping, the graphics device, the frame's time (none: the
        // session ends)
        Timing,
        // what the pause menu's clicks of the last frame asked for
        ShellActions,
        // a vehicle chosen in the menu, read on a worker meanwhile, put down once it is ready
        Placement,
        // the start menu, and the map's first area still loading (the frame ends there)
        Menus,
        Streaming,
        Traffic,
        // The player's vehicle moves before the passengers are placed: they sit in
        // the bus frame, and placing them on the pose of the frame before made everyone
        // aboard tremble at speed (a quarter of a metre behind the seat, every frame).
        Controllers,
        MouseDrive,
        PadActions,
        Player,
        Audio,
        Lan,
        OnFoot,
        Humans,
        AfterHumans,
        Duty,
        Integrations,
        ViewKeys,
        Weather,
        Lights,
        Scripted,
        WheelSurface,
        // the photo mode's camera and panel
        Photo,
        Ui,
        Lighting,
        TraceLook,
        Render,
    ]
};

const fn stage(
    name: &'static str,
    phase: Phase,
    role: Role,
    reads: &'static [Group],
    writes: &'static [Group],
    profile: Profile,
) -> StageInfo {
    StageInfo { name, phase, role, reads, writes, takes: &[Carry::Time], gives: &[], profile }
}

impl frame_stages::Stage for WindowStage {
    fn info(self) -> StageInfo {
        use Group as G;
        use Phase as P;
        use WindowStage::*;
        match self {
            Timing => StageInfo {
                takes: &[],
                gives: &[Carry::Time],
                ..stage("timing", P::Housekeeping, Role::Own, &[G::Clock, G::Gfx, G::Perf, G::View, G::Net], &[G::Clock, G::Gfx, G::Perf, G::View, G::Net, G::World], Profile::None)
            },
            ShellActions => StageInfo { takes: &[], ..stage("shell_actions", P::Housekeeping, Role::Own, &[G::Menus], &[G::Menus, G::Player, G::World, G::Clock, G::Weather], Profile::Key("shell")) },
            Placement => StageInfo { takes: &[], ..stage("placement", P::Housekeeping, Role::Own, &[G::Player, G::World], &[G::Player, G::Gfx], Profile::Key("placement")) },
            Menus => stage("menus", P::Housekeeping, Role::Own, &[G::Menus, G::Input, G::World], &[G::Menus, G::Input, G::Gfx, G::Net, G::World, G::Player], Profile::None),
            Streaming => StageInfo { takes: &[], ..stage("streaming", P::Sim, Role::Own, &[G::View, G::Player, G::World], &[G::World, G::Traffic, G::Player, G::Gfx], Profile::Key("streaming")) },
            Traffic => stage("traffic", P::Sim, Role::Traffic, &[G::Traffic, G::Player, G::People, G::World, G::View, G::Clock, G::Net], &[G::Traffic, G::Player, G::Gfx, G::Sound], Profile::Key("traffic")),
            Controllers => StageInfo { gives: &[Carry::Controls], ..stage("controllers", P::Input, Role::Own, &[G::Input, G::Player], &[G::Input, G::Ui], Profile::Key("player")) },
            MouseDrive => StageInfo { takes: &[Carry::Time, Carry::Controls], gives: &[Carry::Controls], ..stage("mouse_drive", P::Input, Role::Own, &[G::Input, G::Player, G::View], &[G::Input, G::View], Profile::Key("player")) },
            PadActions => StageInfo { takes: &[Carry::Controls], ..stage("pad_actions", P::Input, Role::Own, &[G::Input], &[G::Player, G::Menus, G::View], Profile::Key("player")) },
            Player => stage("player", P::Sim, Role::Player, &[G::Player, G::Input, G::World, G::Traffic], &[G::Player, G::Traffic, G::View, G::Gfx], Profile::Key("player")),
            Audio => stage("audio", P::Present, Role::Own, &[G::Player, G::View], &[G::Sound, G::View, G::Player], Profile::Key("player")),
            Lan => stage("lan", P::Sim, Role::Lan, &[G::Net, G::Player, G::Clock, G::Duty], &[G::Net, G::Traffic, G::People, G::Gfx, G::Sound], Profile::Key("lan")),
            OnFoot => stage("on_foot", P::Sim, Role::Own, &[G::People, G::Input, G::Net, G::World], &[G::People, G::View, G::Gfx], Profile::Key("on_foot")),
            Humans => stage("humans", P::Sim, Role::People, &[G::People, G::Player, G::Traffic, G::Duty, G::Clock, G::World, G::View], &[G::People, G::Player, G::Traffic, G::Duty, G::Gfx], Profile::Key("humans")),
            AfterHumans => stage("after_humans", P::Sim, Role::Own, &[G::People, G::Player], &[G::People, G::View, G::Player, G::Integrations], Profile::Key("service")),
            Duty => stage("duty", P::Sim, Role::Duty, &[G::Duty, G::Player, G::People, G::Clock, G::World, G::Net], &[G::Duty, G::Player, G::Integrations, G::Ui, G::Net], Profile::Key("duty")),
            Integrations => stage("integrations", P::Sim, Role::Own, &[G::Integrations, G::Player, G::Duty, G::World, G::Net], &[G::Integrations, G::Player, G::Menus, G::Duty], Profile::Key("integrations")),
            ViewKeys => stage("view_keys", P::Input, Role::Own, &[G::Input, G::View, G::Player], &[G::View, G::Player, G::Clock], Profile::Key("view_keys")),
            Weather => StageInfo { gives: &[Carry::Daylight], ..stage("weather", P::Sim, Role::Clock, &[G::Clock, G::Weather, G::World], &[G::Clock, G::Weather, G::Traffic, G::Player, G::Gfx], Profile::Key("weather")) },
            Lights => StageInfo { takes: &[Carry::Time, Carry::Daylight], ..stage("lights", P::Present, Role::Surroundings, &[G::Weather, G::Player, G::Traffic, G::Net, G::View, G::People], &[G::Weather, G::Gfx, G::Sound], Profile::Key("lights+rain")) },
            Scripted => StageInfo { takes: &[Carry::Time, Carry::Daylight], ..stage("scripted", P::Present, Role::Own, &[G::World, G::Traffic, G::Duty, G::Clock, G::View], &[G::World, G::Gfx, G::Sound], Profile::Key("scripted")) },
            WheelSurface => StageInfo { takes: &[], ..stage("wheel_surface", P::Sim, Role::Own, &[G::World, G::Player], &[G::Player], Profile::Key("wheel_surface")) },
            Photo => stage("photo", P::Ui, Role::Own, &[G::Input, G::View], &[G::View, G::Ui], Profile::Key("photo")),
            Ui => StageInfo { gives: &[Carry::VrNavDisplay], ..stage("ui", P::Ui, Role::Own, &[G::Menus, G::Player, G::Duty, G::People, G::Integrations, G::Input, G::World], &[G::Ui, G::Gfx, G::Menus], Profile::Inner) },
            Lighting => StageInfo { takes: &[Carry::Time, Carry::Daylight], gives: &[Carry::Lighting], ..stage("lighting", P::Output, Role::Own, &[G::Weather, G::World, G::Player, G::View], &[G::Weather], Profile::Key("lighting")) },
            TraceLook => StageInfo { takes: &[], ..stage("trace_look", P::Output, Role::Own, &[G::Input, G::View], &[G::Perf], Profile::None) },
            Render => StageInfo { takes: &[Carry::Time, Carry::Lighting, Carry::VrNavDisplay], ..stage("render", P::Output, Role::Picture, &[G::Gfx, G::View, G::Ui], &[G::Gfx, G::Perf, G::Integrations], Profile::Inner) },
        }
    }
}

const NO_TIME: &str = "the timing stage comes first";

/// The window's frame as the runner takes it: the `App`, the event loop, and what the
/// stages hand on (each `Carry` of the stages' `gives`).
pub(crate) struct WindowFrame<'a> {
    app: &'a mut App,
    event_loop: &'a ActiveEventLoop,
    time: Option<FrameTime>,
    controls: Option<(crate::controllers::Analog, Vec<(String, bool)>)>,
    daylight: Option<omsi_sim::Daylight>,
    vr_nav_display: Option<crate::vr_navigator::Display>,
    lighting: Option<omsi_render::Lighting>,
}

impl<'a> WindowFrame<'a> {
    pub(crate) fn new(app: &'a mut App, event_loop: &'a ActiveEventLoop) -> Self {
        WindowFrame { app, event_loop, time: None, controls: None, daylight: None, vr_nav_display: None, lighting: None }
    }
}

impl frame_stages::Host<WindowStage> for WindowFrame<'_> {
    fn run_stage(&mut self, stage: WindowStage) -> anyhow::Result<Flow> {
        use WindowStage::*;
        let app = &mut *self.app;
        let el = self.event_loop;
        let times = self.time.as_ref().map(|t| (t.dt, t.raw_dt));
        let dt = || times.expect(NO_TIME).0;
        match stage {
            Timing => match app.frame_timing(el) {
                Some(t) => self.time = Some(t),
                None => return Ok(Flow::Stop),
            },
            ShellActions => app.apply_shell_actions(el),
            Placement => app.poll_vehicle_placement(),
            Menus => {
                if !app.frame_menus(el, dt()) {
                    return Ok(Flow::Stop);
                }
            }
            Streaming => app.drive_streaming(),
            Traffic => app.frame_traffic(dt()),
            Controllers => self.controls = Some(app.frame_controllers(dt())),
            MouseDrive => {
                let (analog, actions) = self.controls.take().expect("the controllers stage comes first");
                self.controls = Some((app.frame_mouse_drive(dt(), analog), actions));
            }
            PadActions => {
                let (analog, actions) = self.controls.take().expect("the controllers stage comes first");
                app.frame_pad_actions(analog, actions);
            }
            Player => app.frame_player(dt()),
            Audio => app.frame_audio(dt()),
            Lan => app.frame_lan(dt()),
            OnFoot => app.frame_on_foot(dt()),
            Humans => app.frame_humans(dt()),
            AfterHumans => app.frame_after_humans(dt()),
            Duty => app.frame_duty(dt()),
            Integrations => app.frame_integrations(el, dt()),
            ViewKeys => app.frame_view_keys(dt()),
            Weather => self.daylight = Some(app.frame_weather(dt())),
            Lights => app.frame_lights(dt(), self.daylight.expect(NO_DAYLIGHT)),
            Scripted => app.frame_scripted(dt(), self.daylight.expect(NO_DAYLIGHT)),
            WheelSurface => app.frame_wheel_surface(),
            Photo => app.frame_photo(times.expect(NO_TIME).1),
            Ui => self.vr_nav_display = app.frame_ui(dt()),
            Lighting => self.lighting = Some(app.frame_lighting(dt(), self.daylight.expect(NO_DAYLIGHT))),
            TraceLook => app.trace_look("frame", app.input.cursor.0, app.input.cursor.1),
            Render => {
                let lighting = self.lighting.take().expect("the lighting stage comes first");
                app.frame_render(el, self.time.as_ref().expect(NO_TIME), &lighting, self.vr_nav_display.take());
            }
        }
        Ok(Flow::Go)
    }

    fn profile(&mut self) -> &mut std::collections::BTreeMap<&'static str, f64> {
        &mut self.app.perf.profile
    }
}

const NO_DAYLIGHT: &str = "the weather stage comes first";
