//! Integration tests for `tome scan`.
//!
//! Each test spins up an isolated SQLite database and temporary directory,
//! then verifies that scan correctly records file state changes.

mod common;
use common::{Env, meta_count};

// ── First scan ──────────────────────────────────────────────────────────────

/// First scan with no prior history: all files should be reported as "added".
#[tokio::test]
async fn scan_first_scan_records_all_files_as_added() {
    let env = Env::new().await;
    env.write("README.txt", b"hello world");
    env.write("src/main.rs", b"fn main() {}");
    env.write("src/lib.rs", b"pub fn greet() {}");

    env.scan().await.unwrap();

    let paths = env.present_paths().await;
    assert_eq!(paths, ["README.txt", "src/lib.rs", "src/main.rs"]);

    let meta = env.last_meta().await;
    assert_eq!(meta_count(&meta, "added"), 3);
    assert_eq!(meta_count(&meta, "scanned"), 3);
    assert_eq!(meta_count(&meta, "modified"), 0);
    assert_eq!(meta_count(&meta, "deleted"), 0);
    assert_eq!(meta_count(&meta, "errors"), 0);
}

// ── Rescan without changes ───────────────────────────────────────────────────

/// Rescanning the same files without any changes should show zero delta.
#[tokio::test]
async fn scan_rescan_unchanged_files_shows_zero_delta() {
    let env = Env::new().await;
    env.write("a.txt", b"content a");
    env.write("b.txt", b"content b");

    env.scan().await.unwrap(); // initial scan
    env.scan().await.unwrap(); // rescan — nothing changed

    let meta = env.last_meta().await;
    assert_eq!(meta_count(&meta, "added"), 0);
    assert_eq!(meta_count(&meta, "modified"), 0);
    assert_eq!(meta_count(&meta, "deleted"), 0);
    assert_eq!(meta_count(&meta, "unchanged"), 2);
    assert_eq!(meta_count(&meta, "scanned"), 2);

    // Entry cache still reflects both files.
    assert_eq!(env.present_paths().await, ["a.txt", "b.txt"]);
}

// ── Add a new file ───────────────────────────────────────────────────────────

/// Adding a file between scans shows exactly one new "added" entry.
#[tokio::test]
async fn scan_detects_added_file_on_rescan() {
    let env = Env::new().await;
    env.write("existing.txt", b"already here");

    env.scan().await.unwrap(); // baseline

    env.write("new.txt", b"just arrived");

    env.scan().await.unwrap();

    let meta = env.last_meta().await;
    assert_eq!(meta_count(&meta, "added"), 1);
    assert_eq!(meta_count(&meta, "unchanged"), 1);
    assert_eq!(meta_count(&meta, "modified"), 0);
    assert_eq!(meta_count(&meta, "deleted"), 0);

    assert_eq!(env.present_paths().await, ["existing.txt", "new.txt"]);
}

// ── Modified file ─────────────────────────────────────────────────────────────

/// Overwriting a file's content between scans shows exactly one "modified" entry.
#[tokio::test]
async fn scan_detects_modified_file_content() {
    let env = Env::new().await;
    env.write("data.txt", b"original content");

    env.scan().await.unwrap();

    env.write("data.txt", b"completely different content");

    env.scan().await.unwrap();

    let meta = env.last_meta().await;
    assert_eq!(meta_count(&meta, "modified"), 1);
    assert_eq!(meta_count(&meta, "added"), 0);
    assert_eq!(meta_count(&meta, "deleted"), 0);

    // Only one entry should remain — the modified file.
    assert_eq!(env.present_paths().await, ["data.txt"]);
}

// ── Deleted file ─────────────────────────────────────────────────────────────

/// Removing a file between scans shows exactly one "deleted" entry.
/// The deleted file disappears from present_entries.
#[tokio::test]
async fn scan_detects_deleted_file() {
    let env = Env::new().await;
    env.write("keep.txt", b"this stays");
    env.write("remove.txt", b"this will be deleted");

    env.scan().await.unwrap();
    assert_eq!(env.present_paths().await, ["keep.txt", "remove.txt"]);

    env.remove("remove.txt");

    env.scan().await.unwrap();

    let meta = env.last_meta().await;
    assert_eq!(meta_count(&meta, "deleted"), 1);
    assert_eq!(meta_count(&meta, "scanned"), 1);
    assert_eq!(meta_count(&meta, "added"), 0);
    assert_eq!(meta_count(&meta, "modified"), 0);

    // Only the surviving file remains in the cache.
    assert_eq!(env.present_paths().await, ["keep.txt"]);
}

// ── Nested directories ────────────────────────────────────────────────────────

/// Files in nested subdirectories are tracked with their relative paths.
#[tokio::test]
async fn scan_tracks_nested_directory_structure() {
    let env = Env::new().await;
    env.write("top.txt", b"root level");
    env.write("a/b/c/deep.txt", b"deeply nested");
    env.write("a/mid.txt", b"middle level");

    env.scan().await.unwrap();

    assert_eq!(env.present_paths().await, ["a/b/c/deep.txt", "a/mid.txt", "top.txt"]);
    assert_eq!(meta_count(&env.last_meta().await, "added"), 3);
}

// ── Snapshot count ────────────────────────────────────────────────────────────

/// Each call to scan creates a new snapshot, building a history chain.
#[tokio::test]
async fn scan_creates_one_snapshot_per_call() {
    let env = Env::new().await;
    env.write("file.txt", b"v1");

    env.scan().await.unwrap();
    assert_eq!(env.snapshots().await.len(), 1);

    env.write("file.txt", b"v2");
    env.scan().await.unwrap();
    assert_eq!(env.snapshots().await.len(), 2);

    env.write("file.txt", b"v3");
    env.scan().await.unwrap();
    assert_eq!(env.snapshots().await.len(), 3);
}
