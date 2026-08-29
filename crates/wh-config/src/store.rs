//! Snapshot backups and user-defined key groups, stored on disk.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    /// Reading only: groups are written as `config.json`, and `toml` stays a dependency solely
    /// to read a `config.toml` written before that change.
    #[error("config parse: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("config JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("no backups found")]
    NoBackups,
    #[error("could not resolve a home directory for this user")]
    NoHomeDirectory,
    #[error("system clock is set before the Unix epoch")]
    ClockBeforeEpoch,
    #[error("no such group: {0}")]
    GroupNotFound(String),
    #[error("group already exists: {0}")]
    GroupExists(String),
}

pub struct Store {
    root: PathBuf,
}

pub const KEEP_BACKUPS: usize = 20;

/// Makes each `save_backup` temp filename unique within this process, regardless of clock
/// resolution. A reused temp name would silently corrupt every backup still linked to it,
/// since `hard_link` shares an inode rather than copying bytes.
static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

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
    /// Filename is `<secs>.<nanos>.json`, zero-padded to 20 and 9 digits. `list_backups` sorts
    /// by the parsed `(secs, nanos)` key, not the raw string, so the padding is cosmetic.
    ///
    /// Content is written to a `.partial` temp file first (name paired with a process-wide
    /// counter so it never repeats, opened with `create_new` so a collision fails loudly and
    /// retries instead of truncating another writer's file), then published by
    /// `fs::hard_link`-ing it onto the final name, retrying the next nanosecond slot if that
    /// name is taken. `hard_link` shares an inode rather than copying bytes, so publishing only
    /// after the temp content is complete, and never reusing a temp name, is what keeps a
    /// collision from silently corrupting an existing backup in place instead of just failing.
    pub fn save_backup(&self, text: &str) -> Result<PathBuf, StoreError> {
        let dir = self.backups_dir();
        fs::create_dir_all(&dir)?;

        let pid = std::process::id();
        let (tmp_path, mut tmp_file) = loop {
            let counter = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let candidate = dir.join(format!(".save-{pid}-{counter}.partial"));
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&candidate)
            {
                Ok(file) => break (candidate, file),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.into()),
            }
        };
        if let Err(e) = std::io::Write::write_all(&mut tmp_file, text.as_bytes()) {
            drop(tmp_file);
            let _ = fs::remove_file(&tmp_path);
            return Err(e.into());
        }
        drop(tmp_file);

        let stamp = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            Ok(stamp) => stamp,
            Err(_) => {
                let _ = fs::remove_file(&tmp_path);
                return Err(StoreError::ClockBeforeEpoch);
            }
        };

        let mut secs = stamp.as_secs();
        let mut nanos = stamp.subsec_nanos();
        let path = loop {
            let candidate = dir.join(format!("{secs:020}.{nanos:09}.json"));
            match fs::hard_link(&tmp_path, &candidate) {
                Ok(()) => break candidate,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    if nanos == 999_999_999 {
                        nanos = 0;
                        secs += 1;
                    } else {
                        nanos += 1;
                    }
                }
                Err(e) => {
                    let _ = fs::remove_file(&tmp_path);
                    return Err(e.into());
                }
            }
        };

        // `path` is a hard link to the same content, so the temp name can go. Best-effort:
        // failure just leaves a harmless `.partial` file, which list_backups ignores.
        let _ = fs::remove_file(&tmp_path);

        // rotate: list_backups() sorts oldest first, so remove from the front.
        let mut all = self.list_backups()?;
        while all.len() > KEEP_BACKUPS {
            fs::remove_file(all.remove(0))?;
        }
        Ok(path)
    }

    /// Sorted oldest to newest, by the parsed `(secs, nanos)` key, not by raw filename string.
    ///
    /// An entry counts as a backup only if its name does not start with `.` and it has a
    /// `.json` or `.toml` extension. Both checks are needed: `Path::extension` still reports
    /// `.toml` for a dot-prefixed temp name like `.tmp-<pid>-<name>.toml`, so the leading-dot
    /// check catches what the extension check alone would miss. A zero-length file is also
    /// excluded, since it is never a valid backup.
    pub fn list_backups(&self) -> Result<Vec<PathBuf>, StoreError> {
        let dir = self.backups_dir();
        if !dir.exists() {
            return Ok(vec![]);
        }
        let mut v = Vec::new();
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            let looks_hidden_or_temp = path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.starts_with('.'))
                .unwrap_or(true);
            if looks_hidden_or_temp {
                continue;
            }
            // Both formats list: `.json` is what we write now, `.toml` is a Phase 1 backup that
            // must stay restorable.
            let ext_ok = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e == "json" || e == "toml");
            if !ext_ok {
                continue;
            }
            if entry.metadata()?.len() == 0 {
                continue;
            }
            v.push(path);
        }
        v.sort_by_key(|p| backup_sort_key(p));
        Ok(v)
    }

    /// `None` loads the newest backup. Returns the path too, so the caller can pick a parser
    /// from the extension.
    pub fn load_backup(&self, path: Option<PathBuf>) -> Result<(PathBuf, String), StoreError> {
        let p = match path {
            Some(p) => p,
            None => self.list_backups()?.pop().ok_or(StoreError::NoBackups)?,
        };
        let text = fs::read_to_string(&p)?;
        Ok((p, text))
    }

    fn config_path(&self) -> PathBuf {
        self.root.join("config.json")
    }

    /// Reads `config.json`, falling back to a `config.toml` written before the format change.
    /// JSON wins when both exist, since every write produces JSON.
    pub fn groups(&self) -> Result<HashMap<String, Vec<u8>>, StoreError> {
        #[derive(serde::Deserialize)]
        struct Cfg {
            #[serde(default)]
            groups: HashMap<String, Vec<u8>>,
        }
        let json_path = self.config_path();
        if json_path.exists() {
            let cfg: Cfg = serde_json::from_str(&fs::read_to_string(json_path)?)?;
            return Ok(cfg.groups);
        }
        let toml_path = self.root.join("config.toml");
        if toml_path.exists() {
            let cfg: Cfg = toml::from_str(&fs::read_to_string(toml_path)?)?;
            return Ok(cfg.groups);
        }
        Ok(HashMap::new())
    }

    pub fn set_group(&self, name: &str, usages: &[u8]) -> Result<(), StoreError> {
        let mut groups = self.groups()?;
        groups.insert(name.to_string(), usages.to_vec());
        self.write_groups(&groups)
    }

    /// Removes a group. Returns whether one was there. Takes the name directly rather than a
    /// selector, so a group whose name collides with a key name can still be removed.
    pub fn remove_group(&self, name: &str) -> Result<bool, StoreError> {
        let mut groups = self.groups()?;
        let removed = groups.remove(name).is_some();
        if removed {
            self.write_groups(&groups)?;
        }
        Ok(removed)
    }

    /// Renames a group. Refuses if `old` is absent or `new` is already taken, and writes
    /// nothing in either failure case.
    pub fn rename_group(&self, old: &str, new: &str) -> Result<(), StoreError> {
        let mut groups = self.groups()?;
        if groups.contains_key(new) {
            return Err(StoreError::GroupExists(new.to_string()));
        }
        let members = groups
            .remove(old)
            .ok_or_else(|| StoreError::GroupNotFound(old.to_string()))?;
        groups.insert(new.to_string(), members);
        self.write_groups(&groups)
    }

    /// Atomic write: land the content in a temp file, then rename into place, so a crash
    /// mid-write cannot truncate the config and drop every group the user defined.
    fn write_groups(&self, groups: &HashMap<String, Vec<u8>>) -> Result<(), StoreError> {
        #[derive(serde::Serialize)]
        struct Cfg<'a> {
            groups: &'a HashMap<String, Vec<u8>>,
        }
        fs::create_dir_all(&self.root)?;
        let text = serde_json::to_string_pretty(&Cfg { groups })?;
        let tmp_path = self
            .root
            .join(format!(".tmp-config-{}.json", std::process::id()));
        fs::write(&tmp_path, text)?;
        fs::rename(&tmp_path, self.config_path())?;
        Ok(())
    }
}

