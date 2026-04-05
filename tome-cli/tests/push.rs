//! Integration tests for `tome push` / `tome pull` composite commands.

mod common;
use common::Env;

use tome_db::ops;

// ── Setup helpers ─────────────────────────────────────────────────────────────

/// Register a local store and a DB-mode sync peer in `env`, returning the peer DB URL.
async fn setup_store_and_peer(env: &Env, store_name: &str, peer_name: &str) -> (String, Env) {
    // Register local store in source env.
    let store_url = format!("file://{}", env.store_dir().display());
    tome_cli::commands::store::run(
        &env.db,
        tome_cli::commands::store::StoreArgs {
            command: tome_cli::commands::store::StoreCommands::Add(tome_cli::commands::store::StoreAddArgs {
                name: store_name.to_string(),
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

    // Create a peer (another Env with its own SQLite DB).
    let peer_env = Env::new().await;
    let peer_db_path = peer_env.root.path().join("tome.db");
    let peer_db_url = format!("sqlite://{}?mode=rwc", peer_db_path.display());

    env.remote_add(peer_name, &peer_db_url, "default", None).await.unwrap();

    (peer_db_url, peer_env)
}

// ── tome push ─────────────────────────────────────────────────────────────────

/// `tome push` runs scan → store push → sync push in sequence.
#[tokio::test]
async fn push_scans_and_pushes_to_store_and_peer() {
    let env = Env::new().await;
    env.write("hello.txt", b"hello world");
    // .git/ needed for ignore crate
    std::fs::create_dir_all(env.files_dir().join(".git")).unwrap();

    let store_name = "local";
    let peer_name = "central";
    let (peer_db_url, peer_env) = setup_store_and_peer(&env, store_name, peer_name).await;

    env.push(peer_name, "default", Some(store_name), false, false, None).await.unwrap();

    // Snapshot was created locally.
    let snapshots = env.snapshots().await;
    assert!(!snapshots.is_empty(), "scan should have created a snapshot");

    // Replica was created in the local store.
    let repos = ops::get_or_create_repository(&env.db, "default").await.unwrap();
    let entries = ops::present_cache_entries(&env.db, repos.id).await.unwrap();
    assert!(!entries.is_empty());
    let blob_id = entries[0].object_id.unwrap();
    let stores = ops::list_stores(&env.db).await.unwrap();
    assert!(ops::replica_exists(&env.db, blob_id, stores[0].id).await.unwrap());

    // Snapshot was pushed to peer DB.
    let peer_repo = ops::get_or_create_repository(&peer_env.db, "default").await.unwrap();
    let peer_snaps = ops::list_snapshots_for_repo(&peer_env.db, peer_repo.id).await.unwrap();
    assert!(!peer_snaps.is_empty(), "sync push should have created a snapshot on the peer");

    drop(peer_db_url);
}

/// `--no-scan` skips scan but still does store push + sync push using existing snapshot.
#[tokio::test]
async fn push_no_scan_uses_existing_snapshot() {
    let env = Env::new().await;
    env.write("file.txt", b"data");
    // .git/ needed for ignore crate
    std::fs::create_dir_all(env.files_dir().join(".git")).unwrap();
    // Pre-scan so there is already a snapshot.
    env.scan().await.unwrap();

    let store_name = "local";
    let peer_name = "central";
    let (_peer_db_url, peer_env) = setup_store_and_peer(&env, store_name, peer_name).await;

    // push with --no-scan: must not fail even though scan is skipped.
    env.push(peer_name, "default", Some(store_name), true, false, None).await.unwrap();

    // Only the pre-existing snapshot exists (no new one from push itself).
    let snapshots = env.snapshots().await;
    assert_eq!(snapshots.len(), 1, "no extra snapshot should be created when --no-scan is used");

    // Snapshot reached the peer.
    let peer_repo = ops::get_or_create_repository(&peer_env.db, "default").await.unwrap();
    let peer_snaps = ops::list_snapshots_for_repo(&peer_env.db, peer_repo.id).await.unwrap();
    assert!(!peer_snaps.is_empty());
}

/// `--no-store` skips blob upload but still runs scan + sync push.
#[tokio::test]
async fn push_no_store_skips_blob_upload() {
    let env = Env::new().await;
    env.write("file.txt", b"data");
    std::fs::create_dir_all(env.files_dir().join(".git")).unwrap();

    let store_name = "local";
    let peer_name = "central";
    let (_peer_db_url, peer_env) = setup_store_and_peer(&env, store_name, peer_name).await;

    env.push(peer_name, "default", Some(store_name), false, true, None).await.unwrap();

    // No replica should exist (store push was skipped).
    let repo = ops::get_or_create_repository(&env.db, "default").await.unwrap();
    let entries = ops::present_cache_entries(&env.db, repo.id).await.unwrap();
    if !entries.is_empty() {
        let blob_id = entries[0].object_id.unwrap();
        let stores = ops::list_stores(&env.db).await.unwrap();
        assert!(!ops::replica_exists(&env.db, blob_id, stores[0].id).await.unwrap(), "no replica should exist");
    }

    // Snapshot still reached the peer.
    let peer_repo = ops::get_or_create_repository(&peer_env.db, "default").await.unwrap();
    let peer_snaps = ops::list_snapshots_for_repo(&peer_env.db, peer_repo.id).await.unwrap();
    assert!(!peer_snaps.is_empty());
}

// ── tome pull ─────────────────────────────────────────────────────────────────

/// `tome pull` brings snapshots from a remote peer into the local DB.
#[tokio::test]
async fn pull_retrieves_snapshots_from_peer() {
    // Set up a "remote" env with a snapshot.
    let remote = Env::new().await;
    remote.write("remote_file.txt", b"remote content");
    std::fs::create_dir_all(remote.files_dir().join(".git")).unwrap();
    remote.scan().await.unwrap();

    // Set up a "local" env that will pull from remote.
    let local = Env::new().await;
    let remote_db_path = remote.root.path().join("tome.db");
    let remote_db_url = format!("sqlite://{}?mode=rwc", remote_db_path.display());

    local.remote_add("remote", &remote_db_url, "default", None).await.unwrap();

    local.pull("remote", "default", false, None, None).await.unwrap();

    // Local DB should now have a snapshot with the remote file.
    let local_repo = ops::get_or_create_repository(&local.db, "default").await.unwrap();
    let local_snaps = ops::list_snapshots_for_repo(&local.db, local_repo.id).await.unwrap();
    assert!(!local_snaps.is_empty(), "pull should have created a local snapshot");

    let local_entries = ops::present_cache_entries(&local.db, local_repo.id).await.unwrap();
    assert!(local_entries.iter().any(|e| e.path == "remote_file.txt"), "pulled file should appear in local cache");
}

/// `tome pull` on a peer that has no new snapshots succeeds without creating snapshots.
#[tokio::test]
async fn pull_up_to_date_is_noop() {
    let remote = Env::new().await;
    let local = Env::new().await;
    let remote_db_path = remote.root.path().join("tome.db");
    let remote_db_url = format!("sqlite://{}?mode=rwc", remote_db_path.display());

    local.remote_add("remote", &remote_db_url, "default", None).await.unwrap();

    // No snapshots on remote — pull should succeed but create nothing.
    local.pull("remote", "default", false, None, None).await.unwrap();

    let local_repo = ops::get_or_create_repository(&local.db, "default").await.unwrap();
    let local_snaps = ops::list_snapshots_for_repo(&local.db, local_repo.id).await.unwrap();
    assert!(local_snaps.is_empty(), "no snapshot should be created when remote is empty");
}
