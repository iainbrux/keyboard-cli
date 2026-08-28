mod cli;
mod picker;
mod run;

use clap::Parser;

fn main() {
    let cli = cli::Cli::parse();
    if let Err(e) = run::run(cli) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
