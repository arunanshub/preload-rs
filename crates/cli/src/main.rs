use clap::Parser;
use config::Config;
use flume::bounded;
use kernel::state::State;
use preload_rs::{
    cli::Cli,
    signals::{wait_for_signal, SignalEvent},
};
use tokio::pin;
use tracing::debug;
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

    let config = match &cli.conffile {
        Some(path) => Config::load(path)?,
        _ => Config::new(),
    };
    debug!(?config);

    let mut state = State::new(config);

    let (signals_tx, signals_rx) = bounded(8);
    pin! {
       let handler = wait_for_signal(signals_tx);
    }

    loop {
        let state = &mut state;

        tokio::select! {
            // bubble up any errors from the signal handlers and timers
            res = &mut handler => { res? }

            // handle the signal events
            event_res = signals_rx.recv_async() => {
                let event = event_res?;
                debug!(?event, "Received signal event");

                match event {
                    SignalEvent::DumpStateInfo => {
                        state.dump_info();
                    }
                    SignalEvent::ManualSaveState => {
                        if let Some(path) = &cli.conffile {
                            state.reload_config(path)?;
                        }
                    }
                }
            }
        }
    }
}
