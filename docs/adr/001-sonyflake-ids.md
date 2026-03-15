# ADR-001: Sonyflake ID Generation

**Status:** Accepted  
**Date:** 2023-09-01  

## Context

tome needs globally unique, roughly time-ordered IDs for snapshots, entries, blobs, and other entities. Auto-increment integers don't work across multiple machines syncing to a central database.

## Decision

Use [Sonyflake](https://github.com/sony/sonyflake) IDs (`i64`) composed of `(timestamp, machine_id, sequence)`.

- **Epoch**: `2023-09-01 00:00:00 UTC` (= 1,693,526,400 seconds). Fixed permanently.
- **machine_id**: `i16` (0–32767 in practice, due to PostgreSQL `SMALLINT` range). `0` is reserved for local-only use.
- IDs are zero-padded to 19 digits in DynamoDB for lexicographic ordering.

## Consequences

- Changing `start_time` or `machine_id` mid-stream breaks ID ordering and risks collisions.
- `machine_id` must be coordinated across machines (allocated via `POST /machines`).
- IDs are roughly time-ordered but not strictly monotonic across machines.
