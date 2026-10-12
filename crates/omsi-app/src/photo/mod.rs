//! The photo mode: the game stands still and a camera of its own flies round the bus - free
//! as a drone or in orbit round it - with a real lens (focal length, aperture, focus, the
//! blades' shape in the bokeh), a shutter that blurs the street past a moving bus, a
//! darkroom (exposure, white balance, tone, colour, film looks, vignetting, grain), a frame
//! of any aspect ratio with composition guides, and the photo saved into `Screenshots`.
//!
//! What is shown is the photo itself, developed as it will be saved: while the camera moves
//! a quick picture, once it stands the pictures of the shot add up frame by frame until the
//! lens and the shutter have been sampled (`render`). The panel is the launcher's toolkit
//! (`panel`).

use std::collections::HashSet;

use glam::{DVec3, Vec3};
use omsi_render::Camera;
use winit::keyboard::KeyCode;

mod pad;
mod panel;
mod render;

// (the lens and the darkroom are the omsi-photo crate)
use omsi_photo::{develop, lens};
pub(crate) use develop::Grade;
pub(crate) use omsi_photo::stretch;

/// A request of the photo mode's panel.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Request {
    Exit,
    Take,
    Reset,
    HideUi,
    /// The horizon level again (no roll).
    Level,
    /// The clock moved by these seconds (the light of another hour).
    Clock(f64),
    /// The photo just saved shown in the file browser.
    Reveal(std::path::PathBuf),
}

/// How far the camera may go from the bus: (metres, label).
pub(crate) const RANGES: [(f32, &str); 5] = [(25.0, "25 m"), (50.0, "50 m"), (100.0, "100 m"), (250.0, "250 m"), (f32::INFINITY, "Unlimited")];
/// The frame: (width / height, label); None: the window's.
pub(crate) const ASPECTS: [(Option<f32>, &str); 8] = [
    (None, "Window"),
    (Some(16.0 / 9.0), "16:9"),
    (Some(21.0 / 9.0), "21:9 (cinema)"),
    (Some(3.0 / 2.0), "3:2"),
    (Some(4.0 / 3.0), "4:3"),
    (Some(1.0), "1:1 (square)"),
    (Some(4.0 / 5.0), "4:5 (portrait)"),
    (Some(9.0 / 16.0), "9:16 (story)"),
];
pub(crate) const GRIDS: [&str; 4] = ["Off", "Rule of thirds", "Golden ratio", "Centre"];
/// The f-numbers of the aperture ring.
pub(crate) const FSTOPS: [f32; 11] = [1.2, 1.4, 1.8, 2.0, 2.8, 4.0, 5.6, 8.0, 11.0, 16.0, 22.0];
/// Shutter times (s) with their labels.
pub(crate) const SHUTTERS: [(f32, &str); 8] = [(1.0 / 8.0, "1/8 s"), (1.0 / 15.0, "1/15 s"), (1.0 / 30.0, "1/30 s"), (1.0 / 60.0, "1/60 s"), (1.0 / 125.0, "1/125 s"), (1.0 / 250.0, "1/250 s"), (1.0 / 500.0, "1/500 s"), (1.0 / 1000.0, "1/1000 s")];
pub(crate) const BLADES: [(u32, &str); 4] = [(0, "Round"), (5, "5 blades"), (6, "6 blades"), (8, "8 blades")];
/// Pictures a photo is made of when it has depth of field or motion blur.
pub(crate) const QUALITY: [(u32, &str); 4] = [(16, "Draft (16 samples)"), (32, "Good (32 samples)"), (64, "High (64 samples)"), (128, "Best (128 samples)")];
/// The saved photo's size, times the window's.
pub(crate) const SIZES: [(f32, &str); 2] = [(1.0, "As the window"), (2.0, "Double (supersampled)")];

/// What the photo mode is set to (kept for the session).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Settings {
    pub orbit: bool,
    /// Vertical field of view (degrees).
    pub fov: f32,
    pub roll: f32,
    pub speed: f32,
    pub range: usize,
    pub grid: usize,
    pub aspect: usize,
    pub dof: bool,
    pub auto_focus: bool,
    pub focus: f32,
    pub fstop: usize,
    pub blades: usize,
    pub motion: bool,
    pub shutter: usize,
    pub quality: usize,
    pub grade: Grade,
    pub size: usize,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { orbit: false, fov: lens::fov_for(35.0), roll: 0.0, speed: 1.0, range: 1, grid: 0, aspect: 0, dof: false, auto_focus: true, focus: 10.0, fstop: 4, blades: 0, motion: false, shutter: 2, quality: 1, grade: Grade::neutral(), size: 0 }
    }
}

