#![forbid(unsafe_code)]

use crate::error::Error;
use crate::persistence::{
    ExeMapRecord, ExeRecord, MapRecord, MarkovRecord, SNAPSHOT_SCHEMA_VERSION, SnapshotMeta,
    StateSnapshot, StoresSnapshot,
};
use async_trait::async_trait;
use rusqlite::{Connection, params};
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
}

impl SqliteRepository {
    pub fn new(path: PathBuf) -> Result<Self, Error> {
        Ok(Self { path })
    }

    fn init_schema(conn: &Connection) -> Result<(), Error> {
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS state (
                id INTEGER PRIMARY KEY CHECK(id = 1),
                schema_version INTEGER NOT NULL,
                app_version TEXT,
                created_at TEXT,
                model_time INTEGER NOT NULL,
                last_accounting_time INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS exes (
                path TEXT PRIMARY KEY,
                total_running_time INTEGER NOT NULL,
                last_seen_time INTEGER
            );

            CREATE TABLE IF NOT EXISTS maps (
                path TEXT NOT NULL,
                offset INTEGER NOT NULL,
                length INTEGER NOT NULL,
                update_time INTEGER NOT NULL,
                PRIMARY KEY (path, offset, length)
            );

            CREATE TABLE IF NOT EXISTS exe_maps (
                exe_path TEXT NOT NULL,
                map_path TEXT NOT NULL,
                map_offset INTEGER NOT NULL,
                map_length INTEGER NOT NULL,
                prob REAL NOT NULL,
                PRIMARY KEY (exe_path, map_path, map_offset, map_length)
            );

            CREATE TABLE IF NOT EXISTS markovs (
                exe_a TEXT NOT NULL,
                exe_b TEXT NOT NULL,
                time_to_leave BLOB NOT NULL,
                transition_prob BLOB NOT NULL,
                both_running_time INTEGER NOT NULL,
                PRIMARY KEY (exe_a, exe_b)
            );
            "#,
        )?;
        Ok(())
    }

    fn save_sync(path: &PathBuf, snapshot: &StoresSnapshot) -> Result<(), Error> {
        let mut conn = Connection::open(path)?;
        Self::init_schema(&conn)?;
        let tx = conn.transaction()?;

        tx.execute("DELETE FROM state", [])?;
        tx.execute("DELETE FROM exes", [])?;
        tx.execute("DELETE FROM maps", [])?;
        tx.execute("DELETE FROM exe_maps", [])?;
        tx.execute("DELETE FROM markovs", [])?;

        let meta = &snapshot.meta;
        let created_at = meta
            .created_at
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_secs().to_string());

        tx.execute(
            "INSERT INTO state (id, schema_version, app_version, created_at, model_time, last_accounting_time) VALUES (1, ?, ?, ?, ?, ?)",
            params![
                meta.schema_version as i64,
                meta.app_version.as_deref(),
                created_at.as_deref(),
                snapshot.state.model_time as i64,
                snapshot.state.last_accounting_time as i64,
            ],
        )?;

        for exe in &snapshot.state.exes {
            tx.execute(
                "INSERT INTO exes (path, total_running_time, last_seen_time) VALUES (?, ?, ?)",
                params![
                    exe.path.to_string_lossy(),
                    exe.total_running_time as i64,
                    exe.last_seen_time.map(|v| v as i64),
                ],
            )?;
        }

        for map in &snapshot.state.maps {
            tx.execute(
                "INSERT INTO maps (path, offset, length, update_time) VALUES (?, ?, ?, ?)",
                params![
                    map.path.to_string_lossy(),
                    map.offset as i64,
                    map.length as i64,
                    map.update_time as i64,
                ],
            )?;
        }

        for map in &snapshot.state.exe_maps {
            tx.execute(
                "INSERT INTO exe_maps (exe_path, map_path, map_offset, map_length, prob) VALUES (?, ?, ?, ?, ?)",
                params![
                    map.exe_path.to_string_lossy(),
                    map.map_key.path.to_string_lossy(),
                    map.map_key.offset as i64,
                    map.map_key.length as i64,
                    map.prob,
                ],
            )?;
        }

        for markov in &snapshot.state.markov_edges {
            let ttl = rkyv::to_bytes::<_, 256>(&markov.time_to_leave)
                .map_err(|err| Error::RkyvSerialize(err.to_string()))?;
            let tp = rkyv::to_bytes::<_, 2048>(&markov.transition_prob)
                .map_err(|err| Error::RkyvSerialize(err.to_string()))?;
            tx.execute(
                "INSERT INTO markovs (exe_a, exe_b, time_to_leave, transition_prob, both_running_time) VALUES (?, ?, ?, ?, ?)",
                params![
                    markov.exe_a.to_string_lossy(),
                    markov.exe_b.to_string_lossy(),
                    ttl.as_ref(),
                    tp.as_ref(),
                    markov.both_running_time as i64,
                ],
            )?;
        }

