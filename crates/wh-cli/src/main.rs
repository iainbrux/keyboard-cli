mod cli;
mod picker;
mod run;

use clap::Parser;

fn main() {
    let cli = cli::Cli::parse();
    if let Err(e) = run::run(cli) {
        // The alternate form prints the full anyhow source chain, not just the top error, so
        // .context(...) added by later tasks stays visible instead of being swallowed here.
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}
