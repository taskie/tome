# HTTP API

Served by `tome serve` (default: `http://127.0.0.1:8080`).
Router defined in `tome-server/src/server.rs`.

The full OpenAPI 3.0 spec is available at [`docs/schema/openapi.json`](../schema/openapi.json)
(auto-generated via `cargo run -p tome-server --example generate_openapi`).

## Endpoints

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
GET /objects/{digest}
GET /objects/{digest}/entries
GET /machines
POST /machines                         register a new machine (returns allocated machine_id)
PUT /machines/{id}                     update machine (name, description)
GET /stores
GET /tags
GET /sync-peers
GET /sync/pull                     ?repo= &after=  (incremental snapshot pull)
POST /sync/push                    ?repo=           (push snapshots, entries, replicas)
```

## Notes

- Digests are stored as binary in the DB and returned as lowercase hex strings in responses.
- `GET /diff` compares current state (`entry_cache`) across two repositories, with independent path prefixes per side. Entry keys are namespaced as `"1:{path}"` / `"2:{path}"` to avoid collisions. Limit: 10,000 entries per side.
- `GET /repositories/{name}/diff` compares two **snapshots** within one repository.
- `POST /sync/push` is idempotent: duplicate pushes from the same `(source_machine_id, source_snapshot_id)` return the existing snapshot.

## `GET /diff` Response Shape

```jsonc
{
  "repo1": { ... },
  "repo2": { ... },
  "objects": { "<object_id>": { ... } },
  "entries": { "1:<path>": { ... }, "2:<path>": { ... } },
  // object_id → ([entry_keys_in_repo1], [entry_keys_in_repo2])
  "diff": { "<object_id>": [["1:<path>"], ["2:<path>"]] },
  // Entry keys for deleted paths (status=0, object_id=null)
  "deleted": ["1:<path>", ...]
}
```

Deleted entries (status=0) are returned in the `deleted` list and also present in `entries`. They are excluded from `diff` (which is keyed by `object_id`) to keep the two concerns separate.
