//! A small software mixer on top of cpal: voices with per-sample linear resampling (pitch),
//! gain, looping and Omsi.exe's 3D model (inverse-distance attenuation beyond a `[3d]`
//! sound's range, a pan of a few dB - see [`pan_gains`]).

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use glam::Vec3;
use hashbrown::HashMap;
use parking_lot::Mutex;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

pub struct Clip {
    pub sample_rate: u32,
    pub channels: u16,
    /// Interleaved 16-bit samples (see `wav::WavData`).
    pub samples: Vec<i16>,
}

impl Clip {
    pub fn frames(&self) -> usize {
        self.samples.len() / self.channels as usize
    }
}

pub type VoiceId = u64;

#[derive(Debug, Clone, Copy)]
pub struct VoiceParams {
    pub gain: f32,
    /// Playback speed relative to the clip's own rate.
    pub pitch: f32,
    pub looping: bool,
    /// World position; `None` = non-spatial.
    pub position: Option<Vec3>,
    /// Whether motion relative to the listener changes playback pitch.
    pub doppler: bool,
    /// Distance of full volume for spatial voices.
    pub range: f32,
    /// A one-pole low-pass cutoff in Hz, or 0.0 for none: a sound heard through the
    /// bodywork from the cabin loses its edge, not just some volume (a straight gain cut
    /// still reads as "the same sound, turned down" rather than "coming from outside").
    pub lowpass_hz: f32,
    /// OMSI's `[important]`: keep this sound ahead of ordinary voices when the
    /// mixer limit is reached.
    pub important: bool,
}

impl Default for VoiceParams {
    fn default() -> Self {
        Self {
            gain: 1.0,
            pitch: 1.0,
            looping: false,
            position: None,
            doppler: true,
            range: 5.0,
            lowpass_hz: 0.0,
            important: false,
        }
    }
}

struct Voice {
    id: VoiceId,
    clip: Arc<Clip>,
    /// A voice fed while it plays (internet radio) instead of from `clip`.
    stream: Option<Arc<crate::stream::StreamBuf>>,
    params: VoiceParams,
    pos: f64,
    finished: bool,
    /// Smoothed gain to avoid clicks.
    cur_gain: f32,
    /// One-pole low-pass filter state, left/right.
    lp: [f32; 2],
    /// The low-pass was just switched on: it starts from the sound as it is (from the
    /// state it was left in a while ago, a voice coming into the cab clicked).
    lp_fresh: bool,
    /// The bus radio's effects on a stream (see `radio_fx`); None plays it as it is.
    fx: Option<Box<crate::radio_fx::RadioFx>>,
    /// The Doppler shift: the distance to the listener when the position last came, when,
    /// and the (smoothed) pitch factor it gives.
    doppler: (f32, Option<std::time::Instant>, f32),
}

/// OMSI's `sound_doppler`: a sound coming closer is higher, one going away lower.
pub static DOPPLER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

#[derive(Debug, Clone, Copy)]
pub struct Listener {
    pub position: Vec3,
    pub forward: Vec3,
    pub right: Vec3,
    pub master: f32,
    /// Echo of the place the listener is in (`[triggerbox_setreverb]`: under a bridge):
    /// its reverberation time in seconds, and how much of it is heard (0..1).
    pub reverb_time: f32,
    pub reverb_mix: f32,
}

impl Default for Listener {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            forward: Vec3::Y,
            right: Vec3::X,
            master: 1.0,
            reverb_time: 0.0,
            reverb_mix: 0.0,
        }
    }
}

/// What the game asks of the mixer, taken in at the start of its next block.
enum Cmd {
    Play(Box<Voice>),
    /// New parameters, with the time they were given (for the Doppler shift).
    Params(VoiceId, VoiceParams, std::time::Instant),
    Stop(VoiceId),
    /// The bus radio's effects on a stream voice, with new ones made ready in case it has none.
    RadioFx(VoiceId, crate::radio_fx::RadioFxParams, Option<Box<crate::radio_fx::RadioFx>>),
}

struct Shared {
    /// The voices as the mixer plays them, by rising id (each is put in by the queue, where
    /// its id is drawn). Only the mixer takes this lock while it plays: it holds it for a
    /// whole block.
    voices: Mutex<Vec<Voice>>,
    /// New voices, parameters and stops, taken in by the mixer at the start of its next
    /// block. The game sets every voice's parameters every frame; through the voice list's
    /// lock each of those calls waited for a whole block to be mixed.
    updates: Mutex<Vec<Cmd>>,
    /// The mixer's half of `updates` (swapped with it each block, so that neither list is
    /// grown again every block).
    taken: Mutex<Vec<Cmd>>,
    /// The voices playing, with the parameters they play with: what the game asks (whether
    /// a voice still plays, how loud it is - thousands of times a frame) without waiting for
    /// a block being mixed. Started and stopped voices go in and out at once; the mixer
    /// takes out those that ended by themselves and puts in the parameters it took in.
    live: parking_lot::RwLock<HashMap<VoiceId, VoiceParams>>,
    listener: Mutex<Listener>,
    /// The echo of an underpass, fed from the mix.
    reverb: Mutex<Reverb>,
    /// The master limiter's gain now (1 = none).
    limiter: Mutex<f32>,
    /// The output device's rate and channels: those of the device played on now (the stream
    /// is opened again on another device when the system's output changes).
    sample_rate: AtomicU32,
    channels: AtomicUsize,
    /// `OMSI_MUTE`: everything is mixed as usual (voices play and end), nothing is heard -
    /// for test runs on a machine somebody is working at.
    muted: bool,
    /// A recording run's clock (see `AudioEngine::capture`): microseconds of mixed sound,
    /// counted from `epoch`, stand in for the wall clock - the Doppler shift is worked out
    /// from how far a source moved between two parameter updates, and a run that renders
    /// slower than real time would otherwise hear every car standing still.
    capture_us: Option<AtomicU64>,
    epoch: std::time::Instant,
}

impl Shared {
    fn new(rate: u32, channels: usize, muted: bool, capture: bool) -> Shared {
        Shared {
            voices: Mutex::new(Vec::new()),
            updates: Mutex::new(Vec::new()),
            taken: Mutex::new(Vec::new()),
            live: Default::default(),
            listener: Mutex::new(Listener::default()),
            reverb: Mutex::new(Reverb::default()),
            limiter: Mutex::new(1.0),
            sample_rate: AtomicU32::new(rate),
            channels: AtomicUsize::new(channels),
            muted,
            capture_us: capture.then(|| AtomicU64::new(0)),
            epoch: std::time::Instant::now(),
        }
    }

    /// The time parameters are stamped with: the wall clock, or a recording's own.
    fn now(&self) -> std::time::Instant {
        match &self.capture_us {
            Some(us) => self.epoch + std::time::Duration::from_micros(us.load(Ordering::Relaxed)),
            None => std::time::Instant::now(),
        }
    }

}

