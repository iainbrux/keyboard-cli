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
    /// Write a setting
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
    /// Read and write SOCD pairs (two keys whose opposing inputs resolve to one)
    Socd {
        #[command(subcommand)]
        what: SocdWhat,
    },
}

#[derive(Subcommand)]
pub enum SocdWhat {
    /// List the board's SOCD pairs: wh socd list
    List,
    /// Pair two keys: wh socd pair a d --priority d
    ///
    /// A key may sit in one pair only, so this refuses if either key is already paired.
    Pair {
        /// The first key of the pair
        key_a: String,
        /// The second key of the pair
        key_b: String,
        /// Which key wins when both are held: one of the two key names, or `last-input`. The
        /// default comes from the same constant the announcements print, so the flag and the
        /// output can never name it differently.
        #[arg(long, default_value = wh_proto::socd::LAST_INPUT)]
        priority: String,
        /// Print the exact reports without sending
        #[arg(long)]
        dry_run: bool,
    },
    /// Remove the pair each named key belongs to, both members at once: wh socd unpair a
    Unpair {
        /// One or more keys, each naming the pair it belongs to
        #[arg(required = true)]
        keys: Vec<String>,
        /// Print the exact reports without sending
        #[arg(long)]
        dry_run: bool,
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
        /// Press sensitivity in mm (overrides --set for press; with --off, the value the keys
        /// are reset to instead of the board's global)
        #[arg(long)]
        press: Option<f64>,
        /// Release sensitivity in mm (overrides --set for release; with --off, the value the
        /// keys are reset to instead of the board's global)
        #[arg(long)]
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
        /// Key selector: "w,a,s,d", "wasd", "all,!space", "f1-f12", user groups. Not used with
        /// --base, which names the board rather than a selection.
        #[arg(long, required_unless_present_any = ["pick", "base"], conflicts_with = "base")]
        keys: Option<String>,
        /// Pick keys interactively instead. Not used with --base.
        #[arg(long, conflicts_with = "base")]
        pick: bool,
        /// Depth in mm
        #[arg(long, required_unless_present = "base", conflicts_with = "base")]
        set: Option<f64>,
        /// Set the board's base actuation point: every key outside every keyset moves to this
        /// depth, and every keyset is left untouched. Takes no --keys, since it names the board
        /// rather than a selection. Deliberately not `--mm`, which is reserved for the
        /// configurator's "MM" CUSTOM VALUE, a different setting.
        #[arg(long)]
        base: Option<f64>,
        /// Print the exact reports without sending
        #[arg(long)]
        dry_run: bool,
    },
    /// The configurator's "MM" CUSTOM VALUE: the step size for its steppers, not an actuation
    /// point. Takes no --keys or --pick: this is a board-global setting, like --base but with no
    /// selection to make at all.
    Mm {
        /// Value in mm, 0 to 4
        #[arg(long)]
        value: f64,
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

    /// `--press`/`--release` mean two things: an override on top of `--set`, and, alongside
    /// `--off`, the sensitivity the keys are reset to. `wh set rt --off` refuses outright when the
    /// board's free keys disagree on that value and names these two flags as the way past, so they
    /// have to parse alongside `--off` or the refusal names a flag the operator cannot use.
    #[test]
    fn press_and_release_parse_alongside_off() {
        let c = Cli::try_parse_from([
            "wh",
            "set",
            "rt",
            "--keys",
            "w",
            "--off",
            "--press",
            "0.3",
            "--release",
            "0.4",
        ])
        .expect("--press/--release must be usable with --off");
        match c.cmd {
            Cmd::Set {
                what:
                    SetWhat::Rt {
                        press,
                        release,
                        off,
                        ..
                    },
            } => {
                assert_eq!(press, Some(0.3));
                assert_eq!(release, Some(0.4));
                assert!(off);
            }
            _ => panic!("wrong parse"),
        }
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
