#![forbid(unsafe_code)]

use crate::error::Error;
use crate::persistence::{
    ExeMapRecord, ExeRecord, MapRecord, MarkovRecord, SNAPSHOT_SCHEMA_VERSION, SnapshotMeta,
    StateSnapshot, StoresSnapshot,
};
use async_trait::async_trait;
use sqlx::Row;
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

        sqlx::query("DELETE FROM state").execute(&mut *tx).await?;
        sqlx::query("DELETE FROM exes").execute(&mut *tx).await?;
        sqlx::query("DELETE FROM maps").execute(&mut *tx).await?;
        sqlx::query("DELETE FROM exe_maps")
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM markovs").execute(&mut *tx).await?;

        let meta = &snapshot.meta;
        let created_at = meta
            .created_at
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_secs().to_string());

        sqlx::query(
            "INSERT INTO state (id, schema_version, app_version, created_at, model_time, last_accounting_time) \
             VALUES (1, ?, ?, ?, ?, ?)",
        )
        .bind(meta.schema_version as i64)
        .bind(meta.app_version.as_deref())
        .bind(created_at.as_deref())
        .bind(snapshot.state.model_time as i64)
        .bind(snapshot.state.last_accounting_time as i64)
        .execute(&mut *tx)
        .await?;

        for exe in &snapshot.state.exes {
            sqlx::query(
                "INSERT INTO exes (path, total_running_time, last_seen_time) VALUES (?, ?, ?)",
            )
            .bind(exe.path.to_string_lossy().to_string())
            .bind(exe.total_running_time as i64)
            .bind(exe.last_seen_time.map(|v| v as i64))
            .execute(&mut *tx)
            .await?;
        }

        for map in &snapshot.state.maps {
            sqlx::query("INSERT INTO maps (path, offset, length, update_time) VALUES (?, ?, ?, ?)")
                .bind(map.path.to_string_lossy().to_string())
                .bind(map.offset as i64)
                .bind(map.length as i64)
                .bind(map.update_time as i64)
                .execute(&mut *tx)
                .await?;
        }

        for map in &snapshot.state.exe_maps {
            sqlx::query(
                "INSERT INTO exe_maps (exe_path, map_path, map_offset, map_length, prob) \
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(map.exe_path.to_string_lossy().to_string())
            .bind(map.map_key.path.to_string_lossy().to_string())
            .bind(map.map_key.offset as i64)
            .bind(map.map_key.length as i64)
            .bind(map.prob as f64)
            .execute(&mut *tx)
            .await?;
        }

        for markov in &snapshot.state.markov_edges {
            let ttl: Vec<u8> = rkyv::to_bytes::<_, 256>(&markov.time_to_leave)
                .map_err(|err| Error::RkyvSerialize(err.to_string()))?
                .into();
            let tp: Vec<u8> = rkyv::to_bytes::<_, 2048>(&markov.transition_prob)
                .map_err(|err| Error::RkyvSerialize(err.to_string()))?
                .into();
            sqlx::query(
                "INSERT INTO markovs (exe_a, exe_b, time_to_leave, transition_prob, both_running_time) \
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(markov.exe_a.to_string_lossy().to_string())
            .bind(markov.exe_b.to_string_lossy().to_string())
            .bind(ttl)
            .bind(tp)
            .bind(markov.both_running_time as i64)
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

        let row = sqlx::query(
            "SELECT schema_version, app_version, created_at, model_time, last_accounting_time \
             FROM state WHERE id = 1",
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let schema_version: i64 = row.try_get("schema_version")?;
            let app_version: Option<String> = row.try_get("app_version")?;
            let created_at: Option<String> = row.try_get("created_at")?;
            let model_time: i64 = row.try_get("model_time")?;
            let last_accounting_time: i64 = row.try_get("last_accounting_time")?;

            meta.schema_version = schema_version as u32;
            meta.app_version = app_version;
            meta.created_at = created_at
                .and_then(|s| s.parse::<u64>().ok())
                .map(|secs| SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs));
            state.model_time = model_time as u64;
            state.last_accounting_time = last_accounting_time as u64;
        }

        let rows = sqlx::query("SELECT path, total_running_time, last_seen_time FROM exes")
            .fetch_all(&self.pool)
            .await?;
        for row in rows {
            let path: String = row.try_get("path")?;
            let total_running_time: i64 = row.try_get("total_running_time")?;
            let last_seen_time: Option<i64> = row.try_get("last_seen_time")?;
            state.exes.push(ExeRecord {
                path: PathBuf::from(path),
                total_running_time: total_running_time as u64,
                last_seen_time: last_seen_time.map(|v| v as u64),
            });
        }

        let rows = sqlx::query("SELECT path, offset, length, update_time FROM maps")
            .fetch_all(&self.pool)
            .await?;
        for row in rows {
            let path: String = row.try_get("path")?;
            let offset: i64 = row.try_get("offset")?;
            let length: i64 = row.try_get("length")?;
            let update_time: i64 = row.try_get("update_time")?;
            state.maps.push(MapRecord {
                path: PathBuf::from(path),
                offset: offset as u64,
                length: length as u64,
                update_time: update_time as u64,
            });
        }

        let rows =
            sqlx::query("SELECT exe_path, map_path, map_offset, map_length, prob FROM exe_maps")
                .fetch_all(&self.pool)
                .await?;
        for row in rows {
            let exe_path: String = row.try_get("exe_path")?;
            let map_path: String = row.try_get("map_path")?;
            let map_offset: i64 = row.try_get("map_offset")?;
            let map_length: i64 = row.try_get("map_length")?;
            let prob: f64 = row.try_get("prob")?;
            state.exe_maps.push(ExeMapRecord {
                exe_path: PathBuf::from(exe_path),
                map_key: crate::domain::MapKey::new(map_path, map_offset as u64, map_length as u64),
                prob: prob as f32,
            });
        }

        let rows = sqlx::query(
            "SELECT exe_a, exe_b, time_to_leave, transition_prob, both_running_time FROM markovs",
        )
        .fetch_all(&self.pool)
        .await?;
        for row in rows {
            let exe_a: String = row.try_get("exe_a")?;
            let exe_b: String = row.try_get("exe_b")?;
            let ttl: Vec<u8> = row.try_get("time_to_leave")?;
            let tp: Vec<u8> = row.try_get("transition_prob")?;
            let both: i64 = row.try_get("both_running_time")?;
            let time_to_leave: [f32; 4] =
                rkyv::from_bytes(&ttl).map_err(|err| Error::RkyvDeserialize(err.to_string()))?;
            let transition_prob: [[f32; 4]; 4] =
                rkyv::from_bytes(&tp).map_err(|err| Error::RkyvDeserialize(err.to_string()))?;
            state.markov_edges.push(MarkovRecord {
                exe_a: PathBuf::from(exe_a),
                exe_b: PathBuf::from(exe_b),
                time_to_leave,
                transition_prob,
                both_running_time: both as u64,
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
