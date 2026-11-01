use avicull::db::*;
use avicull::storage::{FileLocality, StorageBackend, StorageKind};
use avicull::types::*;
use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tempfile::tempdir;

struct MockFailingStorageBackend {
    pub fail_path: PathBuf,
    pub call_count: Arc<AtomicUsize>,
}

impl StorageBackend for MockFailingStorageBackend {
    fn kind(&self) -> StorageKind {
        StorageKind::MacOSCloudStorage
    }

    fn inspect_locality(&self, path: &Path) -> io::Result<FileLocality> {
        let is_dataless = path == self.fail_path;
        let size = if path.exists() {
            std::fs::metadata(path).map(|m| m.len()).unwrap_or(1024)
        } else {
            1024
        };
        Ok(FileLocality {
            is_dataless,
            pre_existing_local: !is_dataless,
            file_size_bytes: size,
        })
    }

    fn materialize(&self, path: &Path) -> io::Result<PathBuf> {
        if path == self.fail_path {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Simulated cloud network timeout",
            ));
        }
        Ok(path.to_path_buf())
    }

    fn evict(&self, _path: &Path) -> io::Result<()> {
        Ok(())
    }

    fn rename_asset(&self, src: &Path, dst: &Path) -> io::Result<()> {
        std::fs::rename(src, dst)
    }
}

#[test]
fn test_db_was_dataless_schema_and_pending_failures() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("manifest.sqlite");

    let db = ManifestDb::open_or_create(&db_path).unwrap();
    let run_id = "run-test-cloud";
    db.record_run(run_id, dir.path(), dir.path(), false, "{}")
        .unwrap();

    let bundle1 = AssetBundle {
        bundle_id: "b1".to_string(),
        source_inode: 101,
        trip_folder_name: "Trip1".to_string(),
        original_stem: "DSC_0001".to_string(),
        primary_image: dir.path().join("DSC_0001.JPG"),
        companion_images: vec![],
        sidecar_files: vec![],
        capture_timestamp_ms: None,
        gps_coords: None,
        resolved_location_name: None,
        relative_path: None,
    };
    let bundle2 = AssetBundle {
        bundle_id: "b2".to_string(),
        source_inode: 102,
        trip_folder_name: "Trip1".to_string(),
        original_stem: "DSC_0002".to_string(),
        primary_image: dir.path().join("DSC_0002.JPG"),
        companion_images: vec![],
        sidecar_files: vec![],
        capture_timestamp_ms: None,
        gps_coords: None,
        resolved_location_name: None,
        relative_path: None,
    };

    let decision1 = BundleDecision {
        bundle_id: "b1".to_string(),
        burst_id: None,
        detection_confidence: 0.9,
        bbox_xyxy_norm: None,
        top_predictions: vec![],
        frame_confidence: 0.9,
        final_confidence: 0.9,
        assigned_species_slug: "House_Sparrow".to_string(),
        routing_tier: RoutingTier::ConfirmedSpecies,
        is_user_override: false,
    };

    // Record bundle with was_dataless = true
    db.record_bundles_with_locality(run_id, &[decision1], &[bundle1], &[true])
        .unwrap();

    // Verify was_dataless can be queried
    assert_eq!(db.get_bundle_was_dataless("b1").unwrap(), Some(true));

    // Record pending download failure for bundle 2
    db.record_pending_download_failure(
        run_id,
        &bundle2,
        "Network connection timed out after retry",
    )
    .unwrap();

    let pending = db.get_pending_failures(Some(run_id)).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].bundle_id, "b2");
    assert!(pending[0].error_message.contains("timed out"));
}

#[test]
fn test_transient_failure_records_pending_and_resumes() {
    let dir = tempdir().unwrap();
    let input_dir = dir.path().join("Photos");
    std::fs::create_dir_all(&input_dir).unwrap();

    // Create 5 files
    let mut files = Vec::new();
    for i in 1..=5 {
        let fpath = input_dir.join(format!("photo_{}.jpg", i));
        let mut f = File::create(&fpath).unwrap();
        f.write_all(format!("test image data {}", i).as_bytes())
            .unwrap();
        files.push(fpath);
    }

    let failing_file = files[2].clone(); // photo_3 fails
    let call_count = Arc::new(AtomicUsize::new(0));
    let backend = MockFailingStorageBackend {
        fail_path: failing_file.clone(),
        call_count: Arc::clone(&call_count),
    };

    // Run organize_chunk_with_resilience helper
    let db_path = input_dir.join(".avicull/manifest.sqlite");
    let db = ManifestDb::open_or_create(&db_path).unwrap();
    let run_id = "run-test-resilience";
    db.record_run(run_id, &input_dir, &input_dir, false, "{}")
        .unwrap();

    let bundles: Vec<AssetBundle> = (1..=5)
        .map(|i| AssetBundle {
            bundle_id: format!("bundle_{}", i),
            source_inode: 200 + i as u64,
            trip_folder_name: "Photos".to_string(),
            original_stem: format!("photo_{}", i),
            primary_image: files[i - 1].clone(),
            companion_images: vec![],
            sidecar_files: vec![],
            capture_timestamp_ms: None,
            gps_coords: None,
            resolved_location_name: None,
            relative_path: None,
        })
        .collect();

    let outcome = avicull::organizer::materialize_chunk_with_retry(
        &bundles,
        &backend,
        &db,
        run_id,
        std::time::Duration::from_millis(10), // fast backoff for tests
    );

    // Call count for failing file should be 2 (initial + 1 retry)
    assert_eq!(call_count.load(Ordering::SeqCst), 2);

    // 4 bundles successfully materialized, 1 bundle failed
    assert_eq!(outcome.successful_bundles.len(), 4);
    assert_eq!(outcome.failed_bundle_ids.len(), 1);
    assert_eq!(outcome.failed_bundle_ids[0], "bundle_3");

    // Check DB recorded pending failure
    let pending = db.get_pending_failures(Some(run_id)).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].bundle_id, "bundle_3");
}

#[test]
fn test_cli_organize_summary_output() {
    use std::process::Command;

    let dir = tempdir().unwrap();
    let input_dir = dir.path().join("Photos");
    std::fs::create_dir_all(&input_dir).unwrap();

    let img1 = input_dir.join("photo1.jpg");
    File::create(&img1)
        .unwrap()
        .write_all(b"test jpeg data 1")
        .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_avicull"))
        .args([
            "organize",
            "--input",
            input_dir.to_str().unwrap(),
            "--no-prompt",
        ])
        .output()
        .expect("run avicull organize");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Done!"),
        "stdout should show completion: {}",
        stdout
    );
}
