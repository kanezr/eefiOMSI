use super::*;

/// A quality preset keeps the graphics mode and what is not graphics; Vanilla's leave
/// out what OMSI 2 does not draw, Enhanced+'s keep its traced shadows, occlusion and
/// reflections; High of Vanilla+ is the defaults.
#[test]
fn quality_presets_keep_the_mode_and_the_rest() {
    for mode in ["vanilla", "vanilla_plus", "enhanced", "enhanced_plus"] {
        for (_, preset) in graphics_presets_for(mode) {
            let mut v = settings_from_text(Some(&format!("graphics={mode}\ngraphics_api=gl\nlanguage=PTB\nai_unsched_factor=25\n")));
            apply_graphics_profile(&preset, &mut v);
            let saved = settings_from_text(Some(&settings_to_text(&v, None)));
            assert_eq!(saved["graphics"], mode);
            assert_eq!(saved["graphics_api"], "gl");
            assert_eq!(saved["language"], "PTB");
            assert_eq!(saved["ai_unsched_factor"], 25);
            for (key, x) in preset.as_object().unwrap() {
                assert_eq!(saved[key.as_str()].to_string().trim_matches('"'), x.to_string().trim_matches('"'), "{mode}: {key}");
            }
        }
    }
    for (_, p) in graphics_presets_for("OMSI 2") {
        for key in ["ssao", "shadows", "detail_textures"] {
            assert!(p.get(key).is_none(), "{key}");
        }
        assert!(p.get("shadow_size").is_some());
    }
    for (_, p) in graphics_presets_for("Enhanced+") {
        for key in ["ssao", "shadows", "reflections"] {
            assert!(p.get(key).is_none(), "{key}");
        }
    }
    let defaults = settings_from_text(None);
    let high = &graphics_presets_for("vanilla_plus")[2].1;
    for (k, x) in high.as_object().unwrap() {
        assert_eq!(defaults[k.as_str()].to_string().trim_matches('"'), x.to_string().trim_matches('"'), "{k}");
    }
}

#[test]
fn presets_keep_reflections_clouds_and_video_memory() {
    for mode in ["vanilla", "vanilla_plus", "enhanced", "enhanced_plus"] {
        let enhanced = mode.starts_with("enhanced");
        for (i, (name, p)) in graphics_presets_for(mode).into_iter().enumerate() {
            assert!(p.get("video_memory").is_none(), "{mode} {name}");
            assert!(p.get("cloud_quality").is_none(), "{mode} {name}");
            assert_eq!(p["clouds"], !(enhanced && i == 0), "{mode} {name}");
            if mode != "enhanced_plus" {
                assert_eq!(p["reflections"], 1.0, "{mode} {name}");
            }
            assert_eq!(p["rain_quality"], ["low", "medium", "high", "high"][i], "{mode} {name}");
        }
        assert_eq!(graphics_presets_for(mode)[0].1["shadow_size"], 1024, "{mode}");
    }
    let mut v = settings_from_text(Some("video_memory=3000\n"));
    apply_graphics_profile(&graphics_presets_for("vanilla_plus")[0].1, &mut v);
    let saved = settings_from_text(Some(&settings_to_text(&v, None)));
    assert_eq!(saved["video_memory"], 3000);
    assert_eq!(saved["reflections"], 1.0);
    assert_eq!(saved["clouds"], true);
}

