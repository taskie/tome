use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20230901_000013_blobs_to_objects"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // SQLite does not support ALTER COLUMN (to drop NOT NULL) or update FK
        // targets after RENAME COLUMN. We therefore recreate affected tables
        // with correct FK references to the new `objects` table.
        db.execute_unprepared("PRAGMA foreign_keys = OFF").await?;

        // 1. Create `objects` from `blobs`.
        db.execute_unprepared(
            "CREATE TABLE objects (
                id              BIGINT NOT NULL PRIMARY KEY,
                digest          BLOB NOT NULL UNIQUE,
                size            BIGINT NOT NULL,
                fast_digest     BIGINT NOT NULL,
                created_at      TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
            )",
        )
        .await?;
        db.execute_unprepared(
            "INSERT INTO objects (id, digest, size, fast_digest, created_at)
             SELECT id, digest, size, fast_digest, created_at FROM blobs",
        )
        .await?;
        db.execute_unprepared("DROP TABLE blobs").await?;

        // 2. Recreate `entries` with object_id FK -> objects.
        db.execute_unprepared(
            "CREATE TABLE entries_new (
                id              BIGINT NOT NULL PRIMARY KEY,
                snapshot_id     BIGINT NOT NULL REFERENCES snapshots(id),
                path            TEXT NOT NULL,
                status          SMALLINT NOT NULL,
                object_id       BIGINT REFERENCES objects(id),
                mode            INTEGER,
                mtime           TIMESTAMP WITH TIME ZONE,
                created_at      TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
            )",
        )
        .await?;
        db.execute_unprepared(
            "INSERT INTO entries_new SELECT id, snapshot_id, path, status, blob_id, mode, mtime, created_at FROM entries",
        )
        .await?;
        db.execute_unprepared("DROP TABLE entries").await?;
        db.execute_unprepared("ALTER TABLE entries_new RENAME TO entries").await?;
        db.execute_unprepared("CREATE UNIQUE INDEX uq_entries_snapshot_path ON entries(snapshot_id, path)").await?;
        db.execute_unprepared("CREATE INDEX ix_entries_object ON entries(object_id)").await?;
        db.execute_unprepared("CREATE INDEX ix_entries_snapshot_status ON entries(snapshot_id, status)").await?;

        // 3. Recreate `entry_cache` with object_id FK -> objects.
        db.execute_unprepared(
            "CREATE TABLE entry_cache_new (
                repository_id   BIGINT NOT NULL REFERENCES repositories(id),
                path            TEXT NOT NULL,
                snapshot_id     BIGINT NOT NULL REFERENCES snapshots(id),
                entry_id        BIGINT NOT NULL REFERENCES entries(id),
                status          SMALLINT NOT NULL,
                object_id       BIGINT REFERENCES objects(id),
                mtime           TIMESTAMP WITH TIME ZONE,
                digest          BLOB,
                size            BIGINT,
                fast_digest     BIGINT,
                updated_at      TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY (repository_id, path)
            )",
        )
        .await?;
        db.execute_unprepared(
            "INSERT INTO entry_cache_new SELECT repository_id, path, snapshot_id, entry_id, status, blob_id, mtime, digest, size, fast_digest, updated_at FROM entry_cache",
        )
        .await?;
        db.execute_unprepared("DROP TABLE entry_cache").await?;
        db.execute_unprepared("ALTER TABLE entry_cache_new RENAME TO entry_cache").await?;

        // 4. Recreate `replicas` with object_id FK -> objects.
        db.execute_unprepared(
            "CREATE TABLE replicas_new (
                id              BIGINT NOT NULL PRIMARY KEY,
                object_id       BIGINT NOT NULL REFERENCES objects(id),
                store_id        BIGINT NOT NULL REFERENCES stores(id),
                path            TEXT NOT NULL,
                encrypted       BOOLEAN NOT NULL DEFAULT 0,
                verified_at     TIMESTAMP WITH TIME ZONE,
                created_at      TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(object_id, store_id)
            )",
        )
        .await?;
        db.execute_unprepared(
            "INSERT INTO replicas_new SELECT id, blob_id, store_id, path, encrypted, verified_at, created_at FROM replicas",
        )
        .await?;
        db.execute_unprepared("DROP TABLE replicas").await?;
        db.execute_unprepared("ALTER TABLE replicas_new RENAME TO replicas").await?;
        db.execute_unprepared("CREATE INDEX ix_replicas_store ON replicas(store_id)").await?;

        // 5. Recreate `tags` with object_id FK -> objects.
        db.execute_unprepared(
            "CREATE TABLE tags_new (
                id              BIGINT NOT NULL PRIMARY KEY,
                object_id       BIGINT NOT NULL REFERENCES objects(id),
                key             TEXT NOT NULL,
                value           TEXT,
                created_at      TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(object_id, key)
            )",
        )
        .await?;
        db.execute_unprepared("INSERT INTO tags_new SELECT id, blob_id, key, value, created_at FROM tags").await?;
        db.execute_unprepared("DROP TABLE tags").await?;
        db.execute_unprepared("ALTER TABLE tags_new RENAME TO tags").await?;
        db.execute_unprepared("CREATE INDEX ix_tags_key_value ON tags(key, value)").await?;

        // 6. Add root_object_id to snapshots.
        db.execute_unprepared("ALTER TABLE snapshots ADD COLUMN root_object_id BIGINT REFERENCES objects(id)").await?;

        db.execute_unprepared("PRAGMA foreign_keys = ON").await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("PRAGMA foreign_keys = OFF").await?;

        db.execute_unprepared("ALTER TABLE snapshots DROP COLUMN root_object_id").await?;

        // Recreate blobs from objects (simplified down migration).
        db.execute_unprepared(
            "CREATE TABLE blobs (
                id BIGINT NOT NULL PRIMARY KEY,
                digest BLOB NOT NULL UNIQUE,
                size BIGINT NOT NULL,
                fast_digest BIGINT NOT NULL,
                created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
            )",
        )
        .await?;
        db.execute_unprepared(
            "INSERT INTO blobs (id, digest, size, fast_digest, created_at)
             SELECT id, digest, COALESCE(size, 0), COALESCE(fast_digest, 0), created_at
             FROM objects",
        )
        .await?;
        db.execute_unprepared("DROP TABLE objects").await?;

        db.execute_unprepared("ALTER TABLE entries RENAME COLUMN object_id TO blob_id").await?;
        db.execute_unprepared("ALTER TABLE entry_cache RENAME COLUMN object_id TO blob_id").await?;
        db.execute_unprepared("ALTER TABLE replicas RENAME COLUMN object_id TO blob_id").await?;
        db.execute_unprepared("ALTER TABLE tags RENAME COLUMN object_id TO blob_id").await?;

        db.execute_unprepared("PRAGMA foreign_keys = ON").await?;
        Ok(())
    }
}
