#![forbid(unsafe_code)]

use config::{Config, MemoryPolicy, SortStrategy};
use orchestrator::domain::{MapSegment, MemStat};
use orchestrator::prediction::Prediction;
use orchestrator::prefetch::GreedyPrefetchPlanner;
use orchestrator::prefetch::PrefetchPlanner;
use orchestrator::stores::Stores;
use std::os::linux::fs::MetadataExt;
use tempfile::tempdir;

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

#[test]
fn planner_sorts_by_block_with_score_tiebreak() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("data.bin");
    std::fs::write(&path, vec![0u8; 16 * 1024]).unwrap();

    let mut config = Config::default();
    config.model.memory = MemoryPolicy {
        memtotal: 0,
        memfree: 100,
        memcached: 0,
    };
    config.system.sortstrategy = SortStrategy::Block;

    let planner = GreedyPrefetchPlanner::new(&config);
    let mut stores = Stores::default();

    let map_a = stores.ensure_map(MapSegment::new(&path, 8192, 1024, 0));
    let map_b = stores.ensure_map(MapSegment::new(&path, 0, 1024, 0));
    let map_c = stores.ensure_map(MapSegment::new(&path, 4096, 1024, 0));

    let mut prediction = Prediction::default();
    prediction.map_scores.insert(map_a, 1.0);
    prediction.map_scores.insert(map_b, 1.0);
    prediction.map_scores.insert(map_c, 1.0);

    let mem = MemStat {
        total: 0,
        free: 64,
        cached: 0,
        pagein: 0,
        pageout: 0,
    };

    let plan = planner.plan(&prediction, &stores, &mem);

    assert_eq!(plan.maps, vec![map_b, map_c, map_a]);
}

/// Regression test for https://github.com/arunanshub/preload-rs/issues/233.
///
/// When some selected maps have no sort key (their backing file is gone, so
/// `fs::metadata` fails) while others do, the old comparator mixed key-based
/// and index-based tie-breaks per pair, violating transitivity and making
/// `sort_by` panic with "user-provided comparison function does not correctly
/// implement a total order".
#[test]
fn planner_sort_survives_maps_with_missing_files() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("data.bin");
    std::fs::write(&path, vec![0u8; 1024 * 1024]).unwrap();

    let mut config = Config::default();
    config.model.memory = MemoryPolicy {
        memtotal: 0,
        memfree: 100,
        memcached: 0,
    };
    config.system.sortstrategy = SortStrategy::Block;

    let planner = GreedyPrefetchPlanner::new(&config);
    let mut stores = Stores::default();

    let mut prediction = Prediction::default();
    let count = 64u64;
    for i in 0..count {
        // Every third map points at a file that does not exist (no sort key);
        // the rest use decreasing offsets so key order opposes insertion order.
        let map_id = if i % 3 == 0 {
            stores.ensure_map(MapSegment::new(format!("/nonexistent/{i}"), 0, 1024, 0))
        } else {
            stores.ensure_map(MapSegment::new(&path, (count - i) * 4096, 1024, 0))
        };
        prediction.map_scores.insert(map_id, 1.0);
    }

    let mem = MemStat {
        total: 0,
        free: 1024 * 1024,
        cached: 0,
        pagein: 0,
        pageout: 0,
    };

    let plan = planner.plan(&prediction, &stores, &mem);
    assert_eq!(plan.maps.len(), count as usize);
}

/// NaN scores must not panic the planner's sorts either.
#[test]
fn planner_sort_survives_nan_scores() {
    let mut config = Config::default();
    config.model.memory = MemoryPolicy {
        memtotal: 0,
        memfree: 100,
        memcached: 0,
    };
    config.system.sortstrategy = SortStrategy::Path;

    let planner = GreedyPrefetchPlanner::new(&config);
    let mut stores = Stores::default();

    let mut prediction = Prediction::default();
    for i in 0..64 {
        let map_id = stores.ensure_map(MapSegment::new(format!("/map/{i}"), 0, 1024, 0));
        let score = match i % 3 {
            0 => f32::NAN,
            1 => 1.0,
            _ => 0.5,
        };
        prediction.map_scores.insert(map_id, score);
    }

    let mem = MemStat {
        total: 0,
        free: 1024 * 1024,
        cached: 0,
        pagein: 0,
        pageout: 0,
    };

    let plan = planner.plan(&prediction, &stores, &mem);
    assert_eq!(plan.maps.len(), 64);
}

#[test]
fn planner_sorts_by_inode_with_score_tiebreak() {
    let dir = tempdir().unwrap();
    let path_a = dir.path().join("a.bin");
    let path_b = dir.path().join("b.bin");
    std::fs::write(&path_a, vec![0u8; 4096]).unwrap();
    std::fs::write(&path_b, vec![1u8; 4096]).unwrap();

    let inode_a = std::fs::metadata(&path_a).unwrap().st_ino();
    let inode_b = std::fs::metadata(&path_b).unwrap().st_ino();

    let mut config = Config::default();
    config.model.memory = MemoryPolicy {
        memtotal: 0,
        memfree: 100,
        memcached: 0,
    };
    config.system.sortstrategy = SortStrategy::Inode;

    let planner = GreedyPrefetchPlanner::new(&config);
    let mut stores = Stores::default();

    let map_a = stores.ensure_map(MapSegment::new(&path_a, 0, 1024, 0));
    let map_b = stores.ensure_map(MapSegment::new(&path_b, 0, 1024, 0));

    let mut prediction = Prediction::default();
    prediction.map_scores.insert(map_a, 1.0);
    prediction.map_scores.insert(map_b, 1.0);

    let mem = MemStat {
        total: 0,
        free: 64,
        cached: 0,
        pagein: 0,
        pageout: 0,
    };

    let plan = planner.plan(&prediction, &stores, &mem);

    let expected = if inode_a <= inode_b {
        vec![map_a, map_b]
    } else {
        vec![map_b, map_a]
    };
    assert_eq!(plan.maps, expected);
}
