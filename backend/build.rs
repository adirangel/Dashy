include!("app_commands.rs");

macro_rules! define_app_command_names {
    ($($command:ident),* $(,)?) => {
        const APP_COMMANDS: &[&str] = &[$(stringify!($command)),*];
    };
}

dashy_app_commands!(define_app_command_names);

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(APP_COMMANDS)),
    )
    .expect("failed to build Dashy's Tauri manifest")
}
