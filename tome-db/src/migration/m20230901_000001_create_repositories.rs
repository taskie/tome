use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20230901_000001_create_repositories"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Repositories::Table)
                    .col(ColumnDef::new(Repositories::Id).big_integer().not_null().primary_key())
                    .col(ColumnDef::new(Repositories::Name).string().not_null().unique_key())
                    .col(ColumnDef::new(Repositories::Description).string().not_null().default(""))
                    .col(ColumnDef::new(Repositories::Config).json().not_null().default("{}"))
                    .col(
                        ColumnDef::new(Repositories::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(Repositories::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.drop_table(Table::drop().table(Repositories::Table).to_owned()).await
    }
}

#[derive(Iden)]
pub enum Repositories {
    Table,
    Id,
    Name,
    Description,
    Config,
    CreatedAt,
    UpdatedAt,
}
