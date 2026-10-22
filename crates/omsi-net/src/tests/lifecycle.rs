//! Socket-level lifecycle coverage. This exercises real UDP sessions, not the renderer
//! or installed bus assets; the two-game-client visual checks remain a separate test.
use super::*;

#[test]
fn late_join_repaint_bus_change_and_reconnect_recover_the_current_variable_schema() {
    let mut host = LanSession::host(0, "host", world("m"), true).unwrap();
    let addr = SocketAddr::from(([127, 0, 0, 1], host.local_addr().unwrap().port()));
    let mut owner = LanSession::join_addr(vec![addr], None, "owner", world("m")).unwrap();
    let mut poses = [pose(100.0), pose(200.0), pose(300.0)];
    pump(&mut [&mut host, &mut owner], &poses[..2], 100, |s| {
        s[1].connected && find(s[0], 200.0).is_some()
    });
    assert!(owner.connected);
    let owner_id = owner.my_id;
    let mut schema = var_schema::Schema {
        floats: vec!["door_0".into()],
        strings: vec!["display".into(), "snapshot".into()],
        ..Default::default()
    };
    // Publish before the observer exists. Its first usable state must be recovered
    // from periodic snapshots, including the unchanged long string and manifest.
    for _ in 0..10 {
        owner.send_var_schema(schema.table_hash(), &schema.encode().unwrap(), 0.1);
        owner.send_vars(
            schema.table_hash(),
            &[0],
            &[0.25],
            &[0, 1],
            &["display 0".into(), "0:".to_string() + &"árvíz".repeat(600)],
            0.1,
        );
        host.tick(0.1, &poses[0]);
        owner.tick(0.1, &poses[1]);
    }
    let mut observer =
        LanSession::join_addr(vec![addr], None, "late observer", world("m")).unwrap();
    pump(
        &mut [&mut host, &mut owner, &mut observer],
        &poses,
        100,
        |s| s[2].connected && find(s[2], 200.0).is_some(),
    );
    assert!(observer.connected);
    for phase in 0..4 {
        match phase {
            1 => poses[1].paint = "replacement repaint".into(),
            2 => {
                poses[1].bus = "Vehicles/other/other.bus".into();
                schema.floats.push("extra_variable".into());
            }
            3 => {
                assert!(owner.reconnect());
            }
            _ => {}
        }
        let table = schema.table_hash();
        poses[1].table = table;
        let text = schema.encode().unwrap();
        let values = [0.25 + phase as f32, 77.0];
        let ids = [0, 1];
        let texts = [
            format!("display {phase}"),
            format!("{phase}:{}", "árvíz".repeat(600)),
        ];
        let mut got_schema = [false; 2];
        let mut got_float = [false; 2];
        let mut got_strings = [[false; 2]; 2];
        let mut got_host = false;
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut last = Instant::now();
        while Instant::now() < deadline {
            let dt = last.elapsed().as_secs_f32();
            last = Instant::now();
            owner.send_var_schema(table, &text, dt);
            owner.send_vars(
                table,
                &ids[..schema.floats.len()],
                &values[..schema.floats.len()],
                &[0, 1],
                &texts,
                dt,
            );
            host.send_vars(123, &[0], &[42.0], &[], &[], dt);
            host.tick(dt, &poses[0]);
            owner.tick(dt, &poses[1]);
            observer.tick(dt, &poses[2]);
            for (slot, session) in [(0, &mut host), (1, &mut observer)] {
                for v in session
                    .take_var_schemas()
                    .into_iter()
                    .filter(|v| v.id == owner_id && v.table == table)
                {
                    got_schema[slot] |= v.strings.iter().any(|(_, s)| s == &text);
                }
                for v in session
                    .take_vars()
                    .into_iter()
                    .filter(|v| v.id == owner_id && v.table == table)
                {
                    got_float[slot] |= v.floats.contains(&(0, values[0]));
                    for (id, s) in v.strings {
                        for i in 0..2 {
                            got_strings[slot][i] |= id == i as u16 && s == texts[i];
                        }
                    }
                }
            }
            got_host |= owner
                .take_vars()
                .iter()
                .any(|v| v.id == 1 && v.table == 123 && v.floats.contains(&(0, 42.0)));
            let appearance = [&host, &observer].iter().all(|s| {
                find(s, 200.0)
                    .is_some_and(|p| p.pose.bus == poses[1].bus && p.pose.paint == poses[1].paint)
            });
            if appearance
                && got_host
                && got_schema == [true; 2]
                && got_float == [true; 2]
                && got_strings == [[true; 2]; 2]
                && owner.connected
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            owner.my_id, owner_id,
            "phase {phase}: returning owner retains its ID"
        );
        assert!(
            owner.connected && got_host,
            "phase {phase}: host-to-client state"
        );
        assert_eq!(
            got_schema, [true; 2],
            "phase {phase}: direct and relayed schemas"
        );
        assert_eq!(got_float, [true; 2], "phase {phase}: numeric values");
        assert_eq!(
            got_strings, [[true; 2]; 2],
            "phase {phase}: complete short and long strings"
        );
        for s in [&host, &observer] {
            let p = find(s, 200.0).expect("owner remains visible");
            assert_eq!(
                (&p.pose.bus, &p.pose.paint),
                (&poses[1].bus, &poses[1].paint),
                "phase {phase}"
            );
        }
    }
}
