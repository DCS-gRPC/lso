mod client;
mod commands;
mod data;
mod draw;
mod error;
mod tasks;
mod track;
mod transform;
mod utils;

use clap::Parser;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{filter, fmt};
use utils::shutdown::Shutdown;

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
    let max_level = match opts.verbose {
        0 => tracing::Level::INFO,
        1 => tracing::Level::DEBUG,
        _ => tracing::Level::TRACE,
    };
    tracing_subscriber::registry()
        .with(filter::filter_fn(move |m| {
            m.target().starts_with("lso") && m.level() <= &max_level
        }))
        .with(fmt::layer())
        .init();

    // shutdown gracefully on CTRL+C
    let shutdown = Shutdown::new();
    let shutdown_handle = shutdown.handle();
    tokio::task::spawn(async {
        tokio::signal::ctrl_c().await.unwrap();
        shutdown.shutdown().await;
    });

    match opts.command {
        Command::Run(opts) => commands::run::execute(opts, shutdown_handle).await,
        // TODO: better error report than unwrap?
        Command::File(opts) => commands::file::execute(opts).unwrap(),
    }
}
