#![forbid(unsafe_code)]

mod cli;
mod signals;

use clap::Parser;
use cli::Cli;
use config::Config;
use orchestrator::{
    ControlEvent, PreloadEngine, ReloadBundle, Services,
    clock::SystemClock,
    observation::{DefaultAdmissionPolicy, DefaultModelUpdater, ProcfsScanner},
    persistence::{NoopRepository, SqliteRepository},
    prediction::MarkovPredictor,
    prefetch::{GreedyPrefetchPlanner, NoopPrefetcher, PosixFadvisePrefetcher, Prefetcher},
};
use std::error::Error;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    init_tracing(cli.verbose);
    let config = load_config_from_cli(&cli)?;

    let repo = if cli.no_persist {
        Box::new(NoopRepository) as Box<dyn orchestrator::persistence::StateRepository>
    } else if let Some(path) = &config.persistence.state_path {
        let repo = SqliteRepository::new(path.clone()).await?;
        Box::new(repo) as Box<dyn orchestrator::persistence::StateRepository>
    } else {
        warn!("no persistence path provided; using in-memory state only");
        Box::new(NoopRepository) as Box<dyn orchestrator::persistence::StateRepository>
    };

    let reload_bundle = build_reload_bundle(config.clone(), cli.no_prefetch);

    let services = Services {
        scanner: Box::new(ProcfsScanner),
        admission: reload_bundle.admission,
        updater: reload_bundle.updater,
        predictor: reload_bundle.predictor,
        planner: reload_bundle.planner,
        prefetcher: reload_bundle.prefetcher,
        repo,
        clock: Box::new(SystemClock),
    };

    let mut engine = PreloadEngine::load(config, services).await?;

    if cli.once {
        let report = engine.tick().await?;
        info!(?report, "tick completed");
        return Ok(());
    }

    let cancel = CancellationToken::new();
    signals::install_ctrl_c(cancel.clone());

    let (control_tx, control_rx) = mpsc::unbounded_channel();
    install_reload_handler(cli.clone(), control_tx);

    engine.run_until(cancel, control_rx).await?;
    Ok(())
}

fn init_tracing(verbosity: u8) {
    let default_level = match verbosity {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };

    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level));

    tracing_subscriber::fmt().with_env_filter(env_filter).init();
}

/// Load configuration files and apply CLI overrides.
fn load_config_from_cli(cli: &Cli) -> Result<Config, Box<dyn Error>> {
    let config_paths = cli.resolve_config_paths()?;
    let mut config = if config_paths.is_empty() {
        warn!("no config files found; falling back to defaults");
        Config::default()
    } else {
        Config::load_multiple(config_paths)?
    };

    if let Some(path) = cli.state.clone().or(config.persistence.state_path.clone()) {
        config.persistence.state_path = Some(path);
    }

    Ok(config)
}

/// Construct runtime services for a new configuration snapshot.
fn build_reload_bundle(config: Config, no_prefetch: bool) -> ReloadBundle {
    ReloadBundle {
        admission: Box::new(DefaultAdmissionPolicy::new(&config)),
        updater: Box::new(DefaultModelUpdater::new(&config)),
        predictor: Box::new(MarkovPredictor::new(&config)),
        planner: Box::new(GreedyPrefetchPlanner::new(&config)),
        prefetcher: build_prefetcher(&config, no_prefetch),
        config,
    }
}

/// Select the prefetcher implementation based on configuration and CLI flags.
fn build_prefetcher(config: &Config, no_prefetch: bool) -> Box<dyn Prefetcher> {
    if no_prefetch || config.system.prefetch_concurrency == 0 {
        Box::new(NoopPrefetcher)
    } else {
        Box::new(PosixFadvisePrefetcher::new(
            config.system.prefetch_concurrency,
        ))
    }
}

/// Install a SIGHUP handler to reload configuration at runtime.
fn install_reload_handler(cli: Cli, control_tx: mpsc::UnboundedSender<ControlEvent>) {
    #[cfg(unix)]
    {
        tokio::spawn(async move {
            use tokio::signal::unix::{SignalKind, signal};
            let mut hup = match signal(SignalKind::hangup()) {
                Ok(stream) => stream,
                Err(err) => {
                    warn!(?err, "failed to install SIGHUP handler");
                    return;
                }
            };
            while hup.recv().await.is_some() {
                match load_config_from_cli(&cli) {
                    Ok(config) => {
                        let bundle = build_reload_bundle(config, cli.no_prefetch);
                        if control_tx.send(ControlEvent::Reload(bundle)).is_err() {
                            break;
                        }
                    }
                    Err(err) => {
                        warn!(?err, "failed to reload config");
                    }
                }
            }
        });
    }

    #[cfg(not(unix))]
    {
        let _ = (cli, control_tx);
    }
}
