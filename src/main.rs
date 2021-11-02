mod client;
mod commands;
mod data;
mod draw;
mod tasks;
mod transform;
mod utils;

use clap::Parser;

/// This doc string acts as a help message when the user runs '--help'
/// as do all doc strings on fields
#[derive(clap::Parser)]
#[clap(version = env!("CARGO_PKG_VERSION"))]
struct Opts {
    /// A level of verbosity, and can be used multiple times
    #[clap(short, long, parse(from_occurrences))]
    verbose: i32,
    #[clap(subcommand)]
    command: Command,
}

#[derive(clap::Parser)]
enum Command {
    Run(commands::run::Opts),
    File(commands::file::Opts),
}

#[tokio::main]
async fn main() {
    let opts: Opts = Opts::parse();
    tracing_subscriber::fmt()
        .with_max_level(match opts.verbose {
            0 => tracing::Level::INFO,
            1 => tracing::Level::DEBUG,
            _ => tracing::Level::TRACE,
        })
        .finish();

    match opts.command {
        Command::Run(opts) => commands::run::execute(opts).await,
        // TODO: better error report than unwrap?
        Command::File(opts) => commands::file::execute(opts).unwrap(),
    }
}
