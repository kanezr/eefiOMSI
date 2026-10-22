use super::*;

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("openomsi-vehicles-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn entry(path: &str, data: &[u8]) -> Entry {
    Entry { path: path.into(), size: data.len() as u64, sha256: super::super::sha256_of(data) }
}

#[test]
fn a_bus_kept_after_a_restart_of_files_kept_before_it_is_served_whole() {
    let store = temp("restart-store");
    let kept = Buses::default();
    kept.open_in(&store).unwrap();
    let b = b"[model]\nmodel\\model.cfg\n".to_vec();
    let t = vec![5u8; 4000];
    let files = [(entry("Vehicles/Before/before.bus", &b), b.clone()), (entry("Vehicles/Before/t.dat", &t), t.clone())];
    let m = VehicleManifest { bus: "Vehicles/Before/before.bus".into(), entries: files.iter().map(|f| f.0.clone()).collect() }.validated().unwrap();
    // its files come (or are the server's own), but the bus is not finished...
    for (e, d) in &files {
        kept.put(e, d).unwrap();
    }
    // ...and the server starts again: the files are in the store, no list names them
    kept.open_in(&store).unwrap();
    assert!(kept.missing(&m).is_empty(), "the files are there");
    let (fp, new) = kept.commit(m.clone()).unwrap();
    assert!(new);
    for (e, d) in &files {
        assert_eq!(kept.blob(&e.sha256).as_deref(), Some(d.as_slice()), "{} is served", e.path);
    }
    assert_eq!(kept.manifest(&fp), Some(m));
    let _ = std::fs::remove_dir_all(&store);
}

#[test]
fn buses_nobody_uses_go_but_never_one_in_use_a_shared_file_or_one_on_its_way() {
    let store = temp("gc-store");
    let kept = Buses::default();
    kept.open_in(&store).unwrap();
    let shared = vec![1u8; 1000];
    let bus = |name: &str, own: &[u8]| -> (VehicleManifest, Vec<(Entry, Vec<u8>)>) {
        let b = format!("[model]
{name}
").into_bytes();
        let files = vec![
            (entry(&format!("Vehicles/{name}/{name}.bus"), &b), b.clone()),
            (entry(&format!("Vehicles/{name}/t.dat"), own), own.to_vec()),
            (entry("Vehicles/Shared/s.dat", &shared), shared.clone()),
        ];
        let m = VehicleManifest { bus: format!("Vehicles/{name}/{name}.bus"), entries: files.iter().map(|f| f.0.clone()).collect() }.validated().unwrap();
        (m, files)
    };
    let keep = |name: &str, own: &[u8], days_ago: u64| -> (String, VehicleManifest) {
        let (m, files) = bus(name, own);
        for (e, d) in &files {
            kept.put(e, d).unwrap();
        }
        let (fp, _) = kept.commit(m.clone()).unwrap();
        let f = std::fs::OpenOptions::new().write(true).open(store.join("vehicles").join(format!("{fp}.json"))).unwrap();
        f.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(days_ago * 86_400 + 3600)).unwrap();
        (fp, m)
    };
    let (old, m_old) = keep("Old", &[2u8; 500], 40);
    let (driven, _) = keep("Driven", &[3u8; 500], 40);
    let (recent, m_recent) = keep("Recent", &[4u8; 500], 1);
    let (coming, m_coming) = keep("Coming", &[5u8; 500], 40);
    // one is being driven now; one is being sent again (its files held)
    kept.in_use([driven.as_str()].into_iter());
    let lease = kept.lease(&m_coming);
    let c = kept.collect(0, IN_USE);
    assert_eq!(c.buses, 1, "only the old one nobody uses");
    assert!(kept.manifest(&old).is_none() && kept.manifest(&driven).is_some() && kept.manifest(&recent).is_some() && kept.manifest(&coming).is_some());
    // its own file went, the shared one (still used by the others) stayed
    assert!(kept.blob(&m_old.entries.iter().find(|e| e.path.ends_with("t.dat")).unwrap().sha256).is_none());
    assert_eq!(kept.blob(&super::super::sha256_of(&shared)), Some(shared.clone()));
    drop(lease);
    // short of room: the least lately used that nobody drives go (here: the one
    // that was on its way, then the recent one) - never the one being driven
    let c = kept.collect(u64::MAX, IN_USE);
    assert_eq!(c.buses, 2);
    assert!(kept.manifest(&coming).is_none() && kept.manifest(&recent).is_none() && kept.manifest(&driven).is_some());
    assert!(kept.blob(&m_recent.entries.iter().find(|e| e.path.ends_with("t.dat")).unwrap().sha256).is_none());
    assert_eq!(kept.blob(&super::super::sha256_of(&shared)), Some(shared));
    let _ = std::fs::remove_dir_all(&store);
}

