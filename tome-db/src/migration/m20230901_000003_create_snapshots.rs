use sea_orm_migration::prelude::*;

use super::m20230901_000001_create_repositories::Repositories;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20230901_000003_create_snapshots"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Snapshots::Table)
                    .col(ColumnDef::new(Snapshots::Id).big_integer().not_null().primary_key())
                    .col(ColumnDef::new(Snapshots::RepositoryId).big_integer().not_null())
                    .col(ColumnDef::new(Snapshots::ParentId).big_integer().null())
                    .col(ColumnDef::new(Snapshots::Message).string().not_null().default(""))
                    .col(ColumnDef::new(Snapshots::Metadata).json().not_null().default("{}"))
                    .col(
                        ColumnDef::new(Snapshots::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(Snapshots::Table, Snapshots::RepositoryId)
                            .to(Repositories::Table, Repositories::Id),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("ix_snapshots_repo_created")
                    .table(Snapshots::Table)
                    .col(Snapshots::RepositoryId)
                    .col(Snapshots::CreatedAt)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.drop_table(Table::drop().table(Snapshots::Table).to_owned()).await
    }
}

#[derive(Iden)]
pub enum Snapshots {
    Table,
    Id,
    RepositoryId,
    ParentId,
    Message,
    Metadata,
    CreatedAt,
}
