use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20230901_000010_create_machines"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Machines::Table)
                    .col(ColumnDef::new(Machines::MachineId).small_integer().not_null().primary_key())
                    .col(ColumnDef::new(Machines::Name).string().not_null())
                    .col(ColumnDef::new(Machines::Description).string().not_null().default(""))
                    .col(ColumnDef::new(Machines::LastSeenAt).timestamp_with_time_zone().null())
                    .col(
                        ColumnDef::new(Machines::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .index(Index::create().name("uq_machines_name").col(Machines::Name).unique())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.drop_table(Table::drop().table(Machines::Table).to_owned()).await
    }
}

#[derive(Iden)]
pub enum Machines {
    Table,
    MachineId,
    Name,
    Description,
    LastSeenAt,
    CreatedAt,
}
