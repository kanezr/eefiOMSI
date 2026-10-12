//! GPU pass timers (OMSI_GPU_TIMERS), the device poller thread and the encoding pool.

use crate::*;

/// GPU time per render pass from timestamp queries (OMSI_GPU_TIMERS). One frame is timed
/// at a time: its readback has to arrive before the next one is measured. The passes
/// partition the frame: each counts from the end of the one the GPU finished before it, so
/// untimed passes are in the next timed one's figure and the figures add up to
/// "(all passes)".
pub(crate) struct GpuTimers {
    pub(crate) set: wgpu::QuerySet,
    pub(crate) resolve: wgpu::Buffer,
    pub(crate) read: wgpu::Buffer,
    /// The passes of the frame being read back, in query order.
    pub(crate) pending: Vec<&'static str>,
    /// The frame's passes are timed, their stamps not yet resolved (see `collect_gpu_timers`).
    pub(crate) unresolved: bool,
    pub(crate) waiting: bool,
    pub(crate) ready: Arc<std::sync::atomic::AtomicBool>,
    /// pass → (seconds, frames)
    pub(crate) totals: std::collections::BTreeMap<&'static str, (f64, u32)>,
}

pub(crate) const GPU_TIMER_PASSES: u32 = 16;

impl GpuTimers {
    pub(crate) fn new(device: &wgpu::Device) -> Option<GpuTimers> {
        if !omsi_cfg::flags::OMSI_GPU_TIMERS.is_set()
            || !device.features().contains(wgpu::Features::TIMESTAMP_QUERY)
        {
            return None;
        }
        let set = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("pass timers"),
            ty: wgpu::QueryType::Timestamp,
            count: GPU_TIMER_PASSES * 2,
        });
        let size = (GPU_TIMER_PASSES as u64 * 16).div_ceil(wgpu::QUERY_RESOLVE_BUFFER_ALIGNMENT)
            * wgpu::QUERY_RESOLVE_BUFFER_ALIGNMENT;
        let resolve = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pass timers"),
            size,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let read = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pass timers read"),
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Some(GpuTimers {
            set,
            resolve,
            read,
            pending: Vec::new(),
            unresolved: false,
            waiting: false,
            ready: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            totals: Default::default(),
        })
    }
}

/// The timestamp writes of the next timed pass (none when this frame is not timed).
pub(crate) fn pass_timer<'a>(
    set: Option<&'a wgpu::QuerySet>,
    timed: &mut Vec<&'static str>,
    label: &'static str,
) -> Option<wgpu::RenderPassTimestampWrites<'a>> {
    let set = set?;
    if timed.len() as u32 >= GPU_TIMER_PASSES {
        return None;
    }
    let i = timed.len() as u32 * 2;
    timed.push(label);
    Some(wgpu::RenderPassTimestampWrites {
        query_set: set,
        beginning_of_pass_write_index: Some(i),
        end_of_pass_write_index: Some(i + 1),
    })
}

pub(crate) struct DevicePoller {
    pub(crate) stop: Arc<std::sync::atomic::AtomicBool>,
    pub(crate) thread: Option<std::thread::JoinHandle<()>>,
}

impl DevicePoller {
    pub(crate) fn start(device: &wgpu::Device) -> Option<Self> {
        // Not on OpenGL: there every poll takes the one GL context, and whenever the thread
        // drawing held it for more than a second (a big shader linked while the world
        // loads, a slow chip's frame) this thread gave up with wgpu-hal's panic "Could not
        // lock adapter context" (#898, after #843). It is not needed there: every submit
        // of the frame runs the same upkeep (wgpu-core's `maintain` after `queue.submit`).
        if cfg!(target_arch = "wasm32") || gl_backend() || omsi_cfg::flags::OMSI_NO_POLL_THREAD.is_set() {
            return None;
        }
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (device, flag) = (device.clone(), stop.clone());
        let thread = std::thread::Builder::new()
            .name("omsi-gpu-poll".into())
            .spawn(move || {
                // (every 4 ms: a frame's own submits run the same upkeep, and this thread
                // only takes the freeing of finished work off the frame's thread; polling
                // every millisecond, a few dozen times a frame, it fought the frame's thread
                // for the queue's lock - a third of its time was spent spinning on it)
                let pause = std::time::Duration::from_millis(4);
                while !flag.load(std::sync::atomic::Ordering::Relaxed) {
                    let _ = device.poll(wgpu::PollType::Poll);
                    std::thread::sleep(pause);
                }
            })
            .ok()?;
        Some(Self { stop, thread: Some(thread) })
    }
}

impl Drop for DevicePoller {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

pub(crate) fn in_scope<'s, R>(pool: Option<&rayon::ThreadPool>, op: impl FnOnce(&rayon::Scope<'s>) -> R) -> R {
    match pool {
        Some(p) => p.in_place_scope(op),
        None => rayon::in_place_scope(op),
    }
}

