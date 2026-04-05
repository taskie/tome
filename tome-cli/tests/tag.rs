//! Integration tests for `tome tag`.
//!
//! Tests cover: setting, listing, deleting, and searching key-value tags on blobs.

mod common;
use common::Env;

use tome_db::ops;

// ── Set and list ─────────────────────────────────────────────────────────────

/// `tome tag set` adds a tag to a blob, and `tome tag list` retrieves it.
#[tokio::test]
async fn tag_set_and_list() {
    let env = Env::new().await;
    env.write("photo.jpg", b"jpeg data");
    env.scan().await.unwrap();

    let digest = env.first_blob_digest_hex().await;

    env.tag_set(&digest, "camera", Some("Nikon D850")).await.unwrap();
    env.tag_set(&digest, "location", Some("Tokyo")).await.unwrap();

    // Verify via ops directly.
    let repo = ops::get_or_create_repository(&env.db, "default").await.unwrap();
    let entries = ops::present_cache_entries(&env.db, repo.id).await.unwrap();
    let blob_id = entries[0].object_id.unwrap();
    let tags = ops::list_tags(&env.db, blob_id).await.unwrap();

    assert_eq!(tags.len(), 2);
    let keys: Vec<&str> = tags.iter().map(|t| t.key.as_str()).collect();
    assert!(keys.contains(&"camera"));
    assert!(keys.contains(&"location"));
}

/// Tags can be set without a value (key-only flag tag).
#[tokio::test]
async fn tag_set_without_value() {
    let env = Env::new().await;
    env.write("file.txt", b"content");
    env.scan().await.unwrap();

    let digest = env.first_blob_digest_hex().await;
    env.tag_set(&digest, "reviewed", None).await.unwrap();

    let repo = ops::get_or_create_repository(&env.db, "default").await.unwrap();
    let entries = ops::present_cache_entries(&env.db, repo.id).await.unwrap();
    let blob_id = entries[0].object_id.unwrap();
    let tags = ops::list_tags(&env.db, blob_id).await.unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].key, "reviewed");
}

// ── Delete ───────────────────────────────────────────────────────────────────

/// `tome tag delete` removes a specific tag from a blob.
#[tokio::test]
async fn tag_delete_removes_tag() {
    let env = Env::new().await;
    env.write("file.txt", b"content");
    env.scan().await.unwrap();

    let digest = env.first_blob_digest_hex().await;
    env.tag_set(&digest, "keep", Some("yes")).await.unwrap();
    env.tag_set(&digest, "remove", Some("yes")).await.unwrap();

    env.tag_delete(&digest, "remove").await.unwrap();

    let repo = ops::get_or_create_repository(&env.db, "default").await.unwrap();
    let entries = ops::present_cache_entries(&env.db, repo.id).await.unwrap();
    let blob_id = entries[0].object_id.unwrap();
    let tags = ops::list_tags(&env.db, blob_id).await.unwrap();

    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].key, "keep");
}

/// `tome tag rm` removes a specific tag (same as `delete`).
#[tokio::test]
async fn tag_rm_removes_tag() {
    let env = Env::new().await;
    env.write("file.txt", b"content");
    env.scan().await.unwrap();

    let digest = env.first_blob_digest_hex().await;
    env.tag_set(&digest, "keep", Some("yes")).await.unwrap();
    env.tag_set(&digest, "remove", Some("yes")).await.unwrap();

    env.tag_rm(&digest, "remove").await.unwrap();

    let repo = ops::get_or_create_repository(&env.db, "default").await.unwrap();
    let entries = ops::present_cache_entries(&env.db, repo.id).await.unwrap();
    let blob_id = entries[0].object_id.unwrap();
    let tags = ops::list_tags(&env.db, blob_id).await.unwrap();

    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].key, "keep");
}

// ── Search ───────────────────────────────────────────────────────────────────

/// `tome tag search` finds blobs by key (and optionally value).
#[tokio::test]
async fn tag_search_by_key() {
    let env = Env::new().await;
    env.write("a.txt", b"content a");
    env.write("b.txt", b"content b");
    env.scan().await.unwrap();

    // Tag both blobs with "category" but different values.
    let repo = ops::get_or_create_repository(&env.db, "default").await.unwrap();
    let entries = ops::present_cache_entries(&env.db, repo.id).await.unwrap();
    for entry in &entries {
        let blob_id = entry.object_id.unwrap();
        let blobs = ops::objects_by_ids(&env.db, &[blob_id]).await.unwrap();
        let digest = tome_core::hash::hex_encode(&blobs[0].digest);
        let val = if entry.path == "a.txt" { "alpha" } else { "beta" };
        env.tag_set(&digest, "category", Some(val)).await.unwrap();
    }

    // Search by key only — should find 2 blobs.
    let results = ops::search_objects_by_tag(&env.db, "category", None).await.unwrap();
    assert_eq!(results.len(), 2);

    // Search by key + value — should find exactly 1.
    let results = ops::search_objects_by_tag(&env.db, "category", Some("alpha")).await.unwrap();
    assert_eq!(results.len(), 1);
}

/// `tome tag search` CLI runs without error.
#[tokio::test]
async fn tag_search_cli_runs() {
    let env = Env::new().await;
    env.write("file.txt", b"content");
    env.scan().await.unwrap();

    let digest = env.first_blob_digest_hex().await;
    env.tag_set(&digest, "status", Some("ok")).await.unwrap();

    // Both with and without value should succeed.
    env.tag_search("status", None).await.unwrap();
    env.tag_search("status", Some("ok")).await.unwrap();
    env.tag_search("nonexistent", None).await.unwrap();
}
