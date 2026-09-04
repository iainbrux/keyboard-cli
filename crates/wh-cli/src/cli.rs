//! The `wh` command tree.

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "wh", about = "Wallhack K-001 keyboard control", version)]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Cmd,
}

#[derive(Subcommand)]
pub enum Cmd {
    /// Read the full board configuration
    Dump {
        /// Print a human-readable table instead of JSON
        #[arg(long)]
        table: bool,
    },
    /// Read a setting for selected keys
    Get {
        #[command(subcommand)]
        what: GetWhat,
    },
    /// Write a setting for selected keys
    Set {
        #[command(subcommand)]
        what: SetWhat,
    },
    /// Snapshot the board config to a file
    Backup {
        /// Destination file; defaults to the store's auto-backup location
        #[arg(long)]
        to: Option<std::path::PathBuf>,
    },
    /// Write a snapshot back to the board
    Restore {
        /// Snapshot file; omit and pass --last to use the newest stored one instead
        file: Option<std::path::PathBuf>,
        /// Use the most recent snapshot in the store, whichever command took it; see `wh backups list`
        #[arg(long)]
        last: bool,
        /// Restore a snapshot with no recorded profile, asserting it belongs to the board's
        /// current profile. Covers two cases: the snapshot predates profile recording, or the
        /// board it was taken from reported a profile index this build does not recognise (the
        /// settings still belong to some real profile, just not one this build can name); either
        /// way, if the assertion is wrong this can overwrite a different profile's settings. Has
        /// no effect, and does not rescue, a snapshot whose recorded profile differs from the
        /// board's: that refusal has no override.
        #[arg(long)]
        force: bool,
    },
    /// Key names and groups
    Keys {
        #[command(subcommand)]
        what: KeysWhat,
    },
    /// Manage stored backups
    Backups {
        #[command(subcommand)]
        what: BackupsWhat,
    },
    /// Read or select the active profile
    Profile {
        /// Profile to select, 1 to 4. Omit to read the current one.
        number: Option<u8>,
    },
    /// No-op write self-test (writes current values back, verifies)
    Selftest,
    /// Read and write keysets (grouped actuation point and rapid trigger settings)
    Keyset {
        #[command(subcommand)]
        what: KeysetWhat,
    },
}

#[derive(clap::Args)]
pub struct KeysArg {
    /// Key selector: "w,a,s,d", "wasd", "all,!space", "f1-f12", user groups
    #[arg(long, required_unless_present = "pick")]
    pub keys: Option<String>,
    /// Pick keys interactively instead
    #[arg(long)]
    pub pick: bool,
}

#[derive(Subcommand)]
pub enum GetWhat {
    /// Rapid trigger
    Rt(KeysArg),
    /// Actuation point
    Ap(KeysArg),
}