impl Shared {
    fn render(&self, out: &mut [f32]) {
        for s in out.iter_mut() {
            *s = 0.0;
        }
        let listener = *self.listener.lock();
        let mut taken = self.taken.lock();
        std::mem::swap(&mut *taken, &mut *self.updates.lock());
        let mut voices = self.voices.lock();
        if !taken.is_empty() {
            self.take_in(&mut voices, &mut taken, listener.position);
        }
        drop(taken);
        let ch = self.channels.load(Ordering::Relaxed).max(1);
        let frames = out.len() / ch;
        let rate = self.sample_rate.load(Ordering::Relaxed).max(1);
        let dev_rate = rate as f64;
        // More voices than OMSI's `[sound_maxcount]` (200 by default): as Omsi.exe, the
        // nearest sounds are kept and the far ones cut - by distance, which does not change
        // from one block to the next, not by loudness, which follows every volume curve and
        // cut voices in and out at the edge of the list (sounds breaking off in traffic).
        // `[important]` ones and the non-spatial ones (the driven bus heard from inside, the
        // interface) are never cut. Voices left out still advance in time, so a loop comes
        // back at the right phase.
        let mixed: Option<Vec<bool>> = if voices.iter().filter(|v| !v.finished && v.stream.is_none()).count() > MAX_VOICES {
            let mut ranked: Vec<(bool, f32, usize)> = voices
                .iter()
                .enumerate()
                .filter(|(_, v)| !v.finished && v.stream.is_none())
                .map(|(i, v)| {
                    let near = v.params.position.map_or(0.0, |p| (p - listener.position).length());
                    (v.params.important || v.params.position.is_none(), -near, i)
                })
                .collect();
            ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.total_cmp(&a.1)));
            let mut keep = vec![false; voices.len()];
            for (_, _, i) in ranked.into_iter().take(MAX_VOICES) {
                keep[i] = true;
            }
            Some(keep)
        } else {
            None
        };
        for (i, v) in voices.iter_mut().enumerate() {
            if v.finished {
                continue;
            }
            if v.stream.is_none() && mixed.as_ref().is_some_and(|keep| !keep[i]) {
                skip_clip(v, frames, dev_rate);
                continue;
            }
            // spatialisation
            let (mut left, mut right) = (1.0f32, 1.0f32);
            let mut spatial_gain = 1.0f32;
            if let Some(p) = v.params.position {
                let d = p - listener.position;
                spatial_gain = distance_gain(v.params.range, d.length());
                (left, right) = pan_gains(&listener, d);
            }
            let target_gain = (v.params.gain * spatial_gain * listener.master).max(0.0);
            if let Some(sb) = v.stream.clone() {
                let lp_alpha = if v.params.lowpass_hz > 0.0 {
                    1.0 - (-2.0 * std::f32::consts::PI * v.params.lowpass_hz / dev_rate as f32).exp()
                } else {
                    1.0
                };
                let mut buf = sb.lock();
                if buf.closed {
                    v.finished = true;
                    continue;
                }
                let step = buf.rate as f64 / dev_rate;
                if let Some(fx) = v.fx.as_mut() {
                    fx.set(fx.params(), rate);
                }
                for f in 0..frames {
                    v.cur_gain += (target_gain - v.cur_gain) * 0.005;
                    // silence while the buffer fills (at the start, after a stall)
                    let Some((a, b)) = buf.pair() else { break };
                    let t = v.pos as f32;
                    let (mut l, mut r) = (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t);
                    if v.params.lowpass_hz > 0.0 {
                        if std::mem::take(&mut v.lp_fresh) {
                            v.lp = [l, r];
                        }
                        v.lp[0] += (l - v.lp[0]) * lp_alpha;
                        v.lp[1] += (r - v.lp[1]) * lp_alpha;
                        (l, r) = (v.lp[0], v.lp[1]);
                    }
                    if let Some(fx) = v.fx.as_mut() {
                        (l, r) = fx.process(l, r);
                    }
                    let g = v.cur_gain;
                    out[f * ch] += l * g * left;
                    if ch > 1 {
                        out[f * ch + 1] += r * g * right;
                    }
                    v.pos += step;
                    while v.pos >= 1.0 {
                        v.pos -= 1.0;
                        buf.advance();
                    }
                }
                continue;
            }
            let clip = &v.clip;
            let cch = clip.channels as usize;
            let nframes = clip.frames();
            if nframes == 0 {
                v.finished = true;
                continue;
            }
            let step = (v.params.pitch * v.doppler.2).max(0.01) as f64 * clip.sample_rate as f64 / dev_rate;
            // one-pole low-pass: alpha such that the filter's -3dB point sits at `lowpass_hz`
            let lp_alpha = if v.params.lowpass_hz > 0.0 {
                1.0 - (-2.0 * std::f32::consts::PI * v.params.lowpass_hz / dev_rate as f32).exp()
            } else {
                1.0
            };
            for f in 0..frames {
                // smooth gain over ~5 ms
                v.cur_gain += (target_gain - v.cur_gain) * 0.005;
                let mut i0 = v.pos as usize;
                if i0 >= nframes {
                    if !v.params.looping {
                        v.finished = true;
                        break;
                    }
                    // wrap and mix this output frame from the loop's start (skipping it left
                    // a silent frame at every turn of the loop: a click on every engine loop)
                    v.pos %= nframes as f64;
                    i0 = (v.pos as usize).min(nframes - 1);
                }
                let i1 = if i0 + 1 < nframes {
                    i0 + 1
                } else if v.params.looping {
                    0
                } else {
                    i0
                };
                let t = (v.pos - i0 as f64) as f32;
                let sample = |c: usize| {
                    let a = clip.samples[i0 * cch + c.min(cch - 1)] as f32 / 32768.0;
                    let b = clip.samples[i1 * cch + c.min(cch - 1)] as f32 / 32768.0;
                    a + (b - a) * t
                };
                let (l, r) = if cch >= 2 {
                    (sample(0), sample(1))
                } else {
                    let m = sample(0);
                    (m, m)
                };
                let (l, r) = if v.params.lowpass_hz > 0.0 {
                    if std::mem::take(&mut v.lp_fresh) {
                        v.lp = [l, r];
                    }
                    v.lp[0] += (l - v.lp[0]) * lp_alpha;
                    v.lp[1] += (r - v.lp[1]) * lp_alpha;
                    (v.lp[0], v.lp[1])
                } else {
                    (l, r)
                };
                let g = v.cur_gain;
                out[f * ch] += l * g * left;
                if ch > 1 {
                    out[f * ch + 1] += r * g * right;
                }
                v.pos += step;
            }
        }
        if voices.iter().any(|v| v.finished) {
            if crate::trace::on() {
                let ended: Vec<VoiceId> = voices.iter().filter(|v| v.finished).map(|v| v.id).collect();
                crate::trace::ended(&ended);
            }
            let mut live = self.live.write();
            for v in voices.iter().filter(|v| v.finished) {
                live.remove(&v.id);
            }
            drop(live);
            voices.retain(|v| !v.finished);
        }
        drop(voices);
        if listener.reverb_mix > 0.001 && listener.reverb_time > 0.05 {
            self.reverb.lock().process(out, ch, rate, listener.reverb_time.min(3.0), listener.reverb_mix.min(1.0));
        }
        // The master limiter: a busy street sums past full scale, and cut off hard there the
        // sound crackled and squeaked. Loud moments are turned down (at once) and back up
        // (over half a second), and what still peaks is rounded off, not cut.
        {
            let mut g = self.limiter.lock();
            let frames = (out.len() / ch.max(1)).max(1);
            let release = (-1.0 / (0.5 * rate as f32)).exp();
            for f in 0..frames {
                let peak = (0..ch).map(|c| out[f * ch + c].abs()).fold(0.0f32, f32::max);
                let want = if peak * *g > 0.9 { 0.9 / peak } else { 1.0 };
                *g = if want < *g { want } else { want + (*g - want) * release };
                for c in 0..ch {
                    out[f * ch + c] = soft_clip(out[f * ch + c] * *g);
                }
            }
        }
        for s in out.iter_mut() {
            *s = if self.muted { 0.0 } else { s.clamp(-1.0, 1.0) };
        }
    }
}

