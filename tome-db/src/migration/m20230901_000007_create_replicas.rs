use sea_orm_migration::prelude::*;

use super::{m20230901_000002_create_blobs::Blobs, m20230901_000006_create_stores::Stores};

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20230901_000007_create_replicas"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Replicas::Table)
                    .col(ColumnDef::new(Replicas::Id).big_integer().not_null().primary_key())
                    .col(ColumnDef::new(Replicas::BlobId).big_integer().not_null())
                    .col(ColumnDef::new(Replicas::StoreId).big_integer().not_null())
                    .col(ColumnDef::new(Replicas::Path).string().not_null())
                    .col(ColumnDef::new(Replicas::Encrypted).boolean().not_null().default(false))
                    .col(ColumnDef::new(Replicas::VerifiedAt).timestamp_with_time_zone().null())
                    .col(
                        ColumnDef::new(Replicas::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create().from(Replicas::Table, Replicas::BlobId).to(Blobs::Table, Blobs::Id),
                    )
                    .foreign_key(
                        ForeignKey::create().from(Replicas::Table, Replicas::StoreId).to(Stores::Table, Stores::Id),
                    )
                    .index(
                        Index::create()
                            .name("uq_replicas_blob_store")
                            .col(Replicas::BlobId)
                            .col(Replicas::StoreId)
                            .unique(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.drop_table(Table::drop().table(Replicas::Table).to_owned()).await
    }
}

#[derive(Iden)]
pub enum Replicas {
    Table,
    Id,
    BlobId,
    StoreId,
    Path,
    Encrypted,
    VerifiedAt,
    CreatedAt,
}
