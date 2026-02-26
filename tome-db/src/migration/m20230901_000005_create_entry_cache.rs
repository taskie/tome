use sea_orm_migration::prelude::*;

use super::{
    m20230901_000002_create_blobs::Blobs, m20230901_000003_create_snapshots::Snapshots,
    m20230901_000004_create_entries::Entries, m20230901_000001_create_repositories::Repositories,
};

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20230901_000005_create_entry_cache"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(EntryCache::Table)
                    .col(ColumnDef::new(EntryCache::RepositoryId).big_integer().not_null())
                    .col(ColumnDef::new(EntryCache::Path).string().not_null())
                    .col(ColumnDef::new(EntryCache::SnapshotId).big_integer().not_null())
                    .col(ColumnDef::new(EntryCache::EntryId).big_integer().not_null())
                    .col(ColumnDef::new(EntryCache::Status).small_integer().not_null())
                    .col(ColumnDef::new(EntryCache::BlobId).big_integer().null())
                    .col(ColumnDef::new(EntryCache::Mtime).timestamp_with_time_zone().null())
                    .col(ColumnDef::new(EntryCache::Digest).blob().null())
                    .col(ColumnDef::new(EntryCache::Size).big_integer().null())
                    .col(ColumnDef::new(EntryCache::FastDigest).big_integer().null())
                    .col(
                        ColumnDef::new(EntryCache::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .primary_key(
                        Index::create()
                            .col(EntryCache::RepositoryId)
                            .col(EntryCache::Path),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(EntryCache::Table, EntryCache::RepositoryId)
                            .to(Repositories::Table, Repositories::Id),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(EntryCache::Table, EntryCache::SnapshotId)
                            .to(Snapshots::Table, Snapshots::Id),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(EntryCache::Table, EntryCache::EntryId)
                            .to(Entries::Table, Entries::Id),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(EntryCache::Table, EntryCache::BlobId)
                            .to(Blobs::Table, Blobs::Id),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.drop_table(Table::drop().table(EntryCache::Table).to_owned()).await
    }
}

#[derive(Iden)]
pub enum EntryCache {
    Table,
    RepositoryId,
    Path,
    SnapshotId,
    EntryId,
    Status,
    BlobId,
    Mtime,
    Digest,
    Size,
    FastDigest,
    UpdatedAt,
}
