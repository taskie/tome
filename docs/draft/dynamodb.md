# DynamoDB as Central Persistence Layer

> Draft: design notes for replacing PostgreSQL with DynamoDB on the central (remote) side.

---

## Motivation

The current central deployment runs **Aurora PostgreSQL** (or DSQL) behind a **Lambda Function URL**.
PostgreSQL is powerful but introduces operational overhead:

- Schema migrations (SeaORM or psqldef) must be coordinated with deploys
- Connection pooling is necessary for Lambda (RDS Proxy / PgBouncer)
- Aurora Serverless v2 has a minimum cost (~$43/month for 0.5 ACU)

DynamoDB is a natural fit for a Lambda-based architecture:

- **Zero connection management** — HTTP-based, no connection pools
- **Pay-per-request** — true scale-to-zero for low-traffic personal use
- **No schema migrations** — schema is implicit in application code
- **IAM-native auth** — Lambda execution role, no DB credentials to manage
- **Single-digit-ms latency** for key-value lookups

The trade-off is that DynamoDB requires careful access-pattern-driven design
and does not support ad-hoc JOINs or complex queries.

---

## Access Pattern Analysis

### Sync Push (write-heavy, most critical)

| # | Operation | Current SQL | Frequency |
|---|-----------|-------------|-----------|
| P1 | Get or create repository by name | `SELECT … WHERE name = ?` / `INSERT` | 1× per push |
| P2 | Idempotency check | `SELECT … WHERE repo_id = ? AND source_machine_id = ? AND source_snapshot_id = ?` | 1× per push |
| P3 | Get latest snapshot (for parent_id) | `SELECT … WHERE repository_id = ? ORDER BY created_at DESC LIMIT 1` | 1× per push |
| P4 | Create snapshot | `INSERT INTO snapshots` | 1× per push |
| P5 | Get or create blob (by digest) | `SELECT … WHERE digest = ?` / `INSERT` | N× per entry |
| P6 | Insert entry | `INSERT INTO entries` | N× per entry |
| P7 | Upsert entry_cache | `INSERT … ON CONFLICT (repo_id, path) DO UPDATE` | N× per entry |
| P8 | Get or create store (by name) | `SELECT … WHERE name = ?` / `INSERT` | M× per replica |
| P9 | Check replica exists | `SELECT … WHERE blob_id = ? AND store_id = ?` | M× per replica |
| P10 | Insert replica | `INSERT INTO replicas` | M× per replica |

### Sync Pull (read-heavy)

| # | Operation | Current SQL | Frequency |
|---|-----------|-------------|-----------|
| L1 | Find repository by name | `SELECT … WHERE name = ?` | 1× per pull |
| L2 | List snapshots after ID | `SELECT … WHERE repo_id = ? AND id > ? ORDER BY created_at` | 1× per pull |
| L3 | List entries for snapshot (with blob) | `SELECT e.*, b.* FROM entries e LEFT JOIN blobs b …` | K× per snapshot |
| L4 | List replicas for blob IDs | `SELECT r.*, s.* FROM replicas r JOIN stores s … WHERE blob_id IN (?)` | K× per snapshot |

### Web UI (read-only, lower priority)

| # | Operation | Notes |
|---|-----------|-------|
| W1 | List repositories | Scan all repos |
| W2 | List snapshots for repo | Paginated, ordered by created_at DESC |
| W3 | List entries for snapshot | Filtered by prefix, paginated |
| W4 | List entry_cache (current files) | Filtered by prefix, paginated |
| W5 | Get blob by digest | Single-item lookup |
| W6 | Path history | Entries across snapshots for a given (repo, path) |
| W7 | Cross-repo diff (entry_cache) | Two repo scans + compare |
| W8 | List stores, machines, tags, sync_peers | Small collections |

### Machine Management

| # | Operation | Notes |
|---|-----------|-------|
| M1 | Register machine | `INSERT INTO machines` |
| M2 | List machines | Scan all |
| M3 | Update machine last_seen_at | Single-item update |

---

## Table Design

