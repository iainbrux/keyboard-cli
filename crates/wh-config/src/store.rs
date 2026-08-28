//! Snapshot backups and user-defined key groups, stored on disk.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("config parse: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("config encode: {0}")]
    TomlEncode(#[from] toml::ser::Error),
    #[error("no backups found")]
    NoBackups,
    #[error("could not resolve a home directory for this user")]
    NoHomeDirectory,
    #[error("system clock is set before the Unix epoch")]
    ClockBeforeEpoch,
}

pub struct Store {
    root: PathBuf,
}

pub const KEEP_BACKUPS: usize = 20;

impl Store {
    /// Default per-user location: `%APPDATA%\wh\config` on Windows, `~/.config/wh` on Linux
    /// (or `$XDG_CONFIG_HOME/wh` when that variable is set).
    pub fn open() -> Result<Self, StoreError> {
        let dirs =
            directories::ProjectDirs::from("", "", "wh").ok_or(StoreError::NoHomeDirectory)?;
        Ok(Self::at(dirs.config_dir().to_path_buf()))
    }
    pub fn at(root: PathBuf) -> Self {
        Self { root }
    }

    fn backups_dir(&self) -> PathBuf {
        self.root.join("backups")
    }

    pub fn save_backup(&self, toml_text: &str) -> Result<PathBuf, StoreError> {
        let dir = self.backups_dir();
        fs::create_dir_all(&dir)?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| StoreError::ClockBeforeEpoch)?;
        let path = dir.join(format!(
            "{}.{:09}.toml",
            stamp.as_secs(),
            stamp.subsec_nanos()
        ));
        fs::write(&path, toml_text)?;
        // rotate: list_backups() sorts oldest first, so remove from the front.
        let mut all = self.list_backups()?;
        while all.len() > KEEP_BACKUPS {
            fs::remove_file(all.remove(0))?;
        }
        Ok(path)
    }

    /// Sorted oldest to newest.
    pub fn list_backups(&self) -> Result<Vec<PathBuf>, StoreError> {
        let dir = self.backups_dir();
        if !dir.exists() {
            return Ok(vec![]);
        }
        let mut v: Vec<PathBuf> = fs::read_dir(&dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "toml"))
            .collect();
        v.sort();
        Ok(v)
    }

    /// `None` loads the newest backup.
    pub fn load_backup(&self, path: Option<PathBuf>) -> Result<String, StoreError> {
        let p = match path {
            Some(p) => p,
            None => self.list_backups()?.pop().ok_or(StoreError::NoBackups)?,
        };
        Ok(fs::read_to_string(p)?)
    }

    fn config_path(&self) -> PathBuf {
        self.root.join("config.toml")
    }

    pub fn groups(&self) -> Result<HashMap<String, Vec<u8>>, StoreError> {
        let p = self.config_path();
        if !p.exists() {
            return Ok(HashMap::new());
        }
        #[derive(serde::Deserialize, Default)]
        struct Cfg {
            #[serde(default)]
            groups: HashMap<String, Vec<u8>>,
        }
        let cfg: Cfg = toml::from_str(&fs::read_to_string(p)?)?;
        Ok(cfg.groups)
    }

    pub fn set_group(&self, name: &str, usages: &[u8]) -> Result<(), StoreError> {
        let mut groups = self.groups()?;
        groups.insert(name.to_string(), usages.to_vec());
        #[derive(serde::Serialize)]
        struct Cfg<'a> {
            groups: &'a HashMap<String, Vec<u8>>,
        }
        fs::create_dir_all(&self.root)?;
        let text = toml::to_string_pretty(&Cfg { groups: &groups })?;
        fs::write(self.config_path(), text)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("wh-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        p
    }

    #[test]
    fn backup_rotation_keeps_20() {
        let dir = test_dir("rotation");
        let store = Store::at(dir.clone());
        for i in 0..25 {
            store.save_backup(&format!("snap {i}")).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let backups = store.list_backups().unwrap();
        assert_eq!(backups.len(), 20);
        let newest = store.load_backup(None).unwrap();
        assert_eq!(newest, "snap 24");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn groups_persist() {
        let dir = test_dir("groups");
        let store = Store::at(dir.clone());
        store.set_group("fps", &[0x1A, 0x04]).unwrap();
        assert_eq!(store.groups().unwrap().get("fps"), Some(&vec![0x1A, 0x04]));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn load_backup_on_empty_dir_returns_no_backups_error() {
        let dir = test_dir("empty-backups");
        let store = Store::at(dir.clone());
        let err = store.load_backup(None).unwrap_err();
        assert!(matches!(err, StoreError::NoBackups));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn groups_on_missing_config_returns_empty_map() {
        let dir = test_dir("no-config");
        let store = Store::at(dir.clone());
        let groups = store.groups().unwrap();
        assert!(groups.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn set_group_overwrites_and_preserves_other_names() {
        let dir = test_dir("group-overwrite");
        let store = Store::at(dir.clone());
        store.set_group("fps", &[0x1A, 0x04]).unwrap();
        store.set_group("moba", &[0x14, 0x1B]).unwrap();
        store.set_group("fps", &[0x1A, 0x04, 0x16]).unwrap();
        let groups = store.groups().unwrap();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups.get("fps"), Some(&vec![0x1A, 0x04, 0x16]));
        assert_eq!(groups.get("moba"), Some(&vec![0x14, 0x1B]));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn list_backups_ignores_non_toml_file() {
        let dir = test_dir("stray-file");
        let store = Store::at(dir.clone());
        store.save_backup("snap a").unwrap();
        std::fs::write(dir.join("backups").join("notes.txt"), "not a backup").unwrap();
        let backups = store.list_backups().unwrap();
        assert_eq!(backups.len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
