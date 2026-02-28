use sea_orm_migration::prelude::*;

use super::m20230901_000003_create_snapshots::Snapshots;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20230901_000011_add_snapshot_source"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Snapshots::Table)
                    .add_column(ColumnDef::new(SnapshotSource::SourceMachineId).small_integer().null())
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Snapshots::Table)
                    .add_column(ColumnDef::new(SnapshotSource::SourceSnapshotId).big_integer().null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(Table::alter().table(Snapshots::Table).drop_column(SnapshotSource::SourceMachineId).to_owned())
            .await?;

        manager
            .alter_table(
                Table::alter().table(Snapshots::Table).drop_column(SnapshotSource::SourceSnapshotId).to_owned(),
            )
            .await
    }
}

#[derive(Iden)]
enum SnapshotSource {
    SourceMachineId,
    SourceSnapshotId,
}
