fn main() {
    // the interface texts come from the game's translation tables
    println!("cargo:rerun-if-changed=../../crates/omsi-app/locales/app.yml");
    tauri_build::build()
}
