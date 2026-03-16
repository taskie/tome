# ADR-008: Directory Listing API for Web UI

**Status:** Proposed
**Date:** 2026-03

## Context

The current `GET /repositories/{name}/files` endpoint supports `prefix` filtering via SQL `LIKE '{prefix}%'`. This returns **all descendants** under the prefix — a flat list of every file in the subtree. For a Web UI that renders a directory browser (file manager style), we need to show only the **direct children** of a given directory: immediate files and first-level subdirectories.

### Current behavior

```
GET /repositories/myrepo/files?prefix=src/

→ src/main.rs
→ src/lib.rs
→ src/commands/scan.rs
→ src/commands/diff.rs
→ src/commands/sync/pull.rs
```

### Desired behavior

```
GET /repositories/myrepo/files?dir=src/

→ src/main.rs        (file)
→ src/lib.rs         (file)
→ src/commands/      (directory, aggregated)
```

The API must:

1. Return files directly in the directory
2. Return subdirectory names (aggregated, not expanded)
3. Support pagination
4. Work for both `entry_cache` (current state) and `entries` (point-in-time snapshot)

### Problem with string-based filtering alone

A naïve approach uses `LIKE 'src/%'` to get all descendants, then applies string functions (`INSTR`, `SUBSTR`) to separate direct children from nested entries:

```sql
-- Can NOT use indexes on the INSTR/SUBSTR conditions
SELECT * FROM entry_cache
WHERE repository_id = ?
  AND path LIKE 'src/%'
  AND INSTR(SUBSTR(path, 5), '/') = 0   -- no further '/' after prefix
  AND status = 1;
```

The `INSTR(SUBSTR(...))` predicate **cannot use B-tree indexes** — the database must evaluate it on every row matching the `LIKE` prefix. For a directory containing 100,000 descendant entries where only 50 are direct children, 99,950 rows are scanned and discarded. This degrades with repository size.

## Decision

### 1. Add `depth` column to `entries` and `entry_cache`

Introduce a `depth` column (SMALLINT, NOT NULL) that records the nesting level of each path:

| Path | `depth` | Explanation |
|------|---------|-------------|
| `foo.txt` | 0 | Root-level file |
| `src/main.rs` | 1 | One `/` separator |
| `src/commands/scan.rs` | 2 | Two `/` separators |
| `src/commands` | 1 | Directory entry (ADR-007), one `/` separator |

The value is computed on insert as the number of `/` characters in the path. This is invariant — a path's depth never changes.

**Schema additions:**

```sql
-- entry_cache
ALTER TABLE entry_cache ADD COLUMN depth SMALLINT NOT NULL DEFAULT 0;
CREATE INDEX idx_entry_cache_repo_depth_path ON entry_cache (repository_id, depth, path);

-- entries
ALTER TABLE entries ADD COLUMN depth SMALLINT NOT NULL DEFAULT 0;
CREATE INDEX idx_entries_snapshot_depth_path ON entries (snapshot_id, depth, path);
```

### 2. Add `mode` column to `entry_cache`

Currently `entry_cache` lacks the `mode` column (available only in `entries`). Denormalize it so that the API can distinguish files from directories (mode = 16384 = `0o040000`) without joining:

```sql
ALTER TABLE entry_cache ADD COLUMN mode INTEGER NULL;
```

This is populated during scan alongside the existing denormalized fields (`digest`, `size`, `fast_digest`).

### 3. New query parameter: `dir`

Add a `dir` query parameter to `GET /repositories/{name}/files` and `GET /snapshots/{id}/entries`:

| Parameter | Type | Description |
|-----------|------|-------------|
| `dir` | `String` (optional) | Directory path. Empty or absent = root. Trailing `/` is optional (server normalizes). |

When `dir` is specified, the endpoint returns a **directory listing response** instead of the flat file list. The existing `prefix` parameter continues to work for flat listing (backward compatible).

`dir` and `prefix` are mutually exclusive; specifying both is an error (400).

### 4. SQL strategy with `depth` index

The key insight: direct children of a directory at depth `D` all have `depth = D`. With a composite index `(repository_id, depth, path)`, the query uses **three-part index lookup**: equality on `repository_id`, equality on `depth`, then a range scan on `path` for the `LIKE` prefix.

#### Listing direct children (entry_cache)

When tree entries (ADR-007) are present, files and directories coexist at the same depth:

```sql
-- All direct children of src/ (both files and directory entries)
-- dir = "src/"  →  target_depth = 1  (number of '/' in "src/")
SELECT * FROM entry_cache
WHERE repository_id = ?
  AND depth = 1
  AND path LIKE 'src/%'
  AND status = 1
ORDER BY path
LIMIT ? OFFSET ?;
```

The result set contains both files (mode ≠ 16384) and directory entries (mode = 16384). The API response splits them by `mode`.

**Index usage:** The composite index `(repository_id, depth, path)` satisfies all three filter conditions. `ORDER BY path` is covered by the index. This is an optimal range scan.

#### Root directory listing

```sql
-- Root-level children: depth = 0
SELECT * FROM entry_cache
WHERE repository_id = ?
  AND depth = 0
  AND status = 1
ORDER BY path
LIMIT ? OFFSET ?;
```

#### Point-in-time listing (entries table)

Same pattern on the `entries` table with `snapshot_id`:

