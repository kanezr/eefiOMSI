#[test]
fn auto_join_passes_the_discovered_gateway_to_the_game() {
    let duty = Duty {
        map: "maps/x/global.cfg".into(), bus: "Vehicles/x.bus".into(), time: "09:00".into(),
        lan: Some("join:play.example.org:27015".into()),
        lan_ws_fallback: Some("https://gateway.example.org".into()),
        ..Default::default()
    };
    let args = duty_args_for_root(&duty, Path::new("test-omsi")).unwrap();
    assert!(args.windows(2).any(|w| w == ["--lan-join", "play.example.org:27015"]));
    assert!(args.windows(2).any(|w| w == ["--lan-ws-fallback", "https://gateway.example.org"]));
    let forced = duty_args_for_root(&Duty { lan_ws_fallback: None, ..duty }, Path::new("test-omsi")).unwrap();
    assert!(!forced.iter().any(|arg| arg == "--lan-ws-fallback"));
}

