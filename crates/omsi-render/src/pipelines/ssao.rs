//! Ambient occlusion, and the lamps in the fog: both read the depth prepass.

use super::common::*;
use crate::*;

pub(crate) struct Ssao {
    pub layout: wgpu::BindGroupLayout,
    pub buf: wgpu::Buffer,
    pub sampler: wgpu::Sampler,
    pub ssao_pipeline: Option<wgpu::RenderPipeline>,
    pub blur_pipeline: Option<wgpu::RenderPipeline>,
}

/// The depth and the AO texture beside the uniform at 0 (ssao.wgsl, fog_lamps.wgsl).
fn depth_reading_layout(device: &wgpu::Device, label: &str) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
        entries: &[
            uniform_entry(0, wgpu::ShaderStages::FRAGMENT),
            texture_entry(1, wgpu::TextureSampleType::Depth, wgpu::TextureViewDimension::D2),
            texture_entry(2, wgpu::TextureSampleType::Float { filterable: false }, wgpu::TextureViewDimension::D2),
        ],
    })
}

pub(crate) fn ssao(device: &wgpu::Device, gl: bool) -> Ssao {
    // ambient occlusion: a depth prepass with the camera projection, then the AO and a blur
    log::info!("renderer: compiling the SSAO shaders");
    let ssao_shader = compile_shader(device, wgpu::ShaderModuleDescriptor {
        label: Some("ssao"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../ssao.wgsl").into()),
    });
    let ao_layout = depth_reading_layout(device, "ssao");
    let ao_buf = uniform_buffer(device, "ssao params", std::mem::size_of::<SsaoUniform>() as u64);
    let ao_sampler = clamped_linear_sampler(device);
    let ao_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("ssao"),
        bind_group_layouts: &[Some(&ao_layout)],
        immediate_size: 0,
    });
    let targets = [target(wgpu::TextureFormat::Rg16Float, None)];
    let make_ao = |entry: &str| no_vertex_pipeline(entry, &ao_pl, &ssao_shader, entry, &targets).create(device);
    let ssao_pipeline = (!gl).then(|| make_ao("fs_ssao"));
    let blur_pipeline = (!gl).then(|| make_ao("fs_blur"));
    Ssao { layout: ao_layout, buf: ao_buf, sampler: ao_sampler, ssao_pipeline, blur_pipeline }
}

pub(crate) struct FogLamps {
    pub layout: wgpu::BindGroupLayout,
    pub buf: wgpu::Buffer,
    pub pipeline: Option<[wgpu::RenderPipeline; 2]>,
}

pub(crate) fn fog_lamps(device: &wgpu::Device, camera_layout: &wgpu::BindGroupLayout, hdr_format: wgpu::TextureFormat, gl: bool) -> FogLamps {
    // the lamps in the fog: the camera group (lights, grid, the enhanced uniform) and the
    // prepass depth, added onto the high-range picture
    let fog_lamps_layout = depth_reading_layout(device, "fog lamps");
    let fog_lamps_buf = uniform_buffer(device, "fog lamps params", std::mem::size_of::<FogLampUniform>() as u64);
    let fog_lamps_pipeline = (!gl && !array_path().no_storage() && !basic_pipelines()).then(|| optional_pipeline(device, "the lamps in the fog", || {
        log::info!("renderer: compiling the lamps in the fog shaders");
        let module = compile_shader(device, wgpu::ShaderModuleDescriptor {
            label: Some("fog lamps"),
            source: wgpu::ShaderSource::Wgsl(fog_lamps_shader_source().into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("fog lamps"),
            bind_group_layouts: &[Some(camera_layout), Some(&fog_lamps_layout)],
            immediate_size: 0,
        });
        let make = |entry: &str, blend: Option<wgpu::BlendState>| {
            no_vertex_pipeline(entry, &layout, &module, entry, &[target(hdr_format, blend)]).vs("vs_fog_lamps").create(device)
        };
        // (the second added onto what is there)
        let add = wgpu::BlendState {
            color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
            alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
        };
        [make("fs_fog_lamps", None), make("fs_fog_composite", Some(add))]
    })).flatten();
    FogLamps { layout: fog_lamps_layout, buf: fog_lamps_buf, pipeline: fog_lamps_pipeline }
}
