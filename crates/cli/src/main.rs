use clap::Parser;
use config::Config;
use flume::bounded;
use kernel::state::State;
use preload_rs::{
    cli::Cli,
    signals::{wait_for_signal, SignalEvent},
};
use std::time::Duration;
use tokio::time;
use tracing::{debug, error};
use tracing_log::AsTrace;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    debug!(?cli);
    tracing_subscriber::fmt()
        .with_max_level(cli.verbosity.log_level_filter().as_trace())
        .with_level(true)
        .with_file(true)
        .with_line_number(true)
        .init();

    // load config
    let config = match &cli.conffile {
        Some(path) => Config::load(path)?,
        _ => Config::new(),
    };
    debug!(?config);

    // install signal handlers
    let (signals_tx, signals_rx) = bounded(8);
    let mut signal_handle = tokio::spawn(async move { wait_for_signal(signals_tx).await });

    // initialize the state
    let state = State::new(config);
    let state_clone = state.clone();
    let mut state_handle = tokio::spawn(async move {
        loop {
            if let Err(err) = state_clone.scan_and_predict().await {
                error!("scan and predict failed with error: {}", err);
                return Err(err);
            }
            // TODO: get cycle value from config
            time::sleep(Duration::from_millis(200)).await;
            if let Err(err) = state_clone.update().await {
                error!("update failed with error: {}", err);
                return Err(err);
            }
            // TODO: get cycle value from config
            time::sleep(Duration::from_millis(200)).await;
        }
    });

    loop {
        tokio::select! {
            // bubble up any errors from the signal handlers and timers
            res = &mut signal_handle => { res?? }

            // bubble up any errors from the state
            res = &mut state_handle => { res?? }

            // handle the signal events
            event_res = signals_rx.recv_async() => {
                let event = event_res?;
                debug!(?event, "Received signal event");

                match event {
                    SignalEvent::DumpStateInfo => {
                        debug!("dumping state info");
                        state.dump_info().await;
                    }
                    SignalEvent::ManualSaveState => {
                        debug!("manual save state");
                        if let Some(path) = &cli.conffile {
                            state.reload_config(path).await?;
                        }
                    }
                }
            }
        }
    }
}
