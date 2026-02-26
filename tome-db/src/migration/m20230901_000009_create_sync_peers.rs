use sea_orm_migration::prelude::*;

use super::m20230901_000001_create_repositories::Repositories;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20230901_000009_create_sync_peers"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(SyncPeers::Table)
                    .col(ColumnDef::new(SyncPeers::Id).big_integer().not_null().primary_key())
                    .col(ColumnDef::new(SyncPeers::Name).string().not_null())
                    .col(ColumnDef::new(SyncPeers::Url).string().not_null())
                    .col(ColumnDef::new(SyncPeers::RepositoryId).big_integer().not_null())
                    .col(ColumnDef::new(SyncPeers::LastSyncedAt).timestamp_with_time_zone().null())
                    .col(ColumnDef::new(SyncPeers::LastSnapshotId).big_integer().null())
                    .col(ColumnDef::new(SyncPeers::Config).json().not_null().default("{}"))
                    .col(
                        ColumnDef::new(SyncPeers::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(SyncPeers::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(SyncPeers::Table, SyncPeers::RepositoryId)
                            .to(Repositories::Table, Repositories::Id),
                    )
                    .index(
                        Index::create()
                            .name("uq_sync_peers_name_repo")
                            .col(SyncPeers::Name)
                            .col(SyncPeers::RepositoryId)
                            .unique(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.drop_table(Table::drop().table(SyncPeers::Table).to_owned()).await
    }
}

#[derive(Iden)]
pub enum SyncPeers {
    Table,
    Id,
    Name,
    Url,
    RepositoryId,
    LastSyncedAt,
    LastSnapshotId,
    Config,
    CreatedAt,
    UpdatedAt,
}