#[test]
fn a_vanilla_preset_leaves_the_shadows_of_vanilla_plus_alone() {
    let mut v = settings_from_text(Some("graphics=vanilla_plus\nshadows=0\nssao=0\ndetail_textures=0\n"));
    v["graphics"] = json!("vanilla");
    apply_graphics_profile(&graphics_presets_for("vanilla")[3].1, &mut v);
    v["graphics"] = json!("vanilla_plus");
    let saved = settings_from_text(Some(&settings_to_text(&v, None)));
    assert_eq!((&saved["shadows"], &saved["ssao"], &saved["detail_textures"], &saved["shadow_size"]), (&json!(false), &json!(false), &json!(false), &json!(4096)));
    let mut v = settings_from_text(Some("graphics=vanilla_plus\n"));
    v["graphics"] = json!("vanilla");
    apply_graphics_profile(&graphics_presets_for("vanilla")[0].1, &mut v);
    v["graphics"] = json!("vanilla_plus");
    let saved = settings_from_text(Some(&settings_to_text(&v, None)));
    assert_eq!((&saved["shadows"], &saved["ssao"], &saved["detail_textures"], &saved["shadow_size"]), (&json!(true), &json!(true), &json!(true), &json!(1024)));
    let mut v = settings_from_text(Some("graphics=enhanced\nshadows=0\nssao=0\nreflections=0\n"));
    v["graphics"] = json!("enhanced_plus");
    apply_graphics_profile(&graphics_presets_for("enhanced_plus")[0].1, &mut v);
    v["graphics"] = json!("enhanced");
    let saved = settings_from_text(Some(&settings_to_text(&v, None)));
    assert_eq!((&saved["shadows"], &saved["ssao"], &saved["reflections"]), (&json!(false), &json!(false), &json!(0.0)));
}

#[test]
fn a_phone_starts_from_the_games_phone_defaults() {
    let phone = settings_from_text_on(None, true);
    let saved = settings_from_text_on(Some(&settings_to_text(&phone, None)), false);
    for (k, x) in [("msaa", json!(2)), ("anisotropy", json!(4)), ("ssao", json!(false)), ("shadow_size", json!(1024)), ("mirror_size", json!(128)), ("max_fps", json!(60)), ("max_obj_dist", json!("900")), ("pax_density", json!(0.7)), ("navigator_corner", json!("top-center"))] {
        assert_eq!(phone[k], x, "{k}");
        assert_eq!(saved[k], x, "{k}");
    }
    let desktop = settings_from_text_on(None, false);
    assert_eq!((&desktop["msaa"], &desktop["max_obj_dist"], &desktop["navigator_corner"]), (&json!(4), &json!("auto"), &json!("bottom-left")));
    let file = settings_from_text_on(Some("msaa=8\nmax_obj_dist=1500\n"), true);
    assert_eq!((&file["msaa"], &file["max_obj_dist"], &file["anisotropy"]), (&json!(8), &json!("1500"), &json!(4)));
    for (k, d) in desktop.as_object().unwrap() {
        if !["msaa", "anisotropy", "ssao", "shadow_size", "mirror_size", "max_fps", "max_obj_dist", "pax_density", "navigator_corner"].contains(&k.as_str()) {
            assert_eq!(&phone[k.as_str()], d, "{k}");
        }
    }
}

#[test]
fn a_graphics_profile_holds_the_whole_graphics_tab() {
    for k in ["shadow_blobs", "night_brightness", "gpu_texture_compression"] {
        assert!(GRAPHICS_PROFILE_KEYS.contains(&k), "{k}");
    }
    let mut mine = settings_from_text(None);
    for (k, x) in [("shadow_blobs", json!(false)), ("night_brightness", json!(1.5)), ("gpu_texture_compression", json!(false)), ("graphics", json!("enhanced")), ("language", json!("PTB"))] {
        mine[k] = x;
    }
    let mut profile = serde_json::Map::new();
    for k in GRAPHICS_PROFILE_KEYS {
        profile.insert(k.to_string(), mine[k].clone());
    }
    let mut other = settings_from_text(None);
    apply_graphics_profile(&Value::Object(profile), &mut other);
    let saved = settings_from_text(Some(&settings_to_text(&other, None)));
    assert_eq!((&saved["shadow_blobs"], &saved["night_brightness"], &saved["gpu_texture_compression"], &saved["graphics"]), (&json!(false), &json!(1.5), &json!(false), &json!("enhanced")));
    assert_eq!(saved["language"], "ENG");
}

