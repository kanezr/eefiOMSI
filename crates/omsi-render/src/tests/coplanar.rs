use super::*;

// The draw order's depth step (`draw_order_level`, `scene_clip`): of an object's surfaces in
// one plane the one later in the model is seen at every pixel, and nothing changes between
// objects. Without the step (`OMSI_NO_DRAW_ORDER_DEPTH=1`) the first two are a coin toss per
// pixel, about half of each.

fn renderer() -> Renderer {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    pollster::block_on(Renderer::new_with(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, ssao: false, shadow_size: 1024, fxaa: false, render_scale: 1.0, ..Default::default() },
    ))
    .expect("test renderer")
}

/// A flat grid of `n` x `n` cells over `half` metres each way from (cx, cy) at height `z`,
/// turned by `turn` degrees, its cells cut along one diagonal or the other.
fn grid(cx: f32, cy: f32, z: f32, half: f32, n: u32, turn: f32, flip: bool) -> MeshData {
    let (s, c) = turn.to_radians().sin_cos();
    let mut m = MeshData { positions: Vec::new(), normals: Vec::new(), uvs: Vec::new(), ranges: Vec::new(), indices: Vec::new(), one_sided: false };
    for j in 0..=n {
        for i in 0..=n {
            let (x, y) = (-half + 2.0 * half * i as f32 / n as f32, -half + 2.0 * half * j as f32 / n as f32);
            m.positions.push(Vec3::new(cx + x * c - y * s, cy + x * s + y * c, z));
            m.normals.push(Vec3::Z);
            m.uvs.push(glam::Vec2::ZERO);
        }
    }
    for j in 0..n {
        for i in 0..n {
            let k = j * (n + 1) + i;
            let (a, b, c2, d) = (k, k + 1, k + n + 2, k + n + 1);
            if flip { m.indices.extend([a, b, d, b, c2, d]) } else { m.indices.extend([a, b, c2, a, c2, d]) }
        }
    }
    m.ranges = vec![(0, m.indices.len() as u32, 0)];
    m
}

/// `grids` as one mesh, a material slot each, in that order.
fn slotted(grids: Vec<MeshData>) -> MeshData {
    let mut m = MeshData { positions: Vec::new(), normals: Vec::new(), uvs: Vec::new(), ranges: Vec::new(), indices: Vec::new(), one_sided: false };
    for (slot, g) in grids.into_iter().enumerate() {
        let (base, first) = (m.positions.len() as u32, m.indices.len() as u32);
        m.positions.extend(&g.positions);
        m.normals.extend(&g.normals);
        m.uvs.extend(&g.uvs);
        m.indices.extend(g.indices.iter().map(|i| i + base));
        m.ranges.push((first, g.indices.len() as u32, slot as u32));
    }
    m
}

/// Pixels of each colour (red, green, blue) from a low, a grazing, a steep and a high camera.
fn count(renderer: &mut Renderer, scene: &mut Scene) -> [usize; 3] {
    let lighting = Lighting { shadows: false, fog_density: 0.0, ..Default::default() };
    let mut n = [0; 3];
    for (pitch, height) in [(-3.0, 1.7), (-12.0, 1.7), (-40.0, 6.0), (-6.0, 25.0)] {
        let camera = Camera { position: DVec3::new(3.3, -2.1, height), yaw: 20.0, pitch, roll: 0.0, fov_deg: 60.0, near: 0.1, far: 3000.0 };
        scene.dirty = true;
        let rgba = renderer.render_to_image(scene, 128, 128, &camera, &lighting).unwrap();
        for p in rgba.chunks(4) {
            for (k, n) in n.iter_mut().enumerate() {
                let others = (0..3).filter(|&o| o != k).all(|o| p[o] < 60);
                if p[k] > 150 && others {
                    *n += 1;
                }
            }
        }
    }
    n
}

/// Two meshes of one object in one plane after four others of it (the second from the same
/// place, created right after it, cut into cells of 8 m along the other diagonal and
/// turned, all over the first's 5 m cells): the second is seen at every pixel.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn an_objects_later_mesh_in_one_plane_is_seen() {
    let mut renderer = renderer();
    let mut scene = renderer.new_scene();
    let red = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [1.0, 0.0, 0.0, 1.0], true);
    let green = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [0.0, 1.0, 0.0, 1.0], true);
    let a = renderer.add_mesh(&mut scene, &grid(0.0, 0.0, 0.0, 150.0, 60, 0.0, false));
    let b = renderer.add_mesh(&mut scene, &grid(0.0, 0.0, 0.0, 400.0, 100, 33.0, true));
    let origin = DVec3::new(-61.0, 12.5, 0.0);
    // (as in a bus: its meshes all from one place, the two far down the list)
    let blue = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [0.0, 0.0, 1.0, 1.0], true);
    renderer.begin_object(&mut scene);
    for k in 0..4 {
        let other = renderer.add_mesh(&mut scene, &grid(0.0, 0.0, -3.0 - k as f32, 2.0, 1, 0.0, false));
        renderer.add_instance(&mut scene, other, origin, Mat4::IDENTITY, vec![blue]);
    }
    renderer.add_instance(&mut scene, a, origin, Mat4::IDENTITY, vec![red]);
    renderer.add_instance(&mut scene, b, origin, Mat4::IDENTITY, vec![green]);
    let [r, g, _] = count(&mut renderer, &mut scene);
    // (a few pixels in a thousand far out at a grazing angle are left, where the step that
    // would hold them would also bury a centimetre between two objects)
    assert!(g > 15_000 && r <= g / 200, "{r} pixels of the first mesh, {g} of the second");
}

