use std::fs;

use sea_orm::{Database, DatabaseConnection};
use sea_orm_migration::prelude::MigratorTrait;

use crate::migrations::Migrator;
use crate::paths::StorePaths;
use crate::schema::ensure_cluster_schema;
use crate::util::{sqlite_url, to_storage_error};

#[derive(Clone)]
pub struct SqliteStore {
    pub(crate) paths: StorePaths,
    pub(crate) database: DatabaseConnection,
}

impl SqliteStore {
    #[tracing::instrument(name = "sqlite_store.initialize", skip_all, fields(database = %paths.database_path().display()))]
    pub async fn initialize(paths: StorePaths) -> miku_core::Result<Self> {
        fs::create_dir_all(paths.root()).map_err(to_storage_error)?;
        fs::create_dir_all(paths.cache_dir()).map_err(to_storage_error)?;
        fs::create_dir_all(paths.logs_dir()).map_err(to_storage_error)?;
        tracing::debug!(root = %paths.root().display(), "ensured store directories");

        let database = Database::connect(sqlite_url(&paths.database_path()))
            .await
            .map_err(to_storage_error)?;
        Migrator::up(&database, None)
            .await
            .map_err(to_storage_error)?;
        ensure_cluster_schema(&database).await?;
        tracing::info!("sqlite store initialized");

        Ok(Self { paths, database })
    }

    pub fn paths(&self) -> &StorePaths {
        &self.paths
    }

    #[cfg(test)]
    pub(crate) fn database(&self) -> &DatabaseConnection {
        &self.database
    }
}
