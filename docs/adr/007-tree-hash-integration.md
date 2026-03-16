# ADR-007: Tree Hash Integration for Efficient Diff and Sync

**Status:** Accepted
**Date:** 2026-03

## Context

tome currently performs **linear scans of all entries** for snapshot comparison and remote synchronization:

- `tome diff`: Fetches all entries from both snapshots, builds `HashMap<path → blob_id>`, and compares — O(n) where n = entry count.
- `sync pull/push`: Copies every entry of each new snapshot, even when the content is identical.
- Web UI: Detecting changes under a specific directory requires a prefix filter over all entries.

None of these operations can answer "has this subtree changed?" in O(1). As file counts grow, unchanged subtrees are unnecessarily traversed.

Meanwhile, the `treblo` crate already implements a Native-mode Merkle tree hash (`treblo::native::tree`):

```
Entry encoding:  kind_byte('F'|'D') || name || b'\0' || hash
Tree hash:       H(sort(entries))     — sorted by (kind_byte, name)
```

Integrating this into the snapshot/entry model would enable O(1) subtree equivalence checks at every directory level.

## Decision

### 1. Algorithm constraint

The tree hash algorithm **must match** the algorithm used for `blob.digest` in the same repository. Since `blob.digest` is already SHA-256 or BLAKE3 (ADR-002) and is fixed per repository, the tree hash inherits the same algorithm. No separate algorithm column is needed — the repository-level setting is authoritative.

This means `blob.digest` values serve directly as leaf nodes in the Merkle tree, with no re-hashing.

### 2. Schema extension

#### `objects` table (replaces `blobs`)

| Column | Type | Constraint | Description |
|--------|------|------------|-------------|
| `id` | `BIGINT` | PK (Sonyflake) | Object ID |
| `digest` | `BYTEA(32)` | UNIQUE | Content hash (blob) or Merkle tree hash (tree) |
| `size` | `BIGINT` | NOT NULL | File size (blob) or serialized tree content size (tree) |
| `fast_digest` | `BIGINT` | NOT NULL | xxHash64 of content |
| `created_at` | `TIMESTAMPTZ` | NOT NULL | Creation timestamp |

Content-addressable and deduplicated by `digest`. Identical file contents share a blob object; identical directory structures share a tree object. The object type (blob vs tree) is not stored in this table — it is inferred from `entries.mode` (`0o040000` = directory/tree, otherwise file/blob). This avoids conflicts when the same digest could theoretically represent both types.

#### `entries` table — column rename

| Change | Description |
|--------|-------------|
| `blob_id` → `object_id` | `BIGINT` (nullable, FK → `objects.id`). Points to a blob or tree object depending on `mode`. |

#### `snapshots` table — new column

| Column | Type | Description |
|--------|------|-------------|
| `root_object_id` | `BIGINT` (nullable, FK → `objects.id`) | Root tree object of the snapshot |

#### `entry_cache` table — column rename

| Change | Description |
|--------|-------------|
| `blob_id` → `object_id` | nullable, mirrors entries |

New `object_id` / `root_object_id` columns are nullable to preserve backward compatibility with existing snapshots. The `blob_id` → `object_id` rename is applied during the migration that creates the `objects` table from `blobs`.

### 3. Blob vs tree distinction via `entries.mode`

The existing `entries.mode` column (`i32`, maps to `FileMode`) already distinguishes file types:

| `mode` | Meaning | `object_id` points to |
|--------|---------|----------------------|
| `0o100644` | Regular file | blob object |
| `0o100755` | Executable file | blob object |
| `0o120000` | Symlink | blob object |
| `0o040000` | Directory | tree object |

No new type column is needed on entries. An entry is a directory when `mode = 0o040000`. The invariant is:

- **File entry** (`mode ≠ 0o040000`): `object_id` references a blob object (file content hash).
- **Directory entry** (`mode = 0o040000`): `object_id` references a tree object (Merkle tree hash).

