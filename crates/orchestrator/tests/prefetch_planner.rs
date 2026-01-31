#![forbid(unsafe_code)]

use config::{Config, MemoryPolicy, SortStrategy};
use orchestrator::domain::{MapSegment, MemStat};
use orchestrator::prediction::Prediction;
use orchestrator::prefetch::GreedyPrefetchPlanner;
use orchestrator::prefetch::PrefetchPlanner;
use orchestrator::stores::Stores;

#[test]
fn planner_selects_maps_within_budget() {
    let mut config = Config::default();
    config.model.memory = MemoryPolicy {
        memtotal: 0,
        memfree: 100,
        memcached: 0,
    };
    config.system.sortstrategy = SortStrategy::None;

    let planner = GreedyPrefetchPlanner::new(&config);
    let mut stores = Stores::default();

    let map_a = stores.ensure_map(MapSegment::new("/a", 0, 2048, 0));
    let map_b = stores.ensure_map(MapSegment::new("/b", 0, 2048, 0));
    let map_c = stores.ensure_map(MapSegment::new("/c", 0, 1024, 0));

    let mut prediction = Prediction::default();
    prediction.map_scores.insert(map_a, 0.9);
    prediction.map_scores.insert(map_b, 0.8);
    prediction.map_scores.insert(map_c, 0.7);

    let mem = MemStat {
        total: 0,
        free: 3,
        cached: 0,
        pagein: 0,
        pageout: 0,
    };

    let plan = planner.plan(&prediction, &stores, &mem);

    assert_eq!(plan.maps.len(), 2);
    assert!(plan.maps.contains(&map_a));
    assert!(plan.maps.contains(&map_c));
    assert!(!plan.maps.contains(&map_b));
    assert_eq!(plan.total_bytes, 2048 + 1024);
    assert_eq!(plan.budget_bytes, 3 * 1024);
}
