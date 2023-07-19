mod client;
mod commands;
mod data;
mod draw;
mod error;
mod tasks;
#[cfg(test)]
mod tests;
mod track;
mod transform;
mod utils;

use clap::{ArgAction, Parser};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{filter, fmt};
use utils::shutdown::Shutdown;

#[derive(clap::Parser)]
#[clap(version = env!("CARGO_PKG_VERSION"))]
struct Opts {
    /// A level of verbosity, and can be used multiple times
    #[clap(short, long, action = ArgAction::Count)]
    verbose: u8,
    /// Enable colorized output
    #[clap(long)]
    color: bool,
    #[clap(subcommand)]
    command: Command,
}

#[derive(clap::Parser)]
enum Command {
    /// Connect to DCS-gRPC to track carrier recoveries.
    Run(commands::run::Opts),

    /// Extract carrier recoveries from ACMI recordings (must be recordings created by the LSO;
    /// recordings directly from TacView will not work).
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
        .with(fmt::layer().with_ansi(opts.color))
        .init();

    // shutdown gracefully on CTRL+C
    let shutdown = Shutdown::new();
    let shutdown_handle = shutdown.handle();
    tokio::task::spawn(async {
        tokio::signal::ctrl_c().await.unwrap();
        shutdown.shutdown().await;
    });

    match opts.command {
        Command::Run(opts) => commands::run::execute(opts, shutdown_handle).await.unwrap(),
        // TODO: better error report than unwrap?
        Command::File(opts) => commands::file::execute(opts).unwrap(),
    }
}