impl Shared {
    /// The game's commands, in the order given, applied to `voices`; the parameters taken
    /// in published to `live`.
    fn take_in(&self, voices: &mut Vec<Voice>, cmds: &mut Vec<Cmd>, listener: Vec3) {
        // (by rising id: a binary search; a voice put in some other way is still found)
        fn find(voices: &[Voice], id: VoiceId) -> Option<usize> {
            voices.binary_search_by_key(&id, |v| v.id).ok().or_else(|| voices.iter().position(|v| v.id == id))
        }
        let mut live = None;
        for cmd in cmds.drain(..) {
            match cmd {
                Cmd::Play(v) => voices.push(*v),
                Cmd::Params(id, params, at) => {
                    if let Some(k) = find(voices, id) {
                        apply_params(&mut voices[k], params, at, listener);
                        if let Some(p) = live.get_or_insert_with(|| self.live.write()).get_mut(&id) {
                            *p = params;
                        }
                    }
                }
                Cmd::Stop(id) => {
                    if let Some(k) = find(voices, id) {
                        voices[k].finished = true;
                    }
                }
                Cmd::RadioFx(id, params, fresh) => {
                    if let Some(k) = find(voices, id) {
                        let v = &mut voices[k];
                        match (v.fx.as_mut(), params.is_off()) {
                            (_, true) => v.fx = None,
                            (Some(fx), false) => fx.set(params, self.sample_rate.load(Ordering::Relaxed)),
                            (None, false) => v.fx = fresh,
                        }
                    }
                }
            }
        }
    }

    /// A command for the mixer: queued for its next block, or (with no mixer taking them
    /// in: an engine without a device) applied to the voices at once.
    fn send(&self, direct: bool, cmd: Cmd) {
        if direct {
            let listener = self.listener.lock().position;
            let mut one = vec![cmd];
            self.take_in(&mut self.voices.lock(), &mut one, listener);
            return;
        }
        let mut q = self.updates.lock();
        // (a game paused while the device stopped calling back must not pile up parameters;
        // new voices and stops are kept)
        if q.len() > 4096 {
            q.retain(|c| !matches!(c, Cmd::Params(..)));
        }
        q.push(cmd);
    }
}

/// Parameters for voice `v`, given at `now` with the listener at `listener`: the Doppler
/// shift from how fast the distance to the listener changes (the bus's own sounds move with
/// the listener and keep their pitch).
fn apply_params(v: &mut Voice, params: VoiceParams, now: std::time::Instant, listener: Vec3) {
    if let (Some(p), true) = (params.position, params.doppler && DOPPLER.load(Ordering::Relaxed)) {
        let dist = (p - listener).length();
        let (last, at, factor) = v.doppler;
        let mut f = factor;
        if let Some(at) = at {
            let dt = now.saturating_duration_since(at).as_secs_f32();
            if (0.004..0.5).contains(&dt) {
                let radial = ((dist - last) / dt).clamp(-60.0, 60.0);
                let target = 343.0 / (343.0 + radial);
                f += (target - f) * (dt / 0.25).min(1.0);
            }
        }
        v.doppler = (dist, Some(now), f);
    } else {
        v.doppler = (0.0, None, 1.0);
    }
    if params.lowpass_hz > 0.0 && !(v.params.lowpass_hz > 0.0) {
        v.lp_fresh = true;
    }
    v.params = params;
}

/// At most this many clip voices are mixed at once (OMSI's `[sound_maxcount]` default).
pub const MAX_VOICES: usize = 200;

/// How loud voice `v` reaches the listener (its gain and distance), to rank voices by.

/// Move a clip voice on by `frames` output frames without mixing it (looping or ending as
/// it would have).
fn skip_clip(v: &mut Voice, frames: usize, dev_rate: f64) {
    let nframes = v.clip.frames();
    if nframes == 0 {
        v.finished = true;
        return;
    }
    let step = (v.params.pitch * v.doppler.2).max(0.01) as f64 * v.clip.sample_rate as f64 / dev_rate;
    v.pos += step * frames as f64;
    v.cur_gain = 0.0;
    if v.pos >= nframes as f64 {
        if v.params.looping {
            v.pos %= nframes as f64;
        } else {
            v.finished = true;
        }
    }
}

/// Owns the output stream. Dropping it stops playback.
pub struct AudioEngine {
    /// The stream on the device played on now (see `follow_device`).
    stream: std::cell::RefCell<Option<cpal::Stream>>,
    /// The name of that device.
    device: std::cell::RefCell<String>,
    /// Set when the stream fails (its device went away) or the system's default output
    /// changed (a watcher thread looks every two seconds).
    reopen: Arc<AtomicBool>,
    /// When the stream was last opened (at most one new stream a second).
    opened: std::cell::Cell<std::time::Instant>,
    shared: Arc<Shared>,
    next_id: AtomicU64,
    /// Clips by file; `None` for a file that is missing or unreadable (not tried again). With
    /// when each was last asked for (see `trim_clips`).
    clips: Arc<Mutex<HashMap<PathBuf, (Option<Arc<Clip>>, std::time::Instant)>>>,
    last_trim: Mutex<std::time::Instant>,
    /// Files a background reader is working on (see `clips_ready`).
    loading: Arc<Mutex<hashbrown::HashSet<PathBuf>>>,
    pub enabled: bool,
    /// Mixed on demand, for a recording (see [`AudioEngine::capture`]).
    capture: bool,
    /// Mixed never (see [`AudioEngine::silent`]): the voices are kept as they are told.
    silent: bool,
}

fn muted() -> bool {
    omsi_cfg::flags::OMSI_MUTE.is_set()
}

