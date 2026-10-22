#[test]
    fn passenger_animation_is_saved_and_restored() {
        assert_eq!(settings_from_text(None)["passenger_animation"], "original");
        assert_eq!(settings_from_text(Some("version=2\npassenger_animation=original\n"))["passenger_animation"], "original");
        assert_eq!(settings_from_text(Some("version=2\npassenger_animation=enhanced\n"))["passenger_animation"], "enhanced");
        assert_eq!(settings_from_text(Some("version=3\npassenger_animation=enhanced\n"))["passenger_animation"], "original");
        assert_eq!(settings_from_text(Some("version=4\npassenger_animation=enhanced\n"))["passenger_animation"], "enhanced");
        let mut settings = settings_from_text(None);
        settings["passenger_animation"] = json!("enhanced");
        let saved = settings_to_text(&settings, Some("passenger_animation=original\n"));
        assert_eq!(saved.lines().filter(|s| s.starts_with("passenger_animation=")).count(), 1);
        assert_eq!(settings_from_text(Some(&saved))["passenger_animation"], "enhanced");
        assert_eq!(settings_from_text(Some("version=4\npassenger_animation=unknown\n"))["passenger_animation"], "original");
        let old = "version=3\npassenger_animation=enhanced\n";
        let saved = settings_to_text(&settings_from_text(Some(old)), Some(old));
        assert_eq!(saved.lines().filter(|l| l.starts_with("version=")).collect::<Vec<_>>(), ["version=4"]);
        assert_eq!(settings_from_text(Some(&saved))["passenger_animation"], "original");
    }

#[test]
    fn photo_mode_is_saved_and_restored() {
        assert_eq!(settings_from_text(None)["photo_mode"], "simple");
        let mut settings = settings_from_text(None);
        settings["photo_mode"] = json!("advanced");
        let saved = settings_to_text(&settings, Some("photo_mode=simple\n"));
        assert_eq!(saved.lines().filter(|s| s.starts_with("photo_mode=")).count(), 1);
        assert_eq!(settings_from_text(Some(&saved))["photo_mode"], "advanced");
    }
