use sea_orm_migration::prelude::*;

use super::{m20230901_000002_create_blobs::Blobs, m20230901_000003_create_snapshots::Snapshots};

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20230901_000004_create_entries"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Entries::Table)
                    .col(ColumnDef::new(Entries::Id).big_integer().not_null().primary_key())
                    .col(ColumnDef::new(Entries::SnapshotId).big_integer().not_null())
                    .col(ColumnDef::new(Entries::Path).string().not_null())
                    .col(ColumnDef::new(Entries::Status).small_integer().not_null())
                    .col(ColumnDef::new(Entries::BlobId).big_integer().null())
                    .col(ColumnDef::new(Entries::Mode).integer().null())
                    .col(ColumnDef::new(Entries::Mtime).timestamp_with_time_zone().null())
                    .col(
                        ColumnDef::new(Entries::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(Entries::Table, Entries::SnapshotId)
                            .to(Snapshots::Table, Snapshots::Id),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(Entries::Table, Entries::BlobId)
                            .to(Blobs::Table, Blobs::Id),
                    )
                    .index(
                        Index::create()
                            .name("uq_entries_snapshot_path")
                            .col(Entries::SnapshotId)
                            .col(Entries::Path)
                            .unique(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("ix_entries_blob")
                    .table(Entries::Table)
                    .col(Entries::BlobId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("ix_entries_snapshot_status")
                    .table(Entries::Table)
                    .col(Entries::SnapshotId)
                    .col(Entries::Status)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.drop_table(Table::drop().table(Entries::Table).to_owned()).await
    }
}

#[derive(Iden)]
pub enum Entries {
    Table,
    Id,
    SnapshotId,
    Path,
    Status,
    BlobId,
    Mode,
    Mtime,
    CreatedAt,
}
