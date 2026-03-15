# ADR-005: DynamoDB Single-Table Design

**Status:** Accepted  
**Date:** 2025-03 (commit `63c949b`)  

## Context

The central sync backend (Lambda + Aurora PostgreSQL) had operational overhead: connection pooling, schema migrations, and a minimum cost of ~$43/month. For a personal tool with low traffic, DynamoDB offers true scale-to-zero with pay-per-request billing.

## Decision

Implement a DynamoDB backend (`tome-dynamo`) using single-table design with composite keys and 3 GSIs.

### Key Schema

| Entity | PK | SK |
|--------|----|----|
| Repository | `REPO#<name>` | `#META` |
| Snapshot | `REPO#<name>` | `SNAP#<zero_padded_id>` |
| Entry | `SNAP#<id>` | `ENTRY#<path>` |
| Entry Cache | `REPO#<name>` | `CACHE#<path>` |
| Blob | `BLOB#<digest_hex>` | `#META` |
| Store | `STORE#<name>` | `#META` |
| Replica | `STORE#<name>` | `REPLICA#<blob_digest>` |
| Tag | `BLOB#<digest_hex>` | `TAG#<key>` |
| Sync Peer | `REPO#<name>` | `PEER#<peer_name>` |
| Machine | `MACHINE#<id>` | `#META` |

### GSIs

| GSI | PK | SK | Purpose |
|-----|----|----|---------|
| GSI1 | `REPO#<name>#SRC#<machine_id>` | `<source_snapshot_id>` | Sync idempotency check |
| GSI2 | `REPO#<name>#PATH#<path>` | `<snap_id>` | Path history across snapshots |
| GSI3 | `_TYPE#<type>` | `<name_or_id>` | Entity type listing (all repos, stores, machines) |

All GSIs use `ALL` projection.

## Consequences

- Zero connection management — HTTP-based, no connection pools needed for Lambda.
- Pay-per-request billing — true scale-to-zero.
- No schema migrations — schema is implicit in application code.
- `entries_for_blob` is not efficiently supported (returns error).
- `blobs_by_ids` uses GSI3 scan + filter (no dedicated BLOB_ID index yet).
- `resolve_repo_name(repository_id)` requires a GSI3 scan; could benefit from caching.
- Sonyflake IDs are zero-padded to 19 digits for correct lexicographic sort order.

See [docs/arch/dynamodb.md](../arch/dynamodb.md) for the full design document.
