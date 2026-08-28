//! Snapshot backups and user-defined key groups, stored on disk.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

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
    /// (or `$XDG_CONFIG_HOME/wh` when that variable is set to an absolute path).
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

    /// Writes a new backup and rotates old ones away, keeping the newest `KEEP_BACKUPS`.
    ///
    /// The backup filename is `<secs>.<nanos>.toml`, where `secs` is zero-padded to 20 digits
    /// (the widest a `u64` seconds-since-epoch value can ever be, so the field never changes
    /// width for any clock value this side of the heat death of the universe) and `nanos` is
    /// zero-padded to 9 digits. The padding keeps a plain lexicographic listing (`ls`, a file
    /// manager) in chronological order too, but the authoritative ordering used by this store
    /// is the parsed numeric key from `backup_sort_key`, not the raw filename string: see there
    /// for why a fixed width alone is not sufficient.
    ///
    /// The write is atomic: content lands in a temporary file in the same directory and is then
    /// renamed into place, so a crash or a full disk mid-write cannot leave a truncated backup
    /// at the final name. The final name is reserved with `create_new` before the rename, so two
    /// saves that land in the same nanosecond cannot silently overwrite one another; the second
    /// one retries at the next nanosecond slot instead.
    pub fn save_backup(&self, toml_text: &str) -> Result<PathBuf, StoreError> {
        let dir = self.backups_dir();
        fs::create_dir_all(&dir)?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| StoreError::ClockBeforeEpoch)?;

        let mut secs = stamp.as_secs();
        let mut nanos = stamp.subsec_nanos();
        let path = loop {
            let candidate = dir.join(format!("{secs:020}.{nanos:09}.toml"));
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&candidate)
            {
                Ok(placeholder) => {
                    // Release the handle immediately: we only needed create_new's atomicity to
                    // reserve the name, and an open handle could block the rename below on
                    // platforms that lock open files.
                    drop(placeholder);
                    break candidate;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    if nanos == 999_999_999 {
                        nanos = 0;
                        secs += 1;
                    } else {
                        nanos += 1;
                    }
                }
                Err(e) => return Err(e.into()),
            }
        };

        let tmp_path = dir.join(format!(
            ".tmp-{}-{}",
            std::process::id(),
            path.file_name()
                .expect("just built with a file name")
                .to_string_lossy()
        ));
        fs::write(&tmp_path, toml_text)?;
        fs::rename(&tmp_path, &path)?;

        // rotate: list_backups() sorts oldest first, so remove from the front.
        let mut all = self.list_backups()?;
        while all.len() > KEEP_BACKUPS {
            fs::remove_file(all.remove(0))?;
        }
        Ok(path)
    }

    /// Sorted oldest to newest, by the parsed `(secs, nanos)` key, not by raw filename string.
    pub fn list_backups(&self) -> Result<Vec<PathBuf>, StoreError> {
        let dir = self.backups_dir();
        if !dir.exists() {
            return Ok(vec![]);
        }
        let mut v: Vec<PathBuf> = fs::read_dir(&dir)?
            .collect::<Result<Vec<_>, std::io::Error>>()?
            .into_iter()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "toml"))
            .collect();
        v.sort_by_key(|p| backup_sort_key(p));
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
        #[derive(serde::Deserialize)]
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
        // Atomic write: land the content in a temp file in the same directory, then rename into
        // place, so a crash or a full disk mid-write cannot leave config.toml truncated and
        // silently drop every group the user defined.
        let tmp_path = self
            .root
            .join(format!(".tmp-config-{}.toml", std::process::id()));
        fs::write(&tmp_path, text)?;
        fs::rename(&tmp_path, self.config_path())?;
        Ok(())
    }
}

