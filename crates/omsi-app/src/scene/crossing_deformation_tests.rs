//! Artificial junctions whose road entrance extends beyond their deformation field.
use super::*;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "openomsi-crossing-field-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn write(&self, name: &str, text: &str) {
        std::fs::write(self.0.join(name), text).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// .x coordinates are x across, z up, y along. Use separate stations so the
// uncovered entrance remains flat rather than interpolating a distant field lift.
fn strip(stations: &[(f32, f32)]) -> String {
    let vertices: Vec<_> = stations
        .iter()
        .flat_map(|&(y, z)| [-3.0, 3.0].map(|x| format!("{x};{z};{y};")))
        .collect();
    let faces: Vec<_> = (0..stations.len() - 1)
        .flat_map(|i| {
            let a = 2 * i;
            [
                format!("3;{a},{},{a_plus};", a + 2, a_plus = a + 1),
                format!("3;{},{},{};", a + 1, a + 2, a + 3),
            ]
        })
        .collect();
    format!(
        "xof 0303txt 0032\nMesh strip {{\n{};\n{};\n{};\n{};\n}}\n",
        vertices.len(),
        vertices.join(",\n"),
        faces.len(),
        faces.join(",\n")
    )
}

fn staged(path: PathBuf, ot: Arc<ObjectType>, pose: Pose) -> StagedTile {
    StagedTile {
        tx: 0,
        ty: 0,
        origin: DVec3::ZERO,
        path,
        base_terrain: Terrain::flat(),
        align: Vec::new(),
        hole_rims: Vec::new(),
        water: None,
        bakes_light_map: false,
        splines: Vec::new(),
        meshes: Mutex::new(Some(Vec::new())),
        drive: Vec::new(),
        lanes: Mutex::new(Vec::new()),
        street_points: Vec::new(),
        objects: vec![StagedObject {
            ot,
            id: 1,
            place: Placement::Pose(pose),
            rules: Vec::new(),
            extra: Vec::new(),
            lamp_parent: None,
            parked: false,
            map_object: true,
            instance: 0,
            key: 1,
        }],
        anchors: Vec::new(),
        counts: LoadStats::default(),
        resolved: std::sync::OnceLock::new(),
    }
}

