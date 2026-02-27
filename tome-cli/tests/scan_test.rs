use tome_cli::commands::scan::{ScanArgs, run as scan_run};
use tome_db::{connection, ops};

/// Open a SQLite DB in its own tempdir (separate from the files being scanned).
async fn open_test_db(db_dir: &tempfile::TempDir) -> sea_orm::DatabaseConnection {
    let db_path = db_dir.path().join("test.db");
    let url = format!("sqlite://{}?mode=rwc", db_path.display());
    connection::open(&url).await.expect("failed to open test DB")
}

/// Run a scan of `dir` using repo `"default"`.
async fn scan(db: &sea_orm::DatabaseConnection, dir: &std::path::Path) {
    let args = ScanArgs { repo: "default".to_owned(), path: Some(dir.to_path_buf()) };
    scan_run(db, args).await.expect("scan failed");
}

// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_scan_adds_files() {
    let db_dir = tempfile::tempdir().unwrap();
    let files_dir = tempfile::tempdir().unwrap();
    let db = open_test_db(&db_dir).await;

    std::fs::write(files_dir.path().join("hello.txt"), b"hello world").unwrap();
    std::fs::write(files_dir.path().join("data.bin"), b"\x00\x01\x02").unwrap();

    scan(&db, files_dir.path()).await;

    let repo = ops::get_or_create_repository(&db, "default").await.unwrap();
    let cache = ops::load_entry_cache(&db, repo.id).await.unwrap();

    assert_eq!(cache.len(), 2, "expected exactly 2 entries; got {:?}", cache.keys().collect::<Vec<_>>());
    assert!(cache.contains_key("hello.txt"));
    assert!(cache.contains_key("data.bin"));

    for entry in cache.values() {
        assert_eq!(entry.status, 1, "expected status=1 (present)");
        assert!(entry.blob_id.is_some(), "blob_id should be set");
        assert!(entry.digest.is_some(), "digest should be set");
        assert!(entry.size.is_some(), "size should be set");
    }

    let snapshot = ops::latest_snapshot(&db, repo.id).await.unwrap().unwrap();
    let entries = ops::entries_in_snapshot(&db, snapshot.id).await.unwrap();
    assert_eq!(entries.len(), 2);
}

#[tokio::test]
async fn test_rescan_unchanged() {
    let db_dir = tempfile::tempdir().unwrap();
    let files_dir = tempfile::tempdir().unwrap();
    let db = open_test_db(&db_dir).await;

    std::fs::write(files_dir.path().join("file.txt"), b"unchanged content").unwrap();

    scan(&db, files_dir.path()).await;

    // Capture blob_id from entry_cache after first scan.
    let repo = ops::get_or_create_repository(&db, "default").await.unwrap();
    let cache_before = ops::load_entry_cache(&db, repo.id).await.unwrap();
    let blob_id_before = cache_before["file.txt"].blob_id;

    scan(&db, files_dir.path()).await; // second scan — file should be unchanged

    // Two snapshots should exist.
    let snapshots = ops::snapshots_after(&db, repo.id, None).await.unwrap();
    assert_eq!(snapshots.len(), 2, "expected 2 snapshots");

    // Unchanged file does NOT produce a new entry in snapshot2 — only added/modified files do.
    // But entry_cache should still point to the same blob.
    let cache_after = ops::load_entry_cache(&db, repo.id).await.unwrap();
    let blob_id_after = cache_after["file.txt"].blob_id;
    assert_eq!(blob_id_before, blob_id_after, "blob_id should be identical for unchanged file");

    // The first snapshot should have the entry; the second should have no entries (all unchanged).
    let mut sorted = snapshots;
    sorted.sort_by_key(|s| s.id);
    let s1_entries = ops::entries_in_snapshot(&db, sorted[0].id).await.unwrap();
    let s2_entries = ops::entries_in_snapshot(&db, sorted[1].id).await.unwrap();
    assert_eq!(s1_entries.len(), 1, "first snapshot should have 1 entry");
    assert_eq!(s2_entries.len(), 0, "second snapshot should have 0 entries (all unchanged)");
}

#[tokio::test]
async fn test_scan_detects_modification() {
    let db_dir = tempfile::tempdir().unwrap();
    let files_dir = tempfile::tempdir().unwrap();
    let db = open_test_db(&db_dir).await;
    let file = files_dir.path().join("changing.txt");

    std::fs::write(&file, b"version 1").unwrap();
    scan(&db, files_dir.path()).await;

    // Ensure mtime differs.
    std::thread::sleep(std::time::Duration::from_millis(10));
    std::fs::write(&file, b"version 2").unwrap();
    scan(&db, files_dir.path()).await;

    let repo = ops::get_or_create_repository(&db, "default").await.unwrap();
    let mut snapshots = ops::snapshots_after(&db, repo.id, None).await.unwrap();
    assert_eq!(snapshots.len(), 2);
    snapshots.sort_by_key(|s| s.id);

    let s1_entries = ops::entries_in_snapshot(&db, snapshots[0].id).await.unwrap();
    let s2_entries = ops::entries_in_snapshot(&db, snapshots[1].id).await.unwrap();
    assert_ne!(s1_entries[0].blob_id, s2_entries[0].blob_id, "blob_id should differ after modification");
}

#[tokio::test]
async fn test_scan_detects_deletion() {
    let db_dir = tempfile::tempdir().unwrap();
    let files_dir = tempfile::tempdir().unwrap();
    let db = open_test_db(&db_dir).await;
    let file = files_dir.path().join("to_delete.txt");

    std::fs::write(&file, b"will be deleted").unwrap();
    scan(&db, files_dir.path()).await;

    std::fs::remove_file(&file).unwrap();
    scan(&db, files_dir.path()).await;

    let repo = ops::get_or_create_repository(&db, "default").await.unwrap();
    let cache = ops::load_entry_cache(&db, repo.id).await.unwrap();
    let entry = cache.get("to_delete.txt").unwrap();
    assert_eq!(entry.status, 0, "expected status=0 (deleted)");
}

#[tokio::test]
async fn test_scan_subdirectory() {
    let db_dir = tempfile::tempdir().unwrap();
    let files_dir = tempfile::tempdir().unwrap();
    let db = open_test_db(&db_dir).await;

    std::fs::create_dir(files_dir.path().join("subdir")).unwrap();
    std::fs::write(files_dir.path().join("root.txt"), b"root").unwrap();
    std::fs::write(files_dir.path().join("subdir").join("nested.txt"), b"nested").unwrap();

    scan(&db, files_dir.path()).await;

    let repo = ops::get_or_create_repository(&db, "default").await.unwrap();
    let cache = ops::load_entry_cache(&db, repo.id).await.unwrap();
    assert_eq!(cache.len(), 2, "expected 2 entries; got {:?}", cache.keys().collect::<Vec<_>>());
    assert!(cache.contains_key("root.txt"));
    assert!(
        cache.keys().any(|k| k.ends_with("nested.txt")),
        "nested file should be in cache; keys: {:?}",
        cache.keys().collect::<Vec<_>>()
    );
}
