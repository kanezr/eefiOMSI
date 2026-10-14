//! Stamps the build with its version (`MAJOR.MINOR.COMMIT`, see docs/VERSIONING.md) and the
//! commit it came from, so a screenshot or a log line says which version is running; on
//! Windows it also puts the application icon into the executable.

use std::process::Command;

fn main() {
    let git = |args: &[&str]| -> Option<String> {
        let out = Command::new("git").args(args).output().ok()?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
    };
    let hash = git(&["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "unknown".into());
    let date = git(&["log", "-1", "--format=%cd", "--date=format:%Y-%m-%d %H:%M"]).unwrap_or_default();
    let dirty = git(&["status", "--porcelain"]).map(|s| !s.trim().is_empty()).unwrap_or(false);
    println!("cargo:rustc-env=OMSI_BUILD={hash}{} {date}", if dirty { "+" } else { "" });
    println!("cargo:rustc-env=OPENOMSI_VERSION={}", version(&git));
    println!("cargo:rerun-if-env-changed=OPENOMSI_VERSION");
    println!("cargo:rerun-if-changed=../../VERSION");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/index");
    windows_icon();
    steam();
    // (the executable exports the two switchable-graphics hints of main.rs, see there)
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        for sym in ["NvOptimusEnablement", "AmdPowerXpressRequestHighPerformance"] {
            println!("cargo:rustc-link-arg-bin=openomsi=/EXPORT:{sym},DATA");
        }
    }
}

/// Steam's rich presence (`src/steam.rs`) on the targets `assets/steam_redist` has the library
/// for: `cfg(steam)`, and the library copied beside the binary, where the game looks for it
/// at its start (`@loader_path` on macOS, `$ORIGIN` on Linux, the exe's folder on Windows) -
/// without it there the game does not start at all. Android, Windows on ARM and Linux on ARM
/// have no Steam library and are built without it.
fn steam() {
    println!("cargo::rustc-check-cfg=cfg(steam)");
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let lib = match (os.as_str(), arch.as_str()) {
        ("windows", "x86_64") => "steam_api64.dll",
        ("linux", "x86_64") => "libsteam_api.so",
        ("macos", _) => "libsteam_api.dylib",
        _ => return,
    };
    println!("cargo:rustc-cfg=steam");
    if os == "linux" {
        println!("cargo:rustc-link-arg-bin=openomsi=-Wl,-rpath,$ORIGIN");
    }
    let src = std::path::Path::new("../../assets/steam_redist").join(lib);
    println!("cargo:rerun-if-changed={}", src.display());
    // (OUT_DIR is <target>/<profile>/build/<crate>/out: the binary lies three folders up)
    if let Some(dir) = std::env::var_os("OUT_DIR").map(std::path::PathBuf::from).and_then(|o| o.ancestors().nth(3).map(|d| d.to_path_buf())) {
        let _ = std::fs::copy(&src, dir.join(lib));
    }
}

/// `MAJOR.MINOR` from the VERSION file, then the number of commits since that file last
/// changed (the CI passes the same number in `OPENOMSI_VERSION`).
fn version(git: &dyn Fn(&[&str]) -> Option<String>) -> String {
    if let Ok(v) = std::env::var("OPENOMSI_VERSION") {
        if !v.trim().is_empty() {
            return v.trim().to_string();
        }
    }
    let base = std::fs::read_to_string("../../VERSION").map(|s| s.trim().to_string()).unwrap_or_else(|_| "0.0".into());
    let n = git(&["log", "-1", "--format=%H", "--", "VERSION"])
        .filter(|h| !h.is_empty())
        .and_then(|h| git(&["rev-list", "--count", &format!("{h}..HEAD")]))
        .unwrap_or_else(|| "0".into());
    format!("{base}.{n}")
}

fn windows_icon() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    println!("cargo:rerun-if-changed=../../assets/icons/app/openomsi.ico");
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../../assets/icons/app/openomsi.ico")
        .set("ProductName", "openOMSI")
        .set("FileDescription", "openOMSI")
        .set("ProductVersion", &std::env::var("OPENOMSI_VERSION").unwrap_or_default());
    if let Err(e) = res.compile() {
        println!("cargo:warning=no icon in the executable: {e}");
    }
}
