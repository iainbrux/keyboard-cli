mod cli;
mod picker;
mod run;

use clap::Parser;

/// A reader that stops early (`wh keys list | head -1`, closing the pipe once `head` has what
/// it wants) closes stdout out from under this process. `println!` has no fallible form: a
/// write after that point panics with "failed printing to stdout: Broken pipe (os error 32)"
/// and a backtrace hint, which is expected Unix pipe behaviour, not a bug in this program, and
/// not something the user did anything wrong to deserve seeing as a crash.
///
/// `println!`'s panic only exposes the formatted message, not a typed `io::Error`, so detection
/// here is a string match rather than a downcast. `Broken pipe (os error 32)` is std's Display
/// text for `ErrorKind::BrokenPipe` on Unix (the `EPIPE` errno), which is the platform this was
/// demonstrated on; every other panic still goes through the previous (default) hook unchanged.
fn install_broken_pipe_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let msg = info
            .payload()
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| info.payload().downcast_ref::<&str>().copied())
            .unwrap_or_default();
        if msg.contains("Broken pipe") {
            std::process::exit(0);
        }
        default_hook(info);
    }));
}

fn main() {
    install_broken_pipe_panic_hook();
    let cli = cli::Cli::parse();
    if let Err(e) = run::run(cli) {
        // The alternate form prints the full anyhow source chain, not just the top error, so
        // .context(...) added by later tasks stays visible instead of being swallowed here.
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}
