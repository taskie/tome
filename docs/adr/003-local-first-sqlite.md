# ADR-003: Local-First with SQLite

**Status:** Accepted  
**Date:** 2023-09-01  

## Context

tome is a personal file tracking tool. Users should be able to start using it immediately without setting up a server or database.

## Decision

SQLite is a first-class citizen. The default `--db` is `tome.db` in the current directory. All features work with SQLite alone; PostgreSQL and DynamoDB are optional sync targets.

- `connection::open()` — connect + run SeaORM migrations (used by CLI)
- `connection::connect()` — connect only, no migrations (used by Lambda / server where schema is managed externally)

## Consequences

- Every machine has its own complete database — no network dependency for local operations.
- Sync is additive: `sync push/pull` copies snapshots between local SQLite and a remote backend.
- Some features (complex queries, full-text search) may be limited by SQLite's capabilities.
