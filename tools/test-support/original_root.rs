// Where the tests that read the original OMSI 2 install find it. `include!`d into the test
// modules that need it (every crate gets its own copy; no crate depends on another for it).
//
// Those tests are `#[ignore]`d, so a checkout without the install (CI) lists them as ignored
// instead of passing them without looking at anything. Run them with
//
//     OMSI_ROOT="/path/to/OMSI 2" cargo test -p <crate> -- --ignored
//
// Without OMSI_ROOT the `OMSI 2 Original` folder next to the checkout is used. A missing install
// or file fails the test: it was asked for by name, so it must not pass on nothing.

/// The original OMSI 2 install: `$OMSI_ROOT`, else `OMSI 2 Original` beside the checkout.
#[allow(dead_code)]
fn original_root() -> std::path::PathBuf {
    let root = std::env::var_os("OMSI_ROOT").map(std::path::PathBuf::from).unwrap_or_else(|| {
        std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../OMSI 2 Original"))
    });
    assert!(root.is_dir(), "no OMSI 2 install at {} (set OMSI_ROOT)", root.display());
    root
}

/// Fail, naming the first one missing, unless every path exists.
#[allow(dead_code)]
fn require_content(paths: &[&std::path::Path]) {
    for p in paths {
        assert!(p.exists(), "this test needs {} (set OMSI_ROOT to an install that has it)", p.display());
    }
}
