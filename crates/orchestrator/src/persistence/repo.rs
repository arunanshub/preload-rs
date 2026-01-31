#![forbid(unsafe_code)]

use crate::error::Error;
use crate::persistence::{
    ExeMapRecord, ExeRecord, MapRecord, MarkovRecord, SNAPSHOT_SCHEMA_VERSION, SnapshotMeta,
    StateSnapshot, StoresSnapshot,
};
use async_trait::async_trait;
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use std::path::PathBuf;
use std::time::SystemTime;
use tracing::debug;

#[async_trait]
pub trait StateRepository: Send + Sync {
    /// Load a snapshot from persistence.
    async fn load(&self) -> Result<StoresSnapshot, Error>;
    /// Persist a snapshot.
    async fn save(&self, snapshot: &StoresSnapshot) -> Result<(), Error>;
}

#[derive(Debug, Default)]
pub struct NoopRepository;

#[async_trait]
impl StateRepository for NoopRepository {
    async fn load(&self) -> Result<StoresSnapshot, Error> {
        Ok(StoresSnapshot {
            meta: SnapshotMeta {
                schema_version: SNAPSHOT_SCHEMA_VERSION,
                app_version: None,
                created_at: None,
            },
            state: StateSnapshot {
                model_time: 0,
                last_accounting_time: 0,
                exes: Vec::new(),
                maps: Vec::new(),
                exe_maps: Vec::new(),
                markov_edges: Vec::new(),
            },
        })
    }

    async fn save(&self, _snapshot: &StoresSnapshot) -> Result<(), Error> {
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct SqliteRepository {
    path: PathBuf,
    pool: SqlitePool,
}

impl SqliteRepository {
    /// Create a repository backed by a SQLite database file.
    pub async fn new(path: PathBuf) -> Result<Self, Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal);

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;

        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(sqlx::Error::from)?;

        Ok(Self { path, pool })
    }

    async fn save_snapshot(&self, snapshot: &StoresSnapshot) -> Result<(), Error> {
        let mut tx = self.pool.begin().await?;

        sqlx::query!("DELETE FROM state").execute(&mut *tx).await?;
        sqlx::query!("DELETE FROM exes").execute(&mut *tx).await?;
        sqlx::query!("DELETE FROM maps").execute(&mut *tx).await?;
        sqlx::query!("DELETE FROM exe_maps")
            .execute(&mut *tx)
            .await?;
        sqlx::query!("DELETE FROM markovs")
            .execute(&mut *tx)
            .await?;

        let meta = &snapshot.meta;
        let created_at = meta
            .created_at
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_secs().to_string());

        let app_version = meta.app_version.clone();
        let schema_version = meta.schema_version as i64;
        let model_time = snapshot.state.model_time as i64;
        let last_accounting_time = snapshot.state.last_accounting_time as i64;
        sqlx::query!(
            "INSERT INTO state (id, schema_version, app_version, created_at, model_time, last_accounting_time) \
             VALUES (1, ?, ?, ?, ?, ?)",
            schema_version,
            app_version,
            created_at,
            model_time,
            last_accounting_time
        )
        .execute(&mut *tx)
        .await?;

        for exe in &snapshot.state.exes {
            let path = exe.path.to_string_lossy().to_string();
            let total_running_time = exe.total_running_time as i64;
            let last_seen_time = exe.last_seen_time.map(|v| v as i64);
            sqlx::query!(
                "INSERT INTO exes (path, total_running_time, last_seen_time) VALUES (?, ?, ?)",
                path,
                total_running_time,
                last_seen_time
            )
            .execute(&mut *tx)
            .await?;
        }

        for map in &snapshot.state.maps {
            let path = map.path.to_string_lossy().to_string();
            let offset = map.offset as i64;
            let length = map.length as i64;
            let update_time = map.update_time as i64;
            sqlx::query!(
                "INSERT INTO maps (path, offset, length, update_time) VALUES (?, ?, ?, ?)",
                path,
                offset,
                length,
                update_time
            )
            .execute(&mut *tx)
            .await?;
        }

        for map in &snapshot.state.exe_maps {
            let exe_path = map.exe_path.to_string_lossy().to_string();
            let map_path = map.map_key.path.to_string_lossy().to_string();
            let map_offset = map.map_key.offset as i64;
            let map_length = map.map_key.length as i64;
            let prob = map.prob as f64;
            sqlx::query!(
                "INSERT INTO exe_maps (exe_path, map_path, map_offset, map_length, prob) \
                 VALUES (?, ?, ?, ?, ?)",
                exe_path,
                map_path,
                map_offset,
                map_length,
                prob
            )
            .execute(&mut *tx)
            .await?;
        }