/// One mesh, three slots in one plane: the third is drawn over the other two.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn a_meshs_last_slot_in_one_plane_is_seen() {
    let mut renderer = renderer();
    let mut scene = renderer.new_scene();
    let red = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [1.0, 0.0, 0.0, 1.0], true);
    let blue = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [0.0, 0.0, 1.0, 1.0], true);
    let green = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [0.0, 1.0, 0.0, 1.0], true);
    let mesh = renderer.add_mesh(&mut scene, &slotted(vec![
        grid(0.0, 0.0, 0.0, 150.0, 60, 0.0, false),
        grid(10.0, 5.0, 0.0, 300.0, 70, 17.0, true),
        grid(0.0, 0.0, 0.0, 400.0, 100, 33.0, true),
    ]));
    renderer.begin_object(&mut scene);
    renderer.add_instance(&mut scene, mesh, DVec3::new(-61.0, 12.5, 0.0), Mat4::IDENTITY, vec![red, blue, green]);
    let [r, g, b] = count(&mut renderer, &mut scene);
    // (as above, and the more so where the plane is seen flatter than 1 in 20, whose step
    // stays at what 1 in 20 gives, so that the edge of a curved mesh is not pulled forward:
    // under one in a hundred)
    assert!(g > 15_000 && r + b <= g / 100, "first slot {r}, second {b}, last {g} pixels");
}

/// Between objects nothing moves: a plate a centimetre over another object's ground, whose
/// slot there is two steps up, stays over it near and far.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn a_plate_a_centimetre_over_another_objects_ground_stays_over_it() {
    let mut renderer = renderer();
    let mut scene = renderer.new_scene();
    let red = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [1.0, 0.0, 0.0, 1.0], true);
    let blue = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [0.0, 0.0, 1.0, 1.0], true);
    let green = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [0.0, 1.0, 0.0, 1.0], true);
    // (80 m each way: further out, at a grazing angle, a centimetre is below what the depth
    // resolves, with or without the step)
    let raised = renderer.add_mesh(&mut scene, &grid(0.0, 0.0, 0.01, 80.0, 32, 0.0, false));
    renderer.begin_object(&mut scene);
    renderer.add_instance(&mut scene, raised, DVec3::ZERO, Mat4::IDENTITY, vec![green]);
    // (the ground under it, inside it: an object of three slots in one plane, the last
    // lying on the other two and so two steps up)
    let ground = renderer.add_mesh(&mut scene, &slotted(vec![
        grid(0.0, 0.0, 0.0, 10.0, 2, 0.0, false),
        grid(0.0, 0.0, 0.0, 8.0, 2, 10.0, false),
        grid(0.0, 0.0, 0.0, 55.0, 14, 33.0, true),
    ]));
    renderer.begin_object(&mut scene);
    renderer.add_instance(&mut scene, ground, DVec3::new(3.0, -2.0, 0.0), Mat4::IDENTITY, vec![blue, blue, red]);
    let [r, g, _] = count(&mut renderer, &mut scene);
    assert!(g > 3_000 && r == 0, "{r} pixels of the ground through the plate a centimetre over it, {g} of the plate");
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn a_text_a_tenth_of_a_millimetre_over_a_later_board_stays_seen_from_far() {
    let mut renderer = renderer();
    let mut scene = renderer.new_scene();
    let red = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [1.0, 0.0, 0.0, 1.0], true);
    let green = renderer.add_material(&mut scene, None, AlphaMode::Opaque, [0.0, 1.0, 0.0, 1.0], true);
    let text = renderer.add_mesh(&mut scene, &grid(0.0, 0.0, 0.0001, 2.0, 4, 0.0, false));
    let board = renderer.add_mesh(&mut scene, &grid(0.0, 0.0, 0.0, 4.0, 4, 0.0, true));
    renderer.begin_object(&mut scene);
    renderer.add_instance(&mut scene, text, DVec3::ZERO, Mat4::IDENTITY, vec![green]);
    renderer.add_instance(&mut scene, board, DVec3::ZERO, Mat4::IDENTITY, vec![red]);
    let lighting = Lighting { shadows: false, fog_density: 0.0, ..Default::default() };
    for d in [3.0, 15.0, 30.0] {
        for pitch in [-90.0f64, -45.0] {
            let (s, c) = pitch.to_radians().sin_cos();
            let camera = Camera { position: DVec3::new(0.0, -d * c, -d * s), yaw: 0.0, pitch: pitch as f32, roll: 0.0, fov_deg: 60.0, near: 0.1, far: 3000.0 };
            scene.dirty = true;
            let rgba = renderer.render_to_image(&mut scene, 256, 256, &camera, &lighting).unwrap();
            let green = rgba.chunks(4).filter(|p| p[1] > 150 && p[0] < 60 && p[2] < 60).count();
            let red = rgba.chunks(4).filter(|p| p[0] > 150 && p[1] < 60 && p[2] < 60).count();
            assert!(green * 4 > red, "at {d} m, pitch {pitch}: {green} pixels of the text, {red} of the board");
        }
    }
}