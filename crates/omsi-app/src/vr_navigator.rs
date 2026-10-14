//! Personal cockpit navigator placement, stored per vehicle outside the repository.

use glam::{DVec3, Mat4, Quat, Vec3, Vec4};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub(crate) struct Placement {
    pub enabled: bool,
    /// Metres relative to the bus's standard driver camera, in vehicle axes.
    pub offset: [f32; 3],
    pub width: f32,
    pub yaw: f32,
    pub tilt: f32,
    pub roll: f32,
    pub opacity: f32,
}

impl Default for Placement {
    fn default() -> Self {
        Self {
            enabled: false,
            offset: [0.28, 0.65, -0.35],
            width: 0.28,
            yaw: -23.0,
            tilt: 26.0,
            roll: 0.0,
            opacity: 0.95,
        }
    }
}

impl Placement {
    pub(crate) fn value(&self, field: &str) -> Option<f32> {
        Some(match field {
            "x" => self.offset[0], "y" => self.offset[1], "z" => self.offset[2],
            "width" => self.width, "yaw" => self.yaw, "tilt" => self.tilt,
            "roll" => self.roll, "opacity" => self.opacity,
            _ => return None,
        })
    }

    fn set_value(&mut self, field: &str, value: f32) {
        match field {
            "x" => self.offset[0] = value, "y" => self.offset[1] = value,
            "z" => self.offset[2] = value, "width" => self.width = value,
            "yaw" => self.yaw = value, "tilt" => self.tilt = value,
            "roll" => self.roll = value, "opacity" => self.opacity = value,
            _ => return,
        }
        *self = self.sanitize();
    }

    fn scroll(&mut self, amount: f32, resize: bool, eye: Vec3, driver: Vec3) {
        if !amount.is_finite() {
            return;
        }
        if resize {
            self.width *= 1.08_f32.powf(amount.clamp(-20.0, 20.0));
        } else {
            let ray = driver + Vec3::from(self.offset) - eye;
            let distance = ray.length();
            if distance > 0.001 {
                let new_distance = (distance - amount * 0.04).clamp(0.25, 2.5);
                self.offset = (eye + ray / distance * new_distance - driver).to_array();
            }
        }
        *self = self.sanitize();
    }

    fn sanitize(mut self) -> Self {
        let default = Self::default();
        let clamp = |v: f32, fallback: f32, lo: f32, hi: f32| {
            if v.is_finite() {
                v.clamp(lo, hi)
            } else {
                fallback
            }
        };
        for i in 0..3 {
            self.offset[i] = clamp(self.offset[i], default.offset[i], -2.0, 2.0);
        }
        self.width = clamp(self.width, default.width, 0.12, 0.65);
        self.yaw = clamp(self.yaw, default.yaw, -180.0, 180.0);
        self.tilt = clamp(self.tilt, default.tilt, -80.0, 80.0);
        self.roll = clamp(self.roll, default.roll, -180.0, 180.0);
        self.opacity = clamp(self.opacity, default.opacity, 0.3, 1.0);
        self
    }

    pub fn adjust(&mut self, field: &str, direction: f32) {
        match field {
            "x" => self.offset[0] += direction * 0.02,
            "y" => self.offset[1] += direction * 0.02,
            "z" => self.offset[2] += direction * 0.02,
            "width" => self.width += direction * 0.02,
            "yaw" => self.yaw += direction * 2.0,
            "tilt" => self.tilt += direction * 2.0,
            "roll" => self.roll += direction * 2.0,
            "opacity" => self.opacity += direction * 0.05,
            "enabled" => self.enabled = !self.enabled,
            "reset" => *self = Self { enabled: self.enabled, ..Self::default() },
            _ => return,
        }
        *self = self.sanitize();
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Profiles {
    buses: BTreeMap<String, Placement>,
}

impl Profiles {
    pub fn load() -> Self {
        let Some(path) =
            crate::settings::Settings::path().map(|p| p.with_file_name("vr-navigator.json"))
        else {
            return Self::default();
        };
        match std::fs::read(&path) {
            Ok(data) => match serde_json::from_slice::<Self>(&data) {
                Ok(mut profiles) => {
                    for value in profiles.buses.values_mut() {
                        *value = value.sanitize();
                    }
                    profiles
                }
                Err(e) => {
                    log::warn!("VR navigator: cannot read {}: {e}", path.display());
                    Self::default()
                }
            },
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    log::warn!("VR navigator: {}: {e}", path.display());
                }
                Self::default()
            }
        }
    }