### Single-Table Design

A single DynamoDB table with a **composite primary key** (`PK`, `SK`) and **GSIs**
for secondary access patterns. Item types are distinguished by PK/SK prefix conventions.

**Table name**: `tome`

```
PK                              SK                          Item Type
─────────────────────────────── ─────────────────────────── ──────────────────
REPO#<name>                     #META                       Repository
REPO#<name>                     SNAP#<snap_id>              Snapshot
REPO#<name>                     CACHE#<path>                EntryCache
SNAP#<snap_id>                  ENTRY#<path>                Entry
BLOB#<digest_hex>               #META                       Blob
BLOB#<digest_hex>               REPLICA#<store_name>        Replica
BLOB#<digest_hex>               TAG#<key>                   Tag
STORE#<name>                    #META                       Store
MACHINE#<machine_id>            #META                       Machine
```

#### Key design rationale

| Access pattern | DynamoDB operation | Key usage |
|---------------|-------------------|-----------|
| P1: Get repo by name | `GetItem(PK=REPO#name, SK=#META)` | Direct lookup |
| P2: Idempotency check | `Query(GSI1, PK=REPO#name#SRC#mid, SK=src_snap_id)` | GSI1 |
| P3: Latest snapshot | `Query(PK=REPO#name, SK begins_with SNAP#, ScanIndexForward=false, Limit=1)` | Main table, reverse |
| P4: Create snapshot | `PutItem(PK=REPO#name, SK=SNAP#snap_id)` | Direct write |
| P5: Get blob by digest | `GetItem(PK=BLOB#digest, SK=#META)` | Direct lookup |
| P6: Insert entry | `PutItem(PK=SNAP#snap_id, SK=ENTRY#path)` | Direct write |
| P7: Upsert cache | `PutItem(PK=REPO#name, SK=CACHE#path)` | Overwrites existing |
| P8: Get store by name | `GetItem(PK=STORE#name, SK=#META)` | Direct lookup |
| P9: Check replica | `GetItem(PK=BLOB#digest, SK=REPLICA#store_name)` | Direct lookup |
| P10: Insert replica | `PutItem(PK=BLOB#digest, SK=REPLICA#store_name)` | Direct write |
| L2: Snapshots after | `Query(PK=REPO#name, SK > SNAP#after_id)` | Range query (Sonyflake IDs are time-ordered) |
| L3: Entries for snapshot | `Query(PK=SNAP#snap_id, SK begins_with ENTRY#)` | Collection query |
| L4: Replicas for blob | `Query(PK=BLOB#digest, SK begins_with REPLICA#)` | Collection query |
| W4: Entry cache by prefix | `Query(PK=REPO#name, SK between CACHE#prefix… and CACHE#prefix~)` | Prefix range |
| W6: Path history | `Query(GSI2, PK=REPO#name#PATH#path)` | GSI2 |

### GSI Design

#### GSI1 — Sync Provenance (Idempotency)

Used for P2: find snapshot by `(repo, source_machine_id, source_snapshot_id)`.

```
GSI1PK:  REPO#<name>#SRC#<source_machine_id>
GSI1SK:  <source_snapshot_id>
```

Only projected on items where `source_machine_id` is set (sparse index).

#### GSI2 — Path History

Used for W6: find all entries for a given `(repo, path)` across snapshots.

```
GSI2PK:  REPO#<repo_name>#PATH#<path>
GSI2SK:  <snap_id>
```

Written on Entry items. Enables "file history" queries without scanning all snapshots.

#### GSI3 — Entity Type Index (for listing / Web UI)

Used for W1, W8: list all items of a given type.

```
GSI3PK:  _TYPE#<type>          (e.g., _TYPE#REPO, _TYPE#STORE, _TYPE#MACHINE)
GSI3SK:  <name or id>
```

Written on all `#META` items. Low cardinality, suitable for small admin collections.

---

## Item Schemas

### Repository (`REPO#<name>`, `#META`)

