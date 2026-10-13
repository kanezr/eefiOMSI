//! Looking round, zooming, the mirrors' keys, the clock's keys and the free camera in the
//! window's frame.

use super::*;
use crate::app_impl::{JumpCx, ShiftClockCx};

crate::app::stage_ctx!(
    /// The view keys' part of the game.
    ViewKeysCx { reads: [net, menus, people, world], writes: [input, view, player, clock, traffic, gfx, ui] }
);

/// The keys held this frame that turn the head, aim the mirrors, run the clock, zoom
/// and fly the free camera.
pub(super) fn view_keys(mut cx: ViewKeysCx, dt: f32) {
    // looking around and zooming work in every view, not only the free camera
    crate::app_impl::sync_view_look(cx.view.cam, cx.view.view, cx.player.player.as_ref());
    if cx.player.player.is_some() && *cx.view.view != "free" {
        // looking around with the keyboard: Alt + I/J/K/L (the plain letters
        // belong to the bus - L is the headlights in Inputs/keyboard.cfg)
        let step = 60.0 * dt;
        let ctrl_alt = mirror_keys(&mut cx, dt);
        // a controller's look buttons (Settings → Controllers: view_look_*)
        cx.view.cam.look.0 += step * 1.5 * (cx.input.pad_look[1] as i32 - cx.input.pad_look[0] as i32) as f32;
        cx.view.cam.look.1 = (cx.view.cam.look.1 + step * 0.7 * (cx.input.pad_look[2] as i32 - cx.input.pad_look[3] as i32) as f32).clamp(-85.0, 85.0);
        // with a wheel steering, the arrow keys look around as in OMSI
        if !ctrl_alt && !cx.common.settings.arrows_switch_cams && cx.input.controllers.as_ref().is_some_and(|c| c.wheel_steering()) && !cx.input.keys.contains(&KeyCode::ControlLeft) && !cx.input.keys.contains(&KeyCode::ControlRight) {
            // a glance: held, the head turns (in the driver's seat to 140 degrees
            // at most, or no further than the mouse had it); let go, it comes back
            // to the road - held, it went round and round, and the other key never
            // brought it back straight
            let (l, r) = (cx.input.keys.contains(&KeyCode::ArrowLeft), cx.input.keys.contains(&KeyCode::ArrowRight));
            if l || r {
                let y = cx.view.cam.look.0 + step * 1.5 * (r as i32 - l as i32) as f32;
                cx.view.cam.look.0 = if *cx.view.view == "pax" { y } else { y.clamp(cx.view.cam.look.0.min(-140.0), cx.view.cam.look.0.max(140.0)) };
                cx.input.arrow_glance = true;
            } else if cx.input.arrow_glance {
                cx.view.cam.look.0 *= (-6.0 * dt).exp();
                // (down to a hundredth of a degree before it is set to 0: at half a
                // degree the last step was a visible snap of several pixels)
                if cx.view.cam.look.0.abs() < 0.02 {
                    cx.view.cam.look.0 = 0.0;
                    cx.input.arrow_glance = false;
                }
            }
            if cx.input.keys.contains(&KeyCode::ArrowUp) {
                cx.view.cam.look.1 = (cx.view.cam.look.1 + step * 0.7).min(85.0);
            }
            if cx.input.keys.contains(&KeyCode::ArrowDown) {
                cx.view.cam.look.1 = (cx.view.cam.look.1 - step * 0.7).max(-85.0);
            }
        }
        let alt = cx.input.keys.contains(&KeyCode::AltLeft)
            || cx.input.keys.contains(&KeyCode::AltRight);
        if alt && cx.input.keys.contains(&KeyCode::KeyJ) {
            cx.view.cam.look.0 -= step;
        }
        if alt && cx.input.keys.contains(&KeyCode::KeyL) {
            cx.view.cam.look.0 += step;
        }
        if alt && cx.input.keys.contains(&KeyCode::KeyI) {
            cx.view.cam.look.1 = (cx.view.cam.look.1 + step * 0.7).min(85.0);
        }
        if alt && cx.input.keys.contains(&KeyCode::KeyK) {
            cx.view.cam.look.1 = (cx.view.cam.look.1 - step * 0.7).max(-85.0);
        }
        if *cx.view.view != "outside" {
            cx.view.cam.look.0 = crate::input_script::cab_look_yaw(cx.view.cam.look.0);
        }
        // Ctrl+Shift+Page Up / Page Down held: the clock runs forwards / backwards,
        // a quarter of an hour per second at first, faster the longer it is held
        // (the menu's whole hours were the only way)
        {
            let ctrl = cx.input.keys.contains(&KeyCode::ControlLeft) || cx.input.keys.contains(&KeyCode::ControlRight);
            let shift = cx.input.keys.contains(&KeyCode::ShiftLeft) || cx.input.keys.contains(&KeyCode::ShiftRight);
            let dir = match (cx.input.keys.contains(&KeyCode::PageUp), cx.input.keys.contains(&KeyCode::PageDown)) {
                (true, false) => 1.0,
                (false, true) => -1.0,
                _ => 0.0,
            };
            let client = cx.net.lan.as_ref().is_some_and(|l| l.role == omsi_net::Role::Client);
            if ctrl && shift && dir != 0.0 && !client {
                if cx.input.clock_hold == 0.0 && crate::app_impl::real_time_locked(cx.common.settings, cx.net) {
                    // (says once why the clock stays)
                    crate::app_impl::shift_clock(crate::app::narrow!(cx => ShiftClockCx { reads: [net, input, world], writes: [clock, traffic, player, ui, gfx] }), dir as f64);
                }
                cx.input.clock_hold += dt;
                if !crate::app_impl::real_time_locked(cx.common.settings, cx.net) {
                    let rate = 900.0 * (1.0 + cx.input.clock_hold * 1.5).min(8.0);
                    crate::app_impl::shift_clock(crate::app::narrow!(cx => ShiftClockCx { reads: [net, input, world], writes: [clock, traffic, player, ui, gfx] }), dir * rate as f64 * dt as f64);
                }
            } else {
                if cx.input.clock_hold > 0.0 {
                    cx.input.clock_hold = 0.0;
                    crate::app_impl::timetable_after_clock_jump(crate::app::narrow!(cx => JumpCx { reads: [world], writes: [clock, traffic, gfx] }));
                }
                cx.input.clock_hold = 0.0;
            }
        }
        // = and - zoom inside the bus (the numpad's are door keys there), unless
        // the player bound them to something of their own (#701)
        let zoom_in = cx.input.keys.contains(&KeyCode::Equal) && !cx.input.own_keys.contains(&13);
        let zoom_out = cx.input.keys.contains(&KeyCode::Minus) && !cx.input.own_keys.contains(&12);
        if matches!(cx.view.view.as_str(), "driver" | "pax") {
            if zoom_in {
                crate::app_impl::zoom_view(cx.view.cam, cx.view.view, 3.0 * dt);
            }
            if zoom_out {
                crate::app_impl::zoom_view(cx.view.cam, cx.view.view, -3.0 * dt);
            }
        }
        // W/S and the wheel pull the outside camera in and out
        if *cx.view.view == "outside" {
            if zoom_in || cx.input.keys.contains(&KeyCode::NumpadAdd)
            {
                cx.view.cam.orbit = (cx.view.cam.orbit - 12.0 * dt).max(ORBIT_MIN);
            }
            if zoom_out || cx.input.keys.contains(&KeyCode::NumpadSubtract)
            {
                cx.view.cam.orbit = (cx.view.cam.orbit + 12.0 * dt).min(ORBIT_MAX);
            }
        }
        // Home held recentres the view - unless keyboard.cfg gives it a job (the
        // stock file makes it the ticket desk camera, which this then turned
        // straight ahead again whenever it was switched to, #733)
        if cx.input.keys.contains(&KeyCode::Home) && !cx.input.game_keys.iter().any(|b| Some(b.scan_code) == crate::keys::dik_code(KeyCode::Home)) {
            cx.view.cam.look = (0.0, 0.0);
            cx.view.cam.orbit = ORBIT_DEFAULT;
            cx.view.cam.view_zoom.remove(cx.view.view.as_str());
            cx.view.cam.f1_reset = None;
        }
    }
    if *cx.view.view != "free" {
        cx.view.cam.ego = false;
    }
    free_camera(&mut cx, dt);
}

