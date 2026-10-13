use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::fd::AsRawFd;
use std::time::{Duration, Instant};

const EV_FF: u16 = 0x15;
const FF_CONSTANT: usize = 0x52;
const FF_GAIN: u16 = 0x60;
const EVIOCSFF: libc::c_ulong = 0x4030_4580;
const EVIOCRMFF: libc::c_ulong = 0x4004_4581;

#[repr(C)]
#[derive(Default)]
struct FfEffect {
    kind: u16,
    id: i16,
    direction: u16,
    trigger: [u16; 2],
    replay: [u16; 2],
    params: [u64; 4],
}

#[repr(C)]
struct InputEvent {
    time: libc::timeval,
    kind: u16,
    code: u16,
    value: i32,
}

pub struct Wheel {
    pub name: String,
    file: File,
    id: i16,
    level: Option<i16>,
    sent: Instant,
    smoothed: f32,
}

impl Wheel {
    pub fn open(name: &str) -> Option<Wheel> {
        let dir = std::fs::read_dir("/sys/class/input").ok()?;
        let mut nodes: Vec<String> = dir.filter_map(|e| e.ok()?.file_name().into_string().ok()).filter(|n| n.starts_with("event")).collect();
        nodes.sort();
        for node in nodes {
            let sys = format!("/sys/class/input/{node}/device");
            let Ok(dev_name) = std::fs::read_to_string(format!("{sys}/name")) else { continue };
            if !crate::controllers::names_match(dev_name.trim(), name) {
                continue;
            }
            let caps = std::fs::read_to_string(format!("{sys}/capabilities/ff")).unwrap_or_default();
            if !has_bit(&caps, FF_CONSTANT) {
                continue;
            }
            let file = match OpenOptions::new().read(true).write(true).open(format!("/dev/input/{node}")) {
                Ok(f) => f,
                Err(e) => {
                    log::warn!("force feedback: {name} (/dev/input/{node}) cannot be opened for writing: {e}");
                    continue;
                }
            };
            let mut wheel = Wheel { name: name.to_string(), file, id: -1, level: None, sent: Instant::now() - Duration::from_secs(1), smoothed: 0.0 };
            wheel.send(FF_GAIN, 0xFFFF);
            if wheel.upload(0) {
                wheel.send(wheel.id as u16, 1);
                log::info!("force feedback: {name} on /dev/input/{node} (constant force)");
                return Some(wheel);
            }
            log::warn!("force feedback: {name} (/dev/input/{node}) refused the constant force effect");
        }
        None
    }

    pub fn set_force(&mut self, f: f32) -> bool {
        let target = f.clamp(-1.0, 1.0);
        let elapsed = self.sent.elapsed();
        if elapsed < Duration::from_millis(10) {
            return true;
        }

        self.smoothed = force_step(self.smoothed, target, elapsed.as_secs_f32(), true);

        let level = (-self.smoothed * i16::MAX as f32) as i16;
        if self.level == Some(level) {
            return true;
        }
        self.upload(level) || std::io::Error::last_os_error().raw_os_error() != Some(libc::ENODEV)
    }

    /// Stop the effect immediately when feedback is disabled. A zero force from
    /// the active steering calculation still passes through the 25 ms smoother.
    pub fn stop_force(&mut self) -> bool {
        self.smoothed = force_step(self.smoothed, 0.0, 0.0, false);
        if self.level == Some(0) {
            return true;
        }
        self.upload(0) || std::io::Error::last_os_error().raw_os_error() != Some(libc::ENODEV)
    }

    pub fn pulse_force(&mut self, force: f32) -> bool {
        let limit = crate::ffb_calibration::MAX_PULSE_FORCE;
        let level = (-force.clamp(-limit, limit) * i16::MAX as f32) as i16;
        if !self.upload_for(level, crate::ffb_calibration::PULSE_MS as u16) {
            return false;
        }
        self.send(self.id as u16, 1)
    }

    fn upload(&mut self, level: i16) -> bool {
        self.upload_for(level, 0)
    }

    fn upload_for(&mut self, level: i16, milliseconds: u16) -> bool {
        let mut effect = FfEffect { kind: FF_CONSTANT as u16, id: self.id, direction: 0x4000, ..Default::default() };
        effect.replay[0] = milliseconds;
        effect.params[0] = level as u16 as u64;
        let r = unsafe { libc::ioctl(self.file.as_raw_fd(), EVIOCSFF as _, &mut effect as *mut FfEffect) };
        self.sent = Instant::now();
        if r < 0 {
            return false;
        }
        self.id = effect.id;
        self.level = Some(level);
        true
    }