fn queued(sha: &str) -> bool {
    buses().recompressing.lock().unwrap().queued.contains(sha)
}

/// A host packs a file at its smallest once a game that reads zstd asked for it (in the
/// background: the first get it at the speed of the moment); the bytes every piece
/// unpacks to stay the file's own, and the packed pieces go with their file.
#[test]
fn a_kept_file_goes_packed_at_its_smallest_once_it_has_been_asked_for() {
    let _turn = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let store = temp("packed-store");
    open_for_test(&store);
    keep_for_days(30);
    // a texture of several pieces that packs, and noise that does not
    let tex: Vec<u8> = (0..(2 * pieces::PIECE + 999)).map(|i| ((i / 7) % 251) as u8 ^ (i % 13) as u8).collect();
    let mut x = 0x1357_9bdfu32;
    let noise: Vec<u8> = (0..300_000)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x as u8
        })
        .collect();
    let bus = b"[model]\nmodel\\model.cfg\n".to_vec();
    let files = [(entry("Vehicles/PackBus/p.bus", &bus), bus.clone()), (entry("Vehicles/PackBus/t.dat", &tex), tex.clone()), (entry("Vehicles/PackBus/n.dat", &noise), noise.clone())];
    for (e, d) in &files {
        put(e, d).unwrap();
    }
    let m = VehicleManifest { bus: "Vehicles/PackBus/p.bus".into(), entries: files.iter().map(|f| f.0.clone()).collect() }.validated().unwrap();
    let (fp, _) = commit(m).unwrap();
    let (sha_t, sha_n) = (super::super::sha256_of(&tex), super::super::sha256_of(&noise));
    // a piece as a game gets it: (codec, bytes on the wire, bytes)
    let ask = |sha: &str, i: u64, zstd: bool| -> (String, u64, Vec<u8>) {
        let mut buf = Vec::new();
        super::super::upload::send_piece(&mut buf, &super::super::upload::Sha256::parse(sha).unwrap(), i, zstd).unwrap();
        let end = buf.iter().position(|b| *b == b'\n').unwrap();
        let head = String::from_utf8(buf[..end].to_vec()).unwrap();
        let w: Vec<&str> = head.strip_prefix("OK ").unwrap().split(' ').collect();
        let (raw, codec, wire): (u64, &str, u64) = (w[0].parse().unwrap(), w[1], w[2].parse().unwrap());
        assert_eq!(buf.len() - end - 1, wire as usize);
        (codec.to_string(), wire, pieces::unpack(codec, buf[end + 1..].to_vec(), raw).unwrap())
    };
    let piece = |i: u64| {
        let (start, len) = pieces::span(tex.len() as u64, i);
        tex[start as usize..(start + len) as usize].to_vec()
    };
    // a game before zstd: zlib, as ever, and nothing to pack
    assert_eq!(ask(&sha_t, 0, false).0, "z");
    assert!(!queued(&sha_t));
    // one that reads zstd, before the file is packed: at the speed of the moment, and the
    // file is queued
    let fast: Vec<(String, u64, Vec<u8>)> = (0..3).map(|i| ask(&sha_t, i, true)).collect();
    for (i, (codec, _, data)) in fast.iter().enumerate() {
        assert_eq!(codec, "s");
        assert_eq!(*data, piece(i as u64));
    }
    assert!(queued(&sha_t));
    assert_eq!(ask(&sha_n, 0, true).0, "-");
    // a file not kept here queues nothing
    let mut buf = Vec::new();
    super::super::upload::send_piece(&mut buf, &super::super::upload::Sha256::parse(&"0".repeat(64)).unwrap(), 0, true).unwrap();
    assert!(buf.starts_with(b"ERR"));
    assert!(!queued(&"0".repeat(64)));
    buses().recompress_queued();
    assert!(!queued(&sha_t) && !queued(&sha_n));
    // packed at its smallest: as small or smaller, the same bytes
    for i in 0..3 {
        let (codec, wire, data) = ask(&sha_t, i, true);
        assert_eq!(codec, "s");
        assert!(wire <= fast[i as usize].1, "piece {i}: {wire} packed, {} at once", fast[i as usize].1);
        assert_eq!(data, piece(i));
    }
    // the whole file: its own SHA-256 (the bus's fingerprint is made of these)
    let whole: Vec<u8> = (0..3).flat_map(|i| ask(&sha_t, i, true).2).collect();
    assert_eq!(super::super::sha256_of(&whole), sha_t);
    // noise is kept as a mark and goes as it is
    let (codec, wire, data) = ask(&sha_n, 0, true);
    assert_eq!((codec.as_str(), wire), ("-", noise.len() as u64));
    assert_eq!(data, noise);
    // the bus let go: its packed pieces go with its files
    let packed_here = || std::fs::read_dir(&store).unwrap().flatten().filter(|e| e.path().extension().is_some_and(|x| x == "zpiece")).count();
    assert_eq!(packed_here(), 4);
    let f = std::fs::OpenOptions::new().write(true).open(store.join("vehicles").join(format!("{fp}.json"))).unwrap();
    f.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(40 * 86_400)).unwrap();
    assert_eq!(buses().collect(0, IN_USE).buses, 1);
    assert_eq!(packed_here(), 0);
    let _ = std::fs::remove_dir_all(&store);
}

