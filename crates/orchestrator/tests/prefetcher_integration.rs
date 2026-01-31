#![forbid(unsafe_code)]

use orchestrator::{MapSegment, PosixFadvisePrefetcher, PrefetchPlan, Prefetcher, Stores};
use tempfile::tempdir;

#[tokio::test]
async fn prefetcher_reports_failures_for_missing_file() {
    let dir = tempdir().unwrap();
    let missing = dir.path().join("missing.bin");

    let mut stores = Stores::default();
    let segment = MapSegment::new(missing.clone(), 0, 4096, 0);
    let map_key = segment.key();
    let map_id = stores.ensure_map(segment);

    let plan = PrefetchPlan {
        maps: vec![map_id],
        total_bytes: 4096,
        budget_bytes: 4096,
    };

    let prefetcher = PosixFadvisePrefetcher::new(1);
    let report = prefetcher.execute(&plan, &stores).await;

    assert_eq!(report.num_maps, 0);
    assert_eq!(report.total_bytes, 4096);
    assert!(report.failures.contains(&map_key));
}
