//! Integration tests for `tome store`.
//!
//! Tests cover: registering a local store, pushing blobs, and verifying that
//! replica records and actual blob files are created on disk.

mod common;
use common::Env;

use tome_db::ops;

// ── Store registration ────────────────────────────────────────────────────────

/// `tome store add` registers the store in the database.
#[tokio::test]
async fn store_add_registers_in_db() {
    let env = Env::new().await;
    let store_url = format!("file://{}", env.store_dir().display());

    tome_cli::commands::store::run(
        &env.db,
        tome_cli::commands::store::StoreArgs {
            command: tome_cli::commands::store::StoreCommands::Add(tome_cli::commands::store::StoreAddArgs {
                name: "backup".to_string(),
                url: store_url,
            }),
        },
        &tome_cli::config::StoreConfig::default(),
    )
    .await
    .unwrap();

    let stores = ops::list_stores(&env.db).await.unwrap();
    assert_eq!(stores.len(), 1);
    assert_eq!(stores[0].name, "backup");
}

// ── Store push ────────────────────────────────────────────────────────────────

/// After scan + push, each unique blob has a replica record and a file on disk.
#[tokio::test]
async fn store_push_creates_replica_records_and_files() {
    let env = Env::new().await;
    env.write("document.txt", b"important document content");
    env.write("image.bin", b"binary image data");
    env.scan().await.unwrap();

    env.store_add_and_push("local").await.unwrap();

    // Two blobs (two distinct files) should each have a replica.
    let stores = ops::list_stores(&env.db).await.unwrap();
    assert_eq!(stores.len(), 1);

    let replicas = ops::replicas_in_store(&env.db, stores[0].id).await.unwrap();
    assert_eq!(replicas.len(), 2);

    // Each blob file must actually exist in the store directory.
    for replica in &replicas {
        let blob_file = env.store_dir().join(&replica.path);
        assert!(blob_file.exists(), "blob file should exist at {}", blob_file.display());
    }
}

/// Pushing the same content twice does not create duplicate blobs or replicas.
#[tokio::test]
async fn store_push_deduplicates_identical_content() {
    let env = Env::new().await;
    // Two files with identical content → they share one blob.
    env.write("copy1.txt", b"same content");
    env.write("copy2.txt", b"same content");
    env.scan().await.unwrap();

    env.store_add_and_push("local").await.unwrap();

    let stores = ops::list_stores(&env.db).await.unwrap();
    let replicas = ops::replicas_in_store(&env.db, stores[0].id).await.unwrap();

    // Identical content = one blob = one replica, even though two entries exist.
    assert_eq!(replicas.len(), 1);

    let blob_file = env.store_dir().join(&replicas[0].path);
    assert!(blob_file.exists());
}

/// Files added in a subsequent scan are pushed on the next `store push`.
#[tokio::test]
async fn store_push_incremental_scan_adds_new_replica() {
    let env = Env::new().await;
    env.write("first.txt", b"first file");
    env.scan().await.unwrap();
    env.store_add_and_push("local").await.unwrap();

    let stores = ops::list_stores(&env.db).await.unwrap();
    let count_before = ops::replicas_in_store(&env.db, stores[0].id).await.unwrap().len();
    assert_eq!(count_before, 1);

    // Add a new file and push again.
    env.write("second.txt", b"second file");
    env.scan().await.unwrap();

    // Push again — should upload only the new blob.
    tome_cli::commands::store::run(
        &env.db,
        tome_cli::commands::store::StoreArgs {
            command: tome_cli::commands::store::StoreCommands::Push(tome_cli::commands::store::StorePushArgs {
                repo: "default".to_string(),
                store: Some("local".to_string()),
                path: Some(env.files_dir()),
            }),
        },
        &tome_cli::config::StoreConfig::default(),
    )
    .await
    .unwrap();

    let count_after = ops::replicas_in_store(&env.db, stores[0].id).await.unwrap().len();
    assert_eq!(count_after, 2);
}

// ── Store verify ────────────────────────────────────────────────────────────

