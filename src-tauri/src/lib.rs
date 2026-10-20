mod commands;
mod config;
mod core;
mod jobs;
mod state;

use tauri::Manager;

/// The app was renamed from "OctoWoW Installation Manager" (`com.octowow.installmanager`).
/// Move its settings, client baseline and caches to the new identifier once.
fn migrate_app_dirs(app: &tauri::AppHandle) {
    const OLD_ID: &str = "com.octowow.installmanager";
    let dirs = [app.path().app_config_dir(), app.path().app_data_dir()];
    for new in dirs.into_iter().flatten() {
        let Some(old) = new.parent().map(|p| p.join(OLD_ID)) else { continue };
        if old.is_dir() && !new.exists() {
            if std::fs::rename(&old, &new).is_err() {
                let _ = core::util::copy_dir(&old, &new);
            }
        }
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            migrate_app_dirs(app.handle());
            let state = state::AppState::load(app.handle())?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::validate_game_dir,
            commands::get_platform,
            commands::get_plugin_catalog,
            commands::scan_game_mods,
            commands::mpq_set_enabled,
            commands::mpq_delete,
            commands::mpq_install,
            commands::custom_dll_add,
            commands::custom_dll_remove,
            commands::scan_addons,
            commands::check_addon_updates,
            commands::addon_install,
            commands::addon_install_zip,
            commands::addon_remove,
            commands::addon_catalog,
            commands::client_status,
            commands::client_check,
            commands::client_verify,
            commands::client_update,
            commands::client_cancel,
            commands::apply_changes,
            commands::launch_game,
            commands::check_app_update,
            commands::gpu_info,
            commands::import_legacy_config,
        ])
        .run(tauri::generate_context!())
        .expect("error while running OctoWoW Manager");
}