/// The photo mode while it is on.
pub(crate) struct Photo {
    pub cam: Camera,
    /// Where the bus stood (the camera's leash, the orbit's centre).
    pub anchor: DVec3,
    /// The bus's velocity (m/s, world) when the game stopped: the rolling shot's.
    pub bus_velocity: Vec3,
    /// The orbit: round the anchor, at this distance, from this side and height.
    orbit: (f32, f32, f32),
    vel: Vec3,
    fov_target: f32,
    pub keys: HashSet<KeyCode>,
    /// The view turned by dragging (the right button anywhere, the left over the picture).
    pub looking: bool,
    pub settings: Settings,
    pub tab: usize,
    pub hide_ui: bool,
    /// The game stood paused before.
    prev_paused: bool,
    /// The view the game had, to return to.
    prev_camera: Option<Camera>,
    /// How far the photo's clock was moved (s): moved back on the way out, so that the
    /// bus's duty goes on at the time it stood at (#2116).
    clock_shift: f64,
    pub(crate) render: render::State,
    /// A word of what was done (the photo saved), and how long it shows.
    pub note: Option<(String, f32)>,
    /// The file the note tells of (a click on it shows it in the file browser).
    pub saved: Option<std::path::PathBuf>,
    pad: pad::PadState,
}

impl Photo {
    fn new(cam: Camera, anchor: DVec3, bus_velocity: Vec3, prev_paused: bool, settings: Settings) -> Photo {
        let to = cam.position - anchor;
        let dist = (to.length() as f32).clamp(3.0, 60.0);
        let around = (to.x as f32).atan2(to.y as f32).to_degrees();
        let up = ((to.z as f32) / dist.max(0.1)).clamp(-1.0, 1.0).asin().to_degrees();
        let mut cam = cam;
        cam.fov_deg = settings.fov;
        cam.roll = settings.roll;
        Photo {
            cam,
            anchor,
            bus_velocity,
            orbit: (dist, around, up.clamp(-10.0, 80.0)),
            vel: Vec3::ZERO,
            fov_target: settings.fov,
            keys: HashSet::new(),
            looking: false,
            settings,
            tab: 0,
            hide_ui: false,
            prev_paused,
            prev_camera: Some(cam),
            clock_shift: 0.0,
            render: render::State::default(),
            note: None,
            saved: None,
            pad: pad::PadState::default(),
        }
    }

    /// The camera turned by (dx, dy) degrees (right, down).
    pub(crate) fn look(&mut self, dx: f32, dy: f32) {
        if self.settings.orbit {
            self.orbit.1 += dx;
            self.orbit.2 = (self.orbit.2 + dy).clamp(-10.0, 85.0);
        } else {
            self.cam.yaw = (self.cam.yaw + dx).rem_euclid(360.0);
            self.cam.pitch = (self.cam.pitch - dy).clamp(-89.0, 89.0);
        }
    }

    /// The lens zoomed by wheel notches (up: longer).
    pub(crate) fn zoom(&mut self, notches: f32) {
        self.fov_target = (self.fov_target * (1.0 - 0.08 * notches)).clamp(lens::fov_for(400.0), lens::fov_for(10.0));
    }

    /// The focus distance the lens is at: the bus (where the camera looks at it) when it
    /// focuses by itself, else the one set.
    pub(crate) fn focus_distance(&self) -> f32 {
        if !self.settings.auto_focus {
            return self.settings.focus;
        }
        let to = (self.anchor + DVec3::new(0.0, 0.0, 1.6) - self.cam.position).as_vec3();
        let along = to.dot(self.cam.forward());
        if along > 0.3 { along } else { self.settings.focus }
    }

