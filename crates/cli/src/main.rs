use clap::Parser;
use config::Config;
use flume::bounded;
use preload_rs::{cli::Cli, signals::wait_for_signal};
use tracing::debug;
use tracing_log::AsTrace;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    tracing_subscriber::fmt()
        .with_max_level(cli.verbosity.log_level_filter().as_trace())
        .with_level(true)
        .with_file(true)
        .with_line_number(true)
        .init();

    debug!(config = ?cli);

    let config = match cli.conffile {
        Some(path) => Config::load(path)?,
        _ => Config::new(),
    };

    let (events_tx, events_rx) = bounded(8);

    loop {
        tokio::select! {
            err = wait_for_signal(&events_tx) => {
                tracing::error!(error = ?err, "Error while waiting for signal");
                err?;
            }
            //
            res = events_rx.recv_async() => {
                let event = res?;
                debug!(?event, "Received signal event");
            }
        }
    }
}