    pub fn get(&self, key: &str) -> Placement {
        self.buses.get(key).copied().unwrap_or_default()
    }

    fn save(&self) -> anyhow::Result<()> {
        let path = crate::settings::Settings::path()
            .ok_or_else(|| anyhow::anyhow!("no user settings directory"))?
            .with_file_name("vr-navigator.json");
        self.save_to(&path)
    }

    fn save_to(&self, path: &Path) -> anyhow::Result<()> {
        std::fs::create_dir_all(path.parent().unwrap())?;
        let temp = path.with_extension("json.tmp");
        std::fs::write(&temp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(temp, path)?;
        Ok(())
    }
}

fn bus_key(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
        .to_lowercase()
}

fn driver_origin(camera: &omsi_vehicle::Camera) -> Vec3 {
    let (sy, cy) = camera.yaw.to_radians().sin_cos();
    let (sp, cp) = camera.pitch.to_radians().sin_cos();
    Vec3::from(camera.pos) - Vec3::new(sy * cp, cy * cp, sp) * camera.dist.max(0.0)
}

/// A display is anchored to the vehicle, independently of view, seat and HMD recentering.
#[derive(Clone, Copy)]
pub(crate) struct Display {
    pub placement: Placement,
    pub local_center: Vec3,
}

impl Display {
    pub fn transform(
        &self,
        bus: DVec3,
        body: Mat4,
        eye: &omsi_render::Camera,
        projection: Mat4,
        aspect: f32,
    ) -> Mat4 {
        let rotation = Quat::from_rotation_z(self.placement.yaw.to_radians())
            * Quat::from_rotation_x(-self.placement.tilt.to_radians())
            * Quat::from_rotation_y(self.placement.roll.to_radians());
        let right = body.transform_vector3(rotation * Vec3::X) * self.placement.width * 0.5;
        let up = body.transform_vector3(rotation * Vec3::Z) * self.placement.width / aspect * 0.5;
        let origin = bus + body.transform_point3(self.local_center).as_dvec3();
        let plane = Mat4::from_cols(
            right.extend(0.0),
            up.extend(0.0),
            Vec4::Z,
            (origin - eye.position).as_vec3().extend(1.0),
        );
        projection * Mat4::look_to_rh(Vec3::ZERO, eye.forward(), eye.up()) * plane
    }
}

pub(crate) struct Editing {
    pub moving: bool,
    pub rotating: bool,
    paused_before: bool,
    mouse_drive_before: bool,
}

impl crate::App {
    pub(crate) fn start_vr_nav_edit(&mut self) {
        if !self.vr_active() || self.player.is_none() || self.vr_nav_edit.is_some() {
            return;
        }
        self.on_left(false);
        // The editor consumes key-up events; release existing driving and script
        // inputs before entering it so none remain held when driving resumes.
        if let Some(p) = self.player.as_mut() {
            let held: Vec<_> = p.held_keys.keys().copied().collect();
            for scan in held {
                p.key(scan, 0, false);
            }
            p.axes.release_all();
            for name in self.door_key_triggers.drain().flat_map(|(_, names)| names) {
                let off = format!("{name}_off");
                if p.vehicle.ty.program.trigger(&off).is_some() {
                    p.vehicle.trigger(&off);
                }
            }
        }
        self.buttons_held = (false, false);
        if self.game_menu.is_some() {
            self.close_game_menu();
        }
        self.chooser = None;
        self.admin_list = None;
        self.list_kind = None;
        self.dropdown = None;
        self.menu_drag = None;
        self.menu_edit = None;
        self.view = "driver".into();
        self.vr_nav_edit = Some(Editing {
            moving: false,
            rotating: false,
            paused_before: self.paused,
            mouse_drive_before: self.mouse_drive,
        });
        if self.lan.is_none() {
            self.paused = true;
        }
        self.mouse_drive = false;
        self.mouse_look = false;
        self.both_drag = None;
        self.keys.clear();
        self.hover_key = None;
        #[cfg(windows)]
        {
            self.vr_zoom_active = false;
            self.reset_vr_pointer();
        }
        if let Some(n) = self.navigator.as_mut() {
            if n.map_open() {
                n.toggle_map();
            }
        }
        if let Some(p) = self.player.as_ref() {
            let key = bus_key(&p.vehicle.ty.def.path, &self.args.root);
            self.vr_nav_profiles.buses.entry(key).or_default().enabled = true;
        }
        if let Some(window) = self.window.as_ref() {
            let _ = window.set_cursor_grab(winit::window::CursorGrabMode::Confined);
            window.set_cursor_visible(false);
        }
        self.cursor_hidden = None;
    }

