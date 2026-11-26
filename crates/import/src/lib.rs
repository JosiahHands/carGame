//! Brings progress over from the old (1.2.x) app: career, garage builds,
//! settings, best times, ghosts and the Living City's memories.
//!
//! The old app was built with Electron and kept everything in its browser
//! storage (Chromium's LevelDB), in its folder under the system's app data.
//! This only ever reads the old data: the folder is copied somewhere
//! temporary first, and the copy is what gets opened.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rusty_leveldb::{DB, LdbIterator, Options};

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

/// The old game's saves (name -> JSON text, like its browser storage), or
/// None if there's no old game on this computer.
pub fn read_old_saves() -> Option<BTreeMap<String, String>> {
    const CITY_KEY: &str = "carGame.pack.npc.city";
    let mut found = BTreeMap::new();
    let mut newest_city = None;
    for folder in old_app_folders() {
        let db = folder.join("Local Storage").join("leveldb");
        if db.is_dir()
            && let Ok(mut entries) = read_local_storage(&db)
        {
            if let Some(text) = entries.remove(CITY_KEY) {
                pick_city(&mut newest_city, text);
            }
            found.extend(entries);
        }
        let npc = folder.join("packs/npc");
        // The backup predates the main file. Equal timestamps prefer the main file.
        for name in ["city.json.backup", "city.json"] {
            let file = npc.join(name);
            if file.metadata().is_ok_and(|m| m.len() <= 40 * 1024 * 1024)
                && let Ok(text) = std::fs::read_to_string(file)
            {
                pick_city(&mut newest_city, text);
            }
        }
        if let Ok(text) = std::fs::read_to_string(npc.join("ai.json"))
            && serde_json::from_str::<serde_json::Value>(&text).is_ok_and(|v| v.is_object())
        {
            found.insert("carGame.pack.npc.ai".into(), text);
        }
    }
    if let Some((_, text)) = newest_city {
        found.insert(CITY_KEY.into(), text);
    }
    if found.is_empty() { None } else { Some(found) }
}

fn pick_city(newest: &mut Option<(u64, String)>, text: String) {
    let Ok(city) = serde_json::from_str::<serde_json::Value>(&text) else {
        return;
    };
    if !city.is_object() {
        return;
    }
    let timestamp = city["savedAt"].as_u64().unwrap_or(0);
    if newest.as_ref().is_none_or(|(current, _)| timestamp >= *current) {
        *newest = Some((timestamp, text));
    }
}

/// Reads the game's entries from a copy of a Chromium "Local Storage/leveldb" folder.
pub fn read_local_storage(dir: &Path) -> std::io::Result<BTreeMap<String, String>> {
    let copy = std::env::temp_dir().join(format!("car-game-import-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&copy);
    std::fs::create_dir_all(&copy)?;
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        // The lock file belongs to the old app, if it's open.
        if name != "LOCK" && entry.file_type()?.is_file() {
            std::fs::copy(entry.path(), copy.join(&name))?;
        }
    }
    let result = read_copy(&copy);
    let _ = std::fs::remove_dir_all(&copy);
    result
}

fn read_copy(dir: &Path) -> std::io::Result<BTreeMap<String, String>> {
    let options = Options {
        create_if_missing: false,
        ..Options::default()
    };
    let mut db = DB::open(dir, options).map_err(|e| std::io::Error::other(e.to_string()))?;
    let mut iter = db.new_iter().map_err(|e| std::io::Error::other(e.to_string()))?;
    let mut out = BTreeMap::new();
    while iter.advance() {
        let Some((key, value)) = iter.current() else { continue };
        if let Some(name) = storage_key(&key)
            && name.starts_with("carGame.")
            && let Some(text) = decode(&value)
        {
            out.insert(name, text);
        }
    }
    Ok(out)
}

/// A Chromium local storage key is "_" + the page's origin + a zero byte +
/// the name, which starts with a byte saying how it's written.
fn storage_key(key: &[u8]) -> Option<String> {
    if key.first() != Some(&b'_') {
        return None;
    }
    let zero = key.iter().position(|b| *b == 0)?;
    decode(&key[zero + 1..])
}

/// Chromium writes strings as 0x01 + Latin-1, or 0x00 + UTF-16 (little-endian).
fn decode(bytes: &[u8]) -> Option<String> {
    let (&kind, rest) = bytes.split_first()?;
    match kind {
        1 => Some(rest.iter().map(|b| *b as char).collect()),
        0 => {
            let units: Vec<u16> = rest.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            String::from_utf16(&units).ok()
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_in_both_old_folder_names() {
        let folders = old_app_folders();
        assert_eq!(folders.len(), 2);
        assert!(folders[0].ends_with("Car Game") && folders[1].ends_with("car-game"));
    }

    #[test]
    fn chooses_newest_readable_city() {
        let mut city = None;
        pick_city(&mut city, r#"{"savedAt":42,"clock":1}"#.into());
        pick_city(&mut city, r#"{"savedAt":10,"clock":0}"#.into());
        pick_city(&mut city, r#"{"savedAt":100,"#.into());
        pick_city(&mut city, "null".into());
        assert_eq!(city, Some((42, r#"{"savedAt":42,"clock":1}"#.into())));
    }

    #[test]
    fn reads_chromium_strings() {
        assert_eq!(
            storage_key(b"_file://\x00\x01carGame.config.v1").as_deref(),
            Some("carGame.config.v1")
        );
        assert_eq!(decode(b"\x01{\"a\":1}").as_deref(), Some("{\"a\":1}"));
        assert_eq!(decode(&[0, b'h', 0, b'i', 0]).as_deref(), Some("hi"));
        assert_eq!(storage_key(b"META:file://"), None);
    }

    /// A database written the way Chromium writes it can be read back.
    #[test]
    fn reads_a_local_storage_database() {
        let dir = std::env::temp_dir().join(format!("car-game-import-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        {
            let mut db = DB::open(
                &dir,
                Options {
                    create_if_missing: true,
                    ..Options::default()
                },
            )
            .unwrap();
            db.put(b"VERSION", b"1").unwrap();
            db.put(b"_file://\x00\x01carGame.career.v1", b"\x01{\"coins\":1234}").unwrap();
            db.put(b"_file://\x00\x01other.thing", b"\x01x").unwrap();
            db.flush().unwrap();
        }
        let saves = read_local_storage(&dir).unwrap();
        assert_eq!(saves.get("carGame.career.v1").map(String::as_str), Some("{\"coins\":1234}"));
        assert_eq!(saves.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
