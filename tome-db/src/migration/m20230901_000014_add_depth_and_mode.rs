use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20230901_000014_add_depth_and_mode"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // 1. Add depth column to entries
        db.execute_unprepared("ALTER TABLE entries ADD COLUMN depth SMALLINT NOT NULL DEFAULT 0").await?;

        // 2. Add depth and mode columns to entry_cache
        db.execute_unprepared("ALTER TABLE entry_cache ADD COLUMN depth SMALLINT NOT NULL DEFAULT 0").await?;
        db.execute_unprepared("ALTER TABLE entry_cache ADD COLUMN mode INTEGER NULL").await?;

        // 3. Backfill depth from path (count '/' occurrences)
        db.execute_unprepared("UPDATE entries SET depth = LENGTH(path) - LENGTH(REPLACE(path, '/', ''))").await?;
        db.execute_unprepared("UPDATE entry_cache SET depth = LENGTH(path) - LENGTH(REPLACE(path, '/', ''))").await?;

        // 4. Create indexes for directory listing queries
        db.execute_unprepared("CREATE INDEX idx_entries_snapshot_depth_path ON entries (snapshot_id, depth, path)")
            .await?;
        db.execute_unprepared(
            "CREATE INDEX idx_entry_cache_repo_depth_path ON entry_cache (repository_id, depth, path)",
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        db.execute_unprepared("DROP INDEX IF EXISTS idx_entry_cache_repo_depth_path").await?;
        db.execute_unprepared("DROP INDEX IF EXISTS idx_entries_snapshot_depth_path").await?;

        // SQLite does not support DROP COLUMN before 3.35.0.
        // For SQLite, recreate tables without the columns. For PostgreSQL, use ALTER TABLE DROP COLUMN.
        // Using a simple approach that works on both:
        db.execute_unprepared("ALTER TABLE entry_cache DROP COLUMN mode").await.ok();
        db.execute_unprepared("ALTER TABLE entry_cache DROP COLUMN depth").await.ok();
        db.execute_unprepared("ALTER TABLE entries DROP COLUMN depth").await.ok();

        Ok(())
    }
}
