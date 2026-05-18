use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use directories::BaseDirs;
use miku_api::LocalPreferenceStore;
use miku_core::{MikuError, MikuPaths};
use rusqlite::{Connection, OptionalExtension, params};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorePaths {
    inner: MikuPaths,
}

impl StorePaths {
    pub fn default_for_user() -> miku_core::Result<Self> {
        let base_dirs = BaseDirs::new()
            .ok_or_else(|| MikuError::Config("could not resolve the user home directory".into()))?;
        Ok(Self::from_root(base_dirs.home_dir().join(".miku")))
    }

    pub fn from_root(root: impl Into<PathBuf>) -> Self {
        Self {
            inner: MikuPaths::new(root),
        }
    }

    pub fn root(&self) -> &Path {
        self.inner.root_path()
    }

    pub fn database_path(&self) -> PathBuf {
        self.inner.database_path()
    }

    pub fn config_path(&self) -> PathBuf {
        self.inner.config_path()
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.inner.cache_dir()
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.inner.logs_dir()
    }
}

#[derive(Clone)]
pub struct SqliteStore {
    paths: StorePaths,
    connection: Arc<Mutex<Connection>>,
}

impl SqliteStore {
    pub fn initialize(paths: StorePaths) -> miku_core::Result<Self> {
        fs::create_dir_all(paths.root()).map_err(to_storage_error)?;
        fs::create_dir_all(paths.cache_dir()).map_err(to_storage_error)?;
        fs::create_dir_all(paths.logs_dir()).map_err(to_storage_error)?;

        let connection = Connection::open(paths.database_path()).map_err(to_storage_error)?;
        run_migrations(&connection)?;

        Ok(Self {
            paths,
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    pub fn paths(&self) -> &StorePaths {
        &self.paths
    }
}

#[async_trait]
impl LocalPreferenceStore for SqliteStore {
    async fn get_preference(&self, key: &str) -> miku_core::Result<Option<serde_json::Value>> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| MikuError::Storage("sqlite connection lock was poisoned".to_owned()))?;
        let raw = connection
            .query_row(
                "select value from preferences where key = ?1",
                params![key],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(to_storage_error)?;

        raw.map(|value| serde_json::from_str(&value).map_err(to_storage_error))
            .transpose()
    }

    async fn set_preference(&self, key: &str, value: serde_json::Value) -> miku_core::Result<()> {
        let serialized = serde_json::to_string(&value).map_err(to_storage_error)?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| MikuError::Storage("sqlite connection lock was poisoned".to_owned()))?;
        connection
            .execute(
                "insert into preferences (key, value) values (?1, ?2)
                 on conflict(key) do update set value = excluded.value, updated_at = unixepoch()",
                params![key, serialized],
            )
            .map_err(to_storage_error)?;
        Ok(())
    }
}

fn run_migrations(connection: &Connection) -> miku_core::Result<()> {
    connection
        .execute_batch(
            "
            create table if not exists preferences (
                key text primary key,
                value text not null,
                updated_at integer not null default (unixepoch())
            );
            create table if not exists clusters (
                id text primary key,
                name text not null,
                context text not null unique,
                last_used_at integer
            );
            ",
        )
        .map_err(to_storage_error)
}

fn to_storage_error(error: impl std::error::Error) -> MikuError {
    MikuError::Storage(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initializes_miku_directory_tree_and_database() {
        let temp = tempfile::tempdir().unwrap();
        let paths = StorePaths::from_root(temp.path().join(".miku"));

        let store = SqliteStore::initialize(paths.clone()).unwrap();

        assert!(paths.root().exists());
        assert!(paths.database_path().exists());
        assert!(paths.cache_dir().exists());
        assert!(paths.logs_dir().exists());
        assert_eq!(store.paths().database_path(), paths.database_path());
    }

    #[tokio::test]
    async fn preferences_round_trip_as_json() {
        let temp = tempfile::tempdir().unwrap();
        let store =
            SqliteStore::initialize(StorePaths::from_root(temp.path().join(".miku"))).unwrap();

        store
            .set_preference("ui.theme", serde_json::json!("dark"))
            .await
            .unwrap();

        let value = store.get_preference("ui.theme").await.unwrap();
        assert_eq!(value, Some(serde_json::json!("dark")));
    }
}
