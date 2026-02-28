# Architecture

> Technical reference for the **tome** file change tracking system.

---

## Crate structure

```
tome-core/    Hash computation (delegates to treblo), ID generation (Sonyflake), shared models
tome-db/      SeaORM entities, migrations, query operations (ops.rs)
tome-store/   Async Storage trait + implementations: Local, SSH, S3, Encrypted
tome-server/  HTTP API server (axum 0.8)
tome-cli/     Unified CLI: scan / store / sync / diff / restore / tag / verify / gc / serve
tome-web/     Next.js 16 web frontend (Server Components, Tailwind CSS v4)
aether/       AEAD encryption library: AES-256-GCM / ChaCha20-Poly1305 + Argon2id KDF
treblo/       Hash algorithms (xxHash64/SHA-256/BLAKE3), file-tree walk, and hex utilities
```

`tome-sync` is not a separate crate; it lives in `tome-cli/src/commands/sync.rs`.

Legacy crates (`ichno`, `ichno_cli`, `ichnome`, `ichnome_cli`, `ichnome_web`, `ichnome_web_front`, `optional_derive`) are archived under `obsolete/` and excluded from the workspace.

---

## Design principles

1. **Single Source of Truth** — Each piece of information lives in exactly one place. Caches are named explicitly (`entry_cache`).
2. **Local-first** — SQLite is a first-class citizen. A remote server DB is just one possible sync target.
3. **Event sourcing** — Changes are recorded as immutable snapshots. Current state is derived from the snapshot chain.
4. **Storage internalization** — The location of every stored blob is tracked in the `replicas` table, not assumed.
5. **Encryption as a layer** — `EncryptedStorage<S>` wraps any `Storage` implementation transparently.

---

## Database schema

9 tables. All IDs are Sonyflake `i64`. All timestamps are `DateTimeWithTimeZone`.

| Table | Description |
|-------|-------------|
| `repositories` | Named scan namespaces (e.g. `default`) |
| `blobs` | Content-addressable file fingerprints (`digest`=SHA-256 or BLAKE3, `fast_digest`=xxHash64) |
| `snapshots` | Scan execution events (analogous to Git commits, chained via `parent_id`) |
| `entries` | Per-file state within a snapshot (`status`: 0=deleted, 1=present) |
| `entry_cache` | Latest state cache per path, PK=(repository\_id, path) |
| `stores` | Storage backend definitions (`url`: `file:///`, `ssh://`, `s3://`) |
| `replicas` | Tracks which store holds which blob |
| `tags` | Key-value attributes on blobs |
| `sync_peers` | Sync peer definitions (`url` + `last_snapshot_id`) |

### entity_cache limitations

`entry_cache` holds only the **current** (latest) state of each path. Comparing two arbitrary points in time requires querying the `entries` table directly, joined with the relevant snapshots.

---

## Hash strategy

Change detection uses a three-stage filter to minimize I/O:

```
mtime / size  →  xxHash64  →  SHA-256 (or BLAKE3)
```

If a stage shows no change, subsequent hashes are skipped. Both hashes are computed in a single pass through the file in `treblo/src/hash.rs::hash_file()` (re-exported via `tome-core::hash`).

The digest algorithm is configured per repository via `repositories.config["digest_algorithm"]` (default: `"sha256"`). Use `tome scan --digest-algorithm blake3` when creating a new repository. The algorithm cannot be changed after the first scan (digest consistency).

---

## Encryption

`aether` crate (internal): AES-256-GCM or ChaCha20-Poly1305 authenticated encryption with Argon2id key derivation.

Key storage:
```
~/.config/tome/keys/<key_id>.key    — 32-byte raw binary key
```

`EncryptedStorage<S>` is implemented in `tome-store/src/encrypted.rs`. It is activated via `tome store copy --encrypt --key-file <path>`.

---

## Storage

### Supported URL schemes

| Scheme | Example |
|--------|---------|
| Local filesystem | `file:///mnt/backup` |
| SSH / SFTP | `ssh://user@host/path` |
| Amazon S3 | `s3://bucket/prefix` |

### Blob path layout

Blobs are stored at a content-addressed path (see `tome-store/src/storage.rs::blob_path()`):

```
objects/<hex[0:2]>/<hex[2:4]>/<full-hex>
```

Example: digest `deadbeef1234…` → `objects/de/ad/deadbeef1234…`

---

## HTTP API

Served by `tome serve` (default: `http://127.0.0.1:8080`).
Router defined in `tome-server/src/server.rs`.

