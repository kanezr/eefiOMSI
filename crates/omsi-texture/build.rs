fn main() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    println!("cargo:rerun-if-changed={}", src.display());
    let mut files: Vec<_> = std::fs::read_dir(&src).unwrap().map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|x| x == "rs")).collect();
    files.sort();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |b: &[u8]| {
        for &x in b {
            h = (h ^ x as u64).wrapping_mul(0x0100_0000_01b3);
        }
    };
    feed(std::env::var("TARGET").unwrap_or_default().as_bytes());
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let lock = manifest.join("../../Cargo.lock");
    println!("cargo:rerun-if-changed={}", lock.display());
    println!("cargo:rerun-if-changed={}", manifest.join("Cargo.toml").display());
    feed(&std::fs::read(manifest.join("Cargo.toml")).unwrap_or_default());
    for package in std::fs::read_to_string(&lock).unwrap_or_default().split("[[package]]").filter(|p| p.contains("\nsource = ")) {
        feed(package.as_bytes());
    }
    for f in &files {
        feed(f.file_name().unwrap().as_encoded_bytes());
        feed(&std::fs::read(f).unwrap());
    }
    println!("cargo:rustc-env=TEXTURE_SOURCE_HASH={h:016x}");
}
