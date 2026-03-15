# ADR-006: Declarative DDL over Runtime Migrations

**Status:** Accepted  
**Date:** 2025-03 (commit `a0aaf91`)  

## Context

SeaORM migrations run at application startup (`connection::open()`). This is convenient for local SQLite but problematic for production PostgreSQL deployments:

- Lambda cold starts should not run migrations.
- Multiple Lambda instances could race on migration execution.
- Schema changes should be reviewed and applied deliberately.

## Decision

Maintain a canonical DDL file (`tome-db/schema.sql`) for use with declarative migration tools like [psqldef](https://github.com/sqldef/sqldef). Split the connection API:

- `connection::open()` — connect + migrate (CLI / local SQLite)
- `connection::connect()` — connect only (Lambda / server deployments)

Schema changes are applied externally before deployment:

```bash
psqldef -U <user> -h <host> <database> < tome-db/schema.sql
```

## Consequences

- Lambda startup is fast — no migration overhead.
- Schema changes are explicit and reviewable.
- SeaORM migrations remain the source of truth for the schema; `schema.sql` must be kept in sync manually.
- The DSQL abstraction layer (conditional FK constraints) was removed as unnecessary — psqldef handles dialect differences.