```jsonc
{
  "PK": "REPO#default",
  "SK": "#META",
  "id": 123456789,               // Sonyflake (kept for compatibility with sync protocol)
  "name": "default",
  "description": "",
  "config": {},
  "created_at": "2025-01-01T00:00:00Z",
  "updated_at": "2025-01-01T00:00:00Z",
  "GSI3PK": "_TYPE#REPO",
  "GSI3SK": "default"
}
```

### Snapshot (`REPO#<name>`, `SNAP#<id>`)

```jsonc
{
  "PK": "REPO#default",
  "SK": "SNAP#0003456789",        // Zero-padded Sonyflake for lexicographic ordering
  "id": 3456789,
  "parent_id": 3456780,
  "message": "scan 2025-01-15",
  "metadata": {},
  "source_machine_id": 1,
  "source_snapshot_id": 9999999,
  "created_at": "2025-01-15T12:00:00Z",
  // GSI1 (sparse — only if source_machine_id is set)
  "GSI1PK": "REPO#default#SRC#1",
  "GSI1SK": "9999999"
}
```

> **Sonyflake ID padding**: IDs are zero-padded to 19 digits (`i64::MAX` = 9223372036854775807)
> so that DynamoDB string sort order matches numeric order. This enables
> `SK > SNAP#<after_id>` range queries for incremental sync pull.

### Entry (`SNAP#<id>`, `ENTRY#<path>`)

```jsonc
{
  "PK": "SNAP#0003456789",
  "SK": "ENTRY#src/main.rs",
  "id": 4567890,
  "status": 1,
  "blob_digest": "abcdef0123456789…",  // Hex string (denormalized from blob)
  "blob_size": 2048,
  "blob_fast_digest": 1234567890,
  "mode": 33188,
  "mtime": "2025-01-15T11:30:00Z",
  "created_at": "2025-01-15T12:00:00Z",
  // GSI2 (path history)
  "GSI2PK": "REPO#default#PATH#src/main.rs",
  "GSI2SK": "0003456789",
  // Repo name stored for GSI2 (entry doesn't natively know its repo)
  "repo_name": "default"
}
```

> **Denormalized blob fields**: `blob_digest`, `blob_size`, `blob_fast_digest` are copied
> from the Blob item into each Entry. This eliminates the need for JOINs during sync pull
> (the current PostgreSQL `entries_with_digest` LEFT JOIN becomes a single Query).

### EntryCache (`REPO#<name>`, `CACHE#<path>`)

```jsonc
{
  "PK": "REPO#default",
  "SK": "CACHE#src/main.rs",
  "snapshot_id": 3456789,
  "entry_id": 4567890,
  "status": 1,
  "blob_digest": "abcdef0123456789…",
  "blob_size": 2048,
  "blob_fast_digest": 1234567890,
  "mtime": "2025-01-15T11:30:00Z",
  "updated_at": "2025-01-15T12:00:00Z"
}
```

### Blob (`BLOB#<digest_hex>`, `#META`)

```jsonc
{
  "PK": "BLOB#abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789",
  "SK": "#META",
  "id": 5678901,
  "size": 2048,
  "fast_digest": 1234567890,
  "created_at": "2025-01-15T12:00:00Z"
}
```

### Replica (`BLOB#<digest_hex>`, `REPLICA#<store_name>`)

```jsonc
{
  "PK": "BLOB#abcdef01…",
  "SK": "REPLICA#my-s3-store",
  "id": 6789012,
  "store_name": "my-s3-store",
  "store_url": "s3://my-bucket/tome",
  "path": "objects/ab/cd/abcdef01…",
  "encrypted": true,
  "verified_at": null,
  "created_at": "2025-01-15T12:00:00Z"
}
```

> **Store info denormalized**: `store_name` and `store_url` are copied into the Replica item.
> The separate Store item still exists for admin/listing, but sync pull can read replicas
> without a secondary lookup.

### Store (`STORE#<name>`, `#META`)

