mod cli;
mod picker;
mod run;

use clap::Parser;

/// True when `e`'s cause chain includes an OS `BrokenPipe`, i.e. this command's own write to a
/// closed pipe (`wh dump | head -1`, once `head` stops reading), not a device, protocol, or
/// selector failure. Walks the whole `anyhow` chain rather than only the outermost error,
/// since a write failure inside `dump`/`get`/`list_keys`/`group` reaches here through however
/// many `?`s and `.context(...)` calls sit between the `writeln!` and `run::run`'s return.
fn is_broken_pipe(e: &anyhow::Error) -> bool {
    e.chain()
        .filter_map(|c| c.downcast_ref::<std::io::Error>())
        .any(|io_err| io_err.kind() == std::io::ErrorKind::BrokenPipe)
}

fn main() {
    let cli = cli::Cli::parse();
    if let Err(e) = run::run(cli) {
        if is_broken_pipe(&e) {
            // The reader on the other end of stdout stopped reading and closed the pipe; the
            // command's own output ran into that closed pipe partway through producing it.
            // That is expected Unix pipe behaviour, not a failure of this program, so exit
            // quietly instead of reporting the write failure as if it were the real error.
            std::process::exit(0);
        }
        // The alternate form prints the full anyhow source chain, not just the top error, so
        // .context(...) added by later tasks stays visible instead of being swallowed here.
        //
        // This write is best-effort: if stderr is itself a closed pipe too (e.g. `2>&1 | head
        // -1`), it can fail the same way. That failure is not the thing that mattered, `e`
        // already is, so the process still exits 1 whether or not this message was delivered,
        // rather than letting an unrelated write failure claim success for a real error.
        use std::io::Write;
        let _ = writeln!(std::io::stderr(), "error: {e:#}");
        std::process::exit(1);
    }
}