Both use the same `object_id` FK. The entry's `mode` determines how the referenced object is interpreted.

### 4. How tree objects are created

After `tome scan` computes all file blob digests, tree objects are built bottom-up:

```
1. For each leaf directory (contains only files):
   children = [(b'F', filename, blob.digest), ...]
   tree_digest = H(sort(children))
   → INSERT INTO objects (digest) or find existing

2. For each parent directory:
   children = [(b'F', filename, blob.digest), ...]
             + [(b'D', dirname, child_tree.digest), ...]
   tree_digest = H(sort(children))
   → INSERT INTO objects (digest) or find existing

3. Root tree object is referenced from snapshot.root_object_id
```

Like blobs, trees are deduplicated by `digest`. If a directory's contents are unchanged across snapshots, the same object is reused. This deduplication is key to efficient comparison: equal `object_id` ⇒ identical subtree without inspecting children.

### 5. Efficiency gains

#### Subtree diff (any directory level)

```
entry_A("src/commands").object_id == entry_B("src/commands").object_id
  → match:    entire subtree is identical — skip all children
  → mismatch: query direct children of both trees, recurse only into changed subdirs
```

This enables **recursive tree diff** with work proportional to the number of changed paths, not total paths. For a repository with 100,000 files where 10 changed, diff touches ~O(10 × depth) entries instead of 100,000.

#### Snapshot-level equivalence

```
snapshot_A.root_object_id == snapshot_B.root_object_id
  → identical: O(1), no entry queries needed
```

Because objects are deduplicated, `object_id` equality (integer comparison) is sufficient — no need to compare digests.

#### Remote sync

During `sync pull/push`, compare root tree digests (via `objects.digest`) before transferring entries. When they differ, subtree comparison can minimize the transferred entry set.

#### Web UI

Directory browser compares `object_id` between snapshots per directory. Only directories with changed object IDs need visual indicators. Expanding a directory queries its direct children only.

### 6. Object model: `objects` as a common abstraction

Blobs and trees share the same fundamental structure — content-addressable objects identified by digest:

| Field | `blobs` (current) | `trees` (proposed) |
|-------|-------------------|-------------------|
| `id` | Sonyflake PK | Sonyflake PK |
| `digest` | UNIQUE, content hash | UNIQUE, Merkle hash |
| `created_at` | timestamp | timestamp |
| `size` | file size (i64) | serialized tree size |
| `fast_digest` | xxHash64 (i64) | xxHash64 of tree content |

Three approaches were considered:

#### Option A: Inline `tree_hash` column on `entries`

Rejected. Mixes blob and tree concerns, denormalizes tree hashes across snapshots, no deduplication, no FK integrity.

#### Option B: Separate `blobs` + `trees` tables

Two parallel tables with independent FK columns (`blob_id`, `tree_id`) on entries. Simple and explicit, but duplicates the content-addressable pattern and requires two nullable FKs per entry.

#### Option C: Unified `objects` table (recommended)

Introduce an `objects` table as the common base:

```sql
CREATE TABLE objects (
    id          BIGINT PRIMARY KEY,
    digest      BYTEA NOT NULL UNIQUE,
    size        BIGINT NOT NULL,         -- file size (blob) or serialized tree size (tree)
    fast_digest BIGINT NOT NULL,         -- xxHash64 of content
    created_at  TIMESTAMPTZ NOT NULL
);
```

Entries reference a single FK:

```
objects:  id, digest, size, fast_digest, created_at
entries:  ..., object_id (FK → objects.id), mode
```

The entry's `mode` (`0o040000` = directory, else file) determines whether the referenced object is a tree or blob. `entries.object_id` replaces both `blob_id` and `tree_id`. The `objects` table itself does not store a type discriminator — this avoids potential conflicts when a file's content hash coincidentally matches a tree hash.

**Why Option C is preferred:**