```jsonc
{
  "PK": "STORE#my-s3-store",
  "SK": "#META",
  "id": 7890123,
  "name": "my-s3-store",
  "url": "s3://my-bucket/tome",
  "config": {},
  "created_at": "2025-01-15T12:00:00Z",
  "updated_at": "2025-01-15T12:00:00Z",
  "GSI3PK": "_TYPE#STORE",
  "GSI3SK": "my-s3-store"
}
```

### Machine (`MACHINE#<machine_id>`, `#META`)

```jsonc
{
  "PK": "MACHINE#00001",
  "SK": "#META",
  "machine_id": 1,
  "name": "laptop-a",
  "description": "",
  "last_seen_at": "2025-01-15T12:00:00Z",
  "created_at": "2025-01-01T00:00:00Z",
  "GSI3PK": "_TYPE#MACHINE",
  "GSI3SK": "laptop-a"
}
```

---

## Sync Flow with DynamoDB

### Push Flow

```
Client                              Lambda + DynamoDB
──────                              ─────────────────
POST /sync/push?repo=default
  { source_machine_id: 1,
    source_snapshot_id: "999",
    entries: [...],
    replicas: [...] }
                                    1. GetItem(REPO#default, #META)
                                       → repo exists? create if not

                                    2. Query(GSI1, REPO#default#SRC#1, SK="999")
                                       → idempotency check

                                    3. Query(REPO#default, SK begins_with SNAP#,
                                            reverse, limit 1)
                                       → latest snapshot (parent_id)

                                    4. PutItem(REPO#default, SNAP#<new_id>)

                                    5. For each entry:
                                       a. GetItem(BLOB#<digest>, #META)
                                          → create if not exists (PutItem, condition)
                                       b. PutItem(SNAP#<new_id>, ENTRY#<path>)
                                          (with denormalized blob fields + GSI2 attrs)
                                       c. PutItem(REPO#default, CACHE#<path>)

                                    6. For each replica:
                                       a. PutItem(BLOB#<digest>, REPLICA#<store>)
                                          (conditional: attribute_not_exists)
                                       b. PutItem(STORE#<store>, #META)
                                          (conditional: attribute_not_exists)

                                    → { snapshot_id: "<new_id>" }
```

### Pull Flow

```
Client                              Lambda + DynamoDB
──────                              ─────────────────
GET /sync/pull?repo=default
    &after=0003456780
                                    1. GetItem(REPO#default, #META)
                                       → 404 if not found

                                    2. Query(REPO#default,
                                            SK > SNAP#0003456780,
                                            SK begins_with SNAP#)
                                       → all new snapshots (in order)

                                    3. For each snapshot:
                                       a. Query(SNAP#<id>, SK begins_with ENTRY#)
                                          → entries with denormalized blob fields
                                       b. Collect unique digests
                                       c. For each digest:
                                          Query(BLOB#<digest>, SK begins_with REPLICA#)
                                          → replicas (with denormalized store info)

                                    → { snapshots: [...] }
```

> **BatchGetItem optimization** (step 3c): Instead of querying replicas per-digest,
> collect all unique digests and use `BatchGetItem` if only checking existence,
> or parallel `Query` calls for replica collections. DynamoDB supports up to
> 100 items per `BatchGetItem` and auto-parallelizes internally.

---

## Write Patterns and Consistency

### Conditional Writes

DynamoDB conditional expressions replace SQL `INSERT … ON CONFLICT`:

| Pattern | DynamoDB |
|---------|----------|
| Get-or-create blob | `PutItem` with `attribute_not_exists(PK)` — succeeds if new, ConditionalCheckFailed if exists |
| Upsert entry_cache | `PutItem` — unconditional overwrite (same as `ON CONFLICT DO UPDATE`) |
| Idempotent replica | `PutItem` with `attribute_not_exists(PK)` |
| Idempotent store | `PutItem` with `attribute_not_exists(PK)` |

### Transaction Support

DynamoDB `TransactWriteItems` supports up to **100 items per transaction** across any tables.

For a push with many entries, batch writes without transactions should suffice
(the push endpoint is already effectively idempotent). If atomicity is required
for the snapshot + entries, `TransactWriteItems` can group:
- 1 snapshot item
- Up to 99 entry items per transaction call

