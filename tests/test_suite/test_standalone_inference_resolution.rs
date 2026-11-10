//! Tests for standalone inference runner resolution and embedded assets extraction.

use avicull::classifier::{
    resolve_inference_script, resolve_inference_script_in, sync_regional_checklists,
    EMBEDDED_INFERENCE_SCRIPT, EMBEDDED_REGIONAL_CHECKLISTS_JSON,
};
use std::fs;
use tempfile::tempdir;

#[test]
fn test_embedded_assets_are_non_empty_and_valid() {
    assert!(
        !EMBEDDED_INFERENCE_SCRIPT.trim().is_empty(),
        "Embedded infer.py must not be empty"
    );
    assert!(
        EMBEDDED_INFERENCE_SCRIPT.contains("class DetectionBox")
            || EMBEDDED_INFERENCE_SCRIPT.contains("BioCLIP")
            || EMBEDDED_INFERENCE_SCRIPT.contains("def main"),
        "Embedded infer.py should contain python inference definitions"
    );

    assert!(
        !EMBEDDED_REGIONAL_CHECKLISTS_JSON.trim().is_empty(),
        "Embedded regional_checklists.json must not be empty"
    );
    let parsed: serde_json::Value = serde_json::from_str(EMBEDDED_REGIONAL_CHECKLISTS_JSON)
        .expect("Embedded regional_checklists.json must be valid JSON");
    assert!(
        parsed.is_object(),
        "regional_checklists.json must deserialize to a JSON object"
    );
}

#[test]
fn test_resolve_inference_script_from_cwd() {
    let cwd_dir = tempdir().expect("Failed to create cwd tempdir");
    let cache_dir = tempdir().expect("Failed to create cache tempdir");

    let scripts_dir = cwd_dir.path().join("scripts");
    fs::create_dir_all(&scripts_dir).expect("Failed to create scripts dir");
    let local_infer = scripts_dir.join("infer.py");
    fs::write(&local_infer, "# local dev infer.py").expect("Failed to write dummy infer.py");

    let resolved = resolve_inference_script_in(cwd_dir.path(), None, cache_dir.path())
        .expect("Should resolve from cwd");
    assert_eq!(resolved, local_infer);
    assert_eq!(
        fs::read_to_string(&resolved).expect("read file"),
        "# local dev infer.py"
    );
}

#[test]
fn test_resolve_inference_script_from_exe_dir() {
    let cwd_dir = tempdir().expect("Failed to create cwd tempdir");
    let exe_dir = tempdir().expect("Failed to create exe tempdir");
    let cache_dir = tempdir().expect("Failed to create cache tempdir");

    let exe_scripts_dir = exe_dir.path().join("scripts");
    fs::create_dir_all(&exe_scripts_dir).expect("Failed to create exe scripts dir");
    let exe_infer = exe_scripts_dir.join("infer.py");
    fs::write(&exe_infer, "# exe next-to-binary infer.py").expect("Failed to write exe infer.py");

    let resolved =
        resolve_inference_script_in(cwd_dir.path(), Some(exe_dir.path()), cache_dir.path())
            .expect("Should resolve from exe_dir");
    assert_eq!(resolved, exe_infer);
    assert_eq!(
        fs::read_to_string(&resolved).expect("read file"),
        "# exe next-to-binary infer.py"
    );
}

#[test]
fn test_resolve_inference_script_extracts_to_cache_when_missing() {
    let cwd_dir = tempdir().expect("Failed to create cwd tempdir");
    let cache_dir = tempdir().expect("Failed to create cache tempdir");

    let target_cache_infer = cache_dir.path().join("infer.py");
    assert!(
        !target_cache_infer.exists(),
        "Target cache script should not exist initially"
    );

    let resolved = resolve_inference_script_in(cwd_dir.path(), None, cache_dir.path())
        .expect("Should extract and resolve from cache");

    assert_eq!(resolved, target_cache_infer);
    assert!(resolved.is_file(), "Resolved path must be an existing file");

    let content = fs::read_to_string(&resolved).expect("Failed to read extracted script");
    assert_eq!(
        content, EMBEDDED_INFERENCE_SCRIPT,
        "Extracted script must match embedded payload exactly"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::metadata(&resolved)
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(
            perms & 0o111,
            0o111,
            "Extracted script should have executable permissions (0o755)"
        );
    }
}