#[test]
fn uncovered_crossing_entrance_preserves_render_and_wheel_heights() {
    for field_sign in [-1.0, 1.0] {
        // Also retain an intentional 20 cm step; deformation must not stitch it away.
        for authored_step in [0.0, 0.2] {
            let fixture = Fixture::new();
            fixture.write("global.cfg", "[name]\nArtificial junction\n");
            fixture.write("junction.sco", "[surface]\n[absheight]\n[mesh]\njunction.x\n[crossing_heightdeformation]\nfield.x\n");
            fixture.write(
                "junction.x",
                &strip(&[0.0, 2.0, 4.0, 6.0, 8.0].map(|y| (y, 0.1 + authored_step))),
            );
            fixture.write(
                "field.x",
                &strip(&[(4.0, field_sign * 0.4), (8.0, field_sign * 0.8)]),
            );
            let world = World::open(&fixture.0, &fixture.0.join("global.cfg"), 20261001).unwrap();
            let ot = world
                .object_type("junction.sco")
                .expect("artificial junction loads");
            let field = ot.deform.as_ref().unwrap();
            assert_eq!(field_height(field, 0.0, 0.0), None);

            for elevation in [0.0, 20.0] {
                for heading in [0.0, 37.0] {
                    fixture.write("tile_0_0.map", &format!(
                        "[version]\n14\n[object]\n0\njunction.sco\n1\n100\n100\n{elevation}\n{heading}\n0\n0\n0\n\n"));
                    let tile = omsi_map::Tile::load(&fixture.0.join("tile_0_0.map")).unwrap();
                    let object = &tile.objects[0];
                    let pose = Pose {
                        pos: DVec3::from(object.pos),
                        rot: object_rotation(omsi_geometry::map_rotation(object.rot)),
                    };
                    let src: HashMap<_, _> = [(
                        (0, 0),
                        Arc::new(staged(fixture.0.join("tile_0_0.map"), ot.clone(), pose)),
                    )]
                    .into_iter()
                    .collect();
                    let warped = world.warp_crossings(&src[&(0, 0)], &src);
                    let mesh = &warped.get(&0).expect("covered stations are deformed")[0];
                    for (before, after) in ot.meshes[0].0.positions.iter().zip(&mesh.positions) {
                        assert_eq!(after.truncate(), before.truncate());
                        let expected = if before.y < 4.0 {
                            before.z
                        } else {
                            before.z + field_sign * before.y * 0.1
                        };
                        assert!(
                            (after.z - expected).abs() < 1e-6,
                            "vertex {before:?}: {after:?}, expected {expected}"
                        );
                        if before.y < 4.0 {
                            assert_eq!(
                                after.z.to_bits(),
                                before.z.to_bits(),
                                "uncovered vertices remain exact"
                            );
                        }
                    }
                    assert!(mesh
                        .normals
                        .iter()
                        .all(|n| n.is_finite() && (n.length() - 1.0).abs() < 1e-5));

                    let road = omsi_geometry::mesh_from_o3d(
                        &omsi_o3d::xfile::parse_x(strip(&[(-5.0, 0.1), (0.0, 0.1)]).as_bytes())
                            .unwrap(),
                    );
                    let mut surface = TileSurface::new(128);
                    surface.add_drive_mesh(&road, &pose.rot, pose.pos, 0, 0);
                    surface.add_drive_mesh(mesh, &pose.rot, pose.pos, 0, 0);
                    surface.finish();
                    // The production wheel query, across the seam at both tyre tracks.
                    for lateral in [-1.5, 1.5] {
                        let sample = |y| {
                            let p = pose.pos
                                + pose
                                    .rot
                                    .transform_point3(glam::Vec3::new(lateral, y, 0.0))
                                    .as_dvec3();
                            probe_tile(Some(&surface), None, (0, 0), p.x, p.y, elevation + 2.0)
                                .below
                                .unwrap()
                        };
                        let approach = sample(-0.05);
                        let entrance = sample(0.05);
                        assert!((approach - (elevation + 0.1)).abs() < 1e-5);
                        assert!(
                            (entrance - approach - authored_step as f64).abs() < 1e-5,
                            "wheel step {approach} -> {entrance}, authored {authored_step}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn a_crossing_entirely_outside_its_field_keeps_its_original_mesh() {
    let fixture = Fixture::new();
    fixture.write("global.cfg", "[name]\nArtificial uncovered junction\n");
    fixture.write(
        "junction.sco",
        "[surface]\n[mesh]\njunction.x\n[crossing_heightdeformation]\nfield.x\n",
    );
    fixture.write("junction.x", &strip(&[(0.0, 0.1), (2.0, 0.1)]));
    fixture.write("field.x", &strip(&[(4.0, 0.4), (8.0, 0.8)]));
    let world = World::open(&fixture.0, &fixture.0.join("global.cfg"), 20261001).unwrap();
    let ot = world.object_type("junction.sco").unwrap();
    let src: HashMap<_, _> = [(
        (0, 0),
        Arc::new(staged(
            fixture.0.join("tile_0_0.map"),
            ot.clone(),
            Pose {
                pos: DVec3::new(100.0, 100.0, 20.0),
                rot: Mat4::IDENTITY,
            },
        )),
    )]
    .into_iter()
    .collect();
    assert!(world.warp_crossings(&src[&(0, 0)], &src).is_empty());
    assert!(ot.meshes[0].0.positions.iter().all(|p| p.z == 0.1));
}

#[test]
fn ai_lane_heights_are_independent_of_visual_vertex_coverage() {
    for covered_vertices in [false, true] {
        for field_sign in [-1.0, 0.0, 1.0] {
            let fixture = Fixture::new();
            fixture.write(
                "global.cfg",
                "[name]\nArtificial AI junction\n[map]\n0\n0\ntile_0_0.map\n",
            );
            fixture.write(
                "tile_0_0.map",
                "[version]\n14\n[object]\n0\njunction.sco\n1\n100\n100\n20\n37\n0\n0\n0\n\n",
            );
            fixture.write("junction.sco", "[surface]\n[absheight]\n[mesh]\njunction.x\n[crossing_heightdeformation]\nfield.x\n[path]\n0\n0\n0.1\n0\n0\n10\n0\n0\n0\n3\n0\n0\n");
            // The coarse variant spans the field with triangles but has no vertex
            // inside it. AI path samples at y=4 and y=6 are covered in both variants.
            let stations = if covered_vertices {
                vec![(0.0, 0.1), (4.0, 0.1), (6.0, 0.1), (10.0, 0.1)]
            } else {
                vec![(0.0, 0.1), (10.0, 0.1)]
            };
            fixture.write("junction.x", &strip(&stations));
            fixture.write(
                "field.x",
                &strip(&[(4.0, field_sign * 0.4), (6.0, field_sign * 0.6)]),
            );
            let world = World::open(&fixture.0, &fixture.0.join("global.cfg"), 20261001).unwrap();
            let (prepared, _) = world.prepare_tiles(&[(0, 0, fixture.0.join("tile_0_0.map"))]);
            assert_eq!(prepared.len(), 1);
            assert_eq!(prepared[0].objects.len(), 1);
            assert_eq!(
                prepared[0].objects[0].warped.is_some(),
                covered_vertices && field_sign != 0.0
            );

            let lanes = world.lanes.lock();
            assert_eq!(lanes.len(), 1, "the map's artificial [path] is placed");
            let lane = &lanes[0];
            assert_eq!(lane.points.len(), 6);
            let rotation = object_rotation([37.0, 0.0, 0.0]);
            for (i, point) in lane.points.iter().enumerate() {
                let y = i as f32 * 2.0;
                let expected_xy = DVec3::new(100.0, 100.0, 20.0)
                    + rotation
                        .transform_point3(glam::Vec3::new(0.0, y, 0.1))
                        .as_dvec3();
                assert!((point.truncate() - expected_xy.truncate()).length() < 1e-5);
                let offset = if (4.0..=6.0).contains(&y) {
                    field_sign * y * 0.1
                } else {
                    0.0
                };
                assert!((point.z - (20.1 + offset as f64)).abs() < 1e-5,
                    "covered_vertices={covered_vertices}, field_sign={field_sign}, y={y}: lane height {}", point.z);
            }
        }
    }
}
