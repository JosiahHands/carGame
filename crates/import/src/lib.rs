//! Brings progress over from the old (1.2.x) app: career, garage builds,
//! settings, best times, ghosts and the Living City's memories.
//!
//! The old app was built with Electron and kept everything in its browser
//! storage (Chromium's LevelDB), in its folder under the system's app data.
//! Filled in during stage 6 of 1.3.0; it only ever reads the old data.

use std::path::PathBuf;

/// Where the old app may have kept its data on this system. The folder was
/// named after the app ("Car Game") or its package ("car-game").
pub fn old_app_folders() -> Vec<PathBuf> {
    let base = if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    } else if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    };
    base.map(|b| vec![b.join("Car Game"), b.join("car-game")]).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    #[test]
    fn looks_in_both_old_folder_names() {
        let folders = super::old_app_folders();
        assert_eq!(folders.len(), 2);
        assert!(folders[0].ends_with("Car Game") && folders[1].ends_with("car-game"));
    }
}