/// Parses a backup filename's `<secs>.<nanos>` stem into a sortable `(secs, nanos)` key.
///
/// A raw string sort is not safe: a clock near the epoch (unsynced VM, dead CMOS battery)
/// produces a shorter number that sorts after every current-day name lexicographically.
/// Parsing and comparing numerically avoids that.
///
/// An unparseable name (a stray file dropped into the backups directory) sorts as `(0, 0)`,
/// the oldest possible key, so it is the first thing rotation trims and never what
/// `load_backup(None)` returns.
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
        let (_, newest) = store.load_backup(None).unwrap();
        assert_eq!(newest, "snap 24");
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// New backups are `.json`. Pre-existing `.toml` backups still list and still load, so a
    /// Phase 1 backup remains restorable.
    #[test]
    fn backups_write_json_and_still_list_toml() {
        let dir = test_dir("backups-mixed");
        let store = Store::at(dir.clone());
        let backups = dir.join("backups");
        std::fs::create_dir_all(&backups).unwrap();
        std::fs::write(backups.join("00000000000000000001.000000000.toml"), "old").unwrap();

        let written = store.save_backup("{\"new\":true}").unwrap();
        assert_eq!(written.extension().unwrap(), "json");

        let all = store.list_backups().unwrap();
        assert_eq!(all.len(), 2, "both formats must list: {all:?}");
        assert_eq!(all[0].extension().unwrap(), "toml", "oldest first");

        let (path, text) = store.load_backup(None).unwrap();
        assert_eq!(path.extension().unwrap(), "json");
        assert_eq!(text, "{\"new\":true}");
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

    /// Groups are written as `config.json`, not `config.toml`.
    #[test]
    fn set_group_writes_json() {
        let dir = test_dir("groups-json");
        let store = Store::at(dir.clone());
        store.set_group("fps", &[0x1A, 0x04]).unwrap();
        assert!(dir.join("config.json").exists(), "config.json must exist");
        assert!(
            !dir.join("config.toml").exists(),
            "config.toml must not be written"
        );
        let text = std::fs::read_to_string(dir.join("config.json")).unwrap();
        assert!(text.contains("\"fps\""), "json must name the group: {text}");
        assert_eq!(store.groups().unwrap()["fps"], vec![0x1A, 0x04]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A config.toml written before the format change is still read, so a user's existing groups
    /// survive the upgrade.
    #[test]
    fn groups_reads_a_pre_existing_toml_config() {
        let dir = test_dir("groups-toml-compat");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.toml"), "[groups]\nfps = [26, 4, 22, 7]\n").unwrap();
        let store = Store::at(dir.clone());
        assert_eq!(store.groups().unwrap()["fps"], vec![26, 4, 22, 7]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// When both exist, JSON wins: it is the format every write produces, so it is the newer of
    /// the two by construction.
    #[test]
    fn groups_prefers_json_when_both_files_exist() {
        let dir = test_dir("groups-both");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.toml"), "[groups]\nfps = [1]\n").unwrap();
        std::fs::write(dir.join("config.json"), r#"{"groups":{"fps":[2]}}"#).unwrap();
        let store = Store::at(dir.clone());
        assert_eq!(store.groups().unwrap()["fps"], vec![2]);
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
        // A short, unpadded seconds field from a clock near the epoch. Must not masquerade as
        // newest despite its leading '9' sorting after '1' as a raw string.
        std::fs::write(
            dir.join("backups").join("946684800.000000000.toml"),
            "year-2000",
        )
        .unwrap();
        let (_, newest) = store.load_backup(None).unwrap();
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
        let (_, newest) = store.load_backup(None).unwrap();
        assert_eq!(newest, "snap 24");
        let oldest_surviving = std::fs::read_to_string(&backups[0]).unwrap();
        assert_eq!(oldest_surviving, "snap 5");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn list_backups_ignores_orphaned_temp_file() {
        let dir = test_dir("orphan-temp");
        let store = Store::at(dir.clone());
        std::fs::create_dir_all(dir.join("backups")).unwrap();
        std::fs::write(
            dir.join("backups").join("1756000000.000000000.toml"),
            "genuine",
        )
        .unwrap();
        // An orphaned temp artifact: starts with '.', but `Path::extension()` still reports
        // "toml" for it despite the embedded dots.
        std::fs::write(
            dir.join("backups")
                .join(".tmp-9999-00000000001756000005.000000000.toml"),
            "",
        )
        .unwrap();
        let backups = store.list_backups().unwrap();
        assert_eq!(backups.len(), 1);
        let (_, newest) = store.load_backup(None).unwrap();
        assert_eq!(newest, "genuine");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_backup_none_ignores_zero_length_newest_file() {
        let dir = test_dir("zero-length-newest");
        let store = Store::at(dir.clone());
        std::fs::create_dir_all(dir.join("backups")).unwrap();
        std::fs::write(
            dir.join("backups").join("1756000000.000000000.toml"),
            "older-real",
        )
        .unwrap();
        // A zero-length file at a later timestamp than the genuine backup: never a valid
        // backup, whatever left it there.
        std::fs::write(dir.join("backups").join("1756000005.000000000.toml"), "").unwrap();
        let backups = store.list_backups().unwrap();
        assert_eq!(backups.len(), 1);
        let (_, newest) = store.load_backup(None).unwrap();
        assert_eq!(newest, "older-real");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A group whose name collides with a key name is refused as a selector, so delete must
    /// address it by name in a position that is unambiguously a group. That is the only recovery.
    #[test]
    fn remove_group_deletes_by_name_even_when_the_name_is_a_key() {
        let dir = test_dir("group-remove");
        let store = Store::at(dir.clone());
        store.set_group("rt", &[0x1A]).unwrap();
        assert!(store.remove_group("rt").unwrap());
        assert!(store.groups().unwrap().is_empty());
        assert!(
            !store.remove_group("rt").unwrap(),
            "second remove reports nothing removed"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rename_group_moves_the_members_and_refuses_an_existing_name() {
        let dir = test_dir("group-rename");
        let store = Store::at(dir.clone());
        store.set_group("old", &[0x1A, 0x04]).unwrap();
        store.set_group("taken", &[0x07]).unwrap();
        store.rename_group("old", "fps").unwrap();
        let groups = store.groups().unwrap();
        assert_eq!(groups["fps"], vec![0x1A, 0x04]);
        assert!(!groups.contains_key("old"));
        assert!(
            store.rename_group("fps", "taken").is_err(),
            "must not clobber"
        );
        assert!(store.rename_group("missing", "x").is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Two back-to-back `save_backup` calls in the same process must produce two distinct
    /// backups with intact contents. Guards the process-wide counter against being dropped or
    /// narrowed in a later refactor; does not reproduce the inode-aliasing hazard itself, which
    /// needs an injected `fs::remove_file` failure to trigger.
    #[test]
    fn same_process_saves_never_share_a_temp_name() {
        let dir = test_dir("no-temp-name-reuse");
        let store = Store::at(dir.clone());
        let first = store.save_backup("first content").unwrap();
        let second = store.save_backup("second content").unwrap();
        assert_ne!(first, second);
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "first content");
        assert_eq!(std::fs::read_to_string(&second).unwrap(), "second content");
        std::fs::remove_dir_all(&dir).ok();
    }
}
