#![forbid(unsafe_code)]

mod cli;
mod signals;

use clap::Parser;
use cli::Cli;
use config::Config;
use orchestrator::{
    PreloadEngine, Services,
    clock::SystemClock,
    observation::{DefaultAdmissionPolicy, DefaultModelUpdater, ProcfsScanner},
    persistence::{NoopRepository, SqliteRepository},
    prediction::MarkovPredictor,
    prefetch::{GreedyPrefetchPlanner, NoopPrefetcher, PosixFadvisePrefetcher},
};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();
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

    let repo = if cli.no_persist {
        Box::new(NoopRepository) as Box<dyn orchestrator::persistence::StateRepository>
    } else if let Some(path) = &config.persistence.state_path {
        let repo = SqliteRepository::new(path.clone()).await?;
        Box::new(repo) as Box<dyn orchestrator::persistence::StateRepository>
    } else {
        warn!("no persistence path provided; using in-memory state only");
        Box::new(NoopRepository) as Box<dyn orchestrator::persistence::StateRepository>
    };

    let prefetcher: Box<dyn orchestrator::prefetch::Prefetcher> =
        if cli.no_prefetch || config.system.prefetch_concurrency == 0 {
            Box::new(NoopPrefetcher)
        } else {
            Box::new(PosixFadvisePrefetcher::new(
                config.system.prefetch_concurrency,
            ))
        };

    let services = Services {
        scanner: Box::new(ProcfsScanner),
        admission: Box::new(DefaultAdmissionPolicy::new(&config)),
        updater: Box::new(DefaultModelUpdater::new(&config)),
        predictor: Box::new(MarkovPredictor::new(&config)),
        planner: Box::new(GreedyPrefetchPlanner::new(&config)),
        prefetcher,
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

    engine.run_until(cancel).await?;
    Ok(())
}
