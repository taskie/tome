# ADR-010: CLI Command Taxonomy Review

**Status:** Proposed  
**Date:** 2026-04-05  

## Context

tome's CLI has grown organically as features were added. With 14 top-level commands and several subcommand trees, it is worth reviewing the overall taxonomy for consistency, discoverability, and conceptual clarity. This ADR catalogs the current state, identifies inconsistencies, and proposes improvements.

### Current command inventory

```
tome scan          # record file state (snapshot creation)
tome diff          # compare two snapshots
tome verify        # bit-rot detection against entry cache
tome restore       # restore files from a store
tome store ...     # manage blob storage backends (add/set/rm/list/push/copy/verify)
tome remote ...    # manage sync peers (add/set/rm/list)
tome sync ...      # low-level sync operations (config/pull/push)
tome tag ...       # key-value metadata on blobs (set/delete/list/search)
tome gc            # garbage collection (prune snapshots + unreferenced blobs)
tome push          # composite: scan → store push → sync push
tome pull          # composite: sync pull → optional store copy
tome init          # register machine with central server
tome serve         # start HTTP API server
tome watch         # continuous monitoring (inotify → auto scan)
```

Planned (from TODO.md):

```
tome log           # list snapshots
tome show          # snapshot detail
tome files         # list tracked files (entry_cache)
tome status        # detect changes since last scan (read-only)
tome history       # file change history
tome repo ...      # repository management (list/rm/rename)
```

## Analysis

### 1. Layering: porcelain vs. plumbing

`push` and `pull` are composite (porcelain) commands that orchestrate `scan`, `store push`, and `sync push/pull`. This is a good pattern borrowed from git. However, the boundary between porcelain and plumbing is blurred:

- **`sync config/pull/push`** are plumbing, correctly separated.
- **`store push/copy`** are plumbing operations exposed under the `store` resource noun — fine.
- **`push`/`pull`** call into `sync::run` by constructing `SyncArgs` directly, coupling the porcelain to the plumbing's internal types.

**Recommendation:** Keep the current layering. The coupling is internal and acceptable for a single-binary CLI. If it grows, extract a `pipeline` module.

### 2. Noun vs. verb confusion at top level

The top-level commands mix two patterns:

| Pattern | Commands |
|---------|----------|
| **Verbs** (actions on the implicit "repository") | `scan`, `diff`, `verify`, `restore`, `push`, `pull`, `init`, `serve`, `watch`, `gc` |
| **Nouns** (resource management subtrees) | `store`, `remote`, `sync`, `tag` |
| **Planned verbs** | `log`, `show`, `files`, `status`, `history` |
| **Planned nouns** | `repo` |

This is the same pattern git uses (`git log` is a verb, `git remote` is a noun tree). It works well when each pattern is internally consistent. The current split is reasonable.

**However**, `sync` is now a hybrid — it lost its noun subcommands (`add/set/rm/list`) to `remote`, keeping only `config` (noun-ish) and `pull/push` (verbs). This makes `sync` an awkward residual.

**Recommendation:** Consider one of:
- **(a)** Merge `sync config` into `remote config` and promote `sync pull`/`sync push` to standalone low-level verbs. Then `sync` as a subtree can be removed entirely.
- **(b)** Keep `sync` as the plumbing counterpart to the porcelain `push`/`pull`. This is the current state and is defensible.

Option (b) is simpler and has no migration cost. Prefer it unless the command set grows further.

### 3. Subcommand verb consistency

| Operation | `store` | `remote` | `tag` | `repo` (planned) |
|-----------|---------|----------|-------|-------------------|
| Create    | `add`   | `add`    | `set` | — |
| Update    | `set`   | `set`    | `set` | `rename` |
| Delete    | `rm`    | `rm`     | `delete` | `rm` |
| List      | `list`  | `list`   | `list`   | `list` |

Issues:
- **`tag delete` vs. `rm`**: Every other resource uses `rm`. TODO.md plans `tag rm` as the canonical form with `delete` as an alias — this should proceed.
- **`tag set` conflates create and update**: This is idempotent upsert, which is appropriate for key-value metadata. No change needed.
- **`store verify`**: This is a verb scoped under a noun, which is fine (analogous to `git remote update`). The TODO.md note about unifying `store verify` under `tome verify --store` is a good simplification for the user while keeping the subcommand as an alias.