/// The wet roads' reflections are saved by the launcher, kept in graphics profiles, and
/// high in a file that has none (as they were).
#[test]
fn puddles_are_saved_and_kept_in_graphics_profiles() {
    assert_eq!(settings_from_text(None)["puddles"], "high");
    for q in ["off", "low", "high"] {
        let v = settings_from_text(Some(&format!("puddles={q}\n")));
        assert_eq!(settings_from_text(Some(&settings_to_text(&v, None)))["puddles"], q);
    }
    assert_eq!(settings_from_text(Some("puddles=ultra"))["puddles"], "high");
    assert!(GRAPHICS_PROFILE_KEYS.contains(&"puddles"));
}

/// The enhanced clouds' quality is saved by the launcher and kept in graphics profiles.
#[test]
fn cloud_quality_is_saved_and_kept_in_graphics_profiles() {
    assert_eq!(settings_from_text(None)["cloud_quality"], "high");
    for q in ["high", "low"] {
        let v = settings_from_text(Some(&format!("graphics=enhanced\ncloud_quality={q}\n")));
        assert_eq!(settings_from_text(Some(&settings_to_text(&v, None)))["cloud_quality"], q);
    }
    assert_eq!(settings_from_text(Some("cloud_quality=auto"))["cloud_quality"], "high");
    assert!(GRAPHICS_PROFILE_KEYS.contains(&"cloud_quality"));
}

#[test]
fn the_day_before_crosses_months_and_years() {
    assert_eq!(day_before(20261007), 20261006);
    assert_eq!(day_before(20261001), 20260930);
    assert_eq!(day_before(20240301), 20240229);
    assert_eq!(day_before(20260101), 20251231);
}

#[test]
fn deleting_one_driver_keeps_other_and_unowned_runs() {
    let dir = std::env::temp_dir().join(format!("omsi_profile_cleanup_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let write = |file: &str, driver: &str| {
        let session = Session {
            driver: driver.into(),
            ..Default::default()
        };
        std::fs::write(dir.join(file), serde_json::to_vec(&session).unwrap()).unwrap();
    };
    write("mine.json", "Isaac");
    write("mine-case.json", "ISAAC");
    write("other.json", "Isaac S.");
    std::fs::write(dir.join("unknown.json"), "{broken").unwrap();
    std::fs::create_dir_all(dir.join("folder.json")).unwrap();
    delete_profile_sessions(&dir, "isaac").unwrap();
    assert!(!dir.join("mine.json").exists());
    assert!(!dir.join("mine-case.json").exists());
    assert!(dir.join("other.json").exists());
    assert!(dir.join("unknown.json").exists());
    assert!(dir.join("folder.json").is_dir());
    // Repeating deletion, and a profile with no history directory, are harmless.
    delete_profile_sessions(&dir, "Isaac").unwrap();
    delete_profile_sessions(&dir.join("absent"), "Isaac").unwrap();
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_delete_request_needs_a_plain_profile_name() {
    for name in ["", " ", "../other", "a\\b", "C:other"] {
        assert!(delete_profile(name).is_err());
    }
}

#[test]
fn the_reflections_are_a_strength_and_older_files_read_as_on_or_off() {
    for (text, x) in [("reflections=1\n", 1.0), ("reflections=0\n", 0.0), ("reflections=true\n", 1.0), ("reflections=0.35\n", 0.35), ("reflections=7\n", 1.0)] {
        let v = settings_from_text(Some(text));
        assert_eq!(v["reflections"], x, "{text}");
        assert_eq!(settings_from_text(Some(&settings_to_text(&v, None)))["reflections"], x, "{text}");
    }
    let mut v = settings_from_text(None);
    v["reflections"] = json!(false);
    assert_eq!(settings_from_text(Some(&settings_to_text(&v, None)))["reflections"], 0.0);
}
