use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20230901_000002_create_blobs"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Blobs::Table)
                    .col(ColumnDef::new(Blobs::Id).big_integer().not_null().primary_key())
                    .col(ColumnDef::new(Blobs::Digest).blob().not_null().unique_key())
                    .col(ColumnDef::new(Blobs::Size).big_integer().not_null())
                    .col(ColumnDef::new(Blobs::FastDigest).big_integer().not_null())
                    .col(
                        ColumnDef::new(Blobs::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.drop_table(Table::drop().table(Blobs::Table).to_owned()).await
    }
}

#[derive(Iden)]
pub enum Blobs {
    Table,
    Id,
    Digest,
    Size,
    FastDigest,
    CreatedAt,
}
