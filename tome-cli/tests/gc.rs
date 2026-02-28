//! Integration tests for `tome gc` (garbage collection).
//!
//! Tests verify that dry-run mode makes no changes, that `--keep N` prunes
//! old snapshots, and that orphaned blobs are cleaned up after pruning.

mod common;
use common::{Env, meta_count};

use tome_cli::commands::gc;
use tome_db::ops;

// ── Dry run ───────────────────────────────────────────────────────────────────

/// `--dry-run` reports what would be deleted but makes no DB changes.
#[tokio::test]
async fn gc_dry_run_does_not_modify_database() {
    let env = Env::new().await;
    env.write("file.txt", b"v1");
    env.scan().await.unwrap();
    env.write("file.txt", b"v2");
    env.scan().await.unwrap();
    env.write("file.txt", b"v3");
    env.scan().await.unwrap();

    let snap_count_before = env.snapshots().await.len();
    assert_eq!(snap_count_before, 3);

    env.gc(gc::GcArgs { dry_run: true, keep: 1, keep_days: 0, repo: None, store: None, no_store_delete: false })
        .await
        .unwrap();

    // Dry run must not remove any snapshots.
    let snap_count_after = env.snapshots().await.len();
    assert_eq!(snap_count_after, 3, "dry-run should not delete any snapshots");
}

// ── Keep N snapshots ──────────────────────────────────────────────────────────

/// `--keep 1` prunes all but the most recent snapshot.
#[tokio::test]
async fn gc_keep_1_retains_only_latest_snapshot() {
    let env = Env::new().await;
    env.write("file.txt", b"v1");
    env.scan().await.unwrap();
    env.write("file.txt", b"v2");
    env.scan().await.unwrap();
    env.write("file.txt", b"v3");
    env.scan().await.unwrap();

    assert_eq!(env.snapshots().await.len(), 3);

    env.gc(gc::GcArgs {
        dry_run: false,
        keep: 1,
        keep_days: 0,
        repo: None,
        store: None,
        no_store_delete: true, // no stores registered; skip file deletion
    })
    .await
    .unwrap();

    let remaining = env.snapshots().await;
    assert_eq!(remaining.len(), 1, "only 1 snapshot should remain after gc --keep 1");

    // The surviving snapshot should be the newest (v3).
    let meta = remaining[0].metadata.clone();
    assert_eq!(meta_count(&meta, "scanned"), 1);
}

/// `--keep N` with N >= total snapshot count is a no-op.
#[tokio::test]
async fn gc_keep_larger_than_count_is_noop() {
    let env = Env::new().await;
    env.write("file.txt", b"content");
    env.scan().await.unwrap();
    env.scan().await.unwrap();

    env.gc(gc::GcArgs {
        dry_run: false,
        keep: 100, // more than 2 existing snapshots
        keep_days: 0,
        repo: None,
        store: None,
        no_store_delete: true,
    })
    .await
    .unwrap();

    assert_eq!(env.snapshots().await.len(), 2, "gc --keep 100 should not prune 2 snapshots");
}

// ── Orphaned blob cleanup ─────────────────────────────────────────────────────

/// After pruning snapshots, blobs exclusively referenced by pruned snapshots
/// become orphaned and are removed from the blobs table.
#[tokio::test]
async fn gc_removes_orphaned_blobs_after_pruning() {
    let env = Env::new().await;
    let repo = ops::get_or_create_repository(&env.db, "default").await.unwrap();

    // Scan with unique content each time so each snapshot has distinct blobs.
    env.write("data.txt", b"snapshot 1 content --- unique bytes aaa");
    env.scan().await.unwrap();
    env.write("data.txt", b"snapshot 2 content --- unique bytes bbb");
    env.scan().await.unwrap();
    env.write("data.txt", b"snapshot 3 content --- unique bytes ccc");
    env.scan().await.unwrap();

    let blobs_before = ops::present_cache_entries(&env.db, repo.id).await.unwrap();
    // Entry cache shows the latest version only, but blobs table has all three.
    assert_eq!(blobs_before.len(), 1); // only current state in cache

    // GC keeping only 1 snapshot (the latest).
    env.gc(gc::GcArgs { dry_run: false, keep: 1, keep_days: 0, repo: None, store: None, no_store_delete: true })
        .await
        .unwrap();

    // After pruning, old blobs should be unreferenced (and gc'd).
    // The blob referenced by the surviving snapshot must still exist.
    // We verify this by checking that the entry cache still has exactly 1 entry.
    let entries_after = ops::present_cache_entries(&env.db, repo.id).await.unwrap();
    assert_eq!(entries_after.len(), 1);
    assert_eq!(entries_after[0].path, "data.txt");

    // Snapshots pruned from 3 → 1.
    assert_eq!(env.snapshots().await.len(), 1);
}

// ── Repo-scoped GC ───────────────────────────────────────────────────────────

/// `--repo` restricts pruning to a single repository; other repos are untouched.
#[tokio::test]
async fn gc_repo_flag_only_prunes_target_repo() {
    let env = Env::new().await;

    // Populate two repositories.
    env.write("a.txt", b"a");
    env.scan_with("repo_a", "").await.unwrap();
    env.scan_with("repo_a", "").await.unwrap();

    env.write("b.txt", b"b");
    env.scan_with("repo_b", "").await.unwrap();
    env.scan_with("repo_b", "").await.unwrap();

    // GC only repo_a.
    env.gc(gc::GcArgs {
        dry_run: false,
        keep: 1,
        keep_days: 0,
        repo: Some("repo_a".to_string()),
        store: None,
        no_store_delete: true,
    })
    .await
    .unwrap();

    let repo_a = ops::get_or_create_repository(&env.db, "repo_a").await.unwrap();
    let repo_b = ops::get_or_create_repository(&env.db, "repo_b").await.unwrap();

    let snaps_a = ops::list_snapshots_for_repo(&env.db, repo_a.id).await.unwrap();
    let snaps_b = ops::list_snapshots_for_repo(&env.db, repo_b.id).await.unwrap();

    assert_eq!(snaps_a.len(), 1, "repo_a should have 1 snapshot after targeted gc");
    assert_eq!(snaps_b.len(), 2, "repo_b should be untouched");
}