/// The mirror editor's keys and Ctrl+Alt+arrows: the mirrors aimed, shifted and zoomed
/// (and kept per bus when the keys are let go). Whether Ctrl+Alt are held.
fn mirror_keys(cx: &mut ViewKeysCx, dt: f32) -> bool {
    // Ctrl+Alt+arrows in the cab: the mirror nearest to where the driver looks
    // turns (kept per bus in mirrors.cfg when the keys are let go)
    // the mirror editor: an arrow held over a panel aims that panel's mirror
    // (kept per bus like Ctrl+Alt+arrows below)
    if let (Some(size), Some(a)) =
        (crate::app_events::mirror_hud_size(cx.common.settings, cx.view.cam, cx.menus.menus, cx.gfx.gfx), cx.gfx.gfx.mirror_hud.turning())
    {
        if let (Some(i), Some(p)) = (
            cx.gfx.gfx.mirror_hud.cam_under(crate::app_impl::hud_cursor(cx.common.settings, cx.gfx.gfx, cx.input), size),
            cx.player.player.as_mut(),
        ) {
            let n = p.vehicle.ty.def.cameras_reflexion.len();
            if p.mirror_offsets.len() < n {
                p.mirror_offsets.resize(n, [0.0; 2]);
            }
            if p.mirror_shifts.len() < n {
                p.mirror_shifts.resize(n, [0.0; 3]);
            }
            // Alt+arrows and Page Up/Down shift the mirror (0.2 m a second, at most
            // 0.6 m across and up, a metre along), the plain arrows aim it
            if p.mirror_fovs.len() < n {
                p.mirror_fovs.resize(n, 0.0);
            }
            let alt = cx.input.keys.contains(&KeyCode::AltLeft) || cx.input.keys.contains(&KeyCode::AltRight);
            let along = (a[4] as i32 - a[5] as i32) as f32;
            let zoom = (a[7] as i32 - a[6] as i32) as f32;
            if zoom != 0.0 {
                // - and + narrow and widen the mirror's field of view (20° a second)
                if let Some(f) = p.mirror_fovs.get_mut(i) {
                    *f = (*f + 20.0 * dt * zoom).clamp(-60.0, 60.0);
                    p.mirrors_dirty = true;
                    let base = p.vehicle.ty.def.cameras_reflexion.get(i).map(|c| if c.fov > 1.0 { c.fov } else { 50.0 }).unwrap_or(50.0);
                    *cx.ui.service_msg = Some((format!("Mirror {}: field of view {:.0}° (the bus's {:.0}°)", i + 1, (base + *f).clamp(8.0, 110.0), base), 2.0));
                }
            } else if alt || along != 0.0 {
                let metres = 0.2 * dt;
                if let Some(s) = p.mirror_shifts.get_mut(i) {
                    if alt {
                        s[0] = (s[0] + metres * (a[1] as i32 - a[0] as i32) as f32).clamp(-0.6, 0.6);
                        s[2] = (s[2] + metres * (a[2] as i32 - a[3] as i32) as f32).clamp(-0.6, 0.6);
                    }
                    s[1] = (s[1] + metres * along).clamp(-1.0, 1.0);
                    p.mirrors_dirty = true;
                    *cx.ui.service_msg = Some((format!("Mirror {} shifted {:+.2} m across, {:+.2} m forward, {:+.2} m up", i + 1, s[0], s[1], s[2]), 2.0));
                }
            } else if let Some(o) = p.mirror_offsets.get_mut(i) {
                let rate = 12.0 * dt;
                o[0] = (o[0] + rate * (a[1] as i32 - a[0] as i32) as f32).clamp(-45.0, 45.0);
                o[1] = (o[1] + rate * (a[2] as i32 - a[3] as i32) as f32).clamp(-30.0, 30.0);
                p.mirrors_dirty = true;
                *cx.ui.service_msg = Some((format!("Mirror {}: {:+.1}° across, {:+.1}° up", i + 1, o[0], o[1]), 2.0));
            }
        }
    }
    let ctrl_alt = (cx.input.keys.contains(&KeyCode::ControlLeft) || cx.input.keys.contains(&KeyCode::ControlRight)) && (cx.input.keys.contains(&KeyCode::AltLeft) || cx.input.keys.contains(&KeyCode::AltRight));
    let arrows = [KeyCode::ArrowLeft, KeyCode::ArrowRight, KeyCode::ArrowUp, KeyCode::ArrowDown].map(|k| cx.input.keys.contains(&k));
    if let (true, Some(p), Some(cam)) = (ctrl_alt && *cx.view.view == "driver" && arrows.iter().any(|a| *a), cx.player.player.as_mut(), cx.view.camera.as_ref()) {
        let cams = &p.vehicle.ty.def.cameras_reflexion;
        let f = cam.forward();
        let best = (0..cams.len())
            .map(|i| (i, (p.vehicle.camera_world_full(&cams[i]).0 - cam.position).as_vec3().normalize_or_zero().dot(f)))
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i);
        if let Some(i) = best {
            if p.mirror_offsets.len() <= i {
                p.mirror_offsets.resize(cams.len(), [0.0; 2]);
            }
            let o = &mut p.mirror_offsets[i];
            let rate = 12.0 * dt;
            o[0] = (o[0] + rate * (arrows[1] as i32 - arrows[0] as i32) as f32).clamp(-45.0, 45.0);
            o[1] = (o[1] + rate * (arrows[2] as i32 - arrows[3] as i32) as f32).clamp(-30.0, 30.0);
            p.mirrors_dirty = true;
            *cx.ui.service_msg = Some((format!("Mirror {}: {:+.1}° across, {:+.1}° up (Ctrl+Alt+arrows)", i + 1, o[0], o[1]), 2.0));
        }
    } else if let Some(p) = cx.player.player.as_mut().filter(|p| p.mirrors_dirty) {
        p.mirrors_dirty = false;
        crate::settings::save_mirror_state(&p.vehicle.ty.def.path, &p.mirror_offsets, &p.mirror_shifts, &p.mirror_fovs);
    }
    ctrl_alt
}