For pushes exceeding 100 entries, a two-phase approach:
1. Write entries in batches (idempotent PutItem)
2. Write snapshot last (marks the batch as committed)
3. Pull queries filter on snapshot existence

### Consistency

- **Reads**: DynamoDB defaults to **eventually consistent** reads.
  Use `ConsistentRead: true` for the idempotency check (P2) and latest-snapshot query (P3).
- **Writes**: All single-item writes are **strongly consistent**.
- **GSI reads**: Always eventually consistent (DynamoDB limitation).
  For GSI1 (idempotency), a brief race window exists but is acceptable
  because duplicate pushes are harmless (they create an extra snapshot that
  can be deduplicated later, or the client retries).

---

## Capacity and Cost Estimation

### On-Demand (Pay-per-Request) Pricing

| Operation | WCU/RCU | Cost per 1M |
|-----------|---------|-------------|
| Write (≤1KB) | 1 WCU | $1.25 |
| Strongly consistent read (≤4KB) | 1 RCU | $0.25 |
| Eventually consistent read (≤4KB) | 0.5 RCU | $0.125 |

### Per-Push Cost Estimate

Assumptions: 1 push = 500 changed files, 200 unique blobs, 100 replicas.

| Operation | Count | WCU | RCU |
|-----------|-------|-----|-----|
| Get repo | 1 | — | 1 |
| Idempotency check (GSI, EC) | 1 | — | 0.5 |
| Latest snapshot (consistent) | 1 | — | 1 |
| Create snapshot | 1 | 1 | — |
| Get-or-create blobs | 200 | 200 | 200 |
| Insert entries | 500 | 500 | — |
| Upsert cache | 500 | 500 | — |
| Check+insert replicas | 100 | 100 | 100 |
| Get+create stores | ~3 | 3 | 3 |
| **Total** | | **~1304 WCU** | **~306 RCU** |

**Cost per push**: ~$0.002 (negligible for personal use).

### Storage

- 25 GB free tier
- $0.25/GB/month beyond that
- Each entry item ≈ 200–500 bytes; 1M entries ≈ 200–500 MB

### Comparison with Aurora Serverless v2

| | DynamoDB On-Demand | Aurora Serverless v2 |
|---|---|---|
| Minimum monthly cost | ~$0 (within free tier) | ~$43 (0.5 ACU minimum) |
| Connection management | None (HTTP API) | RDS Proxy required for Lambda |
| Schema migrations | None | psqldef or SeaORM |
| Complex queries (JOIN, aggregate) | Not supported | Full SQL |
| Latency (single-item) | 1–5 ms | 2–10 ms (via proxy) |

---

## Implementation Approach

### New Crate: `tome-dynamo`

A new crate parallel to `tome-db` that implements the same operations against DynamoDB.

```
tome-dynamo/
  src/
    lib.rs          — public API (mirrors tome-db::ops interface)
    client.rs       — DynamoDB client setup (aws-sdk-dynamodb)
    keys.rs         — PK/SK construction helpers
    ops/
      repository.rs
      snapshot.rs
      entry.rs
      blob.rs
      replica.rs
      store.rs
      machine.rs
      entry_cache.rs
      sync.rs        — high-level sync push/pull orchestration
```

### Trait Abstraction (`tome-db` refactor)

To support both PostgreSQL and DynamoDB backends, extract a trait:

