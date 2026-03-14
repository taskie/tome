use sea_orm::DatabaseConnection;
use sea_orm_migration::MigratorTrait;

mod m20230901_000001_create_repositories;
mod m20230901_000002_create_blobs;
pub(crate) mod m20230901_000003_create_snapshots;
mod m20230901_000004_create_entries;
mod m20230901_000005_create_entry_cache;
mod m20230901_000006_create_stores;
mod m20230901_000007_create_replicas;
mod m20230901_000008_create_tags;
mod m20230901_000009_create_sync_peers;
mod m20230901_000010_create_machines;
mod m20230901_000011_add_snapshot_source;
mod m20230901_000012_add_replicas_store_idx;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn sea_orm_migration::MigrationTrait>> {
        vec![
            Box::new(m20230901_000001_create_repositories::Migration),
            Box::new(m20230901_000002_create_blobs::Migration),
            Box::new(m20230901_000003_create_snapshots::Migration),
            Box::new(m20230901_000004_create_entries::Migration),
            Box::new(m20230901_000005_create_entry_cache::Migration),
            Box::new(m20230901_000006_create_stores::Migration),
            Box::new(m20230901_000007_create_replicas::Migration),
            Box::new(m20230901_000008_create_tags::Migration),
            Box::new(m20230901_000009_create_sync_peers::Migration),
            Box::new(m20230901_000010_create_machines::Migration),
            Box::new(m20230901_000011_add_snapshot_source::Migration),
            Box::new(m20230901_000012_add_replicas_store_idx::Migration),
        ]
    }
}

pub async fn run_migrations(db: &DatabaseConnection) -> anyhow::Result<()> {
    Migrator::up(db, None).await?;
    Ok(())
}
