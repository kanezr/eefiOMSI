use super::*;

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn invisible_blended_pixels_do_not_hide_a_textured_layer_behind_them() {
    let adapter = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    for msaa in [1, 4] {
        let mut renderer = pollster::block_on(Renderer::new_with(
            &adapter,
            None,
            Some(wgpu::TextureFormat::Rgba8UnormSrgb),
            RenderOptions {
                msaa,
                ssao: false,
                shadow_size: 1024,
                fxaa: false,
                render_scale: 1.0,
                ..Default::default()
            },
        ))
        .expect("test renderer");
        let mut scene = renderer.new_scene();
        let texture = |renderer: &mut Renderer, scene: &mut Scene, rgba: Vec<u8>| {
            renderer.add_texture(
                scene,
                &omsi_texture::Image {
                    width: 1,
                    height: 1,
                    rgba,
                    has_alpha: true,
                },
                false,
            )
        };
        let clear = texture(&mut renderer, &mut scene, vec![255, 255, 255, 0]);
        let blue = texture(&mut renderer, &mut scene, vec![0, 0, 255, 128]);
        let near = renderer.add_material(&mut scene, Some(clear), AlphaMode::Blend, [1.0; 4], true);
        let far = renderer.add_material(&mut scene, Some(blue), AlphaMode::Blend, [1.0; 4], true);
        let green = renderer.add_material(
            &mut scene,
            None,
            AlphaMode::Opaque,
            [0.0, 1.0, 0.0, 1.0],
            true,
        );
        let quad = |y: f32, half: f32| {
            [
                Vec3::new(-half, y, -half),
                Vec3::new(half, y, -half),
                Vec3::new(half, y, half),
                Vec3::new(-half, y, half),
            ]
        };
        let wall = renderer.add_mesh(
            &mut scene,
            &MeshData {
                positions: quad(8.0, 20.0).to_vec(),
                normals: vec![-Vec3::Y; 4],
                uvs: vec![glam::Vec2::ZERO; 4],
                ranges: vec![(0, 6, 0)],
                indices: vec![0, 1, 2, 0, 2, 3],
                one_sided: false,
            },
        );
        renderer.add_instance(&mut scene, wall, DVec3::ZERO, Mat4::IDENTITY, vec![green]);
        // Authored order puts the near blend first: invisible pixels must not write
        // depth and reject the textured blend behind it, even with Z-write enabled.
        let layers = renderer.add_mesh(
            &mut scene,
            &MeshData {
                positions: [quad(2.0, 4.0), quad(4.0, 8.0)].concat(),
                normals: vec![-Vec3::Y; 8],
                uvs: vec![glam::Vec2::ZERO; 8],
                ranges: vec![(0, 6, 0), (6, 6, 1)],
                indices: vec![0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7],
                one_sided: false,
            },
        );
        renderer.add_instance(
            &mut scene,
            layers,
            DVec3::ZERO,
            Mat4::IDENTITY,
            vec![near, far],
        );
        let camera = Camera {
            position: DVec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            roll: 0.0,
            fov_deg: 90.0,
            near: 0.1,
            far: 100.0,
        };
        for (classic, enhanced) in [(true, false), (false, false), (false, true)] {
            let lighting = Lighting {
                classic,
                enhanced,
                shadows: false,
                fog_density: 0.0,
                ..Default::default()
            };
            let rgba = renderer
                .render_to_image(&mut scene, 64, 64, &camera, &lighting)
                .unwrap();
            let c = &rgba[(32 * 64 + 32) * 4..][..3];
            assert!(c[2] > 60 && c[1] > 60 && c[0] < 20, "invisible pane hid the blue layer: {c:?}; classic {classic}, enhanced {enhanced}, MSAA {msaa}");
        }
    }
}

#[test]
fn oversized_colour_texture_keeps_its_silhouette_when_fitted() {
    let side = 2048u32;
    let rgba = (0..side * side)
        .flat_map(|i| {
            if i % side % 2 == 0 && i / side % 2 == 0 {
                [40, 80, 120, 255]
            } else {
                [255, 255, 255, 0]
            }
        })
        .collect();
    let data = omsi_texture::TextureData {
        width: side,
        height: side,
        format: omsi_texture::PixelFormat::Rgba8,
        levels: vec![rgba],
        has_alpha: true,
        gpu_mips: true,
    };
    let small = fit_texture(&data, 1024).unwrap();
    assert_eq!((small.width, small.height), (1024, 1024));
    assert!(small.levels[0].chunks_exact(4).all(|p| {
        p[..3]
            .iter()
            .zip([40u8, 80, 120])
            .all(|(a, b)| a.abs_diff(b) <= 1)
            && p[3] == 64
    }));
}