/// Read and decode a clip (any thread).
pub fn read_clip(path: &Path) -> Option<Arc<Clip>> {
    let bytes = omsi_cfg::vfs::read(path).ok()?;
    let compressed = bytes.starts_with(b"OggS") || bytes.starts_with(b"fLaC") || crate::wav::is_mp3(&bytes);
    let clip = match if compressed { crate::wav::parse_compressed(&bytes) } else { crate::wav::parse_wav(&bytes) } {
        Ok(w) => Some(Arc::new(Clip {
            sample_rate: w.sample_rate,
            channels: w.channels,
            samples: w.samples,
        })),
        Err(e) => {
            log::warn!("{}: {e}", path.display());
            None
        }
    };
    if let Some(c) = &clip {
        crate::trace::clip_read(Arc::as_ptr(c) as usize, path);
    }
    clip
}

/// Every two seconds, the name of the system's default output device: when it is not the
/// one played on (`first`, then the last one seen), `reopen` is set. Ends with the engine.
fn watch_default_device(first: String, reopen: std::sync::Weak<AtomicBool>) {
    let _ = std::thread::Builder::new().name("audio device".into()).spawn(move || {
        let mut current = first;
        loop {
            std::thread::sleep(std::time::Duration::from_secs(2));
            let Some(flag) = reopen.upgrade() else { return };
            let name = cpal::default_host().default_output_device().and_then(|d| d.name().ok()).unwrap_or_default();
            if name != current {
                current = name;
                flag.store(true, Ordering::Relaxed);
            }
        }
    });
}

impl AudioEngine {
    /// An engine without a device: clips load and voices are kept, nothing is mixed or
    /// heard - for tools that look at sound configurations.
    pub fn silent() -> AudioEngine {
        crate::trace::engine("silent");
        AudioEngine {
            stream: std::cell::RefCell::new(None),
            device: std::cell::RefCell::new(String::new()),
            reopen: Arc::new(AtomicBool::new(false)),
            opened: std::cell::Cell::new(std::time::Instant::now()),
            shared: Arc::new(Shared::new(48_000, 2, true, false)),
            next_id: AtomicU64::new(1),
            clips: Default::default(),
            last_trim: Mutex::new(std::time::Instant::now()),
            loading: Default::default(),
            enabled: true,
            capture: false,
            silent: true,
        }
    }

    /// An engine that plays on no device and is mixed on demand ([`AudioEngine::render_capture`]):
    /// a recording run writes the sound of the same simulated time as its pictures, however
    /// long a picture takes. Clips are read at once (a background read would leave the
    /// first seconds silent), and the clock of the parameters is the mixed sound's own.
    pub fn capture(rate: u32) -> AudioEngine {
        crate::trace::engine("capture");
        AudioEngine {
            stream: std::cell::RefCell::new(None),
            device: std::cell::RefCell::new(String::new()),
            reopen: Arc::new(AtomicBool::new(false)),
            opened: std::cell::Cell::new(std::time::Instant::now()),
            shared: Arc::new(Shared::new(rate.max(8000), 2, false, true)),
            next_id: AtomicU64::new(1),
            clips: Default::default(),
            last_trim: Mutex::new(std::time::Instant::now()),
            loading: Default::default(),
            enabled: true,
            capture: true,
            silent: false,
        }
    }

    /// A capture engine's next `frames` frames of the mix (stereo, interleaved), with the
    /// parameters given since the last call. Nothing on an engine playing on a device.
    pub fn render_capture(&self, frames: usize) -> Vec<f32> {
        let Some(us) = self.shared.capture_us.as_ref() else {
            return Vec::new();
        };
        let mut out = vec![0.0f32; frames * 2];
        crate::trace::mixed(frames);
        self.shared.render(&mut out);
        let rate = self.shared.sample_rate.load(Ordering::Relaxed).max(1) as u64;
        us.fetch_add(frames as u64 * 1_000_000 / rate, Ordering::Relaxed);
        out
    }

    /// The time the engine goes by: the wall clock, or a recording's own (the mixed sound's,
    /// see [`AudioEngine::capture`]) - a sound's fade-in by the seconds it has played
    /// (`[volcurve] -1`) is heard in a recording as it is in the game, however long the
    /// pictures take to draw.
    pub fn clock(&self) -> std::time::Instant {
        self.shared.now()
    }

    /// The device's (or the capture's) sample rate.
    pub fn sample_rate(&self) -> u32 {
        self.shared.sample_rate.load(Ordering::Relaxed)
    }

    /// Open the default output device. Returns a silent engine if none is available.
    pub fn new() -> AudioEngine {
        crate::trace::engine("device");
        let shared = Arc::new(Shared::new(48_000, 2, muted(), false));
        let reopen = Arc::new(AtomicBool::new(false));
        let engine = AudioEngine {
            stream: std::cell::RefCell::new(None),
            device: std::cell::RefCell::new(String::new()),
            reopen: reopen.clone(),
            opened: std::cell::Cell::new(std::time::Instant::now()),
            shared,
            next_id: AtomicU64::new(1),
            clips: Default::default(),
            last_trim: Mutex::new(std::time::Instant::now()),
            loading: Default::default(),
            enabled: false,
            capture: false,
            silent: false,
        };
        let enabled = engine.open_default();
        let engine = AudioEngine { enabled, ..engine };
        if enabled {
            watch_default_device(engine.device.borrow().clone(), Arc::downgrade(&reopen));
        }
        engine
    }

    /// Play on the system's default output device from now on. Returns whether a stream
    /// is playing.
    fn open_default(&self) -> bool {
        // (the old stream first: some drivers give a device to one stream at a time)
        self.stream.borrow_mut().take();
        let host = cpal::default_host();
        let Some(dev) = host.default_output_device() else {
            log::warn!("audio: no output device");
            return false;
        };
        let name = dev.name().unwrap_or_default();
        let cfg = match dev.default_output_config() {
            Ok(c) => c,
            Err(e) => {
                log::warn!("audio: no output config on {name}: {e}");
                return false;
            }
        };
        self.shared.sample_rate.store(cfg.sample_rate().0, Ordering::Relaxed);
        self.shared.channels.store(cfg.channels().max(1) as usize, Ordering::Relaxed);
        let s2 = self.shared.clone();
        let lost = self.reopen.clone();
        let stream = dev.build_output_stream(
            &cfg.config(),
            move |data: &mut [f32], _| s2.render(data),
            move |e| {
                // (a lost device only: a driver's hiccups are not worth a new stream)
                if matches!(e, cpal::StreamError::DeviceNotAvailable) {
                    lost.store(true, Ordering::Relaxed);
                }
                log::warn!("audio stream error: {e}");
            },
            None,
        );
        match stream {
            Ok(s) => {
                if let Err(e) = s.play() {
                    log::warn!("audio: cannot start stream: {e}");
                }
                log::info!("audio: playing on {name} ({} Hz, {} channels)", cfg.sample_rate().0, cfg.channels());
                *self.stream.borrow_mut() = Some(s);
                *self.device.borrow_mut() = name;
                true
            }
            Err(e) => {
                log::warn!("audio: cannot open stream on {name}: {e}");
                false
            }
        }
    }

