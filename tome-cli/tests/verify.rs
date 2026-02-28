//! Integration tests for `tome verify` (bit-rot detection).
//!
//! Verify re-hashes files on disk and compares against stored digests.
//! It returns Ok when all files match, and Err when mismatches or missing files
//! are detected.

mod common;
use common::Env;

// ── Clean state ───────────────────────────────────────────────────────────────

/// Verify passes (returns Ok) immediately after a clean scan.
#[tokio::test]
async fn verify_passes_for_unchanged_files() {
    let env = Env::new().await;
    env.write("report.pdf", b"binary content here");
    env.write("config.toml", b"[settings]\nvalue = 42");
    env.scan().await.unwrap();

    env.verify().await.unwrap();
}

/// Verify also passes after a rescan that detected no changes.
#[tokio::test]
async fn verify_passes_after_unchanged_rescan() {
    let env = Env::new().await;
    env.write("file.txt", b"stable");
    env.scan().await.unwrap();
    env.scan().await.unwrap(); // second scan, still unchanged

    env.verify().await.unwrap();
}

// ── Silent modification ───────────────────────────────────────────────────────

/// Verify detects a file whose content changed after the last scan (bit-rot simulation).
/// The command returns Err to signal that the file system no longer matches the record.
#[tokio::test]
async fn verify_detects_external_modification() {
    let env = Env::new().await;
    env.write("photo.jpg", b"original jpeg bytes");
    env.scan().await.unwrap();

    // Silently overwrite the file without re-scanning — simulates bit-rot or
    // unauthorized modification.
    env.write("photo.jpg", b"corrupted jpeg bytes");

    // Verify should detect the mismatch and return Err.
    let result = env.verify().await;
    assert!(result.is_err(), "verify should fail when a file's content has changed silently");
}

/// Verify detects multiple silently modified files.
#[tokio::test]
async fn verify_detects_multiple_external_modifications() {
    let env = Env::new().await;
    env.write("a.bin", b"aaaa");
    env.write("b.bin", b"bbbb");
    env.write("c.bin", b"cccc");
    env.scan().await.unwrap();

    // Corrupt two of the three files.
    env.write("a.bin", b"corrupted");
    env.write("c.bin", b"also corrupted");

    let result = env.verify().await;
    assert!(result.is_err());
}

// ── Missing file ──────────────────────────────────────────────────────────────

/// Verify detects a file that was deleted from disk but not re-scanned.
/// (The entry cache still knows about it.)
#[tokio::test]
async fn verify_detects_missing_file() {
    let env = Env::new().await;
    env.write("important.dat", b"critical data");
    env.scan().await.unwrap();

    // Delete the file without running tome scan.
    env.remove("important.dat");

    let result = env.verify().await;
    assert!(result.is_err(), "verify should fail when a tracked file is missing from disk");
}
