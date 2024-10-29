use clap::Parser;
use config::Config;
use flume::bounded;
use kernel::State;
use preload_rs::{
    cli::Cli,
    signals::{wait_for_signal, SignalEvent},
};
use tokio::time;
use tracing::{debug, error, trace};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // NOTE: The verbosity flag takes precedence over the environment variable
    // for log control. For example, `PRELOAD_LOG=warn preload-rs -vvv` will
    // still log at the trace level. The environment variable (`PRELOAD_LOG`)
    // can only set the log level per crate, not override the verbosity flag.
    // Eg. `PRELOAD_LOG=kernel=warn preload-rs -vvv` will log at the trace level
    // for all crates except `kernel` which will log at the warn level.
    let env_filter = EnvFilter::builder()
        .with_default_directive("sqlx=warn".parse()?)
        .with_env_var("PRELOAD_LOG")
        .from_env()?
        .add_directive(cli.verbosity.log_level_filter().as_str().parse()?);

    let layer = tracing_subscriber::fmt::layer()
        .with_level(true)
        .with_file(false)
        .with_line_number(false);

    tracing_subscriber::registry()
        .with(layer)
        .with(env_filter)
        .init();

    // load config
    let config = match &cli.conffile {
        Some(path) => Config::load(path)?,
        _ => {
            let mut candidates = glob::glob("/etc/preload-rs/config.d/*.toml")?
                .filter_map(Result::ok)
                .collect::<Vec<_>>();
            candidates.insert(0, "/etc/preload-rs/config.toml".into());
            trace!(?candidates, "config file candidates");
            Config::load_multiple(candidates)?
        }
    };
    debug!(?config, ?cli);

    // install signal handlers
    let (signals_tx, signals_rx) = bounded(8);
    let mut signal_handle = tokio::spawn(async move { wait_for_signal(signals_tx).await });

    let autosave = config.system.autosave;

    // initialize the state
    let state = State::try_new(config, cli.statefile).await?;
    let state_clone = state.clone();
    let mut state_handle = tokio::spawn(async move { state_clone.start().await });

    // start the saver in a different thread
    let state_clone = state.clone();
    let mut saver_handle = tokio::spawn(async move { saver(state_clone, autosave).await });

    loop {
        tokio::select! {
            // bubble up any errors from the signal handlers and timers
            res = &mut signal_handle => {
                let res = res?;
                if let Err(err) = &res {
                    error!("error happened during handling signals: {}", err);
                }
                res?
            }

            // bubble up any errors from the saver
            res = &mut saver_handle => {
                let res = res?;
                if let Err(err) = &res {
                    error!("error happened during saving state: {}", err);
                }
                res?
            }

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

#[inline]
async fn saver(state: State, period: std::time::Duration) -> anyhow::Result<()> {
    debug!(?period, "autosave interval");
    loop {
        time::sleep(period).await;
        debug!("autosaving state");
        state.write().await?;
    }
}