    /// Follow the system's output: when its default device changed (headphones plugged in,
    /// a Bluetooth headset connected) or the one played on went away, the stream is opened
    /// again on the default device - the sound had stayed on the speakers, or stopped for
    /// good when the headset was disconnected. Cheap; called every frame.
    pub fn follow_device(&self) {
        if !self.enabled || self.opened.get().elapsed().as_secs_f32() < 1.0 || !self.reopen.swap(false, Ordering::Relaxed) {
            return;
        }
        self.opened.set(std::time::Instant::now());
        let before = self.device.borrow().clone();
        if self.open_default() {
            let now = self.device.borrow().clone();
            if now != before {
                log::info!("audio: output moved from {before} to {now}");
            }
        } else {
            // (no device right now: tried again when the watcher sees one)
            self.device.borrow_mut().clear();
        }
    }

    /// A clip from the cache, read now if it is not there.
    pub fn load_clip(&self, path: &Path) -> Option<Arc<Clip>> {
        if let Some(c) = self.clips.lock().get_mut(path) {
            c.1 = std::time::Instant::now();
            return c.0.clone();
        }
        let clip = read_clip(path);
        self.clips.lock().insert(
            path.to_path_buf(),
            (clip.clone(), std::time::Instant::now()),
        );
        clip
    }

    /// Let go of the clips nobody holds (no sound set, no voice) and nobody asked for in
    /// `unused`, once every ten seconds at most: the sounds of every vehicle that ever came
    /// into earshot stayed (140 MB around the Ahlheim main station). They are read again in
    /// the background when a vehicle comes back. Returns the bytes let go.
    pub fn trim_clips(&self, unused: std::time::Duration) -> usize {
        {
            let mut last = self.last_trim.lock();
            if last.elapsed().as_secs_f32() < 10.0 {
                return 0;
            }
            *last = std::time::Instant::now();
        }
        let mut freed = 0usize;
        self.clips.lock().retain(|_, (clip, used)| {
            let idle = used.elapsed() >= unused
                && clip
                    .as_ref()
                    .map(|c| Arc::strong_count(c) == 1)
                    .unwrap_or(false);
            if idle {
                freed += clip.as_ref().map(|c| c.samples.len() * 2).unwrap_or(0);
            }
            !idle
        });
        freed
    }

    /// Whether all of `paths` are in the cache (or known to be missing). The ones that are
    /// not are read on a background thread, started on the first call: a vehicle coming
    /// into earshot for the first time had its sounds read in the frame, which on a map
    /// read from an archive was a stall of up to 400 ms.
    pub fn clips_ready(&self, paths: &[PathBuf]) -> bool {
        if !self.enabled {
            return true;
        }
        if self.capture {
            for p in paths {
                self.load_clip(p);
            }
            return true;
        }
        let missing: Vec<PathBuf> = {
            let mut clips = self.clips.lock();
            let now = std::time::Instant::now();
            paths
                .iter()
                .filter(|p| match clips.get_mut(*p) {
                    Some(c) => {
                        c.1 = now;
                        false
                    }
                    None => true,
                })
                .cloned()
                .collect()
        };
        if missing.is_empty() {
            return true;
        }
        let todo: Vec<PathBuf> = {
            let mut loading = self.loading.lock();
            missing
                .into_iter()
                .filter(|p| loading.insert(p.clone()))
                .collect()
        };
        if !todo.is_empty() {
            let (clips, loading) = (self.clips.clone(), self.loading.clone());
            let back = todo.clone();
            let spawned = std::thread::Builder::new()
                .name("sound loader".into())
                .spawn(move || {
                    for p in todo {
                        let c = read_clip(&p);
                        clips
                            .lock()
                            .insert(p.clone(), (c, std::time::Instant::now()));
                        loading.lock().remove(&p);
                    }
                });
            if spawned.is_err() {
                // no thread: read them here
                for p in back {
                    self.load_clip(&p);
                    self.loading.lock().remove(&p);
                }
                return true;
            }
        }
        false
    }

    pub fn play(&self, clip: Arc<Clip>, params: VoiceParams) -> VoiceId {
        self.play_at(clip, params, 0.0)
    }

    /// Start a loop from a random place in its clip, as Omsi.exe starts a looping buffer
    /// (`SetCurrentPosition(Random(size))`, 0x750c0c): two buses of a type standing side by
    /// side do not drone in phase.
    pub fn play_from_random_place(&self, clip: Arc<Clip>, params: VoiceParams) -> VoiceId {
        static SEED: AtomicU64 = AtomicU64::new(0x2545_f491_4f6c_dd1d);
        let mut x = SEED.load(Ordering::Relaxed);
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        SEED.store(x, Ordering::Relaxed);
        let start = (x % clip.frames().max(1) as u64) as f64;
        self.play_at(clip, params, start)
    }

    fn play_at(&self, clip: Arc<Clip>, params: VoiceParams, start: f64) -> VoiceId {
        let trace_clip = Arc::as_ptr(&clip) as usize;
        self.start_voice(Some(trace_clip), params, start, |id| Voice {
            id,
            clip,
            stream: None,
            params,
            pos: start,
            finished: false,
            cur_gain: 0.0,
            lp: [0.0, 0.0],
            lp_fresh: false,
            fx: None,
            doppler: (0.0, None, 1.0),
        })
    }

    /// A new voice given its id (drawn under the queue's lock, so that the mixer's list
    /// stays in the order of the ids) and handed to the mixer; playing from now on.
    fn start_voice(&self, trace_clip: Option<usize>, params: VoiceParams, start: f64, voice: impl FnOnce(VoiceId) -> Voice) -> VoiceId {
        if self.direct() {
            let id = self.next_id.fetch_add(1, Ordering::Relaxed);
            crate::trace::play(id, trace_clip, &params, start);
            self.shared.live.write().insert(id, params);
            self.shared.send(true, Cmd::Play(Box::new(voice(id))));
            return id;
        }
        // (the queue's lock held from the id to the push)
        let mut q = self.shared.updates.lock();
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        crate::trace::play(id, trace_clip, &params, start);
        self.shared.live.write().insert(id, params);
        q.push(Cmd::Play(Box::new(voice(id))));
        id
    }

    /// No mixer takes the commands in: an engine without a device (see [`AudioEngine::silent`]).
    fn direct(&self) -> bool {
        !self.enabled || self.silent
    }

    /// Play what `stream` is fed with (see `stream::StreamBuf`); the voice ends when the
    /// stream is closed.
    pub fn play_stream(&self, stream: Arc<crate::stream::StreamBuf>, params: VoiceParams) -> VoiceId {
        self.start_voice(None, params, 0.0, |id| Voice {
            id,
            clip: Arc::new(Clip { sample_rate: 44100, channels: 2, samples: Vec::new() }),
            stream: Some(stream),
            params,
            pos: 0.0,
            finished: false,
            cur_gain: 0.0,
            lp: [0.0, 0.0],
            lp_fresh: false,
            fx: None,
            doppler: (0.0, None, 1.0),
        })
    }

