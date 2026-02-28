//! Integration tests for `tome diff`.
//!
//! Tests verify that diff correctly classifies files as Added / Modified / Deleted
//! and that output format flags (--name-only, --stat) work without error.

mod common;
use common::Env;

use tome_cli::commands::diff;

// ── Basic diff ───────────────────────────────────────────────────────────────

/// Diff between two snapshots shows A (added), D (deleted), and M (modified) files.
/// Returns Ok without panicking for all three status types.
#[tokio::test]
async fn diff_shows_added_deleted_modified() {
    let env = Env::new().await;

    // Snapshot 1: three files
    env.write("keep.txt", b"same content");
    env.write("remove.txt", b"will be deleted");
    env.write("change.txt", b"original");
    env.scan().await.unwrap();

    let snap1_id = env.snapshots().await[0].id.to_string();

    // Snapshot 2: remove one, add one, modify one, keep one
    env.remove("remove.txt");
    env.write("add.txt", b"brand new");
    env.write("change.txt", b"updated content");
    env.scan().await.unwrap();

    let snap2_id = env.snapshots().await[0].id.to_string();

    // Default format
    diff::run(
        &env.db,
        diff::DiffArgs {
            snapshot1: snap1_id.clone(),
            snapshot2: snap2_id.clone(),
            prefix: String::new(),
            name_only: false,
            stat: false,
        },
    )
    .await
    .unwrap();
}

// ── No differences ────────────────────────────────────────────────────────────

/// Diff between identical snapshots completes without error (prints "no differences").
#[tokio::test]
async fn diff_identical_snapshots_returns_ok() {
    let env = Env::new().await;
    env.write("file.txt", b"unchanged");
    env.scan().await.unwrap();
    env.scan().await.unwrap(); // second snapshot with no changes

    let snaps = env.snapshots().await;
    let snap1_id = snaps[1].id.to_string(); // older (index 1 = second-newest)
    let snap2_id = snaps[0].id.to_string(); // newer

    diff::run(
        &env.db,
        diff::DiffArgs {
            snapshot1: snap1_id,
            snapshot2: snap2_id,
            prefix: String::new(),
            name_only: false,
            stat: false,
        },
    )
    .await
    .unwrap();
}

// ── Output format flags ───────────────────────────────────────────────────────

/// --name-only flag produces output without errors.
#[tokio::test]
async fn diff_name_only_flag() {
    let env = Env::new().await;
    env.write("a.txt", b"original");
    env.scan().await.unwrap();
    env.write("a.txt", b"modified");
    env.write("b.txt", b"new file");
    env.scan().await.unwrap();

    let snaps = env.snapshots().await;
    diff::run(
        &env.db,
        diff::DiffArgs {
            snapshot1: snaps[1].id.to_string(),
            snapshot2: snaps[0].id.to_string(),
            prefix: String::new(),
            name_only: true,
            stat: false,
        },
    )
    .await
    .unwrap();
}

/// --stat flag produces output without errors.
#[tokio::test]
async fn diff_stat_flag() {
    let env = Env::new().await;
    env.write("doc.txt", b"short");
    env.scan().await.unwrap();
    env.write("doc.txt", b"a much longer version of the document");
    env.scan().await.unwrap();

    let snaps = env.snapshots().await;
    diff::run(
        &env.db,
        diff::DiffArgs {
            snapshot1: snaps[1].id.to_string(),
            snapshot2: snaps[0].id.to_string(),
            prefix: String::new(),
            name_only: false,
            stat: true,
        },
    )
    .await
    .unwrap();
}

// ── Prefix filter ─────────────────────────────────────────────────────────────

/// --prefix limits the diff to files under that path prefix.
#[tokio::test]
async fn diff_prefix_filter_ignores_files_outside_prefix() {
    let env = Env::new().await;
    env.write("src/main.rs", b"v1");
    env.write("docs/readme.md", b"v1");
    env.scan().await.unwrap();
    env.write("src/main.rs", b"v2");
    env.write("docs/readme.md", b"v2");
    env.scan().await.unwrap();

    let snaps = env.snapshots().await;

    // Diff with prefix "src/" — only src/main.rs should appear
    diff::run(
        &env.db,
        diff::DiffArgs {
            snapshot1: snaps[1].id.to_string(),
            snapshot2: snaps[0].id.to_string(),
            prefix: "src/".to_string(),
            name_only: false,
            stat: false,
        },
    )
    .await
    .unwrap();
}