#[test]
fn test_resolve_inference_script_updates_stale_cache_when_hash_differs() {
    let cwd_dir = tempdir().expect("Failed to create cwd tempdir");
    let cache_dir = tempdir().expect("Failed to create cache tempdir");

    let target_cache_infer = cache_dir.path().join("infer.py");
    fs::write(&target_cache_infer, "# stale outdated code")
        .expect("Failed to write stale infer.py");

    let resolved = resolve_inference_script_in(cwd_dir.path(), None, cache_dir.path())
        .expect("Should update stale script in cache");

    assert_eq!(resolved, target_cache_infer);
    let updated_content =
        fs::read_to_string(&resolved).expect("Failed to read updated cache script");
    assert_eq!(
        updated_content, EMBEDDED_INFERENCE_SCRIPT,
        "Stale script must be updated to match embedded content"
    );
}

#[test]
fn test_resolve_inference_script_preserves_valid_cache() {
    let cwd_dir = tempdir().expect("Failed to create cwd tempdir");
    let cache_dir = tempdir().expect("Failed to create cache tempdir");

    let target_cache_infer = cache_dir.path().join("infer.py");
    fs::write(&target_cache_infer, EMBEDDED_INFERENCE_SCRIPT)
        .expect("Failed to write matching infer.py");

    let resolved = resolve_inference_script_in(cwd_dir.path(), None, cache_dir.path())
        .expect("Should resolve valid cache script");

    assert_eq!(resolved, target_cache_infer);
    let content = fs::read_to_string(&resolved).expect("Failed to read cache script");
    assert_eq!(content, EMBEDDED_INFERENCE_SCRIPT);
}

#[test]
fn test_sync_regional_checklists_extracts_when_missing() {
    let models_dir = tempdir().expect("Failed to create models tempdir");
    let checklist_file = models_dir.path().join("regional_checklists.json");
    assert!(!checklist_file.exists());

    let synced =
        sync_regional_checklists(models_dir.path()).expect("Should extract regional checklists");
    assert_eq!(synced, checklist_file);
    assert!(checklist_file.is_file());

    let content = fs::read_to_string(&checklist_file).expect("Failed to read extracted checklists");
    assert_eq!(content, EMBEDDED_REGIONAL_CHECKLISTS_JSON);
}

#[test]
fn test_sync_regional_checklists_overwrites_empty_file() {
    let models_dir = tempdir().expect("Failed to create models tempdir");
    let checklist_file = models_dir.path().join("regional_checklists.json");
    fs::write(&checklist_file, "").expect("Failed to write empty checklists file");

    let synced = sync_regional_checklists(models_dir.path())
        .expect("Should overwrite empty regional checklists");
    assert_eq!(synced, checklist_file);
    let content = fs::read_to_string(&checklist_file).expect("Failed to read checklists");
    assert_eq!(content, EMBEDDED_REGIONAL_CHECKLISTS_JSON);
}

#[test]
fn test_sync_regional_checklists_preserves_existing_custom_content() {
    let models_dir = tempdir().expect("Failed to create models tempdir");
    let checklist_file = models_dir.path().join("regional_checklists.json");
    fs::write(&checklist_file, r#"{"TEST-REG": ["custom_taxon"]}"#)
        .expect("Failed to write custom checklists file");

    let synced = sync_regional_checklists(models_dir.path())
        .expect("Should preserve valid existing checklists");
    assert_eq!(synced, checklist_file);
    let content = fs::read_to_string(&checklist_file).expect("Failed to read checklists");
    assert_eq!(content, r#"{"TEST-REG": ["custom_taxon"]}"#);
}

#[test]
fn test_resolve_inference_script_live() {
    let resolved = resolve_inference_script().expect("resolve_inference_script should succeed");
    assert!(
        resolved.is_file(),
        "Live resolved inference script must exist as a file"
    );
    let content = fs::read_to_string(&resolved).expect("Must read resolved script");
    assert!(!content.trim().is_empty());
}
