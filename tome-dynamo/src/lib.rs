//! DynamoDB-backed MetadataStore implementation.
//!
//! Single-table design with PK/SK patterns:
//!
//! | PK              | SK                          | Item type  |
//! |-----------------|-----------------------------|------------|
//! | REPO#\<name\>   | #META                       | Repository |
//! | REPO#\<name\>   | SNAP#\<snap_id\>            | Snapshot   |
//! | REPO#\<name\>   | CACHE#\<path\>              | EntryCache |
//! | SNAP#\<snap_id\> | ENTRY#\<path\>             | Entry      |
//! | OBJ#\<digest\>   | #META                      | Object     |
//! | OBJ#\<digest\>   | REPLICA#\<store_name\>     | Replica    |
//! | STORE#\<name\>   | #META                      | Store      |
//! | MACHINE#\<id\>   | #META                      | Machine    |
//!
//! GSIs:
//! - GSI1: idempotency check (GSI1PK = REPO#\<name\>#SRC#\<machine_id\>, GSI1SK = source_snapshot_id)
//! - GSI2: path history (GSI2PK = REPO#\<name\>#PATH#\<path\>, GSI2SK = snap_id)
//! - GSI3: entity type listing (GSI3PK = _TYPE#\<type\>, GSI3SK = name_or_id)

mod keys;
mod serde_ddb;
mod store;

pub use store::DynamoStore;