    pub(crate) fn finish_vr_nav_edit(&mut self) {
        let Some(edit) = self.vr_nav_edit.take() else {
            return;
        };
        self.paused = edit.paused_before;
        self.mouse_drive = edit.mouse_drive_before;
        self.mouse_look = false;
        self.hover_key = None;
        self.keys.clear();
        self.cursor_hidden = None;
        #[cfg(windows)]
        self.reset_vr_pointer();
        if let Some(window) = self.window.as_ref() {
            let _ = window.set_cursor_grab(winit::window::CursorGrabMode::None);
            window.set_cursor_visible(true);
            if edit.mouse_drive_before {
                let _ = window.set_cursor_position(winit::dpi::PhysicalPosition::new(
                    self.cursor.0 as f64,
                    self.cursor.1 as f64,
                ));
            }
        }
        self.save_vr_nav_profiles();
    }

    /// The headset's axes in vehicle space, so a drag follows the direction the
    /// driver is looking while the resulting placement stays attached to the bus.
    fn vr_nav_edit_geometry(&self) -> Option<(Vec3, Vec3, Vec3, Vec3)> {
        let p = self.player.as_ref()?;
        let def = &p.vehicle.ty.def;
        let driver = def
            .cameras_driver
            .get(def.camera_std)
            .or(def.cameras_driver.first())?;
        let eye = *self.camera.as_ref()?;
        #[cfg(windows)]
        let eye = self
            .vr
            .as_ref()
            .and_then(|vr| vr.navigator_edit_camera())
            .unwrap_or(eye);
        let inverse = p.vehicle.body_rotation().inverse();
        Some((
            inverse.transform_vector3(eye.right()),
            inverse.transform_vector3(eye.up()),
            inverse.transform_point3((eye.position - p.vehicle.position).as_vec3()),
            driver_origin(driver),
        ))
    }

    pub(crate) fn vr_nav_drag(&mut self, dx: f32, dy: f32) {
        let Some(edit) = self.vr_nav_edit.as_ref() else {
            return;
        };
        if !edit.moving && !edit.rotating {
            return;
        }
        let (moving, rotating) = (edit.moving, edit.rotating);
        let Some((right, up, eye, driver)) = self.vr_nav_edit_geometry() else {
            return;
        };
        let key = bus_key(
            &self.player.as_ref().unwrap().vehicle.ty.def.path,
            &self.args.root,
        );
        let p = self.vr_nav_profiles.buses.entry(key).or_default();
        if moving {
            let distance = (driver + Vec3::from(p.offset) - eye)
                .length()
                .clamp(0.25, 2.5);
            p.offset =
                (Vec3::from(p.offset) + (right * dx - up * dy) * distance * 0.0015).to_array();
        }
        if rotating {
            use winit::keyboard::KeyCode;
            if self.keys.contains(&KeyCode::ShiftLeft) || self.keys.contains(&KeyCode::ShiftRight) {
                p.roll += dx * 0.25;
            } else {
                p.yaw -= dx * 0.25;
                p.tilt -= dy * 0.25;
            }
        }
        *p = p.sanitize();
    }

    pub(crate) fn vr_nav_scroll(&mut self, amount: f32) {
        if !amount.is_finite() {
            return;
        }
        let Some((_, _, eye, driver)) = self.vr_nav_edit_geometry() else {
            return;
        };
        let key = bus_key(
            &self.player.as_ref().unwrap().vehicle.ty.def.path,
            &self.args.root,
        );
        let p = self.vr_nav_profiles.buses.entry(key).or_default();
        use winit::keyboard::KeyCode;
        let resize =
            self.keys.contains(&KeyCode::ControlLeft) || self.keys.contains(&KeyCode::ControlRight);
        p.scroll(amount, resize, eye, driver);
    }

    fn save_vr_nav_profiles(&mut self) {
        if let Err(e) = self.vr_nav_profiles.save() {
            log::warn!("VR navigator: saving placement: {e}");
            self.service_msg = Some((
                format!("{}: {e}", omsi_ui::tr("Could not save navigator position")),
                5.0,
            ));
        }
    }
    pub(crate) fn vr_nav_profile(&self) -> Placement {
        self.player
            .as_ref()
            .map(|p| {
                self.vr_nav_profiles
                    .get(&bus_key(&p.vehicle.ty.def.path, &self.args.root))
            })
            .unwrap_or_default()
    }

