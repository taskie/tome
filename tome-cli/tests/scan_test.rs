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
    let args = ScanArgs {
        repo: "default".to_owned(),
        path: Some(dir.to_path_buf()),
        no_ignore: false,
        message: String::new(),
        digest_algorithm: Default::default(),
        fast_hash_algorithm: Default::default(),
        batch_size: 1000,
    };
    scan_run(db, args).await.expect("scan failed");
}

/// Run a scan with a custom repo name.
async fn scan_repo(db: &sea_orm::DatabaseConnection, repo: &str, dir: &std::path::Path) {
    let args = ScanArgs {
        repo: repo.to_owned(),
        path: Some(dir.to_path_buf()),
        no_ignore: false,
        message: String::new(),
        digest_algorithm: Default::default(),
        fast_hash_algorithm: Default::default(),
        batch_size: 1000,
    };
    scan_run(db, args).await.expect("scan failed");
}

/// Run a scan with no_ignore=true.
async fn scan_no_ignore(db: &sea_orm::DatabaseConnection, dir: &std::path::Path) {
    let args = ScanArgs {
        repo: "default".to_owned(),
        path: Some(dir.to_path_buf()),
        no_ignore: true,
        message: String::new(),
        digest_algorithm: Default::default(),
        fast_hash_algorithm: Default::default(),
        batch_size: 1000,
    };
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
        assert!(entry.object_id.is_some(), "blob_id should be set");
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
    let blob_id_before = cache_before["file.txt"].object_id;

    scan(&db, files_dir.path()).await; // second scan — file should be unchanged

    // Two snapshots should exist.
    let snapshots = ops::snapshots_after(&db, repo.id, None).await.unwrap();
    assert_eq!(snapshots.len(), 2, "expected 2 snapshots");

    // Unchanged file does NOT produce a new entry in snapshot2 — only added/modified files do.
    // But entry_cache should still point to the same blob.
    let cache_after = ops::load_entry_cache(&db, repo.id).await.unwrap();
    let blob_id_after = cache_after["file.txt"].object_id;
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
    assert_ne!(s1_entries[0].object_id, s2_entries[0].object_id, "blob_id should differ after modification");
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

#[tokio::test]
async fn test_scan_respects_gitignore() {
    let db_dir = tempfile::tempdir().unwrap();
    let files_dir = tempfile::tempdir().unwrap();
    let db = open_test_db(&db_dir).await;

    // The `ignore` crate requires a .git directory to recognize .gitignore.
    std::fs::create_dir(files_dir.path().join(".git")).unwrap();
    std::fs::write(files_dir.path().join(".gitignore"), b"ignored.txt\n").unwrap();
    std::fs::write(files_dir.path().join("tracked.txt"), b"tracked").unwrap();
    std::fs::write(files_dir.path().join("ignored.txt"), b"ignored").unwrap();

    scan(&db, files_dir.path()).await;

    let repo = ops::get_or_create_repository(&db, "default").await.unwrap();
    let cache = ops::load_entry_cache(&db, repo.id).await.unwrap();

    assert!(cache.contains_key("tracked.txt"));
    assert!(cache.contains_key(".gitignore"));
    assert!(!cache.contains_key("ignored.txt"), "ignored.txt should not be scanned");
}

#[tokio::test]
async fn test_scan_no_ignore_overrides_gitignore() {
    let db_dir = tempfile::tempdir().unwrap();
    let files_dir = tempfile::tempdir().unwrap();
    let db = open_test_db(&db_dir).await;

    std::fs::create_dir(files_dir.path().join(".git")).unwrap();
    std::fs::write(files_dir.path().join(".gitignore"), b"ignored.txt\n").unwrap();
    std::fs::write(files_dir.path().join("tracked.txt"), b"tracked").unwrap();
    std::fs::write(files_dir.path().join("ignored.txt"), b"ignored").unwrap();

    scan_no_ignore(&db, files_dir.path()).await;

    let repo = ops::get_or_create_repository(&db, "default").await.unwrap();
    let cache = ops::load_entry_cache(&db, repo.id).await.unwrap();

    assert!(cache.contains_key("tracked.txt"));
    assert!(cache.contains_key("ignored.txt"), "ignored.txt should be scanned with --no-ignore");
}

#[tokio::test]
async fn test_scan_multiple_repos() {
    let db_dir = tempfile::tempdir().unwrap();
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let db = open_test_db(&db_dir).await;

    std::fs::write(dir_a.path().join("a.txt"), b"alpha").unwrap();
    std::fs::write(dir_b.path().join("b.txt"), b"bravo").unwrap();

    scan_repo(&db, "repo_a", dir_a.path()).await;
    scan_repo(&db, "repo_b", dir_b.path()).await;

    let repo_a = ops::get_or_create_repository(&db, "repo_a").await.unwrap();
    let repo_b = ops::get_or_create_repository(&db, "repo_b").await.unwrap();
    assert_ne!(repo_a.id, repo_b.id);

    let cache_a = ops::load_entry_cache(&db, repo_a.id).await.unwrap();
    let cache_b = ops::load_entry_cache(&db, repo_b.id).await.unwrap();
    assert_eq!(cache_a.len(), 1);
    assert_eq!(cache_b.len(), 1);
    assert!(cache_a.contains_key("a.txt"));
    assert!(cache_b.contains_key("b.txt"));
}

#[tokio::test]
async fn test_scan_empty_directory() {
    let db_dir = tempfile::tempdir().unwrap();
    let files_dir = tempfile::tempdir().unwrap();
    let db = open_test_db(&db_dir).await;

    scan(&db, files_dir.path()).await;

    let repo = ops::get_or_create_repository(&db, "default").await.unwrap();
    let cache = ops::load_entry_cache(&db, repo.id).await.unwrap();
    assert!(cache.is_empty(), "empty directory should produce no cache entries");

    let snapshot = ops::latest_snapshot(&db, repo.id).await.unwrap().unwrap();
    let entries = ops::entries_in_snapshot(&db, snapshot.id).await.unwrap();
    assert!(entries.is_empty());
}

#[tokio::test]
async fn test_scan_identical_content_shares_blob() {
    let db_dir = tempfile::tempdir().unwrap();
    let files_dir = tempfile::tempdir().unwrap();
    let db = open_test_db(&db_dir).await;

    let content = b"identical content";
    std::fs::write(files_dir.path().join("file_a.txt"), content).unwrap();
    std::fs::write(files_dir.path().join("file_b.txt"), content).unwrap();

    scan(&db, files_dir.path()).await;

    let repo = ops::get_or_create_repository(&db, "default").await.unwrap();
    let cache = ops::load_entry_cache(&db, repo.id).await.unwrap();
    let blob_a = cache["file_a.txt"].object_id;
    let blob_b = cache["file_b.txt"].object_id;
    assert_eq!(blob_a, blob_b, "files with identical content should share the same blob");
}

#[tokio::test]
async fn test_scan_snapshot_parent_chain() {
    let db_dir = tempfile::tempdir().unwrap();
    let files_dir = tempfile::tempdir().unwrap();
    let db = open_test_db(&db_dir).await;

    std::fs::write(files_dir.path().join("f.txt"), b"v1").unwrap();
    scan(&db, files_dir.path()).await;

    std::thread::sleep(std::time::Duration::from_millis(10));
    std::fs::write(files_dir.path().join("f.txt"), b"v2").unwrap();
    scan(&db, files_dir.path()).await;

    std::thread::sleep(std::time::Duration::from_millis(10));
    std::fs::write(files_dir.path().join("f.txt"), b"v3").unwrap();
    scan(&db, files_dir.path()).await;

    let repo = ops::get_or_create_repository(&db, "default").await.unwrap();
    let mut snapshots = ops::snapshots_after(&db, repo.id, None).await.unwrap();
    assert_eq!(snapshots.len(), 3);

    snapshots.sort_by_key(|s| s.id);
    // First snapshot has no parent; subsequent ones chain to the previous.
    assert!(snapshots[0].parent_id.is_none(), "first snapshot should have no parent");
    assert_eq!(snapshots[1].parent_id, Some(snapshots[0].id));
    assert_eq!(snapshots[2].parent_id, Some(snapshots[1].id));
}
