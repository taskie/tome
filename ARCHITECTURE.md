# Architecture

> Technical reference for the **tome** file change tracking system.
> For detailed documentation, see the files under `docs/`.

---

## Crate Structure

| Crate | Description |
|-------|-------------|
| `tome-core` | Hash computation (delegates to treblo), ID generation (Sonyflake), shared models |
| `tome-db` | SeaORM entities, migrations, query operations (`ops/` modules), `MetadataStore` trait |
| `tome-dynamo` | DynamoDB `MetadataStore` implementation (single-table design) |
| `tome-store` | Async `Storage` trait + implementations: Local, SSH, S3, Encrypted |
| `tome-server` | HTTP API server (axum 0.8, `routes/` modules) |
| `tome-cli` | Unified CLI: scan / log / show / diff / files / history / status / restore / store / remote / sync / tag / verify / gc / push / pull / serve / watch |
| `tome-web` | Next.js 16 web frontend (Server Components, Tailwind CSS v4) |
| `aether` | Streaming AEAD encryption: XChaCha20-Poly1305 / ChaCha20-Poly1305 / AES-256-GCM + Argon2id KDF + zstd compression |
| `treblo` | Hash algorithms (xxHash64 / SHA-256 / BLAKE3), file-tree walk, hex utilities |

`tome-sync` is not a separate crate; it lives in `tome-cli/src/commands/sync.rs`.

### Dependency Graph

```mermaid
graph TD
    treblo["treblo<br/><small>hashing, file walk</small>"]
    aether["aether<br/><small>streaming AEAD</small>"]
    core["tome-core<br/><small>models, ID gen</small>"]
    db["tome-db<br/><small>SeaORM, MetadataStore trait</small>"]
    dynamo["tome-dynamo<br/><small>DynamoDB backend</small>"]
    store["tome-store<br/><small>Storage trait</small>"]
    server["tome-server<br/><small>HTTP API</small>"]
    cli["tome-cli<br/><small>CLI entry point</small>"]
    web["tome-web<br/><small>Next.js frontend</small>"]

    core --> treblo
    db --> core
    dynamo --> core
    dynamo --> db
    store --> core
    store --> aether
    server --> core
    server --> db
    server -. "dynamodb feature" .-> dynamo
    cli --> core
    cli --> db
    cli --> server
    cli --> store

    web -. "HTTP" .-> server

    style treblo fill:#e8f5e9
    style aether fill:#e8f5e9
    style core fill:#e3f2fd
    style db fill:#e3f2fd
    style dynamo fill:#e3f2fd
    style store fill:#e3f2fd
    style server fill:#fff3e0
    style cli fill:#fce4ec
    style web fill:#f3e5f5
```

Legacy crates (`ichno`, `ichno_cli`, `ichnome`, `ichnome_cli`, `ichnome_web`, `ichnome_web_front`, `optional_derive`) are archived under `obsolete/` and excluded from the workspace.

---

## Design Principles

1. **Single Source of Truth** — Each piece of information lives in exactly one place. Caches are named explicitly (`entry_cache`).
2. **Local-first** — SQLite is a first-class citizen. A remote server DB is just one possible sync target.
3. **Event sourcing** — Changes are recorded as immutable snapshots. Current state is derived from the snapshot chain.
4. **Storage internalization** — The location of every stored blob is tracked in the `replicas` table, not assumed.
5. **Encryption as a layer** — `EncryptedStorage<S>` wraps any `Storage` implementation transparently.

---

## CLI Command Taxonomy

> Full analysis: [ADR-010](docs/adr/010-cli-command-taxonomy.md)

tome's CLI follows a **two-layer model** inspired by git:

| Layer | Commands | Purpose |
|-------|----------|---------|
| **Porcelain** (verbs) | `scan`, `log`, `show`, `diff`, `files`, `history`, `status`, `push`, `pull`, `watch` | Everyday user-facing actions on the implicit repository |
| **Plumbing** (nouns) | `store`, `remote`, `sync`, `tag` | Resource management subtrees with CRUD subcommands |
| **Infrastructure** | `init`, `serve`, `gc`, `verify`, `restore` | Setup, maintenance, and recovery |

### Subcommand verb consistency

All resource subtrees use the same verbs: `add`, `set`, `rm`, `list`. This mirrors git's `remote add/rm/rename` pattern. `tag rm` is canonical; `tag delete` is a hidden alias for backward compatibility.

### Snapshot references

Commands that accept snapshot identifiers (`diff`, `show`, `restore`) support a shared reference syntax parsed by the `SnapshotRef` module:

| Syntax | Meaning |
|--------|---------|
| `@latest` | Most recent snapshot |
| `@latest~N` | N-th ancestor of the latest |
| `@YYYY-MM-DD` | Latest snapshot on or before that date (local TZ) |
| `@YYYY-MM-DDThh:mm` | Datetime variant |
| Raw `i64` | Direct snapshot ID (backward compatible) |

### Output format

Query commands (`log`, `show`, `files`, `history`, `status`) support `--format json` via a shared `OutputFormat` enum, enabling scripting with tools like `jq`.

---

## Configuration Hierarchy

> Full analysis: [ADR-011](docs/adr/011-config-hierarchy.md)

### Resolution order

tome resolves settings from five layers (highest priority first):

