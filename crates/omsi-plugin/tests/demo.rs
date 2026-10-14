//! The demo plugin (crates/omsi-plugin/demo) driven in this process and through
//! `omsi-plugin-host`, the way the game drives a plugin.
use omsi_plugin::{HostConfig, Plugin, PluginIo, Plugins, Remote};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn demo_library() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let release = !cfg!(debug_assertions);
    let mut cmd = std::process::Command::new(cargo);
    cmd.args(["build", "-q", "-p", "omsi-demo-plugin", "--manifest-path"]).arg(root.join("Cargo.toml"));
    if release {
        cmd.arg("--release");
    }
    assert!(cmd.status().unwrap().success(), "building the demo plugin");
    let name = format!("{}omsi_demo_plugin{}", std::env::consts::DLL_PREFIX, std::env::consts::DLL_SUFFIX);
    // the test binary is target/<profile>/deps/…; the library sits in target/<profile>
    let exe = std::env::current_exe().unwrap();
    exe.parent().unwrap().parent().unwrap().join(name)
}

/// A plugins folder with the demo as `Demo\demo.dll`, listed in `Demo\demo.opl`.
fn plugins_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("omsi-plugin-test-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("Demo")).unwrap();
    std::fs::copy(demo_library(), dir.join("Demo/demo.dll")).unwrap();
    std::fs::write(
        dir.join("Demo/demo.opl"),
        "[dll]\r\ndemo\\DEMO.dll\r\n\r\n[varlist]\r\n2\r\nthrottle\r\nmissing_var\r\n\r\n[stringvarlist]\r\n1\r\nnote\r\n\r\n[systemvarlist]\r\n1\r\nTime\r\n\r\n[triggers]\r\n1\r\nhorn\r\n",
    )
    .unwrap();
    dir
}

#[derive(Default)]
struct Game {
    time: f32,
    vars: HashMap<String, f32>,
    strings: HashMap<String, String>,
    fired: Vec<(String, bool)>,
}

impl PluginIo for Game {
    fn system(&mut self, name: &str) -> Option<f32> {
        (name == "Time").then_some(self.time)
    }
    fn set_system(&mut self, _: &str, _: f32) {}
    fn has_vehicle(&self) -> bool {
        true
    }
    fn var(&mut self, name: &str) -> Option<f32> {
        self.vars.get(name).copied()
    }
    fn set_var(&mut self, name: &str, v: f32) {
        self.vars.insert(name.into(), v);
    }
    fn string(&mut self, name: &str) -> Option<String> {
        self.strings.get(name).cloned()
    }
    fn set_string(&mut self, name: &str, s: &str) {
        self.strings.insert(name.into(), s.into());
    }
    fn fire(&mut self, trigger: &str, down: bool) {
        self.fired.push((trigger.into(), down));
    }
}

fn run(plugins: &mut Plugins) -> Game {
    let mut g = Game::default();
    g.vars.insert("throttle".into(), 0.25);
    g.strings.insert("note".into(), "................".into());
    for t in [0.0, 1.0, 1.5, 2.0] {
        g.time = t;
        plugins.frame(&mut g);
    }
    g
}

#[test]
fn in_process() {
    let dir = plugins_dir("local");
    let mut plugins = Plugins::load(&[dir.clone()], &HostConfig::default());
    assert_eq!(plugins.loaded.len(), 1);
    let g = run(&mut plugins);
    // doubled every frame: 0.25 · 2⁴
    assert_eq!(g.vars["throttle"], 4.0);
    // written into the buffer as long as the text it held
    assert_eq!(g.strings["note"], "seen at 2");
    // the system variable reached the plugin before its trigger was asked: down at 1 s
    // (odd), still down at 1.5 s, up at 2 s
    assert_eq!(g.fired, [("horn".to_string(), true), ("horn".to_string(), false)]);
    plugins.finalize();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn through_the_host() {
    let dir = plugins_dir("remote");
    let dll = dir.join("Demo/demo.dll");
    let host = Path::new(env!("CARGO_BIN_EXE_omsi-plugin-host"));
    let r = Remote::spawn(None, host, &dll).expect("host starts");
    assert!(r.procs().variable && r.procs().trigger && r.procs().system && r.procs().string);
    drop(r);
    // the same frames through a plugin whose library will not load here: force the host
    let hosts = HostConfig { host32: Some(host.to_path_buf()), runner: None };
    std::fs::write(dir.join("Demo/demo.opl"), "[dll]\r\nDemo\\notalibrary.dll\r\n").unwrap();
    std::fs::write(dir.join("Demo/notalibrary.dll"), b"MZ not really").unwrap();
    let err = Plugin::load(&dir.join("Demo/demo.opl"), &dir, &hosts).err().expect("a file that is no library fails");
    assert!(err.contains("could not load") || err.contains("host") || err.contains("Wine"), "{err}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn host_frames() {
    let dir = plugins_dir("frames");
    let host = Path::new(env!("CARGO_BIN_EXE_omsi-plugin-host"));
    let mut r = Remote::spawn(None, host, &dir.join("Demo/demo.dll")).unwrap();
    let f = omsi_plugin::Frame { system: vec![(0, 3.0)], vars: vec![(0, 1.5)], strings: vec![(0, "0123456789ab".into())], triggers: vec![0] };
    let reply = r.frame(&f).unwrap();
    assert_eq!(reply.system, [None]);
    assert_eq!(reply.vars, [Some(3.0)]);
    assert_eq!(reply.strings, [Some("seen at 3".to_string())]);
    assert_eq!(reply.triggers_active, [true]);
    r.finalize();
    let _ = std::fs::remove_dir_all(dir);
}

/// The real thing: the demo built as a 32-bit Windows DLL, run by the 32-bit host under
/// Wine. Needs `OMSI_TEST_WINE_DIR` = the folder holding `omsi-plugin-host.exe` and
/// `omsi_demo_plugin.dll` built for i686-pc-windows-gnu (see docs/PLUGINS.md), and Wine.
#[test]
fn windows_dll_under_wine() {
    let Some(dir) = std::env::var_os("OMSI_TEST_WINE_DIR").map(PathBuf::from) else { return };
    let hosts = HostConfig::detect();
    let wine = hosts.runner.clone().or_else(|| cfg!(windows).then(PathBuf::new));
    let runner = if cfg!(windows) { None } else { Some(wine.expect("wine on the path")) };
    let mut r = Remote::spawn(runner.as_deref(), &dir.join("omsi-plugin-host.exe"), &dir.join("omsi_demo_plugin.dll")).expect("the 32-bit host starts");
    let f = omsi_plugin::Frame { system: vec![(0, 5.0)], vars: vec![(0, 0.75)], strings: vec![(0, "abcdefghijk".into())], triggers: vec![0] };
    let reply = r.frame(&f).unwrap();
    assert_eq!(reply.vars, [Some(1.5)]);
    assert_eq!(reply.strings, [Some("seen at 5".to_string())]);
    assert_eq!(reply.triggers_active, [true]);
    r.finalize();
}