        for markov in &snapshot.state.markov_edges {
            let ttl: Vec<u8> = rkyv::to_bytes::<rkyv::rancor::Error>(&markov.time_to_leave)
                .map_err(|err| Error::RkyvSerialize(err.to_string()))?
                .into();
            let tp: Vec<u8> = rkyv::to_bytes::<rkyv::rancor::Error>(&markov.transition_prob)
                .map_err(|err| Error::RkyvSerialize(err.to_string()))?
                .into();
            let exe_a = markov.exe_a.to_string_lossy().to_string();
            let exe_b = markov.exe_b.to_string_lossy().to_string();
            let both_running_time = markov.both_running_time as i64;
            sqlx::query!(
                "INSERT INTO markovs (exe_a, exe_b, time_to_leave, transition_prob, both_running_time) \
                 VALUES (?, ?, ?, ?, ?)",
                exe_a,
                exe_b,
                ttl,
                tp,
                both_running_time
            )
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        debug!(path = %self.path.display(), "snapshot persisted");
        Ok(())
    }

    async fn load_snapshot(&self) -> Result<StoresSnapshot, Error> {
        let mut meta = SnapshotMeta {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            app_version: None,
            created_at: None,
        };
        let mut state = StateSnapshot {
            model_time: 0,
            last_accounting_time: 0,
            exes: Vec::new(),
            maps: Vec::new(),
            exe_maps: Vec::new(),
            markov_edges: Vec::new(),
        };

        let row = sqlx::query!(
            "SELECT schema_version as \"schema_version!\", app_version, created_at, \
             model_time as \"model_time!\", last_accounting_time as \"last_accounting_time!\" \
             FROM state WHERE id = 1"
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            meta.schema_version = row.schema_version as u32;
            meta.app_version = row.app_version;
            meta.created_at = row
                .created_at
                .and_then(|s| s.parse::<u64>().ok())
                .map(|secs| SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs));
            state.model_time = row.model_time as u64;
            state.last_accounting_time = row.last_accounting_time as u64;
        }

        let rows = sqlx::query!(
            "SELECT path as \"path!\", total_running_time as \"total_running_time!\", last_seen_time \
             FROM exes"
        )
            .fetch_all(&self.pool)
            .await?;
        for row in rows {
            state.exes.push(ExeRecord {
                path: PathBuf::from(row.path),
                total_running_time: row.total_running_time as u64,
                last_seen_time: row.last_seen_time.map(|v| v as u64),
            });
        }

        let rows = sqlx::query!(
            "SELECT path as \"path!\", offset as \"offset!\", length as \"length!\", update_time as \"update_time!\" \
             FROM maps"
        )
            .fetch_all(&self.pool)
            .await?;
        for row in rows {
            state.maps.push(MapRecord {
                path: PathBuf::from(row.path),
                offset: row.offset as u64,
                length: row.length as u64,
                update_time: row.update_time as u64,
            });
        }

        let rows = sqlx::query!(
            "SELECT exe_path as \"exe_path!\", map_path as \"map_path!\", map_offset as \"map_offset!\", \
             map_length as \"map_length!\", prob as \"prob!\" FROM exe_maps"
        )
        .fetch_all(&self.pool)
        .await?;
        for row in rows {
            state.exe_maps.push(ExeMapRecord {
                exe_path: PathBuf::from(row.exe_path),
                map_key: crate::domain::MapKey::new(
                    row.map_path,
                    row.map_offset as u64,
                    row.map_length as u64,
                ),
                prob: row.prob as f32,
            });
        }

        let rows = sqlx::query!(
            "SELECT exe_a as \"exe_a!\", exe_b as \"exe_b!\", time_to_leave as \"time_to_leave!\", \
             transition_prob as \"transition_prob!\", both_running_time as \"both_running_time!\" \
             FROM markovs"
        )
        .fetch_all(&self.pool)
        .await?;
        for row in rows {
            let ttl = row.time_to_leave;
            let tp = row.transition_prob;
            let time_to_leave: [f32; 4] = rkyv::from_bytes::<[f32; 4], rkyv::rancor::Error>(&ttl)
                .map_err(|err| Error::RkyvDeserialize(err.to_string()))?;
            let transition_prob: [[f32; 4]; 4] =
                rkyv::from_bytes::<[[f32; 4]; 4], rkyv::rancor::Error>(&tp)
                    .map_err(|err| Error::RkyvDeserialize(err.to_string()))?;
            state.markov_edges.push(MarkovRecord {
                exe_a: PathBuf::from(row.exe_a),
                exe_b: PathBuf::from(row.exe_b),
                time_to_leave,
                transition_prob,
                both_running_time: row.both_running_time as u64,
            });
        }

        Ok(StoresSnapshot { meta, state })
    }
}

#[async_trait]
impl StateRepository for SqliteRepository {
    async fn load(&self) -> Result<StoresSnapshot, Error> {
        self.load_snapshot().await
    }

    async fn save(&self, snapshot: &StoresSnapshot) -> Result<(), Error> {
        self.save_snapshot(snapshot).await
    }
}
