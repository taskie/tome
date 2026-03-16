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

// ── Per-store encryption config ──────────────────────────────────────────────

/// `tome store add --encrypt` saves encryption config in the store's config JSON.
#[tokio::test]
async fn store_add_with_encrypt_saves_config() {
    let env = Env::new().await;
    let store_url = format!("file://{}", env.store_dir().display());

    tome_cli::commands::store::run(
        &env.db,
        tome_cli::commands::store::StoreArgs {
            command: tome_cli::commands::store::StoreCommands::Add(tome_cli::commands::store::StoreAddArgs {
                name: "encrypted".to_string(),
                url: store_url,
                encrypt: true,
                key_file: None,
                key_source: Some("env://TOME_TEST_KEY".to_string()),
                cipher: Some("chacha20-poly1305".to_string()),
            }),
        },
        &tome_cli::config::StoreConfig::default(),
    )
    .await
    .unwrap();

    let store = ops::find_store_by_name(&env.db, "encrypted").await.unwrap().unwrap();
    assert_eq!(store.config["encrypt"], serde_json::json!(true));
    assert_eq!(store.config["key_source"], serde_json::json!("env://TOME_TEST_KEY"));
    assert_eq!(store.config["cipher"], serde_json::json!("chacha20-poly1305"));
}

/// `tome store set --encrypt` and `--no-encrypt` toggle encryption config.
#[tokio::test]
async fn store_set_encrypt_and_no_encrypt() {
    let env = Env::new().await;
    let store_url = format!("file://{}", env.store_dir().display());

    // Create a plain store.
    tome_cli::commands::store::run(
        &env.db,
        tome_cli::commands::store::StoreArgs {
            command: tome_cli::commands::store::StoreCommands::Add(tome_cli::commands::store::StoreAddArgs {
                name: "toggle".to_string(),
                url: store_url,
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

    // Enable encryption.
    tome_cli::commands::store::run(
        &env.db,
        tome_cli::commands::store::StoreArgs {
            command: tome_cli::commands::store::StoreCommands::Set(tome_cli::commands::store::StoreSetArgs {
                name: "toggle".to_string(),
                url: None,
                encrypt: true,
                no_encrypt: false,
                key_file: None,
                key_source: Some("env://MY_KEY".to_string()),
                cipher: None,
            }),
        },
        &tome_cli::config::StoreConfig::default(),
    )
    .await
    .unwrap();

    let store = ops::find_store_by_name(&env.db, "toggle").await.unwrap().unwrap();
    assert_eq!(store.config["encrypt"], serde_json::json!(true));
    assert_eq!(store.config["key_source"], serde_json::json!("env://MY_KEY"));

    // Disable encryption with --no-encrypt.
    tome_cli::commands::store::run(
        &env.db,
        tome_cli::commands::store::StoreArgs {
            command: tome_cli::commands::store::StoreCommands::Set(tome_cli::commands::store::StoreSetArgs {
                name: "toggle".to_string(),
                url: None,
                encrypt: false,
                no_encrypt: true,
                key_file: None,
                key_source: None,
                cipher: None,
            }),
        },
        &tome_cli::config::StoreConfig::default(),
    )
    .await
    .unwrap();

    let store = ops::find_store_by_name(&env.db, "toggle").await.unwrap().unwrap();
    assert_eq!(store.config["encrypt"], serde_json::json!(false));
    assert!(store.config.get("key_source").is_none() || store.config["key_source"].is_null());
}

/// `tome store push` auto-encrypts when the store has encryption config.
#[tokio::test]
async fn store_push_auto_encrypts() {
    let env = Env::new().await;
    let store_url = format!("file://{}", env.store_dir().display());

    // Write a 32-byte key file.
    let key_dir = env.root.path().join("keys");
    std::fs::create_dir_all(&key_dir).unwrap();
    let key_path = key_dir.join("test.key");
    std::fs::write(&key_path, &[0xABu8; 32]).unwrap();

    // Register store with encryption config.
    tome_cli::commands::store::run(
        &env.db,
        tome_cli::commands::store::StoreArgs {
            command: tome_cli::commands::store::StoreCommands::Add(tome_cli::commands::store::StoreAddArgs {
                name: "enc-store".to_string(),
                url: store_url,
                encrypt: true,
                key_file: Some(key_path.to_str().unwrap().to_string()),
                key_source: None,
                cipher: None,
            }),
        },
        &tome_cli::config::StoreConfig::default(),
    )
    .await
    .unwrap();

    // Scan and push.
    env.write("secret.txt", b"top secret data");
    env.scan().await.unwrap();

    tome_cli::commands::store::run(
        &env.db,
        tome_cli::commands::store::StoreArgs {
            command: tome_cli::commands::store::StoreCommands::Push(tome_cli::commands::store::StorePushArgs {
                repo: "default".to_string(),
                store: Some("enc-store".to_string()),
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

    // Replica should be marked as encrypted.
    let store = ops::find_store_by_name(&env.db, "enc-store").await.unwrap().unwrap();
    let replicas = ops::replicas_in_store(&env.db, store.id).await.unwrap();
    assert_eq!(replicas.len(), 1);
    assert!(replicas[0].encrypted, "replica should be marked as encrypted");

    // The blob on disk should NOT match the original plaintext (it's encrypted).
    let blob_file = env.store_dir().join(&replicas[0].path);
    let stored_bytes = std::fs::read(&blob_file).unwrap();
    assert_ne!(stored_bytes.as_slice(), b"top secret data", "stored blob should be encrypted, not plaintext");
}

/// `tome store list` shows "yes" for stores with encryption enabled.
#[tokio::test]
async fn store_list_shows_encrypt_status() {
    let env = Env::new().await;

    // Add a plain store.
    tome_cli::commands::store::run(
        &env.db,
        tome_cli::commands::store::StoreArgs {
            command: tome_cli::commands::store::StoreCommands::Add(tome_cli::commands::store::StoreAddArgs {
                name: "plain".to_string(),
                url: "file:///tmp/plain".to_string(),
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

    // Add an encrypted store.
    tome_cli::commands::store::run(
        &env.db,
        tome_cli::commands::store::StoreArgs {
            command: tome_cli::commands::store::StoreCommands::Add(tome_cli::commands::store::StoreAddArgs {
                name: "secure".to_string(),
                url: "file:///tmp/secure".to_string(),
                encrypt: true,
                key_file: None,
                key_source: Some("env://KEY".to_string()),
                cipher: None,
            }),
        },
        &tome_cli::config::StoreConfig::default(),
    )
    .await
    .unwrap();

    // Verify store configs directly (store list prints to stdout, hard to capture in test).
    let plain = ops::find_store_by_name(&env.db, "plain").await.unwrap().unwrap();
    let secure = ops::find_store_by_name(&env.db, "secure").await.unwrap().unwrap();

    assert_eq!(plain.config["encrypt"], serde_json::json!(false));
    assert_eq!(secure.config["encrypt"], serde_json::json!(true));
}

// ── Direct remote push with CLI encryption override ──────────────────────────

/// `tome store push --encrypt --key-file ...` encrypts even when the store has no encryption config.
#[tokio::test]
async fn store_push_with_cli_encrypt_override() {
    let env = Env::new().await;
    let store_url = format!("file://{}", env.store_dir().display());

    // Write a 32-byte key file.
    let key_dir = env.root.path().join("keys");
    std::fs::create_dir_all(&key_dir).unwrap();
    let key_path = key_dir.join("test.key");
    std::fs::write(&key_path, &[0xCDu8; 32]).unwrap();

    // Register a plain store (no encryption config).
    tome_cli::commands::store::run(
        &env.db,
        tome_cli::commands::store::StoreArgs {
            command: tome_cli::commands::store::StoreCommands::Add(tome_cli::commands::store::StoreAddArgs {
                name: "remote".to_string(),
                url: store_url,
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

    // Scan a file.
    env.write("data.bin", b"binary payload for remote");
    env.scan().await.unwrap();

    // Push with CLI --encrypt override.
    tome_cli::commands::store::run(
        &env.db,
        tome_cli::commands::store::StoreArgs {
            command: tome_cli::commands::store::StoreCommands::Push(tome_cli::commands::store::StorePushArgs {
                repo: "default".to_string(),
                store: Some("remote".to_string()),
                path: Some(env.files_dir()),
                encrypt: true,
                key_file: Some(key_path),
                key_source: None,
                cipher: None,
            }),
        },
        &tome_cli::config::StoreConfig::default(),
    )
    .await
    .unwrap();

    // Replica should be marked encrypted.
    let store = ops::find_store_by_name(&env.db, "remote").await.unwrap().unwrap();
    let replicas = ops::replicas_in_store(&env.db, store.id).await.unwrap();
    assert_eq!(replicas.len(), 1);
    assert!(replicas[0].encrypted, "replica should be encrypted via CLI override");

    // Blob on disk should not match plaintext.
    let blob_file = env.store_dir().join(&replicas[0].path);
    let stored = std::fs::read(&blob_file).unwrap();
    assert_ne!(stored.as_slice(), b"binary payload for remote");
}
