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
