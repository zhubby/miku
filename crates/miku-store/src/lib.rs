use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use directories::BaseDirs;
use miku_api::LocalPreferenceStore;
use miku_core::{MikuError, MikuPaths};
use sea_orm::entity::prelude::*;
use sea_orm::sea_query::{Expr, Index, OnConflict};
use sea_orm::{Database, DatabaseConnection, EntityTrait, Set};
use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_query::ColumnDef as MigrationColumnDef;

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
    database: DatabaseConnection,
}

impl SqliteStore {
    pub async fn initialize(paths: StorePaths) -> miku_core::Result<Self> {
        fs::create_dir_all(paths.root()).map_err(to_storage_error)?;
        fs::create_dir_all(paths.cache_dir()).map_err(to_storage_error)?;
        fs::create_dir_all(paths.logs_dir()).map_err(to_storage_error)?;

        let database = Database::connect(sqlite_url(&paths.database_path()))
            .await
            .map_err(to_storage_error)?;
        Migrator::up(&database, None)
            .await
            .map_err(to_storage_error)?;

        Ok(Self { paths, database })
    }

    pub fn paths(&self) -> &StorePaths {
        &self.paths
    }

    #[cfg(test)]
    fn database(&self) -> &DatabaseConnection {
        &self.database
    }
}

#[async_trait]
impl LocalPreferenceStore for SqliteStore {
    async fn get_preference(&self, key: &str) -> miku_core::Result<Option<serde_json::Value>> {
        let raw = preferences::Entity::find_by_id(key.to_owned())
            .one(&self.database)
            .await
            .map_err(to_storage_error)?
            .map(|preference| preference.value);

        raw.map(|value| serde_json::from_str(&value).map_err(to_storage_error))
            .transpose()
    }

    async fn set_preference(&self, key: &str, value: serde_json::Value) -> miku_core::Result<()> {
        let serialized = serde_json::to_string(&value).map_err(to_storage_error)?;
        preferences::Entity::insert(preferences::ActiveModel {
            key: Set(key.to_owned()),
            value: Set(serialized),
            updated_at: Set(unix_timestamp()),
        })
        .on_conflict(
            OnConflict::column(preferences::Column::Key)
                .update_columns([preferences::Column::Value, preferences::Column::UpdatedAt])
                .to_owned(),
        )
        .exec(&self.database)
        .await
        .map_err(to_storage_error)?;

        Ok(())
    }
}

fn sqlite_url(path: &Path) -> String {
    format!("sqlite://{}?mode=rwc", path.display())
}

fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

fn to_storage_error(error: impl std::error::Error) -> MikuError {
    MikuError::Storage(error.to_string())
}

mod preferences {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "preferences")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub key: String,
        pub value: String,
        pub updated_at: i64,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod clusters {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "clusters")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: String,
        pub name: String,
        pub kube_context: String,
        pub kubeconfig_path: String,
        pub default_namespace: Option<String>,
        pub last_used_at: Option<i64>,
        pub created_at: i64,
        pub updated_at: i64,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(CreateInitialTables)]
    }
}

#[derive(DeriveMigrationName)]
struct CreateInitialTables;