pub(crate) fn run_parts<T: Send>(pool: Option<&rayon::ThreadPool>, parts: usize, f: impl Fn(usize) -> T + Sync) -> Vec<T> {
    let Some(pool) = pool else {
        return std::thread::scope(|s| {
            let f = &f;
            let helpers: Vec<_> = (1..parts.max(1)).map(|k| s.spawn(move || f(k))).collect();
            let mut out = vec![f(0)];
            out.extend(helpers.into_iter().map(|h| h.join().expect("render part")));
            out
        });
    };
    // The parts are taken in turn by whoever is free, the calling thread too: a helper the
    // system has not got round to running yet holds up no part of its own - the caller,
    // done with one part, takes the next instead of waiting for it.
    let n = parts.max(1);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let out: Vec<std::sync::Mutex<Option<T>>> = (0..n).map(|_| std::sync::Mutex::new(None)).collect();
    let work = || loop {
        let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if k >= n {
            break;
        }
        let v = f(k);
        *out[k].lock().unwrap_or_else(|e| e.into_inner()) = Some(v);
    };
    pool.in_place_scope(|s| {
        let work = &work;
        for _ in 1..n {
            s.spawn(move |_| work());
        }
        work();
    });
    out.into_iter().map(|o| o.into_inner().unwrap_or_else(|e| e.into_inner()).expect("render part")).collect()
}

impl GpuTimers {
    /// Take in the pass times of the last timed frame once its readback has arrived.
    pub(crate) fn collect(&mut self, period: f64) {
        let t = self;
        if !t.waiting || !t.ready.swap(false, std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        let n = t.pending.len() * 2;
        {
            let view = t.read.slice(0..n as u64 * 8).get_mapped_range();
            let stamps: &[u64] = bytemuck::cast_slice(&view[..n * 8]);
            // The passes in the order the GPU finished them, each counted from where the one
            // before it ended (the untimed passes in between go to the next timed one). A
            // tile-based GPU takes a pass's first stamp when its vertex stage starts, well
            // before the pass in front of it has finished its fragments: measured from their
            // own first stamps the post passes of the enhanced path overlapped and added up
            // to 25 ms of a 10 ms frame.
            if omsi_cfg::flags::OMSI_GPU_TIMERS_RAW.is_set() {
                log::info!(
                    "gpu stamps: {:?}",
                    t.pending
                        .iter()
                        .enumerate()
                        .map(|(k, label)| (*label, stamps[k * 2], stamps[k * 2 + 1]))
                        .collect::<Vec<_>>()
                );
            }
            let mut order: Vec<(u64, u64, &'static str)> = t
                .pending
                .iter()
                .enumerate()
                .map(|(k, label)| (stamps[k * 2], stamps[k * 2 + 1], *label))
                .filter(|(a, b, _)| *b >= *a && *b > 0)
                .collect();
            order.sort_by_key(|(_, b, _)| *b);
            let mut prev: Option<u64> = None;
            for (a, b, label) in &order {
                let from = prev.unwrap_or(*a);
                let e = t.totals.entry(label).or_default();
                e.0 += b.saturating_sub(from) as f64 * period * 1e-9;
                e.1 += 1;
                prev = Some(*b);
            }
            if let (Some(first), Some(last)) = (order.iter().map(|o| o.0).min(), order.last()) {
                let e = t.totals.entry("(all passes)").or_default();
                e.0 += last.1.saturating_sub(first) as f64 * period * 1e-9;
                e.1 += 1;
            }
        }
        t.read.unmap();
        t.waiting = false;
    }
}

impl Renderer {
    /// Take in the pass times of the last timed frame once its readback has arrived.
    pub(crate) fn collect_gpu_timers(&mut self) {
        let period = self.queue.get_timestamp_period() as f64;
        for t in self.gpu_timers.iter_mut().flatten() {
            t.collect(period);
            // The last frame's stamps are read in a command buffer of their own, submitted
            // after that frame's: resolved in the frame's own buffer, the end stamp of its
            // last pass was often not written yet (Metal) and read as a stale number.
            if t.unresolved {
                t.unresolved = false;
                let n = t.pending.len() as u32 * 2;
                let mut enc = self
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("pass timers"),
                    });
                enc.resolve_query_set(&t.set, 0..n, &t.resolve, 0);
                enc.copy_buffer_to_buffer(&t.resolve, 0, &t.read, 0, n as u64 * 8);
                self.queue.submit([enc.finish()]);
                let ready = t.ready.clone();
                t.read.map_async(wgpu::MapMode::Read, .., move |r| {
                    ready.store(r.is_ok(), std::sync::atomic::Ordering::Relaxed)
                });
                t.waiting = true;
            }
        }
    }

    /// Average GPU time per pass so far: (pass, milliseconds, frames measured).
    /// The mirrors' frames (drawn without the overlays) are timed on their own.
    pub fn gpu_pass_times(&self) -> Vec<(String, f64, u32)> {
        let mut out = Vec::new();
        for (k, t) in self.gpu_timers.iter().enumerate() {
            let Some(t) = t else { continue };
            for (label, v) in &t.totals {
                let name = if k == 0 {
                    format!("mirrors: {label}")
                } else {
                    label.to_string()
                };
                out.push((name, v.0 / v.1.max(1) as f64 * 1000.0, v.1));
            }
        }
        out
    }
}

#[cfg(test)]
mod run_parts_tests {
    /// Every part runs once and comes back in its place, however the threads take them -
    /// with the caller taking them too while a helper is slow to start.
    #[test]
    fn every_part_once_in_order() {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(2).build().unwrap();
        let runs = std::sync::atomic::AtomicUsize::new(0);
        let out = super::run_parts(Some(&pool), 9, |k| {
            runs.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if k == 1 {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            k * 10
        });
        assert_eq!(out, (0..9).map(|k| k * 10).collect::<Vec<_>>());
        assert_eq!(runs.into_inner(), 9);
        assert_eq!(super::run_parts(None, 3, |k| k), vec![0, 1, 2]);
        assert_eq!(super::run_parts(Some(&pool), 0, |k| k), vec![0]);
    }
}
