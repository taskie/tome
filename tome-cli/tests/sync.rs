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
