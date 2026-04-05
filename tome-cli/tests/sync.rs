//! Integration tests for `tome sync config` subcommand.
//!
//! Peer management (add, set, rm, list) has moved to `tome remote` — see `remote.rs`.

mod common;
use common::Env;

use tome_db::ops;

// ── Sync config ─────────────────────────────────────────────────────────────

/// `tome sync config <peer> <key> <value>` sets a config key.
#[tokio::test]
async fn sync_config_set_key() {
    let env = Env::new().await;
    env.remote_add("remote", "https://tome.example.com", "default", None).await.unwrap();

    env.sync_config("remote", Some("auth"), Some("aws-iam"), None, false, "default").await.unwrap();

    let repo = ops::get_or_create_repository(&env.db, "default").await.unwrap();
    let peer = ops::find_sync_peer(&env.db, "remote", repo.id).await.unwrap().unwrap();
    assert_eq!(peer.config["auth"], "aws-iam");
    // Original peer_repo should be preserved.
    assert_eq!(peer.config["peer_repo"], "default");
}

/// `tome sync config <peer> <key>` reads a config key.
#[tokio::test]
async fn sync_config_get_key() {
    let env = Env::new().await;
    env.remote_add("remote", "https://tome.example.com", "default", Some("prod")).await.unwrap();

    // Should succeed (prints to stdout).
    env.sync_config("remote", Some("peer_repo"), None, None, false, "default").await.unwrap();
}

/// `tome sync config <peer> <key>` errors for a missing key.
#[tokio::test]
async fn sync_config_get_missing_key_errors() {
    let env = Env::new().await;
    env.remote_add("remote", "https://tome.example.com", "default", None).await.unwrap();

    let result = env.sync_config("remote", Some("nonexistent"), None, None, false, "default").await;
    assert!(result.is_err());
}

/// `tome sync config <peer> --unset <key>` removes a config key.
#[tokio::test]
async fn sync_config_unset_key() {
    let env = Env::new().await;
    env.remote_add("remote", "https://tome.example.com", "default", None).await.unwrap();
    env.sync_config("remote", Some("auth"), Some("aws-iam"), None, false, "default").await.unwrap();

    env.sync_config("remote", None, None, Some("auth"), false, "default").await.unwrap();

    let repo = ops::get_or_create_repository(&env.db, "default").await.unwrap();
    let peer = ops::find_sync_peer(&env.db, "remote", repo.id).await.unwrap().unwrap();
    assert!(peer.config.get("auth").is_none());
}

/// `tome sync config <peer> --unset` errors for a missing key.
#[tokio::test]
async fn sync_config_unset_missing_key_errors() {
    let env = Env::new().await;
    env.remote_add("remote", "https://tome.example.com", "default", None).await.unwrap();

    let result = env.sync_config("remote", None, None, Some("nonexistent"), false, "default").await;
    assert!(result.is_err());
}

/// `tome sync config <peer> --list` lists all config values.
#[tokio::test]
async fn sync_config_list() {
    let env = Env::new().await;
    env.remote_add("remote", "https://tome.example.com", "default", Some("prod")).await.unwrap();
    env.sync_config("remote", Some("auth"), Some("aws-iam"), None, false, "default").await.unwrap();
    env.sync_config("remote", Some("region"), Some("us-west-2"), None, false, "default").await.unwrap();

    // Should succeed (prints to stdout).
    env.sync_config("remote", None, None, None, true, "default").await.unwrap();
}

/// `tome sync config` on a non-existent peer returns an error.
#[tokio::test]
async fn sync_config_nonexistent_peer_errors() {
    let env = Env::new().await;
    let result = env.sync_config("ghost", Some("auth"), Some("aws-iam"), None, false, "default").await;
    assert!(result.is_err());
}

/// Multiple `tome sync config` calls accumulate values.
#[tokio::test]
async fn sync_config_multiple_keys() {
    let env = Env::new().await;
    env.remote_add("remote", "https://tome.example.com", "default", None).await.unwrap();

    env.sync_config("remote", Some("auth"), Some("aws-iam"), None, false, "default").await.unwrap();
    env.sync_config("remote", Some("region"), Some("us-west-2"), None, false, "default").await.unwrap();
    env.sync_config("remote", Some("service"), Some("lambda"), None, false, "default").await.unwrap();

    let repo = ops::get_or_create_repository(&env.db, "default").await.unwrap();
    let peer = ops::find_sync_peer(&env.db, "remote", repo.id).await.unwrap().unwrap();
    assert_eq!(peer.config["auth"], "aws-iam");
    assert_eq!(peer.config["region"], "us-west-2");
    assert_eq!(peer.config["service"], "lambda");
    // Original peer_repo should still be there.
    assert_eq!(peer.config["peer_repo"], "default");
}