```sql
SELECT * FROM entries
WHERE snapshot_id = ?
  AND depth = 1
  AND path LIKE 'src/%'
  AND status = 1
ORDER BY path
LIMIT ? OFFSET ?;
```

Uses index `(snapshot_id, depth, path)`.

#### Fallback: repositories without tree entries

For repositories scanned before tree hash was enabled, no directory entries (mode = 16384) exist. Directories must be derived from file paths:

```sql
-- Direct files (file entries at target depth)
SELECT * FROM entry_cache
WHERE repository_id = ?
  AND depth = 1
  AND path LIKE 'src/%'
  AND status = 1
ORDER BY path;

-- Derive subdirectory names from deeper file entries
SELECT DISTINCT SUBSTR(path, LENGTH('src/') + 1,
         INSTR(SUBSTR(path, LENGTH('src/') + 1), '/') - 1
       ) AS name
FROM entry_cache
WHERE repository_id = ?
  AND depth > 1
  AND path LIKE 'src/%'
  AND status = 1
ORDER BY name;
```

The subdirectory query uses the PK `(repository_id, path)` for the `LIKE` range scan, with `depth > 1` as a post-filter. This is less efficient than the tree-entry approach but only needed for legacy data.

The API detects which mode to use: if the latest snapshot has `root_object_id` set, tree entries are available and the single-query approach is used. Otherwise, fall back to the two-query approach.

### 5. Response shape: `DirectoryListingResponse`

```jsonc
{
  "dir": "src/",
  "items": [
    {
      "name": "commands",
      "path": "src/commands",
      "is_directory": true,
      "size": null,
      "mtime": null,
      "digest": "abcdef...",
      "fast_digest": "0123456789abcdef"
    },
    {
      "name": "main.rs",
      "path": "src/main.rs",
      "is_directory": false,
      "size": 1234,
      "mtime": "2026-03-15T10:00:00Z",
      "digest": "abcdef...",
      "fast_digest": "0123456789abcdef"
    }
  ],
  "page": 1,
  "per_page": 100,
  "total": 3
}
```

#### Design notes

- **Unified `items` list**: files and directories are interleaved in alphabetical order (directories first by convention, or mixed — TBD). The `is_directory` flag distinguishes them.
- **`is_directory`**: derived from `mode = 16384` when tree entries exist, or from path analysis in fallback mode.
- **Pagination**: covers all items (files + directories) in a single ordered list. Since both types are at the same `depth`, the `ORDER BY path` naturally interleaves them.
- **Root listing** (`dir` absent or empty): returns top-level items.

### 6. Depth computation

The `depth` value is computed deterministically from the path:

```rust
fn path_depth(path: &str) -> i16 {
    path.chars().filter(|&c| c == '/').count() as i16
}
```

This is set once at insert time (in `insert_entry_present`, `insert_entry_deleted`, `upsert_cache_present`, `upsert_cache_deleted`). No migration of existing data is needed beyond setting the default — a backfill query updates existing rows:

```sql
UPDATE entry_cache SET depth = LENGTH(path) - LENGTH(REPLACE(path, '/', ''));
UPDATE entries SET depth = LENGTH(path) - LENGTH(REPLACE(path, '/', ''));
```

### 7. Tree-based optimization (future)

With tree objects (ADR-007), directory listings can eventually be served via tree object navigation:

```
GET /repositories/{name}/tree/src/commands
→ Read root_object_id from latest snapshot
→ Walk tree objects by path components
→ Return children of the resolved tree object
```

This is O(depth) with no scanning. However, it requires tree-walking queries and joining entry metadata (mtime, status). The `depth`-indexed approach in this ADR provides a practical solution that works with existing data and requires only a column addition + index.

### 8. Web UI usage

The directory browser component would:

1. On initial load: `GET /repositories/{name}/files?dir=` (root listing)
2. On directory click: `GET /repositories/{name}/files?dir=src/commands/`
3. Show 📁 icons for `is_directory = true`, file icons otherwise
4. Breadcrumb navigation from the `dir` path components
5. Pagination over the unified items list

## Consequences

- **Schema change**: adds `depth` (SMALLINT) to `entries` and `entry_cache`, and `mode` (INTEGER) to `entry_cache`. Both are small, non-nullable columns. Existing rows need a one-time backfill.
- **New indexes**: `(repository_id, depth, path)` on `entry_cache` and `(snapshot_id, depth, path)` on `entries`. These are the primary query paths for directory listing and provide optimal range scans.
- **Backward compatible**: the existing `prefix` parameter behavior is unchanged. The `dir` parameter is additive.
- **Sorting**: items are returned in lexicographic path order. Directories (when tree entries exist) naturally sort alongside files.
- **Deleted files**: when `include_deleted=true`, deleted entries appear with `status=0`. Deleted entries do not contribute to the directory structure in fallback mode (only `status=1` entries are considered).
- **Empty directories**: shown only when tree entries are present (as explicit `mode=16384` entries). In fallback mode, empty directories are invisible — consistent with the current behavior where empty directories are not tracked.
- **`dir` normalization**: the server strips or adds trailing `/` as needed. `dir=src` and `dir=src/` are equivalent. `dir=/` and `dir=` both mean root.
- **Insert overhead**: minimal. Computing `depth` is a single character count. Setting `mode` on `entry_cache` copies an existing field from the entry.

