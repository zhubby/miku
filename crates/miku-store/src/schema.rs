use sea_orm::{ConnectionTrait, DatabaseConnection, Statement};

use crate::util::to_storage_error;

pub(crate) async fn ensure_cluster_schema(database: &DatabaseConnection) -> miku_core::Result<()> {
    let mut columns = table_columns(database, "clusters").await?;

    if columns.iter().any(|column| column == "context")
        && !columns.iter().any(|column| column == "kube_context")
    {
        execute_sql(
            database,
            "alter table clusters rename column context to kube_context",
        )
        .await?;
        columns = table_columns(database, "clusters").await?;
    }

    for (name, definition) in [
        ("kube_context", "text not null default ''"),
        ("kubeconfig_path", "text not null default ''"),
        ("config", "text not null default ''"),
        ("default_namespace", "text"),
        ("last_used_at", "integer"),
        ("created_at", "integer not null default 0"),
        ("updated_at", "integer not null default 0"),
    ] {
        if !columns.iter().any(|column| column == name) {
            execute_sql(
                database,
                &format!("alter table clusters add column {name} {definition}"),
            )
            .await?;
        }
    }

    execute_sql(
        database,
        "create unique index if not exists idx_clusters_kubeconfig_context \
         on clusters(kubeconfig_path, kube_context)",
    )
    .await?;
    execute_sql(
        database,
        "create index if not exists idx_clusters_last_used_at on clusters(last_used_at)",
    )
    .await
}

pub(crate) async fn table_columns(
    database: &DatabaseConnection,
    table: &str,
) -> miku_core::Result<Vec<String>> {
    let rows = database
        .query_all(Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            format!("pragma table_info({table})"),
        ))
        .await
        .map_err(to_storage_error)?;

    rows.into_iter()
        .map(|row| row.try_get_by_index::<String>(1).map_err(to_storage_error))
        .collect()
}

async fn execute_sql(database: &DatabaseConnection, sql: &str) -> miku_core::Result<()> {
    database
        .execute(Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            sql.to_owned(),
        ))
        .await
        .map_err(to_storage_error)?;
    Ok(())
}
