#![forbid(unsafe_code)]

use config::Config;
use orchestrator::clock::SystemClock;
use orchestrator::observation::{DefaultAdmissionPolicy, DefaultModelUpdater, ProcfsScanner};
use orchestrator::persistence::NoopRepository;
use orchestrator::prediction::MarkovPredictor;
use orchestrator::prefetch::{GreedyPrefetchPlanner, NoopPrefetcher};
use orchestrator::{PreloadEngine, Services};

#[cfg(target_os = "linux")]
#[tokio::test]
async fn procfs_scanner_observes_current_exe() {
    let exe_path = std::env::current_exe().expect("current exe path");
    let exe_str = exe_path.to_string_lossy().to_string();

    let mut config = Config::default();
    config.model.minsize = 1;
    config.system.exeprefix = vec!["!/".into(), exe_str.clone()];
    config.system.mapprefix = vec!["!/".into(), exe_str.clone()];
    config.system.dopredict = false;

    let services = Services {
        scanner: Box::new(ProcfsScanner),
        admission: Box::new(DefaultAdmissionPolicy::new(&config)),
        updater: Box::new(DefaultModelUpdater::new(&config)),
        predictor: Box::new(MarkovPredictor::new(&config)),
        planner: Box::new(GreedyPrefetchPlanner::new(&config)),
        prefetcher: Box::new(NoopPrefetcher),
        repo: Box::new(NoopRepository),
        clock: Box::new(SystemClock),
    };

    let mut engine = PreloadEngine::new(config, services).await.unwrap();
    let _ = engine.tick().await.unwrap();

    let stores = engine.stores();
    let has_exe = stores.exes.keys().any(|key| key.path() == &exe_path);
    assert!(has_exe, "expected current exe to be admitted");

    let has_map = stores.maps.iter().any(|(_, map)| map.path == exe_path);
    assert!(has_map, "expected at least one map for current exe");
}
