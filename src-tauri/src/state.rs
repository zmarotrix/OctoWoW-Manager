use crate::config::Settings;
use crate::core::updater::Updater;
use crate::core::addons::CatalogEntry;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

pub struct AppState {
    pub settings: Mutex<Settings>,
    settings_path: PathBuf,
    pub updater: Updater,
    /// Addon repo url key -> latest remote commit, from the last update check.
    pub addon_remote: Mutex<HashMap<String, String>>,
    pub catalog: Mutex<Option<Vec<CatalogEntry>>>,
}

impl AppState {
    pub fn load(app: &AppHandle) -> Result<Self, Box<dyn std::error::Error>> {
        let settings_path = app.path().app_config_dir()?.join("settings.json");
        Ok(Self {
            settings: Mutex::new(Settings::load(&settings_path)),
            settings_path,
            updater: Updater::new(app.path().app_data_dir()?),
            addon_remote: Mutex::new(HashMap::new()),
            catalog: Mutex::new(None),
        })
    }

    pub fn snapshot(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }

    pub fn replace(&self, next: Settings) -> Result<(), String> {
        next.save(&self.settings_path).map_err(|e| e.to_string())?;
        *self.settings.lock().unwrap() = next;
        Ok(())
    }

    /// The configured game folder, or an error suitable for showing in the UI.
    pub fn wow_dir(&self) -> Result<PathBuf, String> {
        let dir = self.settings.lock().unwrap().wow_dir.trim().to_string();
        if dir.is_empty() {
            return Err("Set your WoW folder in Game Settings first.".into());
        }
        Ok(PathBuf::from(dir))
    }
}
