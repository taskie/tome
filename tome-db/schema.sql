-- tome-db/schema.sql
-- Canonical PostgreSQL schema for use with psqldef or other declarative migration tools.
-- This file represents the merged result of all SeaORM migrations.
--
-- Usage with psqldef:
--   psqldef -U <user> -h <host> <database> < tome-db/schema.sql

-- 1. repositories
CREATE TABLE repositories (
    id         BIGINT       NOT NULL PRIMARY KEY,
    name       VARCHAR      NOT NULL UNIQUE,
    description VARCHAR     NOT NULL DEFAULT '',
    config     JSON         NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ  NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ  NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- 2. objects (blobs + trees)
CREATE TABLE objects (
    id          BIGINT      NOT NULL PRIMARY KEY,
    object_type SMALLINT    NOT NULL,            -- 0 = blob, 1 = tree
    digest      BYTEA       NOT NULL UNIQUE,     -- content hash (blob) or Merkle tree hash (tree)
    size        BIGINT      NOT NULL,            -- file size (blob) or serialized tree size (tree)
    fast_digest BIGINT      NOT NULL,            -- xxHash64 of content
    created_at  TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- 3. snapshots
CREATE TABLE snapshots (
    id                 BIGINT      NOT NULL PRIMARY KEY,
    repository_id      BIGINT      NOT NULL REFERENCES repositories (id),
    parent_id          BIGINT      NULL,
    message            VARCHAR     NOT NULL DEFAULT '',
    metadata           JSON        NOT NULL DEFAULT '{}',
    created_at         TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    source_machine_id  SMALLINT    NULL,
    source_snapshot_id BIGINT      NULL,
    root_object_id     BIGINT      NULL     REFERENCES objects (id)
);

CREATE INDEX ix_snapshots_repo_created ON snapshots (repository_id, created_at);

-- 4. entries
CREATE TABLE entries (
    id          BIGINT      NOT NULL PRIMARY KEY,
    snapshot_id BIGINT      NOT NULL REFERENCES snapshots (id),
    path        VARCHAR     NOT NULL,
    status      SMALLINT    NOT NULL,
    object_id   BIGINT      NULL     REFERENCES objects (id),
    mode        INTEGER     NULL,
    mtime       TIMESTAMPTZ NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT uq_entries_snapshot_path UNIQUE (snapshot_id, path)
);

CREATE INDEX ix_entries_object          ON entries (object_id);
CREATE INDEX ix_entries_snapshot_status  ON entries (snapshot_id, status);

-- 5. entry_cache
CREATE TABLE entry_cache (
    repository_id BIGINT      NOT NULL REFERENCES repositories (id),
    path          VARCHAR     NOT NULL,
    snapshot_id   BIGINT      NOT NULL REFERENCES snapshots (id),
    entry_id      BIGINT      NOT NULL REFERENCES entries (id),
    status        SMALLINT    NOT NULL,
    object_id     BIGINT      NULL     REFERENCES objects (id),
    mtime         TIMESTAMPTZ NULL,
    digest        BYTEA       NULL,
    size          BIGINT      NULL,
    fast_digest   BIGINT      NULL,
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (repository_id, path)
);

-- 6. stores
CREATE TABLE stores (
    id         BIGINT      NOT NULL PRIMARY KEY,
    name       VARCHAR     NOT NULL UNIQUE,
    url        VARCHAR     NOT NULL,
    config     JSON        NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- 7. replicas
CREATE TABLE replicas (
    id          BIGINT      NOT NULL PRIMARY KEY,
    object_id   BIGINT      NOT NULL REFERENCES objects (id),
    store_id    BIGINT      NOT NULL REFERENCES stores (id),
    path        VARCHAR     NOT NULL,
    encrypted   BOOLEAN     NOT NULL DEFAULT FALSE,
    verified_at TIMESTAMPTZ NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT uq_replicas_object_store UNIQUE (object_id, store_id)
);

CREATE INDEX ix_replicas_store_id ON replicas (store_id);

-- 8. tags
CREATE TABLE tags (
    id         BIGINT      NOT NULL PRIMARY KEY,
    object_id  BIGINT      NOT NULL REFERENCES objects (id),
    key        VARCHAR     NOT NULL,
    value      VARCHAR     NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT uq_tags_object_key UNIQUE (object_id, key)
);

CREATE INDEX ix_tags_key_value ON tags (key, value);

-- 9. sync_peers
CREATE TABLE sync_peers (
    id               BIGINT      NOT NULL PRIMARY KEY,
    name             VARCHAR     NOT NULL,
    url              VARCHAR     NOT NULL,
    repository_id    BIGINT      NOT NULL REFERENCES repositories (id),
    last_synced_at   TIMESTAMPTZ NULL,
    last_snapshot_id BIGINT      NULL,
    config           JSON        NOT NULL DEFAULT '{}',
    created_at       TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT uq_sync_peers_name_repo UNIQUE (name, repository_id)
);

-- 10. machines
CREATE TABLE machines (
    machine_id  SMALLINT    NOT NULL PRIMARY KEY,
    name        VARCHAR     NOT NULL,
    description VARCHAR     NOT NULL DEFAULT '',
    last_seen_at TIMESTAMPTZ NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT uq_machines_name UNIQUE (name)
);
