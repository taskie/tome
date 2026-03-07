use sea_orm_migration::prelude::*;

use super::m20230901_000002_create_blobs::Blobs;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20230901_000008_create_tags"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let mut table = Table::create()
            .table(Tags::Table)
            .col(ColumnDef::new(Tags::Id).big_integer().not_null().primary_key())
            .col(ColumnDef::new(Tags::BlobId).big_integer().not_null())
            .col(ColumnDef::new(Tags::Key).string().not_null())
            .col(ColumnDef::new(Tags::Value).string().null())
            .col(
                ColumnDef::new(Tags::CreatedAt)
                    .timestamp_with_time_zone()
                    .not_null()
                    .default(Expr::current_timestamp()),
            )
            .index(Index::create().name("uq_tags_blob_key").col(Tags::BlobId).col(Tags::Key).unique())
            .to_owned();

        if !crate::dsql::is_dsql() {
            table.foreign_key(ForeignKey::create().from(Tags::Table, Tags::BlobId).to(Blobs::Table, Blobs::Id));
        }

        manager.create_table(table).await?;

        manager
            .create_index(
                Index::create().name("ix_tags_key_value").table(Tags::Table).col(Tags::Key).col(Tags::Value).to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.drop_table(Table::drop().table(Tags::Table).to_owned()).await
    }
}

#[derive(Iden)]
pub enum Tags {
    Table,
    Id,
    BlobId,
    Key,
    Value,
    CreatedAt,
}