```rust
// tome-db/src/traits.rs (or a new tome-ops crate)
#[async_trait]
pub trait MetadataStore: Send + Sync {
    // Repository
    async fn get_or_create_repository(&self, name: &str) -> Result<Repository>;

    // Snapshot
    async fn create_snapshot(&self, repo_id: i64, parent: Option<i64>, msg: &str) -> Result<Snapshot>;
    async fn snapshots_after(&self, repo_id: i64, after: Option<i64>) -> Result<Vec<Snapshot>>;
    async fn latest_snapshot(&self, repo_id: i64) -> Result<Option<Snapshot>>;
    async fn find_snapshot_by_source(&self, repo_id: i64, mid: i16, sid: i64) -> Result<Option<Snapshot>>;

    // Blob
    async fn get_or_create_blob(&self, hash: &FileHash) -> Result<Blob>;
    async fn find_blob_by_digest(&self, digest: &[u8]) -> Result<Option<Blob>>;

    // Entry
    async fn insert_entry_present(&self, snap_id: i64, path: &str, blob_id: i64, mode: Option<i32>, mtime: Option<DateTimeFixedOffset>) -> Result<Entry>;
    async fn insert_entry_deleted(&self, snap_id: i64, path: &str) -> Result<Entry>;
    async fn entries_with_digest(&self, snap_id: i64, prefix: &str) -> Result<Vec<(Entry, Option<Blob>)>>;

    // EntryCache
    async fn upsert_cache_present(&self, params: UpsertCachePresentParams) -> Result<()>;
    async fn upsert_cache_deleted(&self, repo_id: i64, path: &str, snap_id: i64, entry_id: i64) -> Result<()>;

    // Replica
    async fn replicas_for_blobs(&self, blob_ids: &[i64]) -> Result<Vec<(Replica, Store)>>;
    async fn replica_exists(&self, blob_id: i64, store_id: i64) -> Result<bool>;
    async fn insert_replica(&self, blob_id: i64, store_id: i64, path: &str, encrypted: bool) -> Result<()>;

    // Store
    async fn get_or_create_store(&self, name: &str, url: &str, config: Value) -> Result<Store>;

    // Machine
    async fn register_machine(&self, name: &str) -> Result<Machine>;
    async fn list_machines(&self) -> Result<Vec<Machine>>;
}
```

### Server Changes

`tome-server` would accept `Arc<dyn MetadataStore>` instead of `DatabaseConnection`:

```rust
// tome-server/src/server.rs
pub fn build_router(store: Arc<dyn MetadataStore>) -> Router {
    Router::new()
        .route("/sync/pull", get(sync::pull))
        .route("/sync/push", post(sync::push))
        // ...
        .with_state(store)
}
```

### Feature Flags

```toml
# tome-server/Cargo.toml
[features]
default = ["postgres"]
postgres = ["tome-db"]
dynamodb = ["tome-dynamo"]
lambda = ["lambda_http", "lambda_runtime"]
```

### Terraform (tome-tf)

```hcl
resource "aws_dynamodb_table" "tome" {
  name         = "tome"
  billing_mode = "PAY_PER_REQUEST"
  hash_key     = "PK"
  range_key    = "SK"

  attribute {
    name = "PK"
    type = "S"
  }
  attribute {
    name = "SK"
    type = "S"
  }
  attribute {
    name = "GSI1PK"
    type = "S"
  }
  attribute {
    name = "GSI1SK"
    type = "S"
  }
  attribute {
    name = "GSI2PK"
    type = "S"
  }
  attribute {
    name = "GSI2SK"
    type = "S"
  }
  attribute {
    name = "GSI3PK"
    type = "S"
  }
  attribute {
    name = "GSI3SK"
    type = "S"
  }

  global_secondary_index {
    name            = "GSI1"
    hash_key        = "GSI1PK"
    range_key       = "GSI1SK"
    projection_type = "ALL"
  }

  global_secondary_index {
    name            = "GSI2"
    hash_key        = "GSI2PK"
    range_key       = "GSI2SK"
    projection_type = "ALL"
  }

  global_secondary_index {
    name            = "GSI3"
    hash_key        = "GSI3PK"
    range_key       = "GSI3SK"
    projection_type = "KEYS_ONLY"
  }

  point_in_time_recovery {
    enabled = true
  }

  tags = {
    Service = "tome"
  }
}
```

---

## Trade-offs

### Advantages over PostgreSQL