/// Parses a backup filename's `<secs>.<nanos>` stem into a sortable `(secs, nanos)` key.
///
/// A raw filename sort is not safe here: the seconds field's digit width changes over time (a
/// clock that reads near the epoch, such as an unsynced VM clock or a dead CMOS battery,
/// produces a short number that a lexicographic sort places after every correctly stamped,
/// longer, current-day name). Parsing the number and comparing numerically is immune to that.
///
/// A filename that does not match the `<secs>.<nanos>.toml` shape (for example a stray file a
/// user dropped into the backups directory) sorts as `(0, 0)`, the oldest possible key. That
/// direction is deliberate: an unparseable name must never be able to masquerade as the newest
/// backup, only ever as the oldest, so it is the first thing rotation trims and never what
/// `load_backup(None)` returns while any genuine backup exists.
fn backup_sort_key(path: &Path) -> (u64, u32) {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(|stem| {
            let (secs_str, nanos_str) = stem.split_once('.')?;
            let secs = secs_str.parse::<u64>().ok()?;
            let nanos = nanos_str.parse::<u32>().ok()?;
            Some((secs, nanos))
        })
        .unwrap_or((0, 0))
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
    fn load_backup_on_absent_dir_returns_no_backups_error() {
        let dir = test_dir("absent-backups");
        let store = Store::at(dir.clone());
        let err = store.load_backup(None).unwrap_err();
        assert!(matches!(err, StoreError::NoBackups));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_backup_on_present_but_empty_dir_returns_no_backups_error() {
        let dir = test_dir("empty-backups-present");
        let store = Store::at(dir.clone());
        std::fs::create_dir_all(dir.join("backups")).unwrap();
        let err = store.load_backup(None).unwrap_err();
        assert!(matches!(err, StoreError::NoBackups));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn groups_on_missing_config_returns_empty_map() {
        let dir = test_dir("no-config");
        std::fs::create_dir_all(&dir).unwrap();
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

    #[test]
    fn load_backup_none_ignores_short_seconds_filename() {
        let dir = test_dir("short-seconds");
        let store = Store::at(dir.clone());
        std::fs::create_dir_all(dir.join("backups")).unwrap();
        // A correctly stamped, genuinely newest backup (today's 10-digit epoch seconds).
        std::fs::write(
            dir.join("backups").join("1756000000.000000000.toml"),
            "newest",
        )
        .unwrap();
        // A backup written when the clock read close to the epoch (a short, unpadded seconds
        // field). It must never be able to masquerade as the newest despite the leading '9'
        // sorting it after the leading '1' as a raw string.
        std::fs::write(
            dir.join("backups").join("946684800.000000000.toml"),
            "year-2000",
        )
        .unwrap();
        let newest = store.load_backup(None).unwrap();
        assert_eq!(newest, "newest");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn backup_rotation_with_mixed_width_names_keeps_newest_20() {
        let dir = test_dir("rotation-mixed-width");
        let store = Store::at(dir.clone());
        std::fs::create_dir_all(dir.join("backups")).unwrap();
        // Files from a clock that was wrong near the epoch: chronologically ancient, but the
        // leading '9' sorts them after any correctly stamped current-day name as raw strings.
        for i in 0..5u64 {
            std::fs::write(
                dir.join("backups")
                    .join(format!("{}.000000000.toml", 946684800 + i)),
                format!("bogus {i}"),
            )
            .unwrap();
        }
        for i in 0..25 {
            store.save_backup(&format!("snap {i}")).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let backups = store.list_backups().unwrap();
        assert_eq!(backups.len(), 20);
        for p in &backups {
            let name = p.file_name().unwrap().to_string_lossy();
            assert!(
                !name.starts_with("9466848"),
                "bogus old file survived rotation: {name}"
            );
        }
        let newest = store.load_backup(None).unwrap();
        assert_eq!(newest, "snap 24");
        let oldest_surviving = std::fs::read_to_string(&backups[0]).unwrap();
        assert_eq!(oldest_surviving, "snap 5");
        std::fs::remove_dir_all(&dir).ok();
    }
}