#[test]
fn the_same_bytes_refused_under_another_name_do_not_cost_a_bus_its_own_file() {
    let store = temp("poison");
    let kept = Buses::default();
    kept.open_in(&store).unwrap();
    let b = b"[model]\nreal.cfg\n".to_vec();
    // a player sends a bus file's bytes as a sound: refused (no WAVE), held back as that
    let as_sound = entry("Vehicles/X/a.wav", &b);
    kept.withhold(&as_sound, "not a WAVE sound");
    // the bus that has those bytes as its .bus still takes them, and is kept with them
    let bus_file = entry("Vehicles/Real/real.bus", &b);
    let m = VehicleManifest { bus: "Vehicles/Real/real.bus".into(), entries: vec![bus_file.clone()] }.validated().unwrap();
    assert_eq!(kept.missing(&m), vec![0]);
    kept.put(&bus_file, &b).unwrap();
    assert!(kept.commit(m).is_ok());
    let _ = std::fs::remove_dir_all(&store);
}

/// Review of #2564 (P1): a file put together from its pieces is held against the transfers'
/// budget while it is in memory. With no room it waits (`FINISH_WAIT`); past that its pieces
/// stay, its sender is told `busy`, and its last piece is asked for again - which puts it
/// together once there is room.
#[test]
fn putting_a_file_together_waits_for_room_in_the_transfers_budget() {
    use crate::mods::budget::{transfers, TRANSFER_MEMORY};
    let _turn = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let store = temp("finish-budget");
    let kept = Buses::default();
    kept.open_in(&store).unwrap();
    let data = b"a file in one piece".to_vec();
    let e = entry("Vehicles/Pieces/t.dat", &data);
    // no room, ever: told busy after the wait, the piece kept, the last piece asked again
    let all = transfers().hold(TRANSFER_MEMORY).unwrap();
    let t0 = std::time::Instant::now();
    assert_eq!(kept.put_piece_sent(&e, 0, &data), Err(crate::mods::failure::says::BUSY.to_string()));
    assert!(t0.elapsed() >= FINISH_WAIT, "{:?}", t0.elapsed());
    assert_eq!(kept.missing_pieces(&e), vec![0], "the last piece asked for again");
    assert!(kept.blob(&e.sha256).is_none());
    // room comes while it waits: put together then
    std::thread::scope(|s| {
        s.spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(200));
            drop(all);
        });
        assert_eq!(kept.put_piece_sent(&e, 0, &data), Ok(true));
    });
    assert_eq!(kept.blob(&e.sha256).as_deref(), Some(&data[..]));
    assert!(kept.missing_pieces(&e).is_empty());
    assert_eq!(transfers().held(), 0);
    let _ = std::fs::remove_dir_all(&store);
}

