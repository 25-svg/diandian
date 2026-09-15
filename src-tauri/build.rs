mod build_config;

fn main() {
    build_config::configure();
    #[cfg(feature = "gui")]
    {
        println!("cargo:rerun-if-changed=icons/icon.ico");
        tauri_build::build();
    }
}
