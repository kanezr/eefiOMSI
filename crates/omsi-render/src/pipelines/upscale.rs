//! The render scale: the smaller 3D picture scaled up to the window.

use super::common::*;

pub(crate) struct Upscale {
    pub upscale_pipeline: wgpu::RenderPipeline,
    pub copy_pipeline: wgpu::RenderPipeline,
    pub panel_pipeline: wgpu::RenderPipeline,
    pub layout: wgpu::BindGroupLayout,
    pub buf: wgpu::Buffer,
}

pub(crate) fn build(device: &wgpu::Device, format: wgpu::TextureFormat) -> Upscale {
    // --- render scale: the smaller 3D picture scaled up to the window
    log::info!("renderer: compiling the upscaler shaders");
    let upscale_shader = compile_shader(device, wgpu::ShaderModuleDescriptor {
        label: Some("upscale"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../upscale.wgsl").into()),
    });
    let upscale_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("upscale"),
        entries: &[
            uniform_entry(0, wgpu::ShaderStages::FRAGMENT),
            float_texture_entry(1),
            sampler_entry(2),
        ],
    });
    let upscale_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("upscale"),
        bind_group_layouts: &[Some(&upscale_layout)],
        immediate_size: 0,
    });
    let targets = [target(format, None)];
    let upscale_pipeline_for = |entry| no_vertex_pipeline("upscale", &upscale_pl, &upscale_shader, entry, &targets).create(device);
    let upscale_pipeline = upscale_pipeline_for("fs_main");
    let copy_pipeline = upscale_pipeline_for("fs_copy");
    let panel_pipeline = upscale_pipeline_for("fs_panel");
    let upscale_buf = uniform_buffer(device, "upscale params", 16);
    Upscale { upscale_pipeline, copy_pipeline, panel_pipeline, layout: upscale_layout, buf: upscale_buf }
}