| Aspect | DynamoDB | PostgreSQL |
|--------|----------|------------|
| **Operational cost (low traffic)** | ~$0/month (free tier) | ~$43+/month (Aurora min) |
| **Connection management** | None | RDS Proxy needed for Lambda |
| **Schema evolution** | Add attributes freely | Migration required |
| **Lambda cold start** | No connection overhead | TCP + TLS + auth handshake |
| **Scaling** | Automatic, unlimited | ACU scaling (0.5–128) |

### Disadvantages

| Aspect | DynamoDB | PostgreSQL |
|--------|----------|------------|
| **Ad-hoc queries** | Requires pre-planned GSIs | Full SQL |
| **JOINs** | Must denormalize | Native |
| **Transactions** | 100-item limit per tx | Unlimited |
| **Data export / analytics** | Awkward (DynamoDB Streams → S3) | pg_dump, SQL |
| **Local development** | DynamoDB Local (Docker) | SQLite (existing) |
| **Item size limit** | 400 KB per item | No practical limit |
| **Consistency for GSI** | Eventually consistent only | Strong |

### Risks and Mitigations

| Risk | Mitigation |
|------|-----------|
| **Large snapshots (>100K entries)** | BatchWriteItem (25 items/batch), parallel batches |
| **Hot partition (single repo)** | Sonyflake IDs distribute well; DynamoDB adaptive capacity handles bursts |
| **GSI eventual consistency** | Use `ConsistentRead` on main table for critical reads; GSI only for non-critical |
| **Entry denormalization drift** | Blob fields are write-once (content-addressed); no drift possible |
| **Future query requirements** | Export to S3 + Athena for analytics; keep PostgreSQL path as fallback |

---

## Migration Path

### Phase 1: Trait Extraction

- Extract `MetadataStore` trait from current `tome-db::ops` functions
- Implement trait for `SeaOrmStore(DatabaseConnection)` (wraps existing code)
- Update `tome-server` to use `Arc<dyn MetadataStore>`
- **No behavioral change** — all existing tests pass

### Phase 2: DynamoDB Implementation

- Create `tome-dynamo` crate implementing `MetadataStore`
- Use `aws-sdk-dynamodb` (already in workspace via aws-sdk-s3 transitive)
- Add integration tests with DynamoDB Local
- Feature-flag in `tome-server`

### Phase 3: Lambda Deployment

- Update `tome-lambda.rs` to construct DynamoDB-backed store
- Update Terraform to provision DynamoDB table + GSIs
- Lambda IAM role: `dynamodb:GetItem`, `dynamodb:PutItem`, `dynamodb:Query`, `dynamodb:BatchWriteItem`
- Remove RDS Proxy / Aurora from stack

### Phase 4: Cleanup (Optional)

- PostgreSQL path remains available for self-hosted / on-premise deployments
- DynamoDB becomes the default for AWS Lambda deployments

---

## Open Questions

1. **GC (garbage collection)**: Current GC deletes `entry_cache → entries → snapshots` in FK order.
   DynamoDB has no FK constraints — GC needs to enumerate and delete items explicitly.
   Should GC be a separate Lambda (scheduled) or a CLI command against DynamoDB?

2. **Path history (W6)**: GSI2 requires writing `repo_name` into every Entry item.
   The repo name is known at write time (from the push request). Is the GSI worth the
   storage overhead, or should path history be deferred to a future export-to-Athena approach?

3. **Web UI queries**: The Web UI currently relies on SeaORM queries with flexible filtering.
   With DynamoDB, some queries (e.g., cross-repo diff) become more expensive.
   Should the Web UI be backed by a read replica (DynamoDB → S3 → Athena) for complex queries?

4. **Sonyflake ID padding width**: 19 digits covers the full `i64` range but adds overhead.
   Since Sonyflake IDs in practice are much smaller, a shorter padding (e.g., 16 digits) could
   save ~3 bytes per SK. Worth the compatibility risk?

5. **Multi-repo isolation**: Should each repository get its own DynamoDB table,
   or is the single-table design with `REPO#<name>` prefix sufficient?
   Single-table is simpler to manage; separate tables offer better isolation and
   independent deletion.
