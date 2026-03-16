//! PK/SK key helper functions for single-table design.

/// Sonyflake IDs are zero-padded to 19 digits for lexicographic ordering.
pub fn pad_id(id: i64) -> String {
    format!("{:019}", id)
}

pub fn repo_pk(name: &str) -> String {
    format!("REPO#{name}")
}

pub fn snap_pk(snap_id: i64) -> String {
    format!("SNAP#{}", pad_id(snap_id))
}

pub fn obj_pk(digest_hex: &str) -> String {
    format!("OBJ#{digest_hex}")
}

pub fn store_pk(name: &str) -> String {
    format!("STORE#{name}")
}

pub fn machine_pk(machine_id: i16) -> String {
    format!("MACHINE#{machine_id}")
}

pub const META_SK: &str = "#META";

pub fn snap_sk(snap_id: i64) -> String {
    format!("SNAP#{}", pad_id(snap_id))
}

pub fn cache_sk(path: &str) -> String {
    format!("CACHE#{path}")
}

pub fn entry_sk(path: &str) -> String {
    format!("ENTRY#{path}")
}

pub fn replica_sk(store_name: &str) -> String {
    format!("REPLICA#{store_name}")
}

// GSI1: idempotency check
pub fn gsi1pk_source(repo_name: &str, machine_id: i16) -> String {
    format!("REPO#{repo_name}#SRC#{machine_id}")
}

pub fn gsi1sk_source(source_snapshot_id: i64) -> String {
    pad_id(source_snapshot_id)
}

// GSI2: path history
pub fn gsi2pk_path(repo_name: &str, path: &str) -> String {
    format!("REPO#{repo_name}#PATH#{path}")
}

pub fn gsi2sk_snap(snap_id: i64) -> String {
    pad_id(snap_id)
}

// GSI3: entity type listing
pub fn gsi3pk_type(type_name: &str) -> String {
    format!("_TYPE#{type_name}")
}
