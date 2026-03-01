use sea_orm_migration::prelude::*;

use super::m20230901_000007_create_replicas::Replicas;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20230901_000012_add_replicas_store_idx"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // The existing unique index (blob_id, store_id) cannot be used for
        // store_id-only queries. Add a dedicated index so that
        // `replicas WHERE store_id = ?` (store copy, store verify, etc.)
        // can be satisfied without a full table scan.
        manager
            .create_index(
                Index::create().name("ix_replicas_store_id").table(Replicas::Table).col(Replicas::StoreId).to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.drop_index(Index::drop().name("ix_replicas_store_id").to_owned()).await
    }
}