    /// One frame of the camera: the keys and the pad held, smoothed, kept near the bus
    /// and above the ground.
    fn fly(&mut self, dt: f32, ground: impl Fn(f64, f64) -> Option<f64>) {
        let keys = self.keys.clone();
        let k = |c: KeyCode| keys.contains(&c);
        let axis = |plus: bool, minus: bool| (plus as i32 - minus as i32) as f32;
        // W A S D move, Space (or E) rises and Ctrl (or C, Q) sinks, as in a game's free camera
        let up = k(KeyCode::Space) || k(KeyCode::KeyE);
        let down = k(KeyCode::ControlLeft) || k(KeyCode::ControlRight) || k(KeyCode::KeyC) || k(KeyCode::KeyQ);
        let mut m = Vec3::new(axis(k(KeyCode::KeyD), k(KeyCode::KeyA)), axis(k(KeyCode::KeyW), k(KeyCode::KeyS)), axis(up, down));
        m += self.pad.moves();
        let fast = if k(KeyCode::ShiftLeft) || k(KeyCode::ShiftRight) { 4.0 } else if k(KeyCode::AltLeft) || k(KeyCode::AltRight) { 0.25 } else { 1.0 };
        // the arrows and the right stick turn the view; Z and X roll it
        let turn = 70.0 * dt * (self.cam.fov_deg / 60.0).clamp(0.15, 1.5);
        let (lx, ly) = (axis(k(KeyCode::ArrowRight), k(KeyCode::ArrowLeft)) + self.pad.look[0], axis(k(KeyCode::ArrowDown), k(KeyCode::ArrowUp)) + self.pad.look[1]);
        if lx != 0.0 || ly != 0.0 {
            self.look(lx * turn, ly * turn);
        }
        let roll = axis(k(KeyCode::KeyX), k(KeyCode::KeyZ)) + self.pad.roll;
        if roll != 0.0 {
            self.settings.roll = (self.settings.roll + roll * 30.0 * dt).clamp(-45.0, 45.0);
        }
        // the lens: + and - (and the pad's d-pad) zoom
        let zoom = axis(k(KeyCode::Equal) || k(KeyCode::NumpadAdd), k(KeyCode::Minus) || k(KeyCode::NumpadSubtract)) + self.pad.zoom;
        if zoom != 0.0 {
            self.zoom(zoom * dt * 6.0);
        }
        let ease = 1.0 - (-dt / 0.10).exp();
        self.settings.fov += (self.fov_target - self.settings.fov) * ease;
        if (self.settings.fov - self.fov_target).abs() < 0.05 {
            self.settings.fov = self.fov_target;
        }
        let speed = 5.0 * self.settings.speed * fast;
        let range = RANGES[self.settings.range.min(RANGES.len() - 1)].0;
        if self.settings.orbit {
            // W and S come closer and back away, A and D go round, Space and Ctrl up and down
            let target = Vec3::new(m.x * 60.0, -m.y * speed, m.z * 40.0);
            self.vel += (target - self.vel) * (1.0 - (-dt / 0.12).exp());
            if self.vel.length() < 0.05 && target == Vec3::ZERO {
                self.vel = Vec3::ZERO;
            }
            self.orbit.0 = (self.orbit.0 + self.vel.y * dt).clamp(2.0, range.min(400.0));
            self.orbit.1 += self.vel.x * dt;
            self.orbit.2 = (self.orbit.2 + self.vel.z * dt).clamp(-10.0, 85.0);
            let (d, a, e) = (self.orbit.0 as f64, (self.orbit.1 as f64).to_radians(), (self.orbit.2 as f64).to_radians());
            let centre = self.anchor + DVec3::new(0.0, 0.0, 1.5);
            self.cam.position = centre + DVec3::new(a.sin() * e.cos(), a.cos() * e.cos(), e.sin()) * d;
            let dir = (centre - self.cam.position).as_vec3().normalize_or(Vec3::Y);
            self.cam.yaw = dir.x.atan2(dir.y).to_degrees();
            self.cam.pitch = dir.z.clamp(-1.0, 1.0).asin().to_degrees();
        } else {
            // a drone: level over the ground whichever way it looks, up and down apart
            let (s, c) = self.cam.yaw.to_radians().sin_cos();
            let fwd = Vec3::new(s, c, 0.0);
            let right = Vec3::new(c, -s, 0.0);
            let target = (right * m.x + fwd * m.y + Vec3::Z * m.z) * speed;
            self.vel += (target - self.vel) * (1.0 - (-dt / 0.15).exp());
            // (stopped once it no longer shows: a velocity easing out for ever moved the camera
            // a fraction of a millimetre a frame, and every such move started the photo again)
            if self.vel.length() < 0.05 && target == Vec3::ZERO {
                self.vel = Vec3::ZERO;
            }
            self.cam.position += (self.vel * dt).as_dvec3();
        }
        // on its leash round the bus
        let off = self.cam.position - self.anchor;
        if range.is_finite() && off.length() > range as f64 {
            self.cam.position = self.anchor + off.normalize() * range as f64;
        }
        // and never under the ground
        if let Some(g) = ground(self.cam.position.x, self.cam.position.y) {
            if self.cam.position.z < g + 0.25 {
                self.cam.position.z = g + 0.25;
                self.vel.z = self.vel.z.max(0.0);
            }
        }
        self.cam.fov_deg = self.settings.fov;
        self.cam.roll = self.settings.roll;
    }

