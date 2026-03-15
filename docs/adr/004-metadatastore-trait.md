# ADR-004: MetadataStore Trait Abstraction

**Status:** Accepted  
**Date:** 2025-03 (commit `7c574ee`)  

## Context

tome-server was tightly coupled to SeaORM and `DatabaseConnection`. To support DynamoDB as an alternative backend (for serverless Lambda deployments), the server needed a database-agnostic interface.

## Decision

Extract a `MetadataStore` trait (`tome-db/src/store_trait.rs`) with ~40 async methods covering all database operations used by the HTTP API routes. Provide two implementations:

1. **`SeaOrmStore`** (`tome-db/src/sea_orm_store.rs`) — wraps `DatabaseConnection`, delegates to existing `ops::` functions.
2. **`DynamoStore`** (`tome-dynamo/src/store.rs`) — implements the trait against DynamoDB using single-table design.

The server routes accept `State<Arc<dyn MetadataStore>>` for dynamic dispatch.

## Consequences

- `sea-orm` is no longer a direct dependency of `tome-server` — fully hidden behind the trait.
- Adding new backends (e.g., FoundationDB, TiKV) requires only implementing `MetadataStore`.
- `SeaOrmStore` exposes a `connection()` accessor for CLI-only operations not in the trait (GC, scan).
- `async_trait` crate is required because native async fn in traits isn't object-safe for `dyn` dispatch.
- Double deref pattern: `&**db` is needed to go from `Arc<dyn MetadataStore>` → `&dyn MetadataStore`.
