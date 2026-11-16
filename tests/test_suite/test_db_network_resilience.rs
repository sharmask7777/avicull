//! Tests verifying that ManifestDb handles SQLite opening and journal modes
//! resiliently across network shares, WAL failure, and rollback modes.

use avicull::db::ManifestDb;
use rusqlite::Connection;
use tempfile::tempdir;

#[test]
fn test_manifest_db_opens_and_operates_with_delete_journal_mode() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("manifest.sqlite");

    // Initialize database in standard WAL mode first
    {
        let _db = ManifestDb::open_or_create(&db_path).expect("open_or_create initial");
        assert!(db_path.exists());
    }

    // Force database to DELETE journal mode (simulating network mount conversion)
    {
        let conn = Connection::open(&db_path).expect("open connection");
        conn.execute_batch("PRAGMA journal_mode = DELETE;")
            .expect("set DELETE mode");
    }

    // Verify ManifestDb can open and operate on a DELETE mode database without error
    let db = ManifestDb::open(&db_path).expect("open db in DELETE mode");
    let bundles = db.get_all_bundles(None).expect("get bundles");
    assert_eq!(bundles.len(), 0);

    // Verify open_or_create preserves DELETE mode when opened again
    let db2 = ManifestDb::open_or_create(&db_path).expect("open_or_create in DELETE mode");
    let bundles2 = db2.get_all_bundles(None).expect("get bundles 2");
    assert_eq!(bundles2.len(), 0);

    // Verify journal mode is still DELETE
    let raw = db2.raw_conn().lock().expect("lock conn");
    let journal_mode: String = raw
        .query_row("PRAGMA journal_mode;", [], |r| r.get(0))
        .expect("query journal_mode");
    assert_eq!(journal_mode.to_lowercase(), "delete");
}
