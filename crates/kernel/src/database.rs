use crate::Error;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    SqlitePool,
};
use std::{path::Path, str::FromStr};

/// An instance of SQLx migrator.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!();

#[async_trait::async_trait]
pub trait DatabaseWriteExt {
    type Error;

    /// Write the data to the database and returns the number of rows affected.
    async fn write(&self, pool: &SqlitePool) -> Result<u64, Self::Error>;
}

/// Try to create a database connection pool. If the database at the specified
/// path does not exist, it is created.
///
/// # Examples
///
/// ## Create a database at a specified path
///
/// ```
/// # use kernel::database::create_database_pool;
/// # use tempfile::tempdir;
/// # tokio_test::block_on(async {
/// # let dir = tempdir().unwrap();
/// # let path = &dir.path().join("test.db");
/// // assume path is some arbitrary path
/// let pool = create_database_pool(Some(path)).await.unwrap();
/// assert!(path.exists());
/// # })
/// ```
///
/// ## Create an in-memory database
///
/// ```
/// # use kernel::database::create_database_pool;
/// # tokio_test::block_on(async {
/// let pool = create_database_pool::<&str>(None).await.unwrap();
/// # })
/// ```
///
/// and if you want to be verbose:
///
/// ```
/// # use kernel::database::create_database_pool;
/// # tokio_test::block_on(async {
/// let pool = create_database_pool(Some(":memory:")).await.unwrap();
/// # })
/// ```
pub async fn create_database_pool<T>(path: Option<T>) -> Result<SqlitePool, Error>
where
    T: AsRef<Path>,
{
    let path = if let Some(path) = path.as_ref() {
        let path = path.as_ref();
        path.to_str()
            .ok_or_else(|| Error::InvalidPath(path.into()))?
    } else {
        "sqlite::memory:"
    };

    // see https://developer.android.com/topic/performance/sqlite-performance-best-practices for
    // more information on sqlite optimization
    let pool = SqlitePoolOptions::new()
        .connect_with(
            SqliteConnectOptions::from_str(path)?
                .create_if_missing(true)
                .synchronous(SqliteSynchronous::Normal)
                .journal_mode(SqliteJournalMode::Wal),
        )
        .await?;
    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn can_parse_path(x in r"[^\.\x00\/\?%]+") {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join(x);
            tokio_test::block_on(async {
                create_database_pool(Some(&path)).await.unwrap();
            });
            prop_assert!(path.exists());
        }
    }

    #[tokio::test]
    #[should_panic]
    async fn no_null_character_in_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("\0");
        create_database_pool(Some(&path)).await.unwrap();
    }

    #[tokio::test]
    #[should_panic]
    async fn no_trailing_slash() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("");
        create_database_pool(Some(&path)).await.unwrap();
    }

    #[tokio::test]
    #[should_panic]
    async fn no_question_mark_as_prefix_in_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("?abcd");
        create_database_pool(Some(&path)).await.unwrap();
    }

    #[tokio::test]
    #[should_panic]
    async fn no_percent_as_prefix_in_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("%abcd");
        create_database_pool(Some(&path)).await.unwrap();
    }

    #[tokio::test]
    async fn create_in_memory_db() {
        create_database_pool::<&str>(None).await.unwrap();
    }

    #[tokio::test]
    async fn create_in_memory_db_explicit() {
        create_database_pool(Some(":memory:")).await.unwrap();
    }

    #[tokio::test]
    async fn create_db_at_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        create_database_pool(Some(&path)).await.unwrap();
        assert!(path.exists());
    }
}
