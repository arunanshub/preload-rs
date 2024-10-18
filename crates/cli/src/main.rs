use clap::Parser;
use config::Config;
use flume::bounded;
use kernel::State;
use preload_rs::{
    cli::Cli,
    signals::{wait_for_signal, SignalEvent},
};
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
    let state = State::try_new(config, cli.statefile).await?;
    let state_clone = state.clone();
    let mut state_handle = tokio::spawn(async move { state_clone.start().await });

    loop {
        tokio::select! {
            // bubble up any errors from the signal handlers and timers
            res = &mut signal_handle => { res?? }

            // bubble up any errors from the state
            res = &mut state_handle => {
                let res = res?;
                if let Err(err) = &res {
                    error!("error happened in state: {}", err);
                }
                res?
            }

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
                        state.write().await?;
                    }
                }
            }
        }
    }
}