#[test]
fn only_a_bus_on_its_way_is_waited_for() {
    // (a store of its own, never opened: nothing is kept, the waiting is all there is)
    let kept = Buses::default();
    let (announced, made_up) = ("v1-announced", "v1-made-up");
    kept.announce([announced].into_iter());
    let t0 = std::time::Instant::now();
    assert!(!kept.wait_for(made_up, std::time::Duration::from_secs(30)));
    assert!(t0.elapsed() < ANNOUNCE_GRACE + std::time::Duration::from_millis(1500), "a made-up bus is not waited for");
    let t0 = std::time::Instant::now();
    assert!(!kept.wait_for(announced, std::time::Duration::from_secs(4)));
    assert!(t0.elapsed() >= std::time::Duration::from_millis(3900), "an announced one is");
    // one being sent is waited for too
    let b = b"[model]\nm.cfg\n".to_vec();
    let m = VehicleManifest { bus: "Vehicles/Sent/sent.bus".into(), entries: vec![entry("Vehicles/Sent/sent.bus", &b)] }.validated().unwrap();
    let _lease = kept.lease(&m);
    assert!(kept.on_its_way(&m.fingerprint()));
}

#[test]
fn what_players_send_makes_room_only_of_buses_unused_for_a_day() {
    let store = temp("sent-room");
    let kept = Buses::default();
    kept.open_in(&store).unwrap();
    let keep = |name: &str, hours_ago: u64| -> String {
        let b = format!("[model]\n{name}.cfg\n").into_bytes();
        let e = entry(&format!("Vehicles/{name}/{name}.bus"), &b);
        kept.put(&e, &b).unwrap();
        let m = VehicleManifest { bus: e.path.clone(), entries: vec![e] }.validated().unwrap();
        let (fp, _) = kept.commit(m).unwrap();
        let f = std::fs::OpenOptions::new().write(true).open(store.join("vehicles").join(format!("{fp}.json"))).unwrap();
        f.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(hours_ago * 3600)).unwrap();
        fp
    };
    let (played, old) = (keep("Played", 2), keep("Old", 50));
    assert_eq!(kept.collect(u64::MAX, SENT_MAKES_ROOM_OF).buses, 1);
    assert!(kept.manifest(&played).is_some() && kept.manifest(&old).is_none(), "the bus played two hours ago stays");
    // what this game fetches for its own players makes room of more
    assert_eq!(kept.collect(u64::MAX, IN_USE).buses, 1);
    let _ = std::fs::remove_dir_all(&store);
}

/// The host's frame asks which buses are in use once a minute: it never waits for the store
/// an upload holds (writing a whole file) - it looks again next time - and, with the store
/// free, the bus is marked in use.
#[test]
fn the_frame_never_waits_for_the_store_to_mark_buses_in_use() {
    let store = temp("in-use-busy");
    let kept = Buses::default();
    kept.open_in(&store).unwrap();
    let b = b"[model]\nmodel\\model.cfg\n".to_vec();
    let e = entry("Vehicles/Busy/busy.bus", &b);
    kept.put(&e, &b).unwrap();
    let m = VehicleManifest { bus: "Vehicles/Busy/busy.bus".into(), entries: vec![e] }.validated().unwrap();
    let (fp, _) = kept.commit(m).unwrap();
    let list = store.join("vehicles").join(format!("{fp}.json"));
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(5 * 86_400);
    std::fs::OpenOptions::new().write(true).open(&list).unwrap().set_modified(old).unwrap();
    let modified = || std::fs::metadata(&list).unwrap().modified().unwrap();
    {
        // (held as an upload holds it; the same thread would wait for ever if it waited)
        let _held = kept.lock();
        kept.in_use([fp.as_str()].into_iter());
        assert_eq!(modified(), old, "not marked while the store is held: next time");
    }
    kept.in_use([fp.as_str()].into_iter());
    assert!(modified() > old, "marked in use with the store free");
    let _ = std::fs::remove_dir_all(&store);
}