```
GET /health
GET /repositories
GET /repositories/{name}
GET /repositories/{name}/snapshots
GET /repositories/{name}/latest
GET /repositories/{name}/files        ?prefix= &include_deleted= &page= &per_page=
GET /repositories/{name}/diff         ?snapshot1= &snapshot2= &prefix=
GET /repositories/{name}/history      ?path=
GET /diff                              ?repo1= &prefix1= &repo2= &prefix2=
GET /snapshots/{id}/entries           ?prefix=
GET /blobs/{digest}
GET /blobs/{digest}/entries
```

Notes:
- Digests are stored as binary in the DB and returned as lowercase hex strings in responses.
- `GET /diff` compares current state (`entry_cache`) across two repositories, with independent path prefixes per side. Entry keys are namespaced as `"1:{path}"` / `"2:{path}"` to avoid collisions. Limit: 10,000 entries per side.
- `GET /repositories/{name}/diff` compares two **snapshots** within one repository.

### Known issue: `GET /diff` omits deleted files

Entries with `blob_id = NULL` (i.e. `status = 0`, deleted) are stored in the `entries` map of the response but do **not** appear in the `diff` map, because the diff is keyed by `blob_id`. Deleted files are therefore silently excluded from cross-repository diff results.

---

## Web frontend (tome-web)

Next.js 16 + TypeScript + Tailwind CSS v4 + App Router (Server Components only).

### Directory structure

```
tome-web/
  src/
    lib/
      api.ts        fetch-based API client (TOME_API_URL env var)
      types.ts      TypeScript type definitions
    app/
      layout.tsx                              root layout (header nav)
      page.tsx                                repository list (/)
      not-found.tsx
      diff/page.tsx                           cross-repo diff (/diff)
      repositories/[name]/page.tsx            snapshot list
      repositories/[name]/files/page.tsx      current files (entry_cache)
      repositories/[name]/diff/page.tsx       per-snapshot diff
      repositories/[name]/history/page.tsx    per-path history
      snapshots/[id]/page.tsx                 snapshot entry list
      blobs/[digest]/page.tsx                 blob detail
      globals.css                             Tailwind v4 (@import "tailwindcss")
  eslint.config.mjs    ESLint flat config (eslint-config-next 16)
  .prettierrc.json     Prettier config (printWidth: 120)
  env.local.example    TOME_API_URL=http://localhost:8080
  .nvmrc               24
```

### Navigation structure

```
/                           → repository list
  /repositories/[name]      → snapshot list  [Snapshots | Files | Diff] sub-nav
    /repositories/.../files → current files → /repositories/.../history
    /repositories/.../diff  → snapshot diff  → paths link to history
    /repositories/.../history?path=  → file history → /blobs/[digest]
  /snapshots/[id]           → snapshot entries (reached from history or repo page)
  /blobs/[digest]           → blob detail + files using this content
  /diff                     → cross-repo diff → paths link to per-repo history
```

### Key implementation notes

- All API calls are server-side; no CORS required. `TOME_API_URL` is a server-only env var.
- Every page uses `export const dynamic = "force-dynamic"` to prevent build-time SSG (which would fail if `tome serve` is not running).
- Tailwind v4: `@import "tailwindcss"` in `globals.css` only; no `tailwind.config.ts` needed. PostCSS plugin: `@tailwindcss/postcss`.

---

## Known design issues

### 1. `entry_cache` is current-state only

`entry_cache` is a materialized view of the latest snapshot per path. It cannot answer "what did the repository look like at time T?" without re-querying the `entries` + `snapshots` tables. Features like `tome restore` (restoring a historical snapshot) must bypass `entry_cache` entirely.

### 2. Cross-repo diff silently omits deleted files

`GET /diff` groups results by `blob_id`. Files deleted in the scanned repository have `blob_id = NULL`, so they never appear in the `diff` map — even though they exist in `entry_cache`. A future fix could add a separate `deleted` list to `RepoDiffResponse`.

### 3. `tome restore` requires store availability

To restore a file from a historical snapshot, the corresponding blob must exist in at least one reachable store. There is currently no API to check replica availability before attempting a restore.

### 4. ID generation depends on machine-id and start-time

Sonyflake IDs (`i64`) are generated from `(timestamp, machine_id, sequence)`. The epoch is fixed at `2023-09-01 00:00:00 UTC`. Changing either `start_time` or `machine_id` mid-stream breaks ID ordering and risks collisions.