    /// A key of the keyboard while the photo mode is on; the requests it makes.
    fn key(&mut self, code: KeyCode, pressed: bool) -> Option<Request> {
        if !pressed {
            self.keys.remove(&code);
            return None;
        }
        self.keys.insert(code);
        match code {
            KeyCode::Escape => Some(Request::Exit),
            KeyCode::KeyH => Some(Request::HideUi),
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::F12 => Some(Request::Take),
            KeyCode::KeyR => Some(Request::Level),
            KeyCode::Tab => {
                let n = panel::TABS.len();
                let back = self.keys.contains(&KeyCode::ShiftLeft) || self.keys.contains(&KeyCode::ShiftRight);
                self.tab = if back { (self.tab + n - 1) % n } else { (self.tab + 1) % n };
                None
            }
            _ => None,
        }
    }
}

impl Photo {
    /// The photo as last developed (w, h, RGBA).
    pub(crate) fn developed(&self) -> Option<(u32, u32, &[u8])> {
        self.render.developed.as_ref().map(|(w, h, px)| (*w, *h, px.as_slice()))
    }
}

/// `OMSI_PHOTO`: settings for a test, `key=value` by commas.
fn test_settings(s: &mut Settings, text: &str) {
    for part in text.split(',') {
        let Some((k, v)) = part.split_once('=') else { continue };
        let x: f32 = v.trim().parse().unwrap_or(0.0);
        let near = |list: &[f32]| list.iter().enumerate().min_by(|a, b| (a.1 - x).abs().total_cmp(&(b.1 - x).abs())).map(|e| e.0).unwrap_or(0);
        match k.trim() {
            "orbit" => s.orbit = x != 0.0,
            "focal" => s.fov = lens::fov_for(x),
            "dof" => s.dof = x != 0.0,
            "fstop" => s.fstop = near(&FSTOPS),
            "focus" => {
                s.auto_focus = false;
                s.focus = x;
            }
            "motion" => s.motion = x != 0.0,
            "shutter" => s.shutter = near(&SHUTTERS.map(|t| 1.0 / t.0)),
            "filter" => s.grade.filter = develop::Filter::ALL[(x as usize).min(develop::Filter::ALL.len() - 1)],
            "vignette" => s.grade.vignette = x,
            "grain" => s.grade.grain = x,
            "exposure" => s.grade.exposure = x,
            "aspect" => s.aspect = (x as usize).min(ASPECTS.len() - 1),
            "grid" => s.grid = (x as usize).min(GRIDS.len() - 1),
            "quality" => s.quality = (x as usize).min(QUALITY.len() - 1),
            "dist" | "around" | "up" | "kmh" => {}
            _ => log::warn!("OMSI_PHOTO: no setting {k}"),
        }
    }
}

impl crate::App {
    pub(crate) fn photo_on(&self) -> bool {
        self.photo.is_some()
    }

    /// Into the photo mode (from the pause menu or its key): the game stands still, the
    /// camera starts where the view was.
    pub(crate) fn enter_photo(&mut self) {
        if self.photo.is_some() || self.camera.is_none() {
            return;
        }
        self.release_vehicle_keys();
        let cam = self.camera.unwrap();
        let (anchor, velocity) = match self.player.as_ref() {
            Some(p) => {
                let h = p.vehicle.heading.to_radians();
                let v = p.vehicle.physics.speed;
                (p.vehicle.position, Vec3::new(h.sin() as f32, h.cos() as f32, 0.0) * v)
            }
            None => (cam.position, Vec3::ZERO),
        };
        let mut settings = self.photo_settings.clone().unwrap_or_default();
        if let Some(t) = omsi_cfg::flags::OMSI_PHOTO.var() {
            test_settings(&mut settings, t);
        }
        let mut ph = Photo::new(cam, anchor, velocity, self.paused, settings);
        // (a camera inside the bus starts as it is; the field of view is the photo mode's)
        ph.cam.pitch = cam.pitch;
        // (a test's orbit: OMSI_PHOTO dist, around, up)
        for part in omsi_cfg::flags::OMSI_PHOTO.var().unwrap_or("").split(',') {
            let Some((k, v)) = part.split_once('=') else { continue };
            let x: f32 = v.trim().parse().unwrap_or(0.0);
            match k.trim() {
                "dist" => ph.orbit.0 = x,
                "around" => ph.orbit.1 = x,
                "up" => ph.orbit.2 = x,
                // (a moving bus for a rolling shot without driving one)
                "kmh" => {
                    let h = self.player.as_ref().map(|p| p.vehicle.heading.to_radians()).unwrap_or(0.0);
                    ph.bus_velocity = Vec3::new(h.sin() as f32, h.cos() as f32, 0.0) * x / 3.6;
                }
                _ => {}
            }
        }
        self.photo = Some(ph);
        if self.net.lan.is_none() {
            self.paused = true;
        }
        log::info!("photo mode on at {:.1},{:.1},{:.1}, the bus at {:.1} km/h", cam.position.x, cam.position.y, cam.position.z, velocity.length() * 3.6);
    }

