//! Integration tests for `tome remote` subcommands (add, set, rm, list).

mod common;
use common::Env;

use tome_db::ops;

// ── Remote add ───────────────────────────────────────────────────────────────

/// `tome remote add` registers a peer in the database.
#[tokio::test]
async fn remote_add_registers_peer() {
    let env = Env::new().await;

    env.remote_add("origin", "https://tome.example.com", "default", None).await.unwrap();

    let repo = ops::get_or_create_repository(&env.db, "default").await.unwrap();
    let peers = ops::list_sync_peers(&env.db, repo.id).await.unwrap();
    assert_eq!(peers.len(), 1);
    assert_eq!(peers[0].name, "origin");
    assert_eq!(peers[0].url, "https://tome.example.com");
}

/// `tome remote add` with a custom peer_repo stores it in config.
#[tokio::test]
async fn remote_add_with_peer_repo() {
    let env = Env::new().await;

    env.remote_add("origin", "https://tome.example.com", "default", Some("prod")).await.unwrap();

    let repo = ops::get_or_create_repository(&env.db, "default").await.unwrap();
    let peer = ops::find_sync_peer(&env.db, "origin", repo.id).await.unwrap().unwrap();
    assert_eq!(peer.config["peer_repo"], "prod");
}

// ── Remote set ───────────────────────────────────────────────────────────────

/// `tome remote set --peer-url` updates the peer URL.
#[tokio::test]
async fn remote_set_updates_url() {
    let env = Env::new().await;
    env.remote_add("origin", "https://old.example.com", "default", None).await.unwrap();

    env.remote_set("origin", Some("https://new.example.com"), None, "default").await.unwrap();

    let repo = ops::get_or_create_repository(&env.db, "default").await.unwrap();
    let peer = ops::find_sync_peer(&env.db, "origin", repo.id).await.unwrap().unwrap();
    assert_eq!(peer.url, "https://new.example.com");
}

/// `tome remote set --peer-repo` updates the peer_repo in config.
#[tokio::test]
async fn remote_set_updates_peer_repo() {
    let env = Env::new().await;
    env.remote_add("origin", "https://tome.example.com", "default", Some("staging")).await.unwrap();

    env.remote_set("origin", None, Some("production"), "default").await.unwrap();

    let repo = ops::get_or_create_repository(&env.db, "default").await.unwrap();
    let peer = ops::find_sync_peer(&env.db, "origin", repo.id).await.unwrap().unwrap();
    assert_eq!(peer.config["peer_repo"], "production");
    assert_eq!(peer.url, "https://tome.example.com");
}

/// `tome remote set` with no flags returns an error.
#[tokio::test]
async fn remote_set_without_flags_errors() {
    let env = Env::new().await;
    env.remote_add("origin", "https://tome.example.com", "default", None).await.unwrap();

    let result = env.remote_set("origin", None, None, "default").await;
    assert!(result.is_err(), "remote set with no flags should error");
}

/// `tome remote set` on a non-existent peer returns an error.
#[tokio::test]
async fn remote_set_nonexistent_errors() {
    let env = Env::new().await;
    let result = env.remote_set("ghost", Some("https://x.com"), None, "default").await;
    assert!(result.is_err(), "remote set on non-existent peer should error");
}

// ── Remote rm ────────────────────────────────────────────────────────────────

/// `tome remote rm` removes a peer from the database.
#[tokio::test]
async fn remote_rm_removes_peer() {
    let env = Env::new().await;
    env.remote_add("origin", "https://tome.example.com", "default", None).await.unwrap();

    env.remote_rm("origin", "default").await.unwrap();

    let repo = ops::get_or_create_repository(&env.db, "default").await.unwrap();
    let peers = ops::list_sync_peers(&env.db, repo.id).await.unwrap();
    assert!(peers.is_empty());
}

/// `tome remote rm` on a non-existent peer returns an error.
#[tokio::test]
async fn remote_rm_nonexistent_errors() {
    let env = Env::new().await;
    let result = env.remote_rm("ghost", "default").await;
    assert!(result.is_err(), "remote rm on non-existent peer should error");
}

// ── Remote list ──────────────────────────────────────────────────────────────

/// `tome remote list` shows registered peers.
#[tokio::test]
async fn remote_list_shows_peers() {
    let env = Env::new().await;
    env.remote_add("peer1", "https://one.example.com", "default", None).await.unwrap();
    env.remote_add("peer2", "https://two.example.com", "default", None).await.unwrap();

    env.remote_list("default").await.unwrap();

    let repo = ops::get_or_create_repository(&env.db, "default").await.unwrap();
    let peers = ops::list_sync_peers(&env.db, repo.id).await.unwrap();
    assert_eq!(peers.len(), 2);
}

/// `tome remote list` succeeds even when there are no peers.
#[tokio::test]
async fn remote_list_empty() {
    let env = Env::new().await;
    env.remote_list("default").await.unwrap();
}
