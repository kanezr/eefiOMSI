use super::*;

fn renderer() -> Renderer {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    pollster::block_on(Renderer::new_with(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions {
            msaa: 1,
            ssao: false,
            shadow_size: 1024,
            fxaa: false,
            render_scale: 1.0,
            ..Default::default()
        },
    ))
    .expect("test renderer")
}

fn shadowed(renderer: &Renderer, scene: &mut Scene, y: f32) -> Vec<i32> {
    let (_, lamps) = renderer.prepare_lights(scene, Vec3::new(0.0, y, 1.5), true, true, None);
    let mut ys: Vec<i32> = lamps.iter().map(|l| l.position.y.round() as i32).collect();
    ys.sort();
    ys
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn a_mirror_picture_leaves_the_street_lamps_their_shadow_maps() {
    let renderer = renderer();
    let mut scene = renderer.new_scene();
    scene.lights = (0..5)
        .map(|k| PointLight {
            position: DVec3::new(0.0, k as f64 * 10.0, 6.0),
            radius: 20.0,
            color: [1.0, 0.8, 0.5],
            core: 3.0,
            housed: true,
            mode: LightMode::Both,
            ..Default::default()
        })
        .collect();
    assert_eq!(shadowed(&renderer, &mut scene, 15.0), vec![0, 10, 20, 30]);
    assert_eq!(shadowed(&renderer, &mut scene, 21.0), vec![0, 10, 20, 30]);
    let _ = renderer.prepare_lights(&mut scene, Vec3::new(0.0, 21.0, 1.5), false, false, None);
    assert_eq!(shadowed(&renderer, &mut scene, 21.0), vec![0, 10, 20, 30]);
}