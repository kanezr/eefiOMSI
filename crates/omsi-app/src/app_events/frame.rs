//! The window's frame (`WindowEvent::RedrawRequested`): its stages in order in
//! `stages::WINDOW_FRAME`, taken by the runner of `frame_stages`, each a method of its own in
//! the files beside this one. What the offscreen run takes as well is in `steps`.

use super::*;
use crate::view_sync::{self, ViewSync};

mod controls;
mod driving;
mod integrations;
mod interface;
mod people;
mod present;
mod road;
pub(crate) mod stages;
pub(crate) mod steps;
mod surroundings;
mod timing;
mod view_camera;
mod view_keys;

/// The frame's time: when it started, the time since the last one, and that time as the
/// simulation steps it (at most a tenth of a second).
pub(super) struct FrameTime {
    pub(super) now: Instant,
    pub(super) raw_dt: f32,
    pub(super) dt: f32,
}

impl App {
    /// One frame of the window: its stages (`stages::WINDOW_FRAME`) in their order.
    pub(super) fn frame(&mut self, event_loop: &ActiveEventLoop) {
        let mut frame = stages::WindowFrame::new(self, event_loop);
        // (the window's stages do not fail)
        if let Err(e) = crate::frame_stages::run(&mut frame, stages::WINDOW_FRAME) {
            log::error!("frame: {e:#}");
        }
    }

    /// What the bus's scripts are told of the ground under its wheels.
    fn frame_wheel_surface(&mut self) {
        if let (Some(w), Some(p)) = (self.world.as_deref(), self.player.as_mut()) {
            crate::wheel_surface::tell_scripts(w, &mut p.vehicle);
        }
    }
}
