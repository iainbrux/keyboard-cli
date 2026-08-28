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
        #[arg(long)]
        json: bool,
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
        #[arg(long)]
        to: Option<std::path::PathBuf>,
    },
    /// Write a snapshot back to the board
    Restore {
        /// Snapshot file, omit with --last for the newest auto-backup
        file: Option<std::path::PathBuf>,
        #[arg(long)]
        last: bool,
    },
    /// Key names and groups
    Keys {
        #[command(subcommand)]
        what: KeysWhat,
    },
    /// No-op write self-test (writes current values back, verifies)
    Selftest,
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
        #[arg(long)]
        press: Option<f64>,
        /// Release sensitivity in mm (overrides --set for release)
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
        #[command(flatten)]
        keys: KeysArg,
        /// Depth in mm
        #[arg(long)]
        set: f64,
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

    #[test]
    fn keys_required_unless_pick() {
        assert!(Cli::try_parse_from(["wh", "get", "rt"]).is_err());
        assert!(Cli::try_parse_from(["wh", "get", "rt", "--pick"]).is_ok());
    }
}
