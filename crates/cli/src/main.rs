#![forbid(unsafe_code)]

use clap::Parser;
use config::Config;
use orchestrator::{
    PreloadEngine, Services,
    clock::SystemClock,
    observation::{DefaultAdmissionPolicy, DefaultModelUpdater, ProcfsScanner},
    persistence::{NoopRepository, SqliteRepository},
    prediction::MarkovPredictor,
    prefetch::{GreedyPrefetchPlanner, PosixFadvisePrefetcher},
};
use std::path::PathBuf;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

#[derive(Parser, Debug)]
#[command(name = "preload-rs", version, about = "Adaptive prefetching daemon")]
struct Args {
    /// Path to the configuration file.
    #[arg(short, long, default_value = "config.toml")]
    config: PathBuf,

    /// Optional state database path.
    #[arg(short, long)]
    state: Option<PathBuf>,

    /// Run a single tick and exit.
    #[arg(long)]
    once: bool,

    /// Disable persistence entirely.
    #[arg(long)]
    no_persist: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();
    let mut config = Config::load(&args.config)?;

    if let Some(path) = args.state.clone().or(config.persistence.state_path.clone()) {
        config.persistence.state_path = Some(path);
    }

    let repo = if args.no_persist {
        Box::new(NoopRepository::default()) as Box<dyn orchestrator::persistence::StateRepository>
    } else if let Some(path) = &config.persistence.state_path {
        Box::new(SqliteRepository::new(path.clone())?)
            as Box<dyn orchestrator::persistence::StateRepository>
    } else {
        warn!("No persistence path provided; falling back to NoopRepository");
        Box::new(NoopRepository::default()) as Box<dyn orchestrator::persistence::StateRepository>
    };

    let services = Services {
        scanner: Box::new(ProcfsScanner::default()),
        admission: Box::new(DefaultAdmissionPolicy::new(&config)),
        updater: Box::new(DefaultModelUpdater::new(&config)),
        predictor: Box::new(MarkovPredictor::new(&config)),
        planner: Box::new(GreedyPrefetchPlanner::new(&config)),
        prefetcher: {
            let concurrency = config.system.prefetch_concurrency.max(1);
            Box::new(PosixFadvisePrefetcher::new(concurrency))
        },
        repo,
        clock: Box::new(SystemClock::default()),
    };

    let mut engine = PreloadEngine::load(config, services).await?;

    if args.once {
        let report = engine.tick().await?;
        info!(?report, "tick completed");
        return Ok(());
    }

    let cancel = CancellationToken::new();
    let ctrl_c = cancel.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            ctrl_c.cancel();
        }
    });

    engine.run_until(cancel).await?;
    Ok(())
}
