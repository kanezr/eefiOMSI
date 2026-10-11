//! A buffer of sound that is filled while it plays: the decoded internet radio. A reader
//! thread pushes stereo frames at the stream's own rate; the mixer takes them at the
//! device rate (see `AudioEngine::play_stream`).

use parking_lot::{Mutex, MutexGuard};
use std::collections::VecDeque;

pub struct StreamBuf {
    inner: Mutex<StreamInner>,
    /// What the reader says about the stream: the song playing, or why it is silent.
    status: Mutex<String>,
    /// What the station says of itself: its name (`icy-name`) and the title playing now
    /// (`StreamTitle`), each empty while it says nothing.
    meta: Mutex<(String, String)>,
}

pub struct StreamInner {
    /// Frames per second of what is in the buffer.
    pub rate: u32,
    frames: VecDeque<[f32; 2]>,
    /// Playing (else filling up: at the start and after the network fell behind).
    playing: bool,
    /// How much is gathered before playing starts again after a stall, in seconds: a
    /// connection that stalled once tends to stall again, so each stall asks for more.
    prebuffer: f32,
    /// The voice ends (the radio was switched off or to another station).
    pub closed: bool,
}

impl StreamInner {
    /// The frame playing now and the next one, while there are both.
    pub(crate) fn pair(&mut self) -> Option<([f32; 2], [f32; 2])> {
        if !self.playing {
            if (self.frames.len() as f32) < self.rate as f32 * self.prebuffer {
                return None;
            }
            self.playing = true;
        }
        if self.frames.len() < 2 {
            self.playing = false;
            self.prebuffer = (self.prebuffer + 1.0).min(6.0);
            return None;
        }
        Some((self.frames[0], self.frames[1]))
    }

    pub(crate) fn advance(&mut self) {
        self.frames.pop_front();
    }
}

impl Default for StreamBuf {
    fn default() -> Self {
        StreamBuf {
            inner: Mutex::new(StreamInner {
                rate: 44100,
                frames: VecDeque::new(),
                playing: false,
                prebuffer: 1.5,
                closed: false,
            }),
            status: Mutex::new(String::new()),
            meta: Mutex::new((String::new(), String::new())),
        }
    }
}

impl StreamBuf {
    pub(crate) fn lock(&self) -> MutexGuard<'_, StreamInner> {
        self.inner.lock()
    }

    /// Add decoded frames at `rate`. A new rate (another station, a stream that changed
    /// format) starts the buffer over.
    pub fn push(&self, rate: u32, frames: impl IntoIterator<Item = [f32; 2]>) {
        let mut b = self.inner.lock();
        if b.rate != rate {
            b.rate = rate;
            b.frames.clear();
            b.playing = false;
        }
        b.frames.extend(frames);
    }

    /// Seconds of sound waiting to be played.
    pub fn buffered(&self) -> f32 {
        let b = self.inner.lock();
        b.frames.len() as f32 / b.rate.max(1) as f32
    }

    /// Whether the mixer is playing it (not waiting for the buffer to fill).
    pub fn is_playing(&self) -> bool {
        self.inner.lock().playing
    }

    pub fn close(&self) {
        let mut b = self.inner.lock();
        b.closed = true;
        b.frames.clear();
    }

    pub fn set_status(&self, s: impl Into<String>) {
        *self.status.lock() = s.into();
    }

    pub fn status(&self) -> String {
        self.status.lock().clone()
    }

    /// The station's name and the title playing (see `meta`).
    pub fn set_meta(&self, name: &str, title: &str) {
        let mut m = self.meta.lock();
        if m.0 != name || m.1 != title {
            *m = (name.to_string(), title.to_string());
        }
    }

    /// The station's name as it gives it (empty when it gives none).
    pub fn station_name(&self) -> String {
        self.meta.lock().0.clone()
    }

    /// The title playing now as the station gives it (empty when it gives none).
    pub fn title(&self) -> String {
        self.meta.lock().1.clone()
    }

    pub fn is_closed(&self) -> bool {
        self.inner.lock().closed
    }
}