    /// The bus radio's effects on stream voice `id` (see `radio_fx`): made, retuned, or
    /// done away with when all of them are 0.
    pub fn set_radio_fx(&self, id: VoiceId, params: crate::radio_fx::RadioFxParams) {
        crate::trace::radio_fx(id, &params);
        let rate = self.shared.sample_rate.load(Ordering::Relaxed);
        // (made here, should the voice have none yet: the mixer's thread allocates nothing)
        let fresh = if params.is_off() { None } else { crate::radio_fx::RadioFx::new(params, rate).map(Box::new) };
        self.shared.send(self.direct(), Cmd::RadioFx(id, params, fresh));
    }

    /// New parameters for a voice: queued for the mixer's next block (a few milliseconds),
    /// so the game never waits for a block being mixed. Given twice before that block, the
    /// later ones win.
    pub fn set_params(&self, id: VoiceId, params: VoiceParams) {
        crate::trace::set(id, &params);
        // (no device, no mixer to take them in: straight onto the voice)
        self.shared.send(self.direct(), Cmd::Params(id, params, self.shared.now()));
    }

    /// What a voice plays with now, and how loud it arrives at the listener (gain after
    /// distance), while it plays.
    /// (from the voices' published states: never waits for a block being mixed)
    pub fn voice_state(&self, id: VoiceId) -> Option<(VoiceParams, f32)> {
        let params = *self.shared.live.read().get(&id)?;
        let listener = self.shared.listener.lock().position;
        let spatial = params
            .position
            .map(|p| distance_gain(params.range, (p - listener).length()))
            .unwrap_or(1.0);
        Some((params, params.gain * spatial))
    }

    /// (from the voices' published states: never waits for a block being mixed)
    pub fn is_playing(&self, id: VoiceId) -> bool {
        self.shared.live.read().contains_key(&id)
    }

    pub fn stop(&self, id: VoiceId) {
        crate::trace::stop(id);
        if self.shared.live.write().remove(&id).is_some() {
            self.shared.send(self.direct(), Cmd::Stop(id));
        }
    }

    pub fn listener_position(&self) -> Vec3 {
        self.shared.listener.lock().position
    }

    pub fn set_listener(&self, l: Listener) {
        crate::trace::listener(&l);
        *self.shared.listener.lock() = l;
    }

    /// The voices playing.
    pub fn voice_count(&self) -> usize {
        self.shared.live.read().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shared() -> Shared {
        Shared::new(48_000, 1, false, false)
    }

    fn voice(clip: Arc<Clip>, gain: f32) -> Voice {
        Voice {
            id: 1,
            clip,
            stream: None,
            params: VoiceParams { gain, pitch: 1.0, looping: true, position: None, doppler: true, range: 10.0, lowpass_hz: 0.0, important: false },
            pos: 0.0,
            finished: false,
            cur_gain: gain,
            lp: [0.0; 2],
            lp_fresh: false,
            fx: None,
            doppler: (0.0, None, 1.0),
        }
    }

    #[test]
    fn a_loop_has_no_gap_where_it_turns() {
        // a 5-frame loop of a constant level at the device rate: every output frame carries it
        let clip = Arc::new(Clip { sample_rate: 48_000, channels: 1, samples: vec![16_384; 5] });
        let s = shared();
        s.voices.lock().push(voice(clip, 1.0));
        let mut out = vec![0.0f32; 64];
        s.render(&mut out);
        assert!(out.iter().all(|x| (*x - 0.5).abs() < 1e-3), "{out:?}");
    }

    #[test]
    fn parameters_arrive_with_the_next_block() {
        let clip = Arc::new(Clip { sample_rate: 48_000, channels: 1, samples: vec![16_384; 5] });
        let s = shared();
        s.voices.lock().push(voice(clip, 1.0));
        s.updates.lock().push(Cmd::Params(1, VoiceParams { gain: 0.0, looping: true, ..Default::default() }, std::time::Instant::now()));
        let mut out = vec![0.0f32; 4];
        s.render(&mut out);
        assert_eq!(s.voices.lock()[0].params.gain, 0.0);
        assert!(s.updates.lock().is_empty());
    }

    #[test]
    fn listener_vehicle_keeps_spatial_sound_at_its_original_pitch() {
        let clip = Arc::new(Clip { sample_rate: 48_000, channels: 1, samples: vec![0; 5] });
        let mut own = voice(clip.clone(), 1.0);
        let mut passing = voice(clip, 1.0);
        let now = std::time::Instant::now();
        for (distance, elapsed) in [(2.0, 0), (2.2, 20)] {
            let at = now + std::time::Duration::from_millis(elapsed);
            let position = Some(Vec3::new(distance, 0.0, 0.0));
            apply_params(&mut own, VoiceParams { position, doppler: false, ..Default::default() }, at, Vec3::ZERO);
            apply_params(&mut passing, VoiceParams { position, ..Default::default() }, at, Vec3::ZERO);
        }
        assert_eq!(own.doppler.2, 1.0);
        assert!(passing.doppler.2 < 1.0);
    }

    /// A recording's clock is the sound it mixed, not the wall clock: a fade-in by seconds
    /// played comes out the same however slowly the run goes.
    #[test]
    fn a_recording_goes_by_the_sound_it_mixed() {
        let e = AudioEngine::capture(48_000);
        let t0 = e.clock();
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert_eq!(e.clock(), t0);
        e.render_capture(24_000);
        assert_eq!(e.clock() - t0, std::time::Duration::from_millis(500));
    }

    /// The stream opened again (as when the system's output device changed) keeps playing;
    /// on a machine without an output device there is nothing to follow.
    #[test]
    fn follows_the_output_device() {
        let e = AudioEngine::new();
        if !e.enabled {
            return;
        }
        let before = e.device.borrow().clone();
        e.reopen.store(true, Ordering::Relaxed);
        e.opened.set(std::time::Instant::now() - std::time::Duration::from_secs(2));
        e.follow_device();
        assert!(e.stream.borrow().is_some());
        assert_eq!(*e.device.borrow(), before);
        assert!(!e.reopen.load(Ordering::Relaxed));
    }

    /// A voice at `d` metres in front of the listener, heard at full gain whatever `d` is.
    fn far_voice(clip: Arc<Clip>, d: f32) -> Voice {
        let mut v = voice(clip, 1.0);
        v.params.position = Some(Vec3::new(0.0, 0.0, -d));
        v.params.range = 1.0e6;
        v.params.doppler = false;
        v
    }

    /// A capture engine mixes on demand, with its own clock.
    #[test]
    fn a_capture_mixes_voices_on_demand() {
        let e = AudioEngine::capture(48_000);
        let clip = Arc::new(Clip { sample_rate: 48_000, channels: 1, samples: vec![8_192; 4_800] });
        e.play(clip, VoiceParams { gain: 1.0, looping: true, ..Default::default() });
        let t0 = e.shared.now();
        let a = e.render_capture(4_800);
        assert_eq!(a.len(), 9_600);
        assert!((e.shared.now() - t0).as_millis() == 100, "the clock moves with the sound");
        assert!(a[2000..].iter().all(|x| (*x - 0.25).abs() < 0.01));
        assert!(AudioEngine::new_for_test_device_less().render_capture(10).is_empty());
    }

    /// What the game asks and tells a voice never waits for a block being mixed (the mixer
    /// holds the voices a whole block); a voice that ended by itself is gone once mixed, one
    /// stopped at once.
    #[test]
    fn the_game_never_waits_for_the_mixer() {
        let e = AudioEngine::capture(48_000);
        let clip = Arc::new(Clip { sample_rate: 48_000, channels: 1, samples: vec![8_192; 480] });
        let short = e.play(clip.clone(), VoiceParams::default());
        let looped = e.play(clip.clone(), VoiceParams { looping: true, ..Default::default() });
        e.render_capture(4_800);
        assert!(!e.is_playing(short) && e.is_playing(looped) && e.voice_count() == 1);
        // (a block being mixed: the voices held for half a second)
        let (shared, (tx, rx)) = (e.shared.clone(), std::sync::mpsc::channel());
        let mixer = std::thread::spawn(move || {
            let _mixing = shared.voices.lock();
            tx.send(()).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(500));
        });
        rx.recv().unwrap();
        let t = std::time::Instant::now();
        e.set_params(looped, VoiceParams { gain: 0.5, looping: true, ..Default::default() });
        let other = e.play(clip, VoiceParams::default());
        e.set_radio_fx(other, Default::default());
        let asked = (e.is_playing(looped), e.voice_state(looped).map(|s| s.1), e.is_playing(other));
        e.stop(other);
        assert!(t.elapsed() < std::time::Duration::from_millis(250), "the game waited for the mixer");
        assert_eq!((asked, e.is_playing(other)), ((true, Some(1.0), true), false));
        mixer.join().unwrap();
        e.render_capture(480);
        assert_eq!(e.voice_state(looped).map(|s| s.1), Some(0.5));
        assert_eq!(e.shared.voices.lock().len(), 1);
    }