```
1. CLI arguments          --repo photos
2. Environment variables  TOME_REPO=photos
3. Project-local config   ./tome.toml
4. Global config          ~/.config/tome/tome.toml
5. Built-in defaults      "default"
```

Layers 1–2 are handled by clap's argument parser. Layers 3–4 are merged by `config::load_config()`, with project-local values overriding global ones. Layer 5 is the compiled-in fallback.

### Comparison with git

| Aspect | git | tome |
|--------|-----|------|
| **Repo identity** | Implicit (working directory = repo) | Explicit (`--repo` / `TOME_REPO` / config `repo`) |
| **Config discovery** | Walks up to `.git/config` | Fixed: `./tome.toml` + `~/.config/tome/tome.toml` |
| **Per-project anchor** | `.git/` directory | `tome.db` database file |
| **Config layers** | 7 (builtin → system → global → local → worktree → env → CLI) | 5 (builtin → global → local → env → CLI) |

Key difference: git discovers the repository by walking the directory tree up from `$PWD` to find `.git/`. tome does not walk parent directories — the database file `tome.db` is the project anchor, and `./tome.toml` is always read from the current directory. This is intentional: tome is a personal tool where `cwd` is the project root, and the complexity of parent-directory walking provides marginal benefit.

### The `--repo` fallback

A single database can contain multiple named repositories (e.g., `default`, `photos`, `docs`). The `--repo` flag selects which one to operate on. Rather than hardcoding `"default"` in every command, the effective repo name is resolved as:

```
CLI --repo  >  TOME_REPO env  >  tome.toml `repo`  >  [scan] repo  >  "default"
```

This allows users to set `repo = "photos"` once in `tome.toml` and have all commands use it without repeating `--repo photos`.

---

## Detailed Documentation

### Architecture (`docs/arch/`)

| Document | Description |
|----------|-------------|
| [Database Schema](docs/arch/database-schema.md) | 10-table schema, ER diagram, table descriptions, DDL management |
| [Hash Strategy](docs/arch/hash-strategy.md) | Three-stage change detection filter (mtime -> xxHash64 -> SHA-256/BLAKE3) |
| [Encryption](docs/arch/encryption.md) | aether binary format, STREAM construction, key management |
| [Storage](docs/arch/storage.md) | URL schemes, blob path layout |
| [HTTP API](docs/arch/http-api.md) | Endpoint reference, response shapes |
| [Web Frontend](docs/arch/web-frontend.md) | Next.js directory structure, navigation, implementation notes |
| [Central Sync](docs/arch/central-sync.md) | Two-layer sync, modes, AWS IAM auth, machine_id allocation |
| [Lambda Deployment](docs/arch/lambda-deployment.md) | Build, environment variables, backend selection |
| [DynamoDB Backend](docs/arch/dynamodb.md) | Single-table design, access patterns, key schema, GSIs |

### Generated Schema & API (`docs/schema/`)

| File | Generated by | Description |
|------|-------------|-------------|
| [`openapi.json`](docs/schema/openapi.json) | `cargo run -p tome-server --example generate_openapi` | OpenAPI 3.0 spec (pretty-printed) |
| [`tome-db.sql`](docs/schema/tome-db.sql) | `cargo run -p tome-db --example generate_schema` | SQLite DDL from SeaORM migrations |

### Architecture Decision Records (`docs/adr/`)

| ADR | Title |
|-----|-------|
| [ADR-001](docs/adr/001-sonyflake-ids.md) | Sonyflake ID Generation |
| [ADR-002](docs/adr/002-content-addressable-storage.md) | Content-Addressable Storage |
| [ADR-003](docs/adr/003-local-first-sqlite.md) | Local-First with SQLite |
| [ADR-004](docs/adr/004-metadatastore-trait.md) | MetadataStore Trait Abstraction |
| [ADR-005](docs/adr/005-dynamodb-single-table.md) | DynamoDB Single-Table Design |
| [ADR-006](docs/adr/006-declarative-ddl.md) | Declarative DDL over Runtime Migrations |
| [ADR-007](docs/adr/007-tree-hash-integration.md) | Tree Hash Integration |
| [ADR-008](docs/adr/008-directory-listing-api.md) | Directory Listing API |
| [ADR-009](docs/adr/009-aether-in-place-aead.md) | In-Place AEAD and Buffer Reuse |
| [ADR-010](docs/adr/010-cli-command-taxonomy.md) | CLI Command Taxonomy Review |
| [ADR-011](docs/adr/011-config-hierarchy.md) | Configuration Hierarchy and `--repo` Default |
| [ADR-012](docs/adr/012-transparent-compression.md) | Transparent zstd Compression |

---

## Known Design Issues

### 1. `entry_cache` is current-state only

`entry_cache` is a materialized view of the latest snapshot per path. It cannot answer "what did the repository look like at time T?" without re-querying the `entries` + `snapshots` tables. Features like `tome restore` must bypass `entry_cache` entirely.

### 2. `tome restore` requires store availability

To restore a file from a historical snapshot, the corresponding blob must exist in at least one reachable store. There is currently no `--check` flag to verify replica availability before attempting a restore.

### 3. ID generation depends on machine-id and start-time

Sonyflake IDs are generated from `(timestamp, machine_id, sequence)`. The epoch is fixed at `2023-09-01 00:00:00 UTC`. Changing either `start_time` or `machine_id` mid-stream breaks ID ordering and risks collisions.