#[async_trait::async_trait]
impl MigrationTrait for CreateInitialTables {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Preferences::Table)
                    .if_not_exists()
                    .col(
                        MigrationColumnDef::new(Preferences::Key)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        MigrationColumnDef::new(Preferences::Value)
                            .text()
                            .not_null(),
                    )
                    .col(
                        MigrationColumnDef::new(Preferences::UpdatedAt)
                            .big_integer()
                            .not_null()
                            .default(Expr::cust("(unixepoch())")),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Clusters::Table)
                    .if_not_exists()
                    .col(
                        MigrationColumnDef::new(Clusters::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(MigrationColumnDef::new(Clusters::Name).string().not_null())
                    .col(
                        MigrationColumnDef::new(Clusters::KubeContext)
                            .string()
                            .not_null(),
                    )
                    .col(
                        MigrationColumnDef::new(Clusters::KubeconfigPath)
                            .string()
                            .not_null()
                            .default(""),
                    )
                    .col(MigrationColumnDef::new(Clusters::DefaultNamespace).string())
                    .col(MigrationColumnDef::new(Clusters::LastUsedAt).big_integer())
                    .col(
                        MigrationColumnDef::new(Clusters::CreatedAt)
                            .big_integer()
                            .not_null()
                            .default(Expr::cust("(unixepoch())")),
                    )
                    .col(
                        MigrationColumnDef::new(Clusters::UpdatedAt)
                            .big_integer()
                            .not_null()
                            .default(Expr::cust("(unixepoch())")),
                    )
                    .index(
                        Index::create()
                            .name("idx_clusters_kubeconfig_context")
                            .col(Clusters::KubeconfigPath)
                            .col(Clusters::KubeContext)
                            .unique(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_clusters_last_used_at")
                    .table(Clusters::Table)
                    .col(Clusters::LastUsedAt)
                    .if_not_exists()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_clusters_last_used_at")
                    .table(Clusters::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(Table::drop().table(Clusters::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Preferences::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Preferences {
    Table,
    Key,
    Value,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Clusters {
    Table,
    Id,
    Name,
    KubeContext,
    KubeconfigPath,
    DefaultNamespace,
    LastUsedAt,
    CreatedAt,
    UpdatedAt,
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::{ColumnTrait, ConnectionTrait, QueryFilter, Statement};

    #[tokio::test]
    async fn initializes_miku_directory_tree_and_database() {
        let temp = tempfile::tempdir().unwrap();
        let paths = StorePaths::from_root(temp.path().join(".miku"));

        let store = SqliteStore::initialize(paths.clone()).await.unwrap();

        assert!(paths.root().exists());
        assert!(paths.database_path().exists());
        assert!(paths.cache_dir().exists());
        assert!(paths.logs_dir().exists());
        assert_eq!(store.paths().database_path(), paths.database_path());
    }

    #[tokio::test]
    async fn preferences_round_trip_as_json() {
        let temp = tempfile::tempdir().unwrap();
        let store = SqliteStore::initialize(StorePaths::from_root(temp.path().join(".miku")))
            .await
            .unwrap();

        store
            .set_preference("ui.theme", serde_json::json!("dark"))
            .await
            .unwrap();

        let value = store.get_preference("ui.theme").await.unwrap();
        assert_eq!(value, Some(serde_json::json!("dark")));
    }

    #[tokio::test]
    async fn migrator_creates_preferences_and_clusters_tables() {
        let temp = tempfile::tempdir().unwrap();
        let store = SqliteStore::initialize(StorePaths::from_root(temp.path().join(".miku")))
            .await
            .unwrap();

        let tables = store
            .database()
            .query_all(Statement::from_string(
                sea_orm::DatabaseBackend::Sqlite,
                "select name from sqlite_master where type = 'table' and name in ('preferences', 'clusters')",
            ))
            .await
            .unwrap();

        assert_eq!(tables.len(), 2);
    }

    #[tokio::test]
    async fn cluster_reference_is_unique_per_kubeconfig_and_context() {
        let temp = tempfile::tempdir().unwrap();
        let store = SqliteStore::initialize(StorePaths::from_root(temp.path().join(".miku")))
            .await
            .unwrap();

        clusters::Entity::insert(clusters::ActiveModel {
            id: Set("local".to_owned()),
            name: Set("Local".to_owned()),
            kube_context: Set("kind-miku".to_owned()),
            kubeconfig_path: Set(String::new()),
            default_namespace: Set(None),
            last_used_at: Set(None),
            created_at: Set(unix_timestamp()),
            updated_at: Set(unix_timestamp()),
        })
        .exec(store.database())
        .await
        .unwrap();

        let duplicate = clusters::Entity::insert(clusters::ActiveModel {
            id: Set("duplicate".to_owned()),
            name: Set("Duplicate".to_owned()),
            kube_context: Set("kind-miku".to_owned()),
            kubeconfig_path: Set(String::new()),
            default_namespace: Set(None),
            last_used_at: Set(None),
            created_at: Set(unix_timestamp()),
            updated_at: Set(unix_timestamp()),
        })
        .exec(store.database())
        .await;

        assert!(duplicate.is_err());
    }

    #[tokio::test]
    async fn empty_kubeconfig_path_represents_default_kubeconfig() {
        let temp = tempfile::tempdir().unwrap();
        let store = SqliteStore::initialize(StorePaths::from_root(temp.path().join(".miku")))
            .await
            .unwrap();

        clusters::Entity::insert(clusters::ActiveModel {
            id: Set("local".to_owned()),
            name: Set("Local".to_owned()),
            kube_context: Set("kind-miku".to_owned()),
            kubeconfig_path: Set(String::new()),
            default_namespace: Set(None),
            last_used_at: Set(None),
            created_at: Set(unix_timestamp()),
            updated_at: Set(unix_timestamp()),
        })
        .exec(store.database())
        .await
        .unwrap();

        let cluster = clusters::Entity::find()
            .filter(clusters::Column::Id.eq("local"))
            .one(store.database())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(cluster.kubeconfig_path, "");
    }

    #[tokio::test]
    async fn migrator_accepts_existing_legacy_clusters_table() {
        let temp = tempfile::tempdir().unwrap();
        let paths = StorePaths::from_root(temp.path().join(".miku"));
        fs::create_dir_all(paths.root()).unwrap();

        let database = Database::connect(sqlite_url(&paths.database_path()))
            .await
            .unwrap();
        database
            .execute(Statement::from_string(
                sea_orm::DatabaseBackend::Sqlite,
                "create table clusters (
                    id text primary key,
                    name text not null,
                    context text not null unique,
                    last_used_at integer
                )",
            ))
            .await
            .unwrap();
        drop(database);

        let store = SqliteStore::initialize(paths).await.unwrap();

        let tables = store
            .database()
            .query_all(Statement::from_string(
                sea_orm::DatabaseBackend::Sqlite,
                "select name from sqlite_master where type = 'table' and name = 'seaql_migrations'",
            ))
            .await
            .unwrap();

        assert_eq!(tables.len(), 1);
    }
}