    impl AudioEngine {
        fn new_for_test_device_less() -> AudioEngine {
            AudioEngine::silent()
        }
    }

    /// A stream with its effects all at 0 plays exactly as one never given any; with the
    /// speaker on it does not.
    #[test]
    fn a_stream_with_no_effects_is_bit_identical() {
        let render = |fx: Option<crate::radio_fx::RadioFxParams>| {
            let e = AudioEngine::capture(48_000);
            let buf = Arc::new(crate::stream::StreamBuf::default());
            buf.push(48_000, (0..48_000 * 3).map(|i| {
                let t = i as f32 / 48_000.0;
                [(t * 440.0 * std::f32::consts::TAU).sin() * 0.5, (t * 660.0 * std::f32::consts::TAU).sin() * 0.4]
            }));
            let id = e.play_stream(buf, VoiceParams { gain: 0.8, ..Default::default() });
            if let Some(p) = fx {
                e.set_radio_fx(id, p);
            }
            e.render_capture(48_000 * 2)
        };
        let plain = render(None);
        assert!(plain.iter().any(|x| x.abs() > 0.1));
        let off = render(Some(Default::default()));
        assert!(plain.iter().zip(&off).all(|(a, b)| a.to_bits() == b.to_bits()));
        let speaker = render(Some(crate::radio_fx::RadioFxParams { speaker: 1.0, ..Default::default() }));
        assert!(plain.iter().zip(&speaker).any(|(a, b)| (a - b).abs() > 0.01));
    }

    /// The one-pole low-pass of a voice heard through the bodywork: a 5 kHz tone through
    /// 500 Hz comes out some 20 dB down, a 100 Hz one hardly touched.
    #[test]
    fn the_voice_low_pass_follows_its_corner() {
        let level = |hz: f32| {
            let n = 48_000usize;
            let samples: Vec<i16> = (0..n).map(|i| ((i as f32 / 48_000.0 * hz * std::f32::consts::TAU).sin() * 16_000.0) as i16).collect();
            let clip = Arc::new(Clip { sample_rate: 48_000, channels: 1, samples });
            let s = shared();
            let mut v = voice(clip, 1.0);
            v.params.lowpass_hz = 500.0;
            v.params.looping = false;
            s.voices.lock().push(v);
            let mut out = vec![0.0f32; n];
            s.render(&mut out);
            let tail = &out[n / 2..];
            (tail.iter().map(|x| x * x).sum::<f32>() / tail.len() as f32).sqrt() / (16_000.0 / 32_768.0 / 2f32.sqrt())
        };
        let high = 20.0 * level(5_000.0).log10();
        assert!((high + 20.0).abs() < 1.5, "{high} dB");
        assert!(level(100.0) > 0.97);
    }

    #[test]
    fn a_low_pass_switched_on_starts_from_the_sound() {
        let mut v = voice(Arc::new(Clip { sample_rate: 48_000, channels: 1, samples: vec![0; 4] }), 1.0);
        v.lp = [0.7, -0.7];
        apply_params(&mut v, VoiceParams { lowpass_hz: 800.0, ..Default::default() }, std::time::Instant::now(), Vec3::ZERO);
        assert!(v.lp_fresh);
        apply_params(&mut v, VoiceParams { lowpass_hz: 0.0, ..Default::default() }, std::time::Instant::now(), Vec3::ZERO);
        let mut w = voice(Arc::new(Clip { sample_rate: 48_000, channels: 1, samples: vec![0; 4] }), 1.0);
        apply_params(&mut w, VoiceParams { lowpass_hz: 0.0, ..Default::default() }, std::time::Instant::now(), Vec3::ZERO);
        assert!(!w.lp_fresh);
    }

    #[test]
    fn important_voices_win_the_mixer_limit() {
        let clip = Arc::new(Clip { sample_rate: 48_000, channels: 1, samples: vec![64; 100] });
        let s = shared();
        for _ in 0..MAX_VOICES {
            s.voices.lock().push(far_voice(clip.clone(), 10.0));
        }
        // the farthest of all, but [important]: it stays, one of the near ones goes
        let mut far_important = far_voice(clip, 500.0);
        far_important.id = 9_999;
        far_important.params.important = true;
        s.voices.lock().push(far_important);
        let mut out = vec![0.0f32; 16];
        s.render(&mut out);
        let expect = MAX_VOICES as f32 * 64.0 / 32_768.0;
        assert!((out[0] - expect).abs() < 1e-3, "{} vs {expect}", out[0]);
    }

