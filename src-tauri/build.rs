mod build_config;

fn main() {
    build_config::configure();
    #[cfg(feature = "gui")]
    tauri_build::build();
}
