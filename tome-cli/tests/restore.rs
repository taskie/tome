//! Integration tests for `tome restore`.
//!
//! Tests cover: restoring files from a snapshot via a local store,
//! prefix-filtered restore, and handling of missing replicas.

mod common;
use common::Env;

// ── Full restore ────────────────────────────────────────────────────────────

/// Scan → store push → restore: all files should appear in the destination.
#[tokio::test]
async fn restore_recovers_all_files_from_store() {
    let env = Env::new().await;
    env.write("readme.txt", b"hello world");
    env.write("src/main.rs", b"fn main() {}");
    env.scan().await.unwrap();

    env.store_add_and_push("backup").await.unwrap();

    let snap_id = env.snapshots().await[0].id.to_string();
    let dest = env.root.path().join("restored");
    std::fs::create_dir_all(&dest).unwrap();

    env.restore(&snap_id, dest.clone(), Some("backup"), "").await.unwrap();

    // Both files should be restored with correct content.
    assert_eq!(std::fs::read_to_string(dest.join("readme.txt")).unwrap(), "hello world");
    assert_eq!(std::fs::read_to_string(dest.join("src/main.rs")).unwrap(), "fn main() {}");
}

// ── Prefix filter ────────────────────────────────────────────────────────────

/// `--prefix` restricts which files are restored.
#[tokio::test]
async fn restore_with_prefix_only_restores_matching_files() {
    let env = Env::new().await;
    env.write("docs/guide.md", b"# Guide");
    env.write("docs/faq.md", b"# FAQ");
    env.write("src/lib.rs", b"pub fn lib() {}");
    env.scan().await.unwrap();

    env.store_add_and_push("backup").await.unwrap();

    let snap_id = env.snapshots().await[0].id.to_string();
    let dest = env.root.path().join("restored");
    std::fs::create_dir_all(&dest).unwrap();

    env.restore(&snap_id, dest.clone(), Some("backup"), "docs/").await.unwrap();

    // Only docs/ files should be restored.
    assert!(dest.join("docs/guide.md").exists());
    assert!(dest.join("docs/faq.md").exists());
    assert!(!dest.join("src/lib.rs").exists());
}

// ── Restore from historical snapshot ─────────────────────────────────────────

/// Restore from an older snapshot recovers the content as it was at that point.
#[tokio::test]
async fn restore_historical_snapshot_recovers_old_content() {
    let env = Env::new().await;
    env.write("data.txt", b"version 1");
    env.scan().await.unwrap();
    env.store_add_and_push("backup").await.unwrap();

    let old_snap_id = env.snapshots().await[0].id.to_string();

    // Update the file and scan again.
    env.write("data.txt", b"version 2");
    env.scan().await.unwrap();

    // Push the new version too (so both blobs exist in the store).
    tome_cli::commands::store::run(
        &env.db,
        tome_cli::commands::store::StoreArgs {
            command: tome_cli::commands::store::StoreCommands::Push(tome_cli::commands::store::StorePushArgs {
                repo: "default".to_string(),
                store: Some("backup".to_string()),
                path: Some(env.files_dir()),
                encrypt: false,
                key_file: None,
                key_source: None,
                cipher: None,
            }),
        },
        &tome_cli::config::StoreConfig::default(),
    )
    .await
    .unwrap();

    let dest = env.root.path().join("restored");
    std::fs::create_dir_all(&dest).unwrap();

    // Restore from the OLD snapshot — should get "version 1", not "version 2".
    env.restore(&old_snap_id, dest.clone(), Some("backup"), "").await.unwrap();

    assert_eq!(std::fs::read_to_string(dest.join("data.txt")).unwrap(), "version 1");
}