    #[test]
    fn the_nearest_voices_are_mixed_and_the_driven_bus_is_never_cut() {
        let clip = Arc::new(Clip { sample_rate: 48_000, channels: 1, samples: vec![64; 100] });
        let s = shared();
        // 50 far voices, MAX_VOICES near ones, and the bus's own (non-spatial) at half gain
        for k in 0..MAX_VOICES + 50 {
            s.voices.lock().push(far_voice(clip.clone(), if k < 50 { 900.0 } else { 5.0 }));
        }
        s.voices.lock().push(voice(clip, 0.5));
        let mut out = vec![0.0f32; 16];
        s.render(&mut out);
        // the bus kept, then the nearest: MAX_VOICES - 1 near ones at full gain + 0.5
        let expect = ((MAX_VOICES - 1) as f32 + 0.5) * 64.0 / 32_768.0;
        assert!((out[0] - expect).abs() < 1e-3, "{} vs {expect}", out[0]);
    }
}

/// A small Schroeder reverb (four combs in parallel, two all-passes in series, per channel)
/// for the echo under a bridge: the combs' feedback gives the reverberation time.
#[derive(Default)]
struct Reverb {
    lines: Vec<Vec<(Vec<f32>, usize, f32)>>,
    allpass: Vec<Vec<(Vec<f32>, usize)>>,
    rate: u32,
}

impl Reverb {
    const COMBS: [f32; 4] = [0.0297, 0.0371, 0.0411, 0.0437];
    const ALLPASS: [f32; 2] = [0.005, 0.0017];

    fn process(&mut self, out: &mut [f32], ch: usize, rate: u32, rt60: f32, mix: f32) {
        if self.rate != rate || self.lines.len() != ch {
            self.rate = rate;
            self.lines = (0..ch).map(|c| Self::COMBS.iter().map(|d| (vec![0.0; ((d + c as f32 * 0.0011) * rate as f32) as usize + 1], 0, 0.0)).collect()).collect();
            self.allpass = (0..ch).map(|_| Self::ALLPASS.iter().map(|d| (vec![0.0; (d * rate as f32) as usize + 1], 0)).collect()).collect();
        }
        let frames = out.len() / ch;
        for c in 0..ch {
            let combs = &mut self.lines[c];
            let aps = &mut self.allpass[c];
            // feedback so that a comb decays by 60 dB in rt60 seconds
            let gains: Vec<f32> = Self::COMBS.iter().map(|d| 10f32.powf(-3.0 * d / rt60)).collect();
            for f in 0..frames {
                let x = out[f * ch + c];
                let mut y = 0.0;
                for (k, (buf, i, lp)) in combs.iter_mut().enumerate() {
                    let d = buf[*i];
                    *lp = d * 0.8 + *lp * 0.2;
                    buf[*i] = x + *lp * gains[k];
                    *i = (*i + 1) % buf.len();
                    y += d;
                }
                y *= 0.25;
                for (buf, i) in aps.iter_mut() {
                    let d = buf[*i];
                    let v = y + d * 0.5;
                    buf[*i] = v;
                    *i = (*i + 1) % buf.len();
                    y = d - v * 0.5;
                }
                out[f * ch + c] = x + y * mix;
            }
        }
    }
}

/// How loud a sound `dist` metres away arrives, with `range` its `[3d]` reference distance:
/// Omsi.exe multiplies the volume by `min(range / distance, 1)` itself (`TSound` update
/// 0x7509fe) - full up to the range, then 1/d (6 dB per doubling); a range of 0 is silent
/// anywhere but at the very spot.
pub fn distance_gain(range: f32, dist: f32) -> f32 {
    if dist <= 0.0 {
        return 1.0;
    }
    (range.max(0.0) / dist).min(1.0)
}

/// OMSI's `[sound_stereo]` (0 to 20, 10 when options.cfg has none): how far a `[3d]` sound
/// is panned.
pub static STEREO: AtomicU32 = AtomicU32::new(10);

/// The channel gains of a sound in direction `d` from the listener, as Omsi.exe pans a
/// `[3d]` sound (0x7508ef): `s` is the side component of the horizontal direction (1 =
/// straight right), the buffer's pan `50 * [sound_stereo] * s^3` hundredths of a dB, and
/// DirectSound lowers the far channel by that much and leaves the near one alone - at the
/// default stereo setting a sound straight to the right is 5 dB down on the left. Ours gave
/// the near channel 1.2 and the far one 0, a centred sound 0.85 on both.
pub fn pan_gains(listener: &Listener, d: Vec3) -> (f32, f32) {
    let stereo = STEREO.load(Ordering::Relaxed) as f32;
    if stereo <= 0.0 {
        return (1.0, 1.0);
    }
    let right = listener.right.normalize_or_zero();
    let up = right.cross(listener.forward).normalize_or_zero();
    let s = (d - up * d.dot(up)).normalize_or_zero().dot(right).clamp(-1.0, 1.0);
    let pan = (50.0 * stereo * s * s * s).clamp(-10_000.0, 10_000.0);
    let far = 10f32.powf(-pan.abs() / 2000.0);
    if pan > 0.0 {
        (far, 1.0)
    } else {
        (1.0, far)
    }
}

#[cfg(test)]
mod distance_tests {
    #[test]
    fn inverse_distance_beyond_the_reference() {
        assert_eq!(super::distance_gain(2.0, 1.0), 1.0);
        assert!((super::distance_gain(2.0, 4.0) - 0.5).abs() < 1e-6);
        assert!((super::distance_gain(1.0, 10.0) - 0.1).abs() < 1e-6);
        assert_eq!(super::distance_gain(0.0, 3.0), 0.0, "a range of 0");
    }

    #[test]
    fn a_sound_to_the_right_is_5_db_down_on_the_left() {
        use glam::Vec3;
        let l = super::Listener { forward: Vec3::Y, right: Vec3::X, ..Default::default() };
        let (left, right) = super::pan_gains(&l, Vec3::new(3.0, 0.0, 1.0));
        assert!((left - 10f32.powf(-0.25)).abs() < 1e-4 && right == 1.0, "{left} {right}");
        let (left, right) = super::pan_gains(&l, Vec3::new(0.0, 5.0, 0.0));
        assert_eq!((left, right), (1.0, 1.0), "ahead: both at full");
        let (left, right) = super::pan_gains(&l, Vec3::new(-1.0, 1.0, 0.0));
        assert!(left == 1.0 && right < 1.0 && right > 0.8, "{right}");
    }
}

/// Past 0.9 a sample is bent smoothly towards 1 instead of being cut off there.
fn soft_clip(x: f32) -> f32 {
    let a = x.abs();
    if a <= 0.9 {
        x
    } else {
        x.signum() * (0.9 + 0.1 * ((a - 0.9) / 0.1).tanh())
    }
}

#[cfg(test)]
mod limiter_tests {
    #[test]
    fn soft_clip_is_smooth_and_bounded() {
        assert_eq!(super::soft_clip(0.5), 0.5);
        assert!(super::soft_clip(3.0) <= 1.0 && super::soft_clip(-3.0) >= -1.0);
        assert!(super::soft_clip(0.95) > 0.9 && super::soft_clip(0.95) < 0.95);
    }
}