- **Single FK** on entries instead of two nullable FKs — cleaner invariant (every present entry has exactly one object).
- **Uniform code paths** — GC, sync, deduplication logic operates on "objects" regardless of type.
- **No type discriminator in objects** — the entry's `mode` field determines interpretation. This avoids conflicts when hash collisions between blob and tree content could produce inconsistent `object_type` values.
- **Mirrors Git's object store** — Git uses a single object database for blobs, trees, commits, and tags. tome adopts the same principle (without commits/tags as objects, for now).
- **Extensible** — future object types (e.g., manifest, chunk) can be distinguished by entry mode or context.
- **`size` / `fast_digest` always populated** — both blobs and trees store size and fast_digest. For blobs, these are file content metrics; for trees, they represent the serialized tree content.

#### Migration path from `blobs`

The existing `blobs` table is renamed/migrated to `objects`. All existing rows are preserved as-is. `entries.blob_id` is renamed to `entries.object_id`. Existing blob IDs and digests are preserved.

### 7. Why treblo native format over Git format

| Aspect | Git format | treblo native format |
|--------|-----------|---------------------|
| Hash input for files | `blob {size}\0{content}` prefix | Raw file content |
| Tree encoding | Git binary format (octal mode + NUL + raw hash) | `kind_byte \|\| name \|\| \0 \|\| hash` (simple, extensible) |
| Algorithm | Assumes SHA-1 | Same as `blob.digest` (BLAKE3 / SHA-256) |
| Leaf node compatibility | `blob.digest` ≠ Git blob hash — requires recomputation | `blob.digest` used directly — zero overhead |

### 8. Rollout phases

| Phase | Scope |
|-------|-------|
| Phase 1 | `blobs` → `objects` migration + tree object creation during `tome scan` |
| Phase 2 | `tome diff`: recursive tree diff via `object_id` comparison |
| Phase 3 | `sync pull/push`: tree-based skip and selective entry transfer |
| Phase 4 | Web UI: directory-level change indicators and lazy subtree expansion |

### 9. Storage overhead

Each directory adds one entry row per snapshot and one tree row (deduplicated across snapshots):

| Metric | Files only (current) | With directory entries |
|--------|--------------------|-----------------------|
| 10,000 files, 500 dirs | 10,000 entries | 10,500 entries (+5%) |
| 100,000 files, 5,000 dirs | 100,000 entries | 105,000 entries (+5%) |

The `objects` table grows much more slowly than entries due to deduplication — unchanged directories share tree objects across snapshots.

## Consequences

- `tome scan` gains a bottom-up tree hash computation pass. Since it reuses blob digests already in memory/DB, no additional file I/O is needed.
- The `entries` table grows by ~5% (one row per directory per snapshot). Tree objects in `objects` are compact due to deduplication.
- The `blobs` → `objects` migration renames the table and renames `entries.blob_id` → `object_id`. Existing data is preserved.
- Existing snapshots without `root_object_id` remain valid; the nullable column means no forced migration. Backfill of tree objects is possible via `compute_tree_from_entries()` on historical entry data.
- `entry_cache` must be extended to track directory entries, affecting cache rebuild logic.
- `tome diff` can be rewritten as a recursive tree comparison, providing orders-of-magnitude speedup for large repositories with localized changes.
- Sync operations gain the ability to skip unchanged snapshots and, in future, transfer only changed subtrees.
- After scan, every directory on every tracked path must have a corresponding entry with `mode = 0o040000` and a valid `object_id` pointing to a tree object. The scan logic must ensure this invariant.
- GC operates uniformly on `objects`: an object (blob or tree) can only be deleted when no entries or snapshots reference it.
- The unified `objects` table simplifies Rust code — a single `Object` entity replaces the separate `Blob` entity. The entry's `mode` determines whether the object is a blob or tree. Existing code that references `blob_id` is updated to `object_id`.
- The `replicas` table continues to reference objects via `blob_id` → `object_id`. Replicas are only meaningful for blob objects (file content stored on external storage); tree objects exist only in the metadata DB.
