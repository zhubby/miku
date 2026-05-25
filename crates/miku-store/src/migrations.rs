use sea_orm::sea_query::{Expr, Index};
use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_query::ColumnDef as MigrationColumnDef;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(CreateInitialTables)]
    }
}

struct CreateInitialTables;

impl MigrationName for CreateInitialTables {
    fn name(&self) -> &str {
        "lib"
    }
}

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
                    .col(
                        MigrationColumnDef::new(Clusters::Config)
                            .text()
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
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(AgentConversations::Table)
                    .if_not_exists()
                    .col(
                        MigrationColumnDef::new(AgentConversations::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        MigrationColumnDef::new(AgentConversations::Title)
                            .string()
                            .not_null(),
                    )
                    .col(
                        MigrationColumnDef::new(AgentConversations::Context)
                            .text()
                            .not_null(),
                    )
                    .col(
                        MigrationColumnDef::new(AgentConversations::CreatedAt)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        MigrationColumnDef::new(AgentConversations::UpdatedAt)
                            .big_integer()
                            .not_null(),
                    )
                    .col(MigrationColumnDef::new(AgentConversations::LastMessageAt).big_integer())
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(AgentMessages::Table)
                    .if_not_exists()
                    .col(
                        MigrationColumnDef::new(AgentMessages::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        MigrationColumnDef::new(AgentMessages::ConversationId)
                            .string()
                            .not_null(),
                    )
                    .col(
                        MigrationColumnDef::new(AgentMessages::Role)
                            .string()
                            .not_null(),
                    )
                    .col(
                        MigrationColumnDef::new(AgentMessages::Content)
                            .text()
                            .not_null(),
                    )
                    .col(
                        MigrationColumnDef::new(AgentMessages::CreatedAt)
                            .big_integer()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_agent_conversations_last_message_at")
                    .table(AgentConversations::Table)
                    .col(AgentConversations::LastMessageAt)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_agent_messages_conversation_created_at")
                    .table(AgentMessages::Table)
                    .col(AgentMessages::ConversationId)
                    .col(AgentMessages::CreatedAt)
                    .if_not_exists()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_agent_messages_conversation_created_at")
                    .table(AgentMessages::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                Index::drop()
                    .name("idx_agent_conversations_last_message_at")
                    .table(AgentConversations::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(Table::drop().table(AgentMessages::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(AgentConversations::Table).to_owned())
            .await?;
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
    Config,
    DefaultNamespace,
    LastUsedAt,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum AgentConversations {
    Table,
    Id,
    Title,
    Context,
    CreatedAt,
    UpdatedAt,
    LastMessageAt,
}

#[derive(DeriveIden)]
enum AgentMessages {
    Table,
    Id,
    ConversationId,
    Role,
    Content,
    CreatedAt,
}