/// `tome store verify` passes when all blob files in the store are intact.
#[tokio::test]
async fn store_verify_passes_for_intact_store() {
    let env = Env::new().await;
    env.write("a.txt", b"content a");
    env.write("b.txt", b"content b");
    env.scan().await.unwrap();
    env.store_add_and_push("local").await.unwrap();

    env.store_verify("local").await.unwrap();
}

/// `tome store verify` detects corrupted blob files.
#[tokio::test]
async fn store_verify_detects_corrupted_blob() {
    let env = Env::new().await;
    env.write("data.txt", b"important data");
    env.scan().await.unwrap();
    env.store_add_and_push("local").await.unwrap();

    // Corrupt the blob file on disk.
    let stores = ops::list_stores(&env.db).await.unwrap();
    let replicas = ops::replicas_in_store(&env.db, stores[0].id).await.unwrap();
    assert_eq!(replicas.len(), 1);
    let blob_file = env.store_dir().join(&replicas[0].path);
    std::fs::write(&blob_file, b"corrupted!!!").unwrap();

    let result = env.store_verify("local").await;
    assert!(result.is_err(), "store verify should fail when a blob is corrupted");
}

// ── Store set ─────────────────────────────────────────────────────────────────

/// `tome store set` updates the URL of an existing store.
#[tokio::test]
async fn store_set_updates_url() {
    let env = Env::new().await;
    env.store_add_and_push("local").await.unwrap();

    let new_url = "s3://new-bucket/prefix";
    env.store_set("local", Some(new_url)).await.unwrap();

    let store = ops::find_store_by_name(&env.db, "local").await.unwrap().unwrap();
    assert_eq!(store.url, new_url);
}

/// `tome store set` with no flags returns an error.
#[tokio::test]
async fn store_set_without_flags_errors() {
    let env = Env::new().await;
    env.store_add_and_push("local").await.unwrap();

    let result = env.store_set("local", None).await;
    assert!(result.is_err(), "store set with no flags should error");
}

/// `tome store set` on a non-existent store returns an error.
#[tokio::test]
async fn store_set_nonexistent_errors() {
    let env = Env::new().await;
    let result = env.store_set("ghost", Some("file:///tmp")).await;
    assert!(result.is_err(), "store set on non-existent store should error");
}

// ── Store rm ──────────────────────────────────────────────────────────────────

/// `tome store rm` removes an empty store.
#[tokio::test]
async fn store_rm_removes_empty_store() {
    let env = Env::new().await;
    let store_url = format!("file://{}", env.store_dir().display());

    // Register but don't push (no replicas).
    tome_cli::commands::store::run(
        &env.db,
        tome_cli::commands::store::StoreArgs {
            command: tome_cli::commands::store::StoreCommands::Add(tome_cli::commands::store::StoreAddArgs {
                name: "empty".to_string(),
                url: store_url,
            }),
        },
        &tome_cli::config::StoreConfig::default(),
    )
    .await
    .unwrap();

    env.store_rm("empty", false).await.unwrap();

    let stores = ops::list_stores(&env.db).await.unwrap();
    assert!(stores.is_empty());
}

/// `tome store rm` refuses removal when replicas exist (without --force).
#[tokio::test]
async fn store_rm_rejects_when_replicas_exist() {
    let env = Env::new().await;
    env.write("a.txt", b"data");
    env.scan().await.unwrap();
    env.store_add_and_push("local").await.unwrap();

    let result = env.store_rm("local", false).await;
    assert!(result.is_err(), "rm should refuse when replicas exist");

    // Store should still be there.
    let stores = ops::list_stores(&env.db).await.unwrap();
    assert_eq!(stores.len(), 1);
}

/// `tome store rm --force` removes a store even when replicas exist.
#[tokio::test]
async fn store_rm_force_removes_with_replicas() {
    let env = Env::new().await;
    env.write("a.txt", b"data");
    env.scan().await.unwrap();
    env.store_add_and_push("local").await.unwrap();

    env.store_rm("local", true).await.unwrap();

    let stores = ops::list_stores(&env.db).await.unwrap();
    assert!(stores.is_empty());
}
