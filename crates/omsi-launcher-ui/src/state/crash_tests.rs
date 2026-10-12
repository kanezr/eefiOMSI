#[test]
fn a_panic_is_found_and_a_clean_end_is_not() {
    let dir = std::env::temp_dir().join("openomsi-crash-test");
    let _ = std::fs::create_dir_all(&dir);
    let p = dir.join("game.log");
    std::fs::write(&p, "[t INFO x] loading\n[t ERROR openomsi_game] the game stopped on an error (build x): panicked at a.rs:1:1:\n    index out of bounds\n\n   0: std::backtrace\n").unwrap();
    let (what, tail) = super::crash_of(&p).unwrap();
    assert!(what.contains("index out of bounds"), "{what}");
    assert!(tail.contains("loading"));
    std::fs::write(&p, "[t INFO x] loading\n[t INFO openomsi_game::app_events] game ends\n").unwrap();
    assert!(super::crash_of(&p).is_none());
    std::fs::write(&p, "[t ERROR omsi_render] the graphics device was lost (Unknown): Unexpected error variant\n[t INFO openomsi_game::app_events] game ends\n").unwrap();
    assert!(super::crash_of(&p).unwrap().0.contains("device was lost"));
    // an error before the game started, or one it got over, is not the crash
    std::fs::write(&p, "[t ERROR omsi_render] a part of the picture could not be recorded (left out)\n[t INFO x] starting the game: omsi\n[t INFO omsi_render] renderer: compiling the sky and clouds shaders\n").unwrap();
    assert!(super::crash_of(&p).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

/// The report says what the game ran on (the start of the log), and its title is the
/// error alone, not the records after it (#1187).
#[test]
fn the_report_has_the_computer_and_a_clean_title() {
    let dir = std::env::temp_dir().join(format!("openomsi-crash-machine-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let p = dir.join("game.log");
    let mut log = String::from("[t INFO openomsi_game::applog] system: windows x86_64 (10.0), 8 threads, 8192 MB memory\n[t INFO openomsi_game::applog] command line: openomsi --map maps/X/global.cfg\n[t INFO omsi_render] graphics adapter: GTX 750 (DiscreteGpu, Dx12, 2048 MB of its own), texture memory taken for it: 716 MB\n");
    // (more than the end that goes with the report)
    for k in 0..400 {
        log.push_str(&format!("[t INFO openomsi_game::scene] tile loading: placed tile {k},0\n"));
    }
    log.push_str("[t ERROR openomsi_game::app_events] ending the session: the graphics device was lost (Unknown: Out of memory)\n[t INFO openomsi_game::app_events] game ends\n[t WARN openomsi_game::scene] tile loading: first-area batch prepared in 352.71 s\n");
    std::fs::write(&p, log).unwrap();
    let (what, tail) = super::crash_of(&p).unwrap();
    assert_eq!(what, "ending the session: the graphics device was lost (Unknown: Out of memory)");
    let (machine, end) = tail.split_once(&format!("\n{}\n", super::CRASH_TAIL_GAP)).unwrap();
    assert!(machine.contains("8192 MB memory") && machine.contains("GTX 750") && machine.contains("maps/X/global.cfg"), "{machine}");
    assert!(end.contains("352.71 s") && !end.contains("placed tile 0,0"));
    let _ = std::fs::remove_dir_all(&dir);
}
