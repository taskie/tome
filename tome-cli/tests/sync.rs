//! Integration tests for `tome sync` subcommands (add, set, rm, list).

mod common;
use common::Env;

use tome_db::ops;

// ── Sync add ──────────────────────────────────────────────────────────────────

/// `tome sync add` registers a peer in the database.
#[tokio::test]
async fn sync_add_registers_peer() {
    let env = Env::new().await;

    env.sync_add("remote", "https://tome.example.com", "default", None).await.unwrap();

    let repo = ops::get_or_create_repository(&env.db, "default").await.unwrap();
    let peers = ops::list_sync_peers(&env.db, repo.id).await.unwrap();
    assert_eq!(peers.len(), 1);
    assert_eq!(peers[0].name, "remote");
    assert_eq!(peers[0].url, "https://tome.example.com");
}

/// `tome sync add` with a custom peer_repo stores it in config.
#[tokio::test]
async fn sync_add_with_peer_repo() {
    let env = Env::new().await;

    env.sync_add("remote", "https://tome.example.com", "default", Some("prod")).await.unwrap();

    let repo = ops::get_or_create_repository(&env.db, "default").await.unwrap();
    let peer = ops::find_sync_peer(&env.db, "remote", repo.id).await.unwrap().unwrap();
    assert_eq!(peer.config["peer_repo"], "prod");
}

// ── Sync set ──────────────────────────────────────────────────────────────────

/// `tome sync set --peer-url` updates the peer URL.
#[tokio::test]
async fn sync_set_updates_url() {
    let env = Env::new().await;
    env.sync_add("remote", "https://old.example.com", "default", None).await.unwrap();

    env.sync_set("remote", Some("https://new.example.com"), None, "default").await.unwrap();

    let repo = ops::get_or_create_repository(&env.db, "default").await.unwrap();
    let peer = ops::find_sync_peer(&env.db, "remote", repo.id).await.unwrap().unwrap();
    assert_eq!(peer.url, "https://new.example.com");
}

/// `tome sync set --peer-repo` updates the peer_repo in config.
#[tokio::test]
async fn sync_set_updates_peer_repo() {
    let env = Env::new().await;
    env.sync_add("remote", "https://tome.example.com", "default", Some("staging")).await.unwrap();

    env.sync_set("remote", None, Some("production"), "default").await.unwrap();

    let repo = ops::get_or_create_repository(&env.db, "default").await.unwrap();
    let peer = ops::find_sync_peer(&env.db, "remote", repo.id).await.unwrap().unwrap();
    assert_eq!(peer.config["peer_repo"], "production");
    // URL should remain unchanged.
    assert_eq!(peer.url, "https://tome.example.com");
}

/// `tome sync set` with no flags returns an error.
#[tokio::test]
async fn sync_set_without_flags_errors() {
    let env = Env::new().await;
    env.sync_add("remote", "https://tome.example.com", "default", None).await.unwrap();

    let result = env.sync_set("remote", None, None, "default").await;
    assert!(result.is_err(), "sync set with no flags should error");
}

/// `tome sync set` on a non-existent peer returns an error.
#[tokio::test]
async fn sync_set_nonexistent_errors() {
    let env = Env::new().await;
    let result = env.sync_set("ghost", Some("https://x.com"), None, "default").await;
    assert!(result.is_err(), "sync set on non-existent peer should error");
}

// ── Sync rm ───────────────────────────────────────────────────────────────────

/// `tome sync rm` removes a peer from the database.
#[tokio::test]
async fn sync_rm_removes_peer() {
    let env = Env::new().await;
    env.sync_add("remote", "https://tome.example.com", "default", None).await.unwrap();

    env.sync_rm("remote", "default").await.unwrap();

    let repo = ops::get_or_create_repository(&env.db, "default").await.unwrap();
    let peers = ops::list_sync_peers(&env.db, repo.id).await.unwrap();
    assert!(peers.is_empty());
}

/// `tome sync rm` on a non-existent peer returns an error.
#[tokio::test]
async fn sync_rm_nonexistent_errors() {
    let env = Env::new().await;
    let result = env.sync_rm("ghost", "default").await;
    assert!(result.is_err(), "sync rm on non-existent peer should error");
}

// ── Sync list ─────────────────────────────────────────────────────────────────

/// `tome sync list` shows registered peers (does not error when peers exist).
#[tokio::test]
async fn sync_list_shows_peers() {
    let env = Env::new().await;
    env.sync_add("peer1", "https://one.example.com", "default", None).await.unwrap();
    env.sync_add("peer2", "https://two.example.com", "default", None).await.unwrap();

    // Should not error.
    env.sync_list("default").await.unwrap();

    let repo = ops::get_or_create_repository(&env.db, "default").await.unwrap();
    let peers = ops::list_sync_peers(&env.db, repo.id).await.unwrap();
    assert_eq!(peers.len(), 2);
}

/// `tome sync list` succeeds even when there are no peers.
#[tokio::test]
async fn sync_list_empty() {
    let env = Env::new().await;
    env.sync_list("default").await.unwrap();
}

// ── Sync config ─────────────────────────────────────────────────────────────

/// `tome sync config <peer> <key> <value>` sets a config key.
#[tokio::test]
async fn sync_config_set_key() {
    let env = Env::new().await;
    env.sync_add("remote", "https://tome.example.com", "default", None).await.unwrap();

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
    env.sync_add("remote", "https://tome.example.com", "default", Some("prod")).await.unwrap();

    // Should succeed (prints to stdout).
    env.sync_config("remote", Some("peer_repo"), None, None, false, "default").await.unwrap();
}

/// `tome sync config <peer> <key>` errors for a missing key.
#[tokio::test]
async fn sync_config_get_missing_key_errors() {
    let env = Env::new().await;
    env.sync_add("remote", "https://tome.example.com", "default", None).await.unwrap();

    let result = env.sync_config("remote", Some("nonexistent"), None, None, false, "default").await;
    assert!(result.is_err());
}

/// `tome sync config <peer> --unset <key>` removes a config key.
#[tokio::test]
async fn sync_config_unset_key() {
    let env = Env::new().await;
    env.sync_add("remote", "https://tome.example.com", "default", None).await.unwrap();
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
    env.sync_add("remote", "https://tome.example.com", "default", None).await.unwrap();

    let result = env.sync_config("remote", None, None, Some("nonexistent"), false, "default").await;
    assert!(result.is_err());
}

/// `tome sync config <peer> --list` lists all config values.
#[tokio::test]
async fn sync_config_list() {
    let env = Env::new().await;
    env.sync_add("remote", "https://tome.example.com", "default", Some("prod")).await.unwrap();
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
    env.sync_add("remote", "https://tome.example.com", "default", None).await.unwrap();

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
