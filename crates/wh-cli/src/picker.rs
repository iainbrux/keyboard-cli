//! Interactive key picker for `--pick`.
//!
//! This is a stub so the crate compiles and the rest of the command tree can be built and
//! tested ahead of the real terminal UI. Task 17 replaces the body with a ratatui picker.

/// Prompts the user to pick keys interactively and returns their usages.
#[allow(dead_code)] // wired up in Task 17
pub fn pick(_universe: &[u8]) -> anyhow::Result<Vec<u8>> {
    anyhow::bail!("picker lands in Task 17")
}