    fn send(&mut self, code: u16, value: i32) -> bool {
        let ev = InputEvent { time: libc::timeval { tv_sec: 0, tv_usec: 0 }, kind: EV_FF, code, value };
        let bytes = unsafe { std::slice::from_raw_parts(&ev as *const InputEvent as *const u8, std::mem::size_of::<InputEvent>()) };
        self.file.write_all(bytes).is_ok()
    }

    fn force_step(current: f32, target: f32, dt: f32, enabled: bool) -> f32 {
        if enabled { crate::controllers::smooth_toward(current, target, dt, 0.025) } else { 0.0 }
    }
}

impl Drop for Wheel {
    fn drop(&mut self) {
        if self.id >= 0 {
            self.send(self.id as u16, 0);
            unsafe { libc::ioctl(self.file.as_raw_fd(), EVIOCRMFF as _, self.id as libc::c_int) };
        }
    }
}

fn constant_force_device(name: Option<&str>) -> bool {
    let Ok(dir) = std::fs::read_dir("/sys/class/input") else { return false };
    dir.filter_map(|e| e.ok()).filter(|e| e.file_name().to_string_lossy().starts_with("event")).any(|e| {
        let device = e.path().join("device");
        if let Some(name) = name {
            let Ok(device_name) = std::fs::read_to_string(device.join("name")) else { return false };
            if !crate::controllers::names_match(device_name.trim(), name) {
                return false;
            }
        }
        let caps = std::fs::read_to_string(device.join("capabilities/ff")).unwrap_or_default();
        has_bit(&caps, FF_CONSTANT)
    })
}

/// A device with a constant force is connected (a wheel: openOMSI drives its forces itself).
pub fn wheel_connected() -> bool {
    constant_force_device(None)
}

/// This gilrs device is also an evdev constant-force wheel.
/// SDL mappings can describe wheels such as the G29 as gamepads; the kernel capability is
/// authoritative for openOMSI's steering semantics and native force-feedback path.
pub fn wheel_named(name: &str) -> bool {
    constant_force_device(Some(name))
}

fn has_bit(bitmap: &str, bit: usize) -> bool {
    let words: Vec<u64> = bitmap.split_whitespace().rev().filter_map(|w| u64::from_str_radix(w, 16).ok()).collect();
    words.get(bit / 64).is_some_and(|w| w >> (bit % 64) & 1 != 0)
}

#[cfg(test)]
mod tests {
    #[test]
    fn active_force_crossing_zero_is_smoothed_but_disable_can_stop_now() {
        let from_positive = super::force_step(0.5, 0.0, 0.01, true);
        assert!(from_positive > 0.0 && from_positive < 0.5);
        let crossing = super::force_step(0.5, -0.5, 0.01, true);
        assert!(crossing > 0.0 && crossing < 0.5);
        // The explicit disabled path resets its stored force synchronously.
        let stopped = super::force_step(0.5, 0.0, 0.01, false);
        assert_eq!(stopped, 0.0);
    }

    #[test]
    fn zero_force_smoothing_is_time_based_at_different_update_rates() {
        let mut at_50_hz: f32 = 0.8;
        for _ in 0..5 { at_50_hz = super::force_step(at_50_hz, 0.0, 0.02, true); }
        let mut at_100_hz: f32 = 0.8;
        for _ in 0..10 { at_100_hz = super::force_step(at_100_hz, 0.0, 0.01, true); }
        assert!((at_50_hz - at_100_hz).abs() < 1e-6);
        assert!(at_50_hz > 0.0 && at_50_hz < 0.02);
    }
    
    #[test]
    fn layouts_match_the_kernel() {
        assert_eq!(std::mem::size_of::<super::FfEffect>(), 48);
        assert_eq!(std::mem::size_of::<super::InputEvent>(), 24);
    }

    #[test]
    fn ff_bits() {
        assert!(super::has_bit("11fff0000 0", 0x52));
        assert!(!super::has_bit("11fff0000 0", 0x61));
        assert!(!super::has_bit("0", 0x52));
    }
}
