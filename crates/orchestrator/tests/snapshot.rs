#![forbid(unsafe_code)]

use orchestrator::StateRepository;
use orchestrator::domain::MapKey;
use orchestrator::persistence::{
    ExeMapRecord, ExeRecord, MapRecord, MarkovRecord, SNAPSHOT_SCHEMA_VERSION, SnapshotMeta,
    SqliteRepository, StateSnapshot, StoresSnapshot,
};
use std::path::PathBuf;
use tempfile::tempdir;

#[tokio::test]
async fn sqlite_roundtrip_snapshot() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("state.db");

    let snapshot = StoresSnapshot {
        meta: SnapshotMeta {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            app_version: Some("test".into()),
            created_at: None,
        },
        state: StateSnapshot {
            model_time: 10,
            last_accounting_time: 5,
            exes: vec![ExeRecord {
                path: PathBuf::from("/usr/bin/app"),
                total_running_time: 42,
                last_seen_time: Some(9),
            }],
            maps: vec![MapRecord {
                path: PathBuf::from("/usr/lib/libfoo.so"),
                offset: 0,
                length: 4096,
                update_time: 10,
            }],
            exe_maps: vec![ExeMapRecord {
                exe_path: PathBuf::from("/usr/bin/app"),
                map_key: MapKey::new(PathBuf::from("/usr/lib/libfoo.so"), 0, 4096),
                prob: 1.0,
            }],
            markov_edges: vec![MarkovRecord {
                exe_a: PathBuf::from("/usr/bin/app"),
                exe_b: PathBuf::from("/usr/bin/app2"),
                time_to_leave: [0.0; 4],
                transition_prob: [[0.0; 4]; 4],
                both_running_time: 0,
            }],
        },
    };

    let repo = SqliteRepository::new(db_path).await.unwrap();
    repo.save(&snapshot).await.unwrap();
    let loaded = repo.load().await.unwrap();

    assert_eq!(loaded.state.exes.len(), 1);
    assert_eq!(loaded.state.maps.len(), 1);
    assert_eq!(loaded.state.exe_maps.len(), 1);
    assert_eq!(loaded.state.markov_edges.len(), 1);
    assert_eq!(loaded.state.model_time, 10);
}