#[derive(Subcommand)]
pub enum SetWhat {
    /// Rapid trigger
    Rt {
        #[command(flatten)]
        keys: KeysArg,
        /// Sensitivity in mm (sets press and release)
        #[arg(long, conflicts_with = "off")]
        set: Option<f64>,
        /// Press sensitivity in mm (overrides --set for press)
        #[arg(long, conflicts_with = "off")]
        press: Option<f64>,
        /// Release sensitivity in mm (overrides --set for release)
        #[arg(long, conflicts_with = "off")]
        release: Option<f64>,
        /// Disable rapid trigger on these keys
        #[arg(long)]
        off: bool,
        /// Print the exact reports without sending
        #[arg(long)]
        dry_run: bool,
    },
    /// Actuation point
    Ap {
        #[command(flatten)]
        keys: KeysArg,
        /// Depth in mm
        #[arg(long)]
        set: f64,
        /// Print the exact reports without sending
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
pub enum KeysWhat {
    /// List known key names and groups
    List,
    /// Define a user group: wh keys group fps "w,a,s,d,space"
    Group { name: String, selector: String },
    /// Delete a user group: wh keys ungroup fps
    Ungroup { name: String },
    /// Rename a user group: wh keys rename fps arrows
    Rename { old: String, new: String },
}

#[derive(Subcommand)]
pub enum BackupsWhat {
    /// List stored backups, oldest first
    List,
}

/// Which of the two independent keyset groupings a command operates on. They have separate
/// indices, so every keyset command names one.
#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum KeysetKindArg {
    /// Actuation point keysets (layout 0xFF)
    Ap,
    /// Rapid trigger keysets (layout 0xFE)
    Rt,
}

#[derive(Subcommand)]
pub enum KeysetWhat {
    /// List keysets and their members: wh keyset list ap
    List {
        /// Omit to list both kinds
        kind: Option<KeysetKindArg>,
    },
    /// Create a keyset over selected keys: wh keyset create ap --keys u,i,o,p
    Create {
        kind: KeysetKindArg,
        #[command(flatten)]
        keys: KeysArg,
        /// Value in mm: the actuation point for a new ap keyset, or the rapid trigger base
        /// (press and release both) for a new rt keyset. Defaults to the board's global, and is
        /// required when the keys outside every keyset disagree on it.
        #[arg(long)]
        value: Option<f64>,
        /// Press sensitivity in mm for a new rt keyset, overriding --value's press half. Refused
        /// on an ap keyset; pass --value instead.
        #[arg(long)]
        press: Option<f64>,
        /// Release sensitivity in mm for a new rt keyset, overriding --value's release half.
        /// Refused on an ap keyset; pass --value instead.
        #[arg(long)]
        release: Option<f64>,
        /// Print the exact reports without sending
        #[arg(long)]
        dry_run: bool,
    },
    /// Change an existing keyset's value: wh keyset set ap 3 --value 1.2
    Set {
        kind: KeysetKindArg,
        /// The keyset's own index, as shown by `wh keyset list`
        index: u16,
        /// Value in mm: the actuation point for an ap keyset, or the rapid trigger base (press
        /// and release both) for an rt keyset. Required for an ap keyset. For an rt keyset,
        /// --press requires --release or --value, and --release requires --press or --value.
        #[arg(long)]
        value: Option<f64>,
        /// Press sensitivity in mm, overriding --value's press half. Refused on an ap keyset;
        /// pass --value instead.
        #[arg(long)]
        press: Option<f64>,
        /// Release sensitivity in mm, overriding --value's release half. Refused on an ap
        /// keyset; pass --value instead.
        #[arg(long)]
        release: Option<f64>,
        /// Print the exact reports without sending
        #[arg(long)]
        dry_run: bool,
    },
    /// Delete a keyset, returning its members to the global value: wh keyset delete ap 3
    Delete {
        kind: KeysetKindArg,
        /// The keyset's own index, as shown by `wh keyset list`
        index: u16,
        /// Value in mm to return members to: the actuation point for an ap keyset, or the rapid
        /// trigger base for an rt keyset. Defaults to the board's global, and is required when
        /// the keys outside every keyset disagree on it.
        #[arg(long)]
        value: Option<f64>,
        /// Press sensitivity in mm to return members to, overriding --value's press half.
        /// Refused on an ap keyset; pass --value instead.
        #[arg(long)]
        press: Option<f64>,
        /// Release sensitivity in mm to return members to, overriding --value's release half.
        /// Refused on an ap keyset; pass --value instead.
        #[arg(long)]
        release: Option<f64>,
        /// Print the exact reports without sending
        #[arg(long)]
        dry_run: bool,
    },
    /// Reset keys to the board's base value and no keyset: wh keyset remove ap --keys j
    ///
    /// For ap, this promotes a key still on touch nibble 0 ("follow global travel") to nibble 1,
    /// a pinned per-key actuation point, the same promotion create/set/delete already apply.
    Remove {
        kind: KeysetKindArg,
        #[command(flatten)]
        keys: KeysArg,
        /// Print the exact reports without sending
        #[arg(long)]
        dry_run: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_headline_command() {
        let c =
            Cli::try_parse_from(["wh", "set", "rt", "--keys", "w,a,s,d", "--set", "0.5"]).unwrap();
        match c.cmd {
            Cmd::Set {
                what: SetWhat::Rt { keys, set, off, .. },
            } => {
                assert_eq!(keys.keys.as_deref(), Some("w,a,s,d"));
                assert_eq!(set, Some(0.5));
                assert!(!off);
            }
            _ => panic!("wrong parse"),
        }
    }

    #[test]
    fn set_and_off_conflict() {
        assert!(
            Cli::try_parse_from(["wh", "set", "rt", "--keys", "w", "--set", "0.5", "--off"])
                .is_err()
        );
    }

    /// `--press`/`--release` only mean anything as an override on top of `--set`, so each must
    /// refuse to parse alongside `--off`, the same as `--set --off` does above.
    #[test]
    fn press_and_off_conflict() {
        assert!(
            Cli::try_parse_from(["wh", "set", "rt", "--keys", "w", "--press", "0.4", "--off"])
                .is_err()
        );
    }

    #[test]
    fn release_and_off_conflict() {
        assert!(Cli::try_parse_from([
            "wh",
            "set",
            "rt",
            "--keys",
            "w",
            "--release",
            "0.4",
            "--off"
        ])
        .is_err());
    }

    #[test]
    fn keys_required_unless_pick() {
        assert!(Cli::try_parse_from(["wh", "get", "rt"]).is_err());
        assert!(Cli::try_parse_from(["wh", "get", "rt", "--pick"]).is_ok());
    }

    #[test]
    fn keyset_list_takes_an_optional_kind() {
        let c = Cli::try_parse_from(["wh", "keyset", "list"]).unwrap();
        match c.cmd {
            Cmd::Keyset {
                what: KeysetWhat::List { kind },
            } => assert!(kind.is_none()),
            _ => panic!("wrong parse"),
        }
        assert!(Cli::try_parse_from(["wh", "keyset", "list", "ap"]).is_ok());
        assert!(Cli::try_parse_from(["wh", "keyset", "list", "nonsense"]).is_err());
    }

    #[test]
    fn keyset_create_requires_a_kind_and_a_selector() {
        assert!(Cli::try_parse_from(["wh", "keyset", "create", "--keys", "w"]).is_err());
        assert!(Cli::try_parse_from(["wh", "keyset", "create", "ap"]).is_err());
        assert!(Cli::try_parse_from(["wh", "keyset", "create", "ap", "--keys", "w"]).is_ok());
    }

    #[test]
    fn keyset_set_and_delete_take_a_decimal_index() {
        let c = Cli::try_parse_from(["wh", "keyset", "set", "ap", "3", "--value", "1.2"]).unwrap();
        match c.cmd {
            Cmd::Keyset {
                what: KeysetWhat::Set { index, value, .. },
            } => {
                assert_eq!(index, 3);
                assert_eq!(value, Some(1.2));
            }
            _ => panic!("wrong parse"),
        }
        assert!(Cli::try_parse_from(["wh", "keyset", "delete", "rt", "2"]).is_ok());
    }
}