        tx.commit()?;
        debug!("snapshot persisted");
        Ok(())
    }

    fn load_sync(path: &PathBuf) -> Result<StoresSnapshot, Error> {
        let conn = Connection::open(path)?;
        Self::init_schema(&conn)?;

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

        let mut stmt = conn.prepare("SELECT schema_version, app_version, created_at, model_time, last_accounting_time FROM state WHERE id = 1")?;
        if let Ok(row) = stmt.query_row([], |row| {
            let schema_version: i64 = row.get(0)?;
            let app_version: Option<String> = row.get(1)?;
            let created_at: Option<String> = row.get(2)?;
            let model_time: i64 = row.get(3)?;
            let last_accounting_time: i64 = row.get(4)?;
            Ok((
                schema_version,
                app_version,
                created_at,
                model_time,
                last_accounting_time,
            ))
        }) {
            meta.schema_version = row.0 as u32;
            meta.app_version = row.1;
            meta.created_at = row
                .2
                .and_then(|s| s.parse::<u64>().ok())
                .map(|secs| SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs));
            state.model_time = row.3 as u64;
            state.last_accounting_time = row.4 as u64;
        }

        let mut stmt = conn.prepare("SELECT path, total_running_time, last_seen_time FROM exes")?;
        let exes = stmt.query_map([], |row| {
            Ok(ExeRecord {
                path: PathBuf::from(row.get::<_, String>(0)?),
                total_running_time: row.get::<_, i64>(1)? as u64,
                last_seen_time: row.get::<_, Option<i64>>(2)?.map(|v| v as u64),
            })
        })?;
        for exe in exes {
            state.exes.push(exe?);
        }

        let mut stmt = conn.prepare("SELECT path, offset, length, update_time FROM maps")?;
        let maps = stmt.query_map([], |row| {
            Ok(MapRecord {
                path: PathBuf::from(row.get::<_, String>(0)?),
                offset: row.get::<_, i64>(1)? as u64,
                length: row.get::<_, i64>(2)? as u64,
                update_time: row.get::<_, i64>(3)? as u64,
            })
        })?;
        for map in maps {
            state.maps.push(map?);
        }

        let mut stmt =
            conn.prepare("SELECT exe_path, map_path, map_offset, map_length, prob FROM exe_maps")?;
        let exe_maps = stmt.query_map([], |row| {
            let exe_path = PathBuf::from(row.get::<_, String>(0)?);
            let map_path = PathBuf::from(row.get::<_, String>(1)?);
            let offset = row.get::<_, i64>(2)? as u64;
            let length = row.get::<_, i64>(3)? as u64;
            let prob = row.get::<_, f64>(4)? as f32;
            Ok(ExeMapRecord {
                exe_path,
                map_key: crate::domain::MapKey::new(map_path, offset, length),
                prob,
            })
        })?;
        for record in exe_maps {
            state.exe_maps.push(record?);
        }

        let mut stmt = conn.prepare(
            "SELECT exe_a, exe_b, time_to_leave, transition_prob, both_running_time FROM markovs",
        )?;
        let markovs = stmt.query_map([], |row| {
            let exe_a = PathBuf::from(row.get::<_, String>(0)?);
            let exe_b = PathBuf::from(row.get::<_, String>(1)?);
            let ttl: Vec<u8> = row.get(2)?;
            let tp: Vec<u8> = row.get(3)?;
            let both: i64 = row.get(4)?;
            Ok((exe_a, exe_b, ttl, tp, both))
        })?;
        for record in markovs {
            let (exe_a, exe_b, ttl, tp, both) = record?;
            let time_to_leave: [f32; 4] =
                rkyv::from_bytes(&ttl).map_err(|err| Error::RkyvDeserialize(err.to_string()))?;
            let transition_prob: [[f32; 4]; 4] =
                rkyv::from_bytes(&tp).map_err(|err| Error::RkyvDeserialize(err.to_string()))?;
            state.markov_edges.push(MarkovRecord {
                exe_a,
                exe_b,
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
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || Self::load_sync(&path))
            .await
            .map_err(|err| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, err)))?
    }

    async fn save(&self, snapshot: &StoresSnapshot) -> Result<(), Error> {
        let path = self.path.clone();
        let snapshot = snapshot.clone();
        tokio::task::spawn_blocking(move || Self::save_sync(&path, &snapshot))
            .await
            .map_err(|err| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, err)))?
    }
}