/// The free camera's flight (and the walk with `ego`).
fn free_camera(cx: &mut ViewKeysCx, dt: f32) {
    // (the free camera flies; with no bus it is the view too - but not out of the
    // walker's eyes: on foot without a bus of one's own (started on foot, the bus
    // removed) the keys flew the camera on from where the walk had put it every
    // frame, and walking jumped about, the more so the lower the frame rate, #807)
    if let (Some(cam), true) = (
        cx.view.camera.as_mut(),
        *cx.view.view == "free" || (cx.player.player.is_none() && cx.people.on_foot.is_none()),
    ) {
        // the free camera keeps the horizon level: taken over from the driver's view
        // of a bus leaning on a slope, it went on with the bus's roll (#2220)
        if cx.ui.photo.is_none() {
            cam.roll = 0.0;
        }
        let mut v = Vec3::ZERO;
        let f = cam.forward();
        let r = cam.right();
        if cx.input.keys.contains(&KeyCode::KeyW) {
            v += f;
        }
        if cx.input.keys.contains(&KeyCode::KeyS) {
            v -= f;
        }
        if cx.input.keys.contains(&KeyCode::KeyD) {
            v += r;
        }
        if cx.input.keys.contains(&KeyCode::KeyA) {
            v -= r;
        }
        if cx.input.keys.contains(&KeyCode::KeyE) || cx.input.keys.contains(&KeyCode::Space) {
            v += Vec3::Z;
        }
        if cx.input.keys.contains(&KeyCode::KeyQ) {
            v -= Vec3::Z;
        }
        let boost = if cx.input.keys.contains(&KeyCode::ShiftLeft) {
            5.0
        } else {
            1.0
        };
        if cx.view.cam.ego {
            // walking: along the ground at eye height, 1.4 m/s (running 4.5)
            let flat = Vec3::new(v.x, v.y, 0.0).normalize_or_zero();
            let pace = if boost > 1.0 { 4.5 } else { 1.4 };
            cam.position += (flat * pace * dt).as_dvec3();
            if let Some(g) = cx.world.world.as_ref().and_then(|w| w.walk_height(cam.position.x, cam.position.y)) {
                cam.position.z = g + 1.7;
            }
        } else {
            cam.position += (v.normalize_or_zero() * cx.view.cam.speed * boost * dt).as_dvec3();
        }
        if cx.input.keys.contains(&KeyCode::ArrowLeft) {
            cam.yaw -= 60.0 * dt;
        }
        if cx.input.keys.contains(&KeyCode::ArrowRight) {
            cam.yaw += 60.0 * dt;
        }
        if cx.input.keys.contains(&KeyCode::ArrowUp) {
            cam.pitch = (cam.pitch + 40.0 * dt).min(89.0);
        }
        if cx.input.keys.contains(&KeyCode::ArrowDown) {
            cam.pitch = (cam.pitch - 40.0 * dt).max(-89.0);
        }
    }
}
