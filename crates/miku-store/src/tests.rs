use std::fs;

use miku_api::{ClusterRegistry, CreateClusterRequest, LocalPreferenceStore};
use sea_orm::{ColumnTrait, ConnectionTrait, Database, EntityTrait, QueryFilter, Set, Statement};

use crate::clusters;
use crate::paths::StorePaths;
use crate::schema::table_columns;
use crate::store::SqliteStore;
use crate::util::{sqlite_url, unix_timestamp};

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
        config: Set("apiVersion: v1".to_owned()),
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
        config: Set("apiVersion: v1".to_owned()),
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
        config: Set("apiVersion: v1".to_owned()),
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
async fn create_cluster_stores_config_but_list_only_returns_summary() {
    let temp = tempfile::tempdir().unwrap();
    let store = SqliteStore::initialize(StorePaths::from_root(temp.path().join(".miku")))
        .await
        .unwrap();

    let summary = store
        .create_cluster(CreateClusterRequest {
            context: "kind-miku".to_owned(),
            config: "apiVersion: v1\nclusters: []".to_owned(),
        })
        .await
        .unwrap();

    assert_eq!(summary.context, "kind-miku");
    let clusters = store.list_clusters().await.unwrap();
    assert_eq!(clusters, vec![summary]);

    let stored = clusters::Entity::find_by_id("kind-miku".to_owned())
        .one(store.database())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.config, "apiVersion: v1\nclusters: []");
}

#[tokio::test]
async fn create_cluster_rejects_empty_context_or_config() {
    let temp = tempfile::tempdir().unwrap();
    let store = SqliteStore::initialize(StorePaths::from_root(temp.path().join(".miku")))
        .await
        .unwrap();

    let missing_context = store
        .create_cluster(CreateClusterRequest {
            context: " ".to_owned(),
            config: "apiVersion: v1".to_owned(),
        })
        .await;
    let missing_config = store
        .create_cluster(CreateClusterRequest {
            context: "kind-miku".to_owned(),
            config: " ".to_owned(),
        })
        .await;

    assert!(missing_context.is_err());
    assert!(missing_config.is_err());
}

#[tokio::test]
async fn create_cluster_rejects_duplicate_context() {
    let temp = tempfile::tempdir().unwrap();
    let store = SqliteStore::initialize(StorePaths::from_root(temp.path().join(".miku")))
        .await
        .unwrap();
    let request = CreateClusterRequest {
        context: "kind-miku".to_owned(),
        config: "apiVersion: v1".to_owned(),
    };

    store.create_cluster(request.clone()).await.unwrap();
    let duplicate = store.create_cluster(request).await;

    assert!(duplicate.is_err());
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

    store
        .create_cluster(CreateClusterRequest {
            context: "kind-next".to_owned(),
            config: "apiVersion: v1".to_owned(),
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn migrator_accepts_pre_split_lib_migration_version() {
    let temp = tempfile::tempdir().unwrap();
    let paths = StorePaths::from_root(temp.path().join(".miku"));
    fs::create_dir_all(paths.root()).unwrap();

    let database = Database::connect(sqlite_url(&paths.database_path()))
        .await
        .unwrap();
    database
        .execute(Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "create table seaql_migrations (
                version varchar not null primary key,
                applied_at bigint not null
            )",
        ))
        .await
        .unwrap();
    database
        .execute(Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "insert into seaql_migrations (version, applied_at) values ('lib', 0)",
        ))
        .await
        .unwrap();
    database
        .execute(Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "create table preferences (
                key text primary key,
                value text not null,
                updated_at integer not null default 0
            )",
        ))
        .await
        .unwrap();
    database
        .execute(Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "create table clusters (
                id text primary key,
                name text not null,
                kube_context text not null,
                kubeconfig_path text not null default '',
                config text not null default '',
                default_namespace text,
                last_used_at integer,
                created_at integer not null default 0,
                updated_at integer not null default 0
            )",
        ))
        .await
        .unwrap();
    drop(database);

    let store = SqliteStore::initialize(paths).await.unwrap();
    store
        .set_preference("ui.theme", serde_json::json!("dark"))
        .await
        .unwrap();
}

#[tokio::test]
async fn migrator_creates_config_column() {
    let temp = tempfile::tempdir().unwrap();
    let store = SqliteStore::initialize(StorePaths::from_root(temp.path().join(".miku")))
        .await
        .unwrap();

    let columns = table_columns(store.database(), "clusters").await.unwrap();

    assert!(columns.iter().any(|column| column == "config"));
}
