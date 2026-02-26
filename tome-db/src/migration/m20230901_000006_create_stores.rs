use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20230901_000006_create_stores"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Stores::Table)
                    .col(ColumnDef::new(Stores::Id).big_integer().not_null().primary_key())
                    .col(ColumnDef::new(Stores::Name).string().not_null().unique_key())
                    .col(ColumnDef::new(Stores::Url).string().not_null())
                    .col(ColumnDef::new(Stores::Config).json().not_null().default("{}"))
                    .col(
                        ColumnDef::new(Stores::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(Stores::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.drop_table(Table::drop().table(Stores::Table).to_owned()).await
    }
}

#[derive(Iden)]
pub enum Stores {
    Table,
    Id,
    Name,
    Url,
    Config,
    CreatedAt,
    UpdatedAt,
}
