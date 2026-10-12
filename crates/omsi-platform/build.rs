//! Stamps the build with its version (`MAJOR.MINOR.COMMIT`, see docs/VERSIONING.md) and the
//! commit it came from, so a screenshot or a log line says which version is running
//! (`omsi_platform::VERSION`, `omsi_platform::BUILD`; the updater compares the version with
//! the latest release).

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
}

/// `MAJOR.MINOR` from the VERSION file, then the number of commits since that file last
/// changed, not counting the never-released `[skip ci]` / `[skip actions]` ones (the CI
/// passes the same number in `OPENOMSI_VERSION`).
fn version(git: &dyn Fn(&[&str]) -> Option<String>) -> String {
    if let Ok(v) = std::env::var("OPENOMSI_VERSION") {
        if !v.trim().is_empty() {
            return v.trim().to_string();
        }
    }
    let base = std::fs::read_to_string("../../VERSION").map(|s| s.trim().to_string()).unwrap_or_else(|_| "0.0".into());
    let n = git(&["log", "-1", "--format=%H", "--", "VERSION"])
        .filter(|h| !h.is_empty())
        .and_then(|h| git(&["rev-list", "--count", "--invert-grep", "--fixed-strings", "--grep=[skip ci]", "--grep=[skip actions]", &format!("{h}..HEAD")]))
        .unwrap_or_else(|| "0".into());
    format!("{base}.{n}")
}
