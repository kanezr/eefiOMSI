//! What the device reports on its own: errors nobody caught and its loss.

use crate::*;

/// Set by the device's handlers, read by `render_inner` and `Renderer::device_lost`.
pub(crate) struct ErrorFlags {
    pub gpu_error: Arc<std::sync::atomic::AtomicBool>,
    pub rt_error: Arc<std::sync::atomic::AtomicBool>,
    pub out_of_memory: Arc<std::sync::atomic::AtomicBool>,
    pub device_lost: Arc<std::sync::Mutex<Option<String>>>,
}

pub(crate) fn install(device: &wgpu::Device, format: wgpu::TextureFormat, options: &RenderOptions) -> ErrorFlags {
    let msaa = options.msaa;
    // A GPU error while multisampling is on is logged and switches multisampling off
    // at the next frame (`render_inner`). Otherwise it is logged and the game goes on:
    // wgpu's own handler ends the process, and one call a driver refused (a limit of
    // that card, a bigger map than the last) closed the game a few seconds into the
    // drive - a wrong picture for a frame is better than no game. The first errors and
    // then every thousandth reach the log.
    let gpu_error = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let rt_error = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let out_of_memory = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let device_lost: Arc<std::sync::Mutex<Option<String>>> = Default::default();
    {
        let lost = device_lost.clone();
        device.set_device_lost_callback(move |reason, message| {
            // (dropping the device at the end also calls this: that is no loss)
            if matches!(reason, wgpu::DeviceLostReason::Destroyed) {
                return;
            }
            log::error!("the graphics device was lost ({reason:?}): {message}");
            *lost.lock().unwrap_or_else(|e| e.into_inner()) = Some(format!("{reason:?}: {message}"));
        });
    }
    {
        let flag = gpu_error.clone();
        let rt_flag = rt_error.clone();
        let rt_on = options.ray_tracing;
        let oom = out_of_memory.clone();
        let count = Arc::new(std::sync::atomic::AtomicU64::new(0));
        device.on_uncaptured_error(Arc::new(move |e: wgpu::Error| {
            if matches!(e, wgpu::Error::OutOfMemory { .. }) {
                oom.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            let n = count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if rt_on {
                // (the ray tracing goes first: the likelier cause, and the dearer feature)
                if !rt_flag.swap(true, std::sync::atomic::Ordering::Relaxed) {
                    log::error!("GPU error with ray tracing (Enhanced+ draws as Enhanced from now on): {}", gpu_error_text(&e));
                }
            } else if msaa > 1 && !flag.swap(true, std::sync::atomic::Ordering::Relaxed) {
                log::error!("GPU error with {msaa}x MSAA (drawing without it from now on): {}", gpu_error_text(&e));
            } else if n < 20 || n % 1000 == 0 {
                log::error!("GPU error #{} (the game goes on): {}", n + 1, gpu_error_text(&e));
            }
        }));
    }
    #[cfg(not(feature = "test-hooks"))]
    let _ = format;
    #[cfg(feature = "test-hooks")]
    if omsi_cfg::flags::OMSI_FAKE_GPU_ERROR.var() == Some("build") && msaa > 1 {
        // test hook for the fallback in `new_with`: a sample count no device takes
        let _ = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("invalid"),
            size: wgpu::Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 3,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
    }
    ErrorFlags { gpu_error, rt_error, out_of_memory, device_lost }
}
