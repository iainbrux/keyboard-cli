mod cli;
mod keyset;
mod picker;
mod run;

use clap::Parser;

/// True when `e`'s cause chain includes an OS `BrokenPipe`, i.e. a write to a closed pipe
/// (`wh dump | head -1`, once `head` stops reading), not a device or protocol failure. Walks
/// the full `anyhow` chain since the error may reach here through several `?`s and `.context(...)`
/// calls.
fn is_broken_pipe(e: &anyhow::Error) -> bool {
    e.chain()
        .filter_map(|c| c.downcast_ref::<std::io::Error>())
        .any(|io_err| io_err.kind() == std::io::ErrorKind::BrokenPipe)
}

fn main() {
    let cli = cli::Cli::parse();
    if let Err(e) = run::run(cli) {
        if is_broken_pipe(&e) {
            // Expected Unix pipe behaviour, not a program failure: exit quietly.
            std::process::exit(0);
        }
        // `{e:#}` prints the full anyhow chain, not just the top error. The write is
        // best-effort: if stderr is also a closed pipe, we still exit 1 regardless.
        use std::io::Write;
        let _ = writeln!(std::io::stderr(), "error: {e:#}");
        std::process::exit(1);
    }
}