    /// Out of the photo mode: the game as it was.
    pub(crate) fn exit_photo(&mut self) {
        let Some(ph) = self.photo.take() else { return };
        self.photo_settings = Some(ph.settings.clone());
        self.paused = ph.prev_paused;
        if ph.clock_shift != 0.0 {
            self.shift_clock(-ph.clock_shift);
        }
        if let Some(c) = ph.prev_camera {
            self.camera = Some(c);
        }
        self.shell.drop_picture();
        self.shell.opaque = false;
        self.shell.clear();
        log::info!("photo mode off");
    }

    pub(crate) fn photo_request(&mut self, r: Request) {
        match r {
            Request::Exit => self.exit_photo(),
            Request::Take => {
                if let Some(ph) = self.photo.as_mut() {
                    ph.render.capture = true;
                }
            }
            Request::Reset => {
                if let Some(ph) = self.photo.as_mut() {
                    ph.settings = Settings { fov: ph.settings.fov, orbit: ph.settings.orbit, ..Settings::default() };
                    ph.fov_target = ph.settings.fov;
                }
            }
            Request::Level => {
                if let Some(ph) = self.photo.as_mut() {
                    ph.settings.roll = 0.0;
                }
            }
            Request::HideUi => {
                if let Some(ph) = self.photo.as_mut() {
                    ph.hide_ui = !ph.hide_ui;
                }
            }
            Request::Reveal(path) => crate::platform::reveal(&path),
            Request::Clock(secs) => {
                if self.net.lan.as_ref().is_some_and(|l| l.role == omsi_net::Role::Client) {
                    if let Some(ph) = self.photo.as_mut() {
                        ph.note = Some((omsi_ui::tr("In a LAN session the host sets the clock").into_owned(), 3.0));
                    }
                } else {
                    let moved = !self.real_time_locked();
                    self.shift_clock(secs);
                    if let Some(ph) = self.photo.as_mut() {
                        if moved {
                            ph.clock_shift += secs;
                        }
                        ph.render.reset();
                    }
                }
            }
        }
    }

    /// A key while the photo mode is on: all of them are its own.
    pub(crate) fn photo_key(&mut self, code: KeyCode, pressed: bool, repeat: bool) {
        let Some(ph) = self.photo.as_mut() else { return };
        if repeat {
            return;
        }
        if let Some(r) = ph.key(code, pressed) {
            self.photo_request(r);
        }
    }

    /// The photo mode's frame before the picture: the pad, the camera flown, the view the
    /// game streams the world round, and the panel.
    pub(crate) fn frame_photo(&mut self, dt: f32) {
        if self.photo.is_none() {
            return;
        }
        let requests = match (self.photo.as_mut(), self.input.controllers.as_ref()) {
            (Some(ph), Some(ctl)) => ph.pad.read(ctl),
            (Some(ph), None) => {
                ph.pad = pad::PadState::default();
                Vec::new()
            }
            _ => Vec::new(),
        };
        for r in requests {
            self.photo_request(r);
        }
        let world = self.world.clone();
        let Some(ph) = self.photo.as_mut() else { return };
        ph.fly(dt.min(0.1), |x, y| world.as_ref().and_then(|w| w.ground_height(x, y)));
        if let Some(n) = ph.note.as_mut() {
            n.1 -= dt;
        }
        if ph.note.as_ref().is_some_and(|n| n.1 <= 0.0) {
            ph.note = None;
            ph.saved = None;
        }
        self.camera = Some(ph.cam);
        self.frame_photo_panel(dt);
    }
}
