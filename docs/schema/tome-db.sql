CREATE TABLE "entries" (
                id              BIGINT NOT NULL PRIMARY KEY,
                snapshot_id     BIGINT NOT NULL REFERENCES snapshots(id),
                path            TEXT NOT NULL,
                status          SMALLINT NOT NULL,
                object_id       BIGINT REFERENCES objects(id),
                mode            INTEGER,
                mtime           TIMESTAMP WITH TIME ZONE,
                created_at      TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
            , depth SMALLINT NOT NULL DEFAULT 0);

CREATE TABLE "entry_cache" (
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
                updated_at      TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP, depth SMALLINT NOT NULL DEFAULT 0, mode INTEGER NULL,
                PRIMARY KEY (repository_id, path)
            );

CREATE TABLE "machines" ( "machine_id" smallint NOT NULL PRIMARY KEY, "name" varchar NOT NULL, "description" varchar NOT NULL DEFAULT '', "last_seen_at" timestamp_with_timezone_text NULL, "created_at" timestamp_with_timezone_text NOT NULL DEFAULT CURRENT_TIMESTAMP, CONSTRAINT "uq_machines_name" UNIQUE ("name") );

CREATE TABLE objects (
                id              BIGINT NOT NULL PRIMARY KEY,
                digest          BLOB NOT NULL UNIQUE,
                size            BIGINT NOT NULL,
                fast_digest     BIGINT NOT NULL,
                created_at      TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
            );

CREATE TABLE "replicas" (
                id              BIGINT NOT NULL PRIMARY KEY,
                object_id       BIGINT NOT NULL REFERENCES objects(id),
                store_id        BIGINT NOT NULL REFERENCES stores(id),
                path            TEXT NOT NULL,
                encrypted       BOOLEAN NOT NULL DEFAULT 0,
                verified_at     TIMESTAMP WITH TIME ZONE,
                created_at      TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(object_id, store_id)
            );

CREATE TABLE "repositories" ( "id" bigint NOT NULL PRIMARY KEY, "name" varchar NOT NULL UNIQUE, "description" varchar NOT NULL DEFAULT '', "config" json_text NOT NULL DEFAULT '{}', "created_at" timestamp_with_timezone_text NOT NULL DEFAULT CURRENT_TIMESTAMP, "updated_at" timestamp_with_timezone_text NOT NULL DEFAULT CURRENT_TIMESTAMP );

CREATE TABLE "snapshots" ( "id" bigint NOT NULL PRIMARY KEY, "repository_id" bigint NOT NULL, "parent_id" bigint NULL, "message" varchar NOT NULL DEFAULT '', "metadata" json_text NOT NULL DEFAULT '{}', "created_at" timestamp_with_timezone_text NOT NULL DEFAULT CURRENT_TIMESTAMP, "source_machine_id" smallint NULL, "source_snapshot_id" bigint NULL, root_object_id BIGINT REFERENCES objects(id), FOREIGN KEY ("repository_id") REFERENCES "repositories" ("id") );

CREATE TABLE "stores" ( "id" bigint NOT NULL PRIMARY KEY, "name" varchar NOT NULL UNIQUE, "url" varchar NOT NULL, "config" json_text NOT NULL DEFAULT '{}', "created_at" timestamp_with_timezone_text NOT NULL DEFAULT CURRENT_TIMESTAMP, "updated_at" timestamp_with_timezone_text NOT NULL DEFAULT CURRENT_TIMESTAMP );

CREATE TABLE "sync_peers" ( "id" bigint NOT NULL PRIMARY KEY, "name" varchar NOT NULL, "url" varchar NOT NULL, "repository_id" bigint NOT NULL, "last_synced_at" timestamp_with_timezone_text NULL, "last_snapshot_id" bigint NULL, "config" json_text NOT NULL DEFAULT '{}', "created_at" timestamp_with_timezone_text NOT NULL DEFAULT CURRENT_TIMESTAMP, "updated_at" timestamp_with_timezone_text NOT NULL DEFAULT CURRENT_TIMESTAMP, CONSTRAINT "uq_sync_peers_name_repo" UNIQUE ("name", "repository_id"), FOREIGN KEY ("repository_id") REFERENCES "repositories" ("id") );

CREATE TABLE "tags" (
                id              BIGINT NOT NULL PRIMARY KEY,
                object_id       BIGINT NOT NULL REFERENCES objects(id),
                key             TEXT NOT NULL,
                value           TEXT,
                created_at      TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(object_id, key)
            );

CREATE INDEX idx_entries_snapshot_depth_path ON entries (snapshot_id, depth, path);

CREATE INDEX idx_entry_cache_repo_depth_path ON entry_cache (repository_id, depth, path);

CREATE INDEX ix_entries_object ON entries(object_id);

CREATE INDEX ix_entries_snapshot_status ON entries(snapshot_id, status);

CREATE INDEX ix_replicas_store ON replicas(store_id);

CREATE INDEX "ix_snapshots_repo_created" ON "snapshots" ("repository_id", "created_at");

CREATE INDEX ix_tags_key_value ON tags(key, value);

CREATE UNIQUE INDEX uq_entries_snapshot_path ON entries(snapshot_id, path);