    pub(crate) fn vr_nav_display(&self) -> Option<Display> {
        if !self.vr_active() || self.view != "driver" {
            return None;
        }
        let p = self.player.as_ref()?;
        let def = &p.vehicle.ty.def;
        let eye = def
            .cameras_driver
            .get(def.camera_std)
            .or(def.cameras_driver.first())?;
        let placement = self.vr_nav_profile();
        Some(Display {
            placement,
            local_center: driver_origin(eye) + Vec3::from(placement.offset),
        })
    }

    pub(crate) fn vr_nav_adjust(&mut self, field: &str, direction: f32) {
        self.update_vr_nav_profile(|p| p.adjust(field, direction));
    }

    pub(crate) fn vr_nav_set(&mut self, field: &str, value: f32) {
        self.update_vr_nav_profile(|p| p.set_value(field, value));
    }

    fn update_vr_nav_profile(&mut self, update: impl FnOnce(&mut Placement)) {
        if !self.vr_active() {
            return;
        }
        let Some(player) = self.player.as_ref() else {
            return;
        };
        let key = bus_key(&player.vehicle.ty.def.path, &self.args.root);
        update(self.vr_nav_profiles.buses.entry(key).or_default());
        if self.vr_nav_edit.is_none() {
            self.save_vr_nav_profiles();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_values_keep_visibility_and_clamp_to_placement_limits() {
        let mut p = Placement::default();
        p.set_value("x", 0.42);
        p.set_value("opacity", 0.7);
        assert_eq!(p.value("x"), Some(0.42));
        assert_eq!(p.value("opacity"), Some(0.7));
        assert!(!p.enabled);
        p.set_value("width", 20.0);
        p.set_value("tilt", -100.0);
        assert_eq!(p.width, 0.65);
        assert_eq!(p.tilt, -80.0);
        let saved = p;
        p.set_value("unknown", 1.0);
        assert_eq!(p, saved);
    }

    #[test]
    fn navigator_translations_are_loaded_for_all_languages() {
        let keys = [
            "Navigator position (this bus)",
            "Position right / left",
            "Position forward / back",
            "Position up / down",
            "Display width",
            "Display rotation",
            "Display tilt",
            "Display roll",
            "Move and rotate with the mouse...",
            "Positioning navigator - changes apply to this bus",
            "Hold left mouse: move | Hold right mouse: rotate",
            "Wheel: distance | Ctrl+wheel: size | Shift+right drag: roll",
            "Esc / Enter: save and finish | R: reset position",
            "Reset navigator position",
            "VR: Toggle navigator",
            "VR: Position navigator",
            "Could not save navigator position",
        ];
        for &(_, _, language, _) in omsi_launcher_lib::LANGUAGES.iter().filter(|l| l.2 != "en") {
            for key in keys {
                let translated = crate::_rust_i18n_try_translate(language, key);
                assert!(
                    translated.as_ref().is_some_and(|text| !text.trim().is_empty() && text.as_ref() != key),
                    "Missing navigator translation: {language} / {key}"
                );
            }
        }
    }

    #[test]
    fn navigator_starts_hidden_and_reset_preserves_visibility() {
        let mut p = Placement::default();
        assert!(!p.enabled);
        p.adjust("enabled", 1.0);
        p.adjust("x", 1.0);
        p.adjust("reset", 1.0);
        assert!(p.enabled);
        assert_eq!(p.offset, Placement::default().offset);
    }

    #[test]
    fn standard_driver_camera_distance_is_respected() {
        let camera = omsi_vehicle::Camera {
            pos: [1.0, 2.0, 3.0],
            yaw: 90.0,
            pitch: 0.0,
            dist: 0.4,
            ..Default::default()
        };
        assert!(driver_origin(&camera).abs_diff_eq(Vec3::new(0.6, 2.0, 3.0), 0.0001));
    }

    fn camera(position: DVec3, yaw: f32) -> omsi_render::Camera {
        omsi_render::Camera {
            position,
            yaw,
            pitch: 0.0,
            roll: 0.0,
            fov_deg: 90.0,
            near: 0.1,
            far: 1000.0,
        }
    }

    #[test]
    fn cockpit_panel_follows_the_bus_without_drifting() {
        let display = Display {
            placement: Placement::default(),
            local_center: Vec3::new(0.3, 1.0, -0.3),
        };
        let projection = Mat4::perspective_rh(90.0_f32.to_radians(), 1.0, 1000.0, 0.1);
        let first = display.transform(
            DVec3::ZERO,
            Mat4::IDENTITY,
            &camera(DVec3::ZERO, 0.0),
            projection,
            1.2,
        );
        // Heading is clockwise, whereas the vehicle rotation uses mathematical angles.
        let position = DVec3::new(1_000_000.0, -2_000_000.0, 15.0);
        let moved = display.transform(
            position,
            Mat4::from_rotation_z(-0.7),
            &camera(position, 0.7_f32.to_degrees()),
            projection,
            1.2,
        );
        assert!(first.abs_diff_eq(moved, 0.00001));
        let looked = display.transform(
            DVec3::ZERO,
            Mat4::IDENTITY,
            &camera(DVec3::ZERO, 25.0),
            projection,
            1.2,
        );
        assert!(
            !first.abs_diff_eq(looked, 0.01),
            "the panel must not follow head rotation"
        );
    }

    #[test]
    fn stereo_panel_keeps_aspect_and_has_parallax() {
        let placement = Placement {
            yaw: 0.0,
            tilt: 0.0,
            roll: 0.0,
            ..Placement::default()
        };
        let display = Display {
            placement,
            local_center: Vec3::Y,
        };
        let projection = Mat4::perspective_rh(90.0_f32.to_radians(), 1.0, 1000.0, 0.1);
        let left = display.transform(
            DVec3::ZERO,
            Mat4::IDENTITY,
            &camera(DVec3::new(-0.032, 0.0, 0.0), 0.0),
            projection,
            1.4,
        );
        let right = display.transform(
            DVec3::ZERO,
            Mat4::IDENTITY,
            &camera(DVec3::new(0.032, 0.0, 0.0), 0.0),
            projection,
            1.4,
        );
        assert!(left.project_point3(Vec3::ZERO).x > right.project_point3(Vec3::ZERO).x);
        let width = (left.project_point3(Vec3::X) - left.project_point3(-Vec3::X)).length();
        let height = (left.project_point3(Vec3::Y) - left.project_point3(-Vec3::Y)).length();
        assert!((width / height - 1.4).abs() < 0.0001);
    }

    #[test]
    fn wheel_changes_depth_and_ctrl_wheel_only_changes_size() {
        let mut p = Placement::default();
        let original = p;
        let direction = Vec3::from(p.offset).normalize();
        p.scroll(2.0, false, Vec3::ZERO, Vec3::ZERO);
        assert!(
            (Vec3::from(original.offset).length() - Vec3::from(p.offset).length() - 0.08).abs()
                < 0.0001
        );
        assert!(direction.abs_diff_eq(Vec3::from(p.offset).normalize(), 0.0001));
        assert_eq!(p.width, original.width);
        let moved = p;
        p.scroll(2.0, true, Vec3::ZERO, Vec3::ZERO);
        assert_eq!(p.offset, moved.offset);
        assert!(p.width > moved.width);
        p.scroll(1000.0, false, Vec3::ZERO, Vec3::ZERO);
        assert!((Vec3::from(p.offset).length() - 0.25).abs() < 0.0001);
        p.scroll(f32::NAN, true, Vec3::ZERO, Vec3::ZERO);
        assert!(p.width.is_finite());
    }

    #[test]
    fn profiles_survive_replacing_the_file_and_stay_separate_per_bus() {
        let dir = std::env::temp_dir().join(format!(
            "openomsi-vr-nav-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = dir.join("profiles.json");
        let mut profiles = Profiles::default();
        let a = bus_key(Path::new("OMSI/Vehicles/Mod/Bus.bus"), Path::new("OMSI"));
        let b = bus_key(
            Path::new("OMSI/Vehicles/Payware/Bus.bus"),
            Path::new("OMSI"),
        );
        assert_ne!(a, b);
        let mut placement = Placement::default();
        placement.adjust("x", -1.0);
        profiles.buses.insert(a.clone(), placement);
        profiles.save_to(&path).unwrap();
        profiles.buses.insert(
            b.clone(),
            Placement {
                enabled: false,
                ..Placement::default()
            },
        );
        profiles.save_to(&path).unwrap();
        let restored: Profiles = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(restored.get(&a), placement);
        assert!(!restored.get(&b).enabled);
        assert_eq!(restored.get("another bus"), Placement::default());
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
}
