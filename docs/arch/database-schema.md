# Database Schema

10 tables. All IDs are Sonyflake `i64` (except `machines.machine_id` which is `i16`). All timestamps are `DateTimeWithTimeZone`.

## ER Diagram

```mermaid
erDiagram
    repositories ||--o{ snapshots : contains
    repositories ||--o{ entry_cache : caches
    snapshots ||--o{ entries : contains
    snapshots }o--o| snapshots : "parent_id"
    entries }o--|| blobs : references
    entry_cache }o--|| blobs : references
    blobs ||--o{ replicas : "stored in"
    blobs ||--o{ tags : "tagged with"
    stores ||--o{ replicas : provides
    machines ||--o{ snapshots : "source_machine_id"

    repositories {
        i64 id PK
        string name UK
    }
    blobs {
        i64 id PK
        bytes digest UK
        i64 fast_digest
        i64 size
    }
    snapshots {
        i64 id PK
        i64 repository_id FK
        i64 parent_id FK
        i16 source_machine_id
        i64 source_snapshot_id
    }
    entries {
        i64 id PK
        i64 snapshot_id FK
        i64 blob_id FK
        string path
        i16 status
    }
    entry_cache {
        i64 repository_id PK
        string path PK
        i64 blob_id FK
        i16 status
    }
    stores {
        i64 id PK
        string name UK
        string url
    }
    replicas {
        i64 id PK
        i64 store_id FK
        i64 blob_id FK
        string state
    }
    tags {
        i64 id PK
        i64 blob_id FK
        string key
        string value
    }
    sync_peers {
        i64 id PK
        string name UK
        string url
        i64 last_snapshot_id
    }
    machines {
        i16 machine_id PK
        string name
        datetime last_seen_at
    }
```

## Table Descriptions

| Table | Description |
|-------|-------------|
| `repositories` | Named scan namespaces (e.g. `default`) |
| `blobs` | Content-addressable file fingerprints (`digest`=SHA-256 or BLAKE3, `fast_digest`=xxHash64) |
| `snapshots` | Scan execution events (analogous to Git commits, chained via `parent_id`). `source_machine_id` / `source_snapshot_id` track sync provenance |
| `entries` | Per-file state within a snapshot (`status`: 0=deleted, 1=present) |
| `entry_cache` | Latest state cache per path, PK=(repository\_id, path) |
| `stores` | Storage backend definitions (`url`: `file:///`, `ssh://`, `s3://`) |
| `replicas` | Tracks which store holds which blob |
| `tags` | Key-value attributes on blobs |
| `sync_peers` | Sync peer definitions (`url` + `last_snapshot_id`) |
| `machines` | Registered machines for central sync (`machine_id` as PK, `name`, `last_seen_at`) |

## entry_cache Limitations

`entry_cache` holds only the **current** (latest) state of each path. Comparing two arbitrary points in time requires querying the `entries` table directly, joined with the relevant snapshots.

## Declarative Schema Management

A canonical DDL file (`tome-db/schema.sql`) is maintained for use with declarative
migration tools such as [psqldef](https://github.com/sqldef/sqldef). This DDL
represents the full target schema and can be applied directly:

```bash
psqldef -U <user> -h <host> <database> < tome-db/schema.sql
```

SeaORM migrations (`tome-db/src/migration/`) are still used by `connection::open()`
(CLI / local SQLite), while `connection::connect()` skips migrations for deployments
where the schema is managed externally.