**Recommendation:** Implement `tag rm` (alias `tag delete`). Unify `store verify` under `verify --store`.

### 4. Snapshot reference syntax gap

Snapshots are currently referenced by raw `i64` IDs. This makes `diff`, `show`, and `restore` cumbersome. The planned `@latest`, `@latest~N`, `@YYYY-MM-DD` syntax (TODO.md) is critical for usability.

This affects the design of `log`, `show`, `diff`, `restore`, `status`, and `history`. The reference parser should be a shared utility in `tome-cli`, not reimplemented per command.

**Recommendation:** Implement a `SnapshotRef` parser as a shared module. Support at minimum:
- `@latest` / `@latest~N` — relative to the most recent snapshot
- `@YYYY-MM-DD` / `@YYYY-MM-DDThh:mm` — timestamp-based lookup
- Raw `i64` — backward compatible

### 5. Query commands: `log`, `show`, `files`, `history`, `status`

These five planned commands form a coherent "query" layer. They share characteristics:
- Read-only
- Should support `--format json` for scripting
- Should support `--repo` for multi-repo databases

Their scope is clear and they map to distinct user questions:

| Command | Question it answers |
|---------|-------------------|
| `log` | "What snapshots exist?" |
| `show` | "What happened in snapshot X?" |
| `files` | "What files are currently tracked?" |
| `history` | "When did file X change?" |
| `status` | "What changed since the last scan?" |

This is a clean set. No changes to the planned taxonomy are needed.

### 6. `--repo` default inconsistency

Most commands default `--repo` to `"default"` via `#[arg(default_value = "default")]`. But `tome.toml`'s `[scan] repo` is only read by `scan`. If a user sets `repo = "photos"` in their config, they still need `--repo photos` for `diff`, `verify`, `restore`, etc.

**Recommendation (from TODO.md):** Read `[scan] repo` (or better: a top-level `repo` key) as the default for all commands. Low priority but improves consistency.

### 7. `--format json` as a horizontal concern

The TODO.md lists `--format json` for many commands. This should be a consistent pattern:
- Enum: `text` (default), `json`, optionally `csv` for tabular output
- All structured output commands should support it
- JSON output should be stable (treat field removal as a breaking change)

**Recommendation:** Define a shared `OutputFormat` enum and argument definition. Apply to all query and action commands that produce structured output.

### 8. `init` scope

`init` currently only registers the machine with a remote server (`POST /machines`). But "init" in most tools means "initialize a new project/database". Consider whether `init` should also:
- Create `tome.db` if absent
- Generate a default `tome.toml`
- Prompt for repository name

Currently `tome scan` implicitly creates the database. This is convenient but invisible. A dedicated `init` for local setup would make the database creation explicit.

**Recommendation:** Keep `init` as server-registration only. The implicit creation by `scan` is a feature (zero-config local use). If local init becomes needed later, add `--local` to `init` or a separate `tome setup`.

## Summary of recommendations

| # | Change | Priority | Breaking? |
|---|--------|----------|-----------|
| 1 | Implement `tag rm` (alias `delete`) | Medium | No |
| 2 | Implement snapshot reference syntax (`@latest`, `@YYYY-MM-DD`) | High | No |
| 3 | Unify `store verify` under `verify --store` (keep alias) | Medium | No |
| 4 | Shared `--repo` default from `tome.toml` | Low | No |
| 5 | Shared `OutputFormat` enum for `--format json` | Medium | No |
| 6 | Keep `sync` as plumbing (no further restructuring) | — | — |
| 7 | Implement the five query commands (`log/show/files/history/status`) | High | No |

None of the recommendations are breaking changes. The current taxonomy is sound; the main gaps are missing query commands and snapshot reference syntax, both already identified in TODO.md.

## Consequences

- The CLI will stabilize around a two-layer model: porcelain verbs (`scan`, `push`, `pull`, `log`, `show`, `status`) and noun subtrees (`store`, `remote`, `tag`, `repo`, `sync`).
- Snapshot references will become the standard way to address snapshots, replacing raw IDs in documentation and examples.
- `--format json` will enable scripting and integration with jq/automation pipelines.
