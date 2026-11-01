//! # Manifest Database & Atomic SQLite Operations
//!
//! Organizes state persistence across avicull runs:
//! - **Schema Versioning**: Embedded SQL migrations for bundle classifications, hardlink journals, and directory locations.
//! - **Two-Phase Journaling**: Hard link creations and in-place file renames are logged with status `"planned"`
//!   before executing disk writes, guaranteeing atomic crash recovery.
//! - **Sub-Second Delta Re-runs**: In-memory [`ExistingBundleMap`] caches previously processed bundles by
//!   APFS inode, relative path, and portable `(trip_folder, original_stem)` keys, bypassing neural inference
//!   on subsequent runs.

use crate::types::{AssetBundle, BundleDecision, OrganizationMode, RoutingTier, SpeciesPrediction};
use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq)]
pub struct HardlinkOperation {
    pub op_id: Option<i64>,
    pub run_id: String,
    pub bundle_id: String,
    pub source_path: PathBuf,
    pub dest_path: PathBuf,
    pub source_inode: u64,
    pub status: String, // "planned", "linked", "unlinked", "renamed", "reverted"
    pub operation_type: String, // "hardlink", "in_place_rename"
    pub executed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PendingFailure {
    pub bundle_id: String,
    pub run_id: String,
    pub primary_path: PathBuf,
    pub error_message: String,
    pub status: String,
    pub failed_at: String,
}

pub struct ManifestDb {
    conn: Arc<Mutex<Connection>>,
}

/// Cache map for existing bundles indexed by bundle_id, source_inode, relative_path, and portable key.
#[derive(Debug, Clone, Default)]
pub struct ExistingBundleMap {
    pub by_id: std::collections::HashMap<String, (BundleDecision, AssetBundle)>,
    pub by_inode: std::collections::HashMap<u64, (BundleDecision, AssetBundle)>,
    pub by_relative_path: std::collections::HashMap<PathBuf, (BundleDecision, AssetBundle)>,
    pub by_portable_key: std::collections::HashMap<(String, String), (BundleDecision, AssetBundle)>,
}

impl ExistingBundleMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
            && self.by_inode.is_empty()
            && self.by_relative_path.is_empty()
            && self.by_portable_key.is_empty()
    }

    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.by_id.contains_key(key)
    }

    pub fn get(&self, key: &str) -> Option<&(BundleDecision, AssetBundle)> {
        self.by_id.get(key)
    }

    pub fn get_by_portable_key(
        &self,
        trip_folder_name: &str,
        original_stem: &str,
    ) -> Option<&(BundleDecision, AssetBundle)> {
        self.by_portable_key
            .get(&(trip_folder_name.to_string(), original_stem.to_string()))
    }

    pub fn get_by_relative_path(
        &self,
        relative_path: &Path,
    ) -> Option<&(BundleDecision, AssetBundle)> {
        self.by_relative_path.get(relative_path)
    }

    pub fn get_by_bundle(&self, bundle: &AssetBundle) -> Option<&(BundleDecision, AssetBundle)> {
        self.by_id
            .get(&bundle.bundle_id)
            .or_else(|| {
                if bundle.source_inode != 0 {
                    self.by_inode.get(&bundle.source_inode)
                } else {
                    None
                }
            })
            .or_else(|| {
                bundle
                    .relative_path
                    .as_ref()
                    .and_then(|rel| self.by_relative_path.get(rel))
            })
            .or_else(|| {
                self.by_portable_key.get(&(
                    bundle.trip_folder_name.clone(),
                    bundle.original_stem.clone(),
                ))
            })
            .or_else(|| {
                self.by_portable_key
                    .iter()
                    .find_map(|((trip, stem), entry)| {
                        if *trip == bundle.trip_folder_name
                            && (bundle.original_stem == *stem
                                || bundle.original_stem.starts_with(&format!("{stem}_")))
                        {
                            Some(entry)
                        } else {
                            None
                        }
                    })
            })
    }

    /// Removes a bundle from all map indices.
    pub fn remove(&mut self, bundle_id: &str) -> Option<(BundleDecision, AssetBundle)> {
        if let Some(removed) = self.by_id.remove(bundle_id) {
            let inode = removed.1.source_inode;
            let rel_path = removed.1.relative_path.clone();
            let key = (
                removed.1.trip_folder_name.clone(),
                removed.1.original_stem.clone(),
            );
            if inode != 0 {
                self.by_inode.remove(&inode);
            }
            if let Some(rel) = rel_path {
                self.by_relative_path.remove(&rel);
            }
            self.by_portable_key.remove(&key);
            Some(removed)
        } else {
            None
        }
    }

    /// Updates the decision for a bundle across all map indices.
    pub fn update_decision(&mut self, bundle_id: &str, new_decision: BundleDecision) {
        if let Some(entry) = self.by_id.get_mut(bundle_id) {
            entry.0 = new_decision.clone();
            let inode = entry.1.source_inode;
            let rel_path = entry.1.relative_path.clone();
            let key = (
                entry.1.trip_folder_name.clone(),
                entry.1.original_stem.clone(),
            );

            if inode != 0 {
                if let Some(inode_entry) = self.by_inode.get_mut(&inode) {
                    inode_entry.0 = new_decision.clone();
                }
            }
            if let Some(rel) = rel_path {
                if let Some(rel_entry) = self.by_relative_path.get_mut(&rel) {
                    rel_entry.0 = new_decision.clone();
                }
            }
            if let Some(key_entry) = self.by_portable_key.get_mut(&key) {
                key_entry.0 = new_decision;
            }
        }
    }
}

impl ManifestDb {
    /// Opens or creates the SQLite manifest database and initializes schemas.
    pub fn open_or_create(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(db_path)
            .with_context(|| format!("Failed to open SQLite db at {}", db_path.display()))?;

        // Initialize SQLite tables
        conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS runs (
                run_id TEXT PRIMARY KEY,
                started_at TEXT NOT NULL,
                completed_at TEXT,
                input_root TEXT NOT NULL,
                output_root TEXT NOT NULL,
                dry_run INTEGER NOT NULL,
                config_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS bundles (
                bundle_id TEXT PRIMARY KEY,
                run_id TEXT NOT NULL,
                source_inode INTEGER NOT NULL,
                trip_folder TEXT NOT NULL,
                original_stem TEXT NOT NULL,
                capture_timestamp_ms INTEGER,
                lat REAL,
                lon REAL,
                location_source TEXT,
                burst_id TEXT,
                detection_conf REAL,
                bbox_json TEXT,
                assigned_species_slug TEXT NOT NULL,
                frame_conf REAL NOT NULL,
                final_conf REAL NOT NULL,
                routing_tier TEXT NOT NULL,
                top5_predictions_json TEXT NOT NULL,
                bundle_json TEXT NOT NULL,
                is_user_override INTEGER NOT NULL DEFAULT 0,
                relative_path TEXT,
                was_dataless INTEGER DEFAULT 0,
                FOREIGN KEY(run_id) REFERENCES runs(run_id)
            );

            CREATE TABLE IF NOT EXISTS hardlink_operations (
                op_id INTEGER PRIMARY KEY AUTOINCREMENT,
                run_id TEXT NOT NULL,
                bundle_id TEXT,
                source_path TEXT NOT NULL,
                dest_path TEXT NOT NULL,
                source_inode INTEGER NOT NULL,
                status TEXT NOT NULL,
                operation_type TEXT NOT NULL DEFAULT 'hardlink',
                executed_at TEXT,
                FOREIGN KEY(bundle_id) REFERENCES bundles(bundle_id)
            );

            CREATE TABLE IF NOT EXISTS pending_failures (
                bundle_id TEXT PRIMARY KEY,
                run_id TEXT NOT NULL,
                primary_path TEXT NOT NULL,
                error_message TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending_download_failed',
                failed_at TEXT NOT NULL,
                FOREIGN KEY(run_id) REFERENCES runs(run_id)
            );

            CREATE TABLE IF NOT EXISTS directory_locations (
                path_key TEXT PRIMARY KEY,
                resolved_name TEXT NOT NULL,
                region_code TEXT NOT NULL,
                latitude REAL NOT NULL,
                longitude REAL NOT NULL,
                source TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS bundle_locations (
                bundle_id TEXT PRIMARY KEY,
                resolved_name TEXT NOT NULL,
                region_code TEXT NOT NULL,
                latitude REAL NOT NULL,
                longitude REAL NOT NULL,
                source TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS bundle_thumbnails (
                bundle_id TEXT PRIMARY KEY,
                data_uri TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_bundles_run ON bundles(run_id);
            CREATE INDEX IF NOT EXISTS idx_bundles_portable ON bundles(trip_folder, original_stem);
            CREATE INDEX IF NOT EXISTS idx_ops_run ON hardlink_operations(run_id);
            CREATE INDEX IF NOT EXISTS idx_pending_run ON pending_failures(run_id);
            CREATE INDEX IF NOT EXISTS idx_dir_loc_region ON directory_locations(region_code);
            CREATE INDEX IF NOT EXISTS idx_bundle_loc_region ON bundle_locations(region_code);
            CREATE INDEX IF NOT EXISTS idx_bundle_thumbnails_time ON bundle_thumbnails(updated_at);
            "#,
        )?;

        // Ensure operation_type column exists if table was created in an earlier schema version
        let has_op_type: bool = {
            let mut stmt = conn.prepare("PRAGMA table_info(hardlink_operations)")?;
            let mut rows = stmt.query([])?;
            let mut found = false;
            while let Some(row) = rows.next()? {
                let col_name: String = row.get(1)?;
                if col_name == "operation_type" {
                    found = true;
                    break;
                }
            }
            found
        };

        if !has_op_type {
            let _ = conn.execute(
                "ALTER TABLE hardlink_operations ADD COLUMN operation_type TEXT NOT NULL DEFAULT 'hardlink'",
                [],
            );
        }

        // Ensure bundles table contains portable relative path, user override, and was_dataless columns
        let (has_override_col, has_relpath_col, has_dataless_col): (bool, bool, bool) = {
            let mut stmt = conn.prepare("PRAGMA table_info(bundles)")?;
            let mut rows = stmt.query([])?;
            let mut has_override = false;
            let mut has_relpath = false;
            let mut has_dataless = false;
            while let Some(row) = rows.next()? {
                let col_name: String = row.get(1)?;
                if col_name == "is_user_override" {
                    has_override = true;
                } else if col_name == "relative_path" {
                    has_relpath = true;
                } else if col_name == "was_dataless" {
                    has_dataless = true;
                }
            }
            (has_override, has_relpath, has_dataless)
        };

        if !has_override_col {
            let _ = conn.execute(
                "ALTER TABLE bundles ADD COLUMN is_user_override INTEGER NOT NULL DEFAULT 0",
                [],
            );
        }
        if !has_relpath_col {
            let _ = conn.execute("ALTER TABLE bundles ADD COLUMN relative_path TEXT", []);
        }
        if !has_dataless_col {
            let _ = conn.execute(
                "ALTER TABLE bundles ADD COLUMN was_dataless INTEGER DEFAULT 0",
                [],
            );
        }

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Opens or creates the SQLite manifest database.
    pub fn open(db_path: &Path) -> Result<Self> {
        Self::open_or_create(db_path)
    }

    /// Records a new run session.
    pub fn record_run(
        &self,
        run_id: &str,
        input_root: impl AsRef<Path>,
        output_root: impl AsRef<Path>,
        dry_run: bool,
        config_json: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let started_at = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT OR REPLACE INTO runs (run_id, started_at, input_root, output_root, dry_run, config_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                run_id,
                started_at,
                input_root.as_ref().to_string_lossy().to_string(),
                output_root.as_ref().to_string_lossy().to_string(),
                if dry_run { 1 } else { 0 },
                config_json,
            ],
        )?;

        Ok(())
    }

    /// Records a single bundle classification decision and metadata.
    pub fn record_bundle(
        &self,
        run_id: &str,
        bundle: &AssetBundle,
        decision: &BundleDecision,
    ) -> Result<()> {
        self.record_bundles(
            run_id,
            std::slice::from_ref(decision),
            std::slice::from_ref(bundle),
        )
    }

    /// Marks a run session as completed.
    pub fn complete_run(&self, run_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let completed_at = Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE runs SET completed_at = ?1 WHERE run_id = ?2",
            params![completed_at, run_id],
        )?;
        Ok(())
    }

    /// Records bundle classification decisions and metadata.
    pub fn record_bundles(
        &self,
        run_id: &str,
        decisions: &[BundleDecision],
        source_bundles: &[AssetBundle],
    ) -> Result<()> {
        let was_dataless_flags = vec![false; decisions.len()];
        self.record_bundles_with_locality(run_id, decisions, source_bundles, &was_dataless_flags)
    }

    /// Records bundle classification decisions, metadata, and dataless status flags.
    pub fn record_bundles_with_locality(
        &self,
        run_id: &str,
        decisions: &[BundleDecision],
        source_bundles: &[AssetBundle],
        was_dataless_flags: &[bool],
    ) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        let bundle_meta_map: std::collections::HashMap<_, _> =
            source_bundles.iter().map(|b| (&b.bundle_id, b)).collect();

        for (idx, dec) in decisions.iter().enumerate() {
            if let Some(bundle) = bundle_meta_map.get(&dec.bundle_id) {
                let bbox_json = serde_json::to_string(&dec.bbox_xyxy_norm).unwrap_or_default();
                let top5_json = serde_json::to_string(&dec.top_predictions).unwrap_or_default();
                let bundle_json = serde_json::to_string(bundle).unwrap_or_default();

                let (lat, lon, loc_source) = match bundle.gps_coords {
                    Some(c) => (
                        Some(c.latitude),
                        Some(c.longitude),
                        Some(format!("{:?}", c.source)),
                    ),
                    None => (None, None, None),
                };

                let routing_tier_str = match dec.routing_tier {
                    RoutingTier::ConfirmedSpecies => "ConfirmedSpecies",
                    RoutingTier::NeedsReview => "NeedsReview",
                    RoutingTier::NoBirdDetected => "NoBirdDetected",
                };

                let was_dataless_val = if was_dataless_flags.get(idx).copied().unwrap_or(false) {
                    1
                } else {
                    0
                };

                tx.execute(
                    "INSERT OR REPLACE INTO bundles (
                        bundle_id, run_id, source_inode, trip_folder, original_stem,
                        capture_timestamp_ms, lat, lon, location_source, burst_id,
                        detection_conf, bbox_json, assigned_species_slug, frame_conf,
                        final_conf, routing_tier, top5_predictions_json, bundle_json,
                        is_user_override, relative_path, was_dataless
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)",
                    params![
                        dec.bundle_id,
                        run_id,
                        bundle.source_inode as i64,
                        bundle.trip_folder_name,
                        bundle.original_stem,
                        bundle.capture_timestamp_ms,
                        lat,
                        lon,
                        loc_source,
                        dec.burst_id,
                        dec.detection_confidence,
                        bbox_json,
                        dec.assigned_species_slug,
                        dec.frame_confidence,
                        dec.final_confidence,
                        routing_tier_str,
                        top5_json,
                        bundle_json,
                        if dec.is_user_override { 1 } else { 0 },
                        bundle.relative_path.as_ref().map(|p| p.to_string_lossy().to_string()),
                        was_dataless_val,
                    ],
                )?;
            }
        }

        tx.commit()?;
        Ok(())
    }

    /// Updates the routing tier of a specific bundle in the database.
    pub fn update_bundle_routing_tier(&self, bundle_id: &str, tier: RoutingTier) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let tier_str = match tier {
            RoutingTier::ConfirmedSpecies => "ConfirmedSpecies",
            RoutingTier::NeedsReview => "NeedsReview",
            RoutingTier::NoBirdDetected => "NoBirdDetected",
        };
        conn.execute(
            "UPDATE bundles SET routing_tier = ?1 WHERE bundle_id = ?2",
            params![tier_str, bundle_id],
        )?;
        Ok(())
    }

    /// Updates a bundle's classification as a manual user override.
    pub fn update_bundle_override(
        &self,
        bundle_id: &str,
        new_species_slug: &str,
        routing_tier: RoutingTier,
        final_conf: f32,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let tier_str = match routing_tier {
            RoutingTier::ConfirmedSpecies => "ConfirmedSpecies",
            RoutingTier::NeedsReview => "NeedsReview",
            RoutingTier::NoBirdDetected => "NoBirdDetected",
        };
        conn.execute(
            "UPDATE bundles
             SET assigned_species_slug = ?1,
                 routing_tier = ?2,
                 final_conf = ?3,
                 is_user_override = 1
             WHERE bundle_id = ?4",
            params![new_species_slug, tier_str, final_conf, bundle_id],
        )?;
        Ok(())
    }

    /// Prunes/deletes a bundle and its related operations from SQLite manifest.
    pub fn delete_bundle(&self, bundle_id: &str) -> Result<bool> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM hardlink_operations WHERE bundle_id = ?1",
            params![bundle_id],
        )?;
        let rows = tx.execute(
            "DELETE FROM bundles WHERE bundle_id = ?1",
            params![bundle_id],
        )?;
        tx.commit()?;
        Ok(rows > 0)
    }

    /// Returns whether the bundle was recorded as dataless during its processing run.
    pub fn get_bundle_was_dataless(&self, bundle_id: &str) -> Result<Option<bool>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT was_dataless FROM bundles WHERE bundle_id = ?1 LIMIT 1")?;
        let mut rows = stmt.query(params![bundle_id])?;
        if let Some(row) = rows.next()? {
            let val: i64 = row.get(0)?;
            Ok(Some(val != 0))
        } else {
            Ok(None)
        }
    }

    /// Records a pending download failure for a bundle in manifest.sqlite.
    pub fn record_pending_download_failure(
        &self,
        run_id: &str,
        bundle: &AssetBundle,
        error_message: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT OR REPLACE INTO pending_failures (
                bundle_id, run_id, primary_path, error_message, status, failed_at
            ) VALUES (?1, ?2, ?3, ?4, 'pending_download_failed', ?5)",
            params![
                bundle.bundle_id,
                run_id,
                bundle.primary_image.to_string_lossy().to_string(),
                error_message,
                now,
            ],
        )?;
        Ok(())
    }

    /// Queries all pending failures, optionally scoped to a run_id.
    pub fn get_pending_failures(&self, run_id: Option<&str>) -> Result<Vec<PendingFailure>> {
        let conn = self.conn.lock().unwrap();
        let query = if run_id.is_some() {
            "SELECT bundle_id, run_id, primary_path, error_message, status, failed_at
             FROM pending_failures WHERE run_id = ?1 ORDER BY failed_at ASC"
        } else {
            "SELECT bundle_id, run_id, primary_path, error_message, status, failed_at
             FROM pending_failures ORDER BY failed_at ASC"
        };

        let mut stmt = conn.prepare(query)?;
        let row_mapper = |row: &rusqlite::Row| {
            let bundle_id: String = row.get(0)?;
            let run_id: String = row.get(1)?;
            let primary_path_str: String = row.get(2)?;
            let error_message: String = row.get(3)?;
            let status: String = row.get(4)?;
            let failed_at: String = row.get(5)?;
            Ok(PendingFailure {
                bundle_id,
                run_id,
                primary_path: PathBuf::from(primary_path_str),
                error_message,
                status,
                failed_at,
            })
        };

        let mut list = Vec::new();
        if let Some(id) = run_id {
            let rows = stmt.query_map(params![id], row_mapper)?;
            for r in rows {
                list.push(r?);
            }
        } else {
            let rows = stmt.query_map([], row_mapper)?;
            for r in rows {
                list.push(r?);
            }
        }
        Ok(list)
    }

    /// Looks up the bundle ID for a given file inode from recorded hardlink operations.
    pub fn get_bundle_id_by_inode(&self, inode: u64) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT bundle_id FROM hardlink_operations WHERE source_inode = ?1 LIMIT 1")?;
        let mut rows = stmt.query(params![inode as i64])?;
        if let Some(row) = rows.next()? {
            let b_id: Option<String> = row.get(0).ok();
            Ok(b_id)
        } else {
            Ok(None)
        }
    }

    /// Updates the destination path for an existing hardlink operation by bundle ID and source inode.
    pub fn update_hardlink_destination_by_inode(
        &self,
        bundle_id: &str,
        source_inode: u64,
        new_dest_path: &Path,
    ) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let rows = conn.execute(
            "UPDATE hardlink_operations
             SET dest_path = ?1, status = 'linked'
             WHERE bundle_id = ?2 AND source_inode = ?3 AND operation_type = 'hardlink'",
            params![
                new_dest_path.to_string_lossy().to_string(),
                bundle_id,
                source_inode as i64,
            ],
        )?;
        Ok(rows > 0)
    }

    /// Updates the destination path for an existing hardlink operation by bundle ID and filename or extension pattern.
    pub fn update_hardlink_destination_by_filename(
        &self,
        bundle_id: &str,
        filename: &str,
        new_dest_path: &Path,
    ) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let pattern = format!("%{}", filename);
        let rows = conn.execute(
            "UPDATE hardlink_operations
             SET dest_path = ?1, status = 'linked'
             WHERE bundle_id = ?2 AND (dest_path LIKE ?3 OR source_path LIKE ?3) AND operation_type = 'hardlink'",
            params![
                new_dest_path.to_string_lossy().to_string(),
                bundle_id,
                pattern,
            ],
        )?;
        if rows > 0 {
            return Ok(true);
        }

        if let Some(ext) = Path::new(filename).extension().and_then(|e| e.to_str()) {
            let ext_pattern = format!("%.{}", ext);
            let rows = conn.execute(
                "UPDATE hardlink_operations
                 SET dest_path = ?1, status = 'linked'
                 WHERE bundle_id = ?2 AND (dest_path LIKE ?3 OR source_path LIKE ?3) AND operation_type = 'hardlink'",
                params![
                    new_dest_path.to_string_lossy().to_string(),
                    bundle_id,
                    ext_pattern,
                ],
            )?;
            return Ok(rows > 0);
        }

        Ok(false)
    }

    /// Updates the destination path for an existing in-place rename operation by bundle ID and source inode.
    pub fn update_inplace_destination_by_inode(
        &self,
        bundle_id: &str,
        source_inode: u64,
        new_dest_path: &Path,
    ) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let rows = conn.execute(
            "UPDATE hardlink_operations
             SET dest_path = ?1, status = 'renamed'
             WHERE bundle_id = ?2 AND source_inode = ?3 AND operation_type = 'in_place_rename'",
            params![
                new_dest_path.to_string_lossy().to_string(),
                bundle_id,
                source_inode as i64,
            ],
        )?;
        Ok(rows > 0)
    }

    /// Updates the destination path for an existing in-place rename operation by bundle ID and filename pattern.
    pub fn update_inplace_destination_by_filename(
        &self,
        bundle_id: &str,
        filename: &str,
        new_dest_path: &Path,
    ) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let pattern = format!("%{}", filename);
        let rows = conn.execute(
            "UPDATE hardlink_operations
             SET dest_path = ?1, status = 'renamed'
             WHERE bundle_id = ?2 AND (dest_path LIKE ?3 OR source_path LIKE ?3) AND operation_type = 'in_place_rename'",
            params![
                new_dest_path.to_string_lossy().to_string(),
                bundle_id,
                pattern,
            ],
        )?;
        if rows > 0 {
            return Ok(true);
        }

        if let Some(ext) = Path::new(filename).extension().and_then(|e| e.to_str()) {
            let ext_pattern = format!("%.{}", ext);
            let rows = conn.execute(
                "UPDATE hardlink_operations
                 SET dest_path = ?1, status = 'renamed'
                 WHERE bundle_id = ?2 AND dest_path LIKE ?3 AND operation_type = 'in_place_rename'",
                params![
                    new_dest_path.to_string_lossy().to_string(),
                    bundle_id,
                    ext_pattern,
                ],
            )?;
            return Ok(rows > 0);
        }

        Ok(false)
    }

    /// Records hardlink operations in the database.
    pub fn record_hardlink_operations(&self, ops: &[HardlinkOperation]) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        for op in ops {
            let op_type = if op.operation_type.is_empty() {
                "hardlink"
            } else {
                &op.operation_type
            };
            tx.execute(
                "INSERT INTO hardlink_operations (run_id, bundle_id, source_path, dest_path, source_inode, status, operation_type, executed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    op.run_id,
                    op.bundle_id,
                    op.source_path.to_string_lossy().to_string(),
                    op.dest_path.to_string_lossy().to_string(),
                    op.source_inode as i64,
                    op.status,
                    op_type,
                    op.executed_at,
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    /// Queries all operations recorded for a given run ID.
    pub fn get_run_operations(&self, run_id: &str) -> Result<Vec<HardlinkOperation>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT op_id, run_id, bundle_id, source_path, dest_path, source_inode, status, executed_at, operation_type
             FROM hardlink_operations WHERE run_id = ?1 ORDER BY op_id ASC",
        )?;

        let rows = stmt.query_map(params![run_id], |row| {
            let op_id: i64 = row.get(0)?;
            let run_id: String = row.get(1)?;
            let bundle_id: Option<String> = row.get(2).ok();
            let source_str: String = row.get(3)?;
            let dest_str: String = row.get(4)?;
            let source_inode: i64 = row.get(5)?;
            let status: String = row.get(6)?;
            let executed_at: Option<String> = row.get(7)?;
            let operation_type: String = row.get(8).unwrap_or_else(|_| "hardlink".to_string());

            Ok(HardlinkOperation {
                op_id: Some(op_id),
                run_id,
                bundle_id: bundle_id.unwrap_or_default(),
                source_path: PathBuf::from(source_str),
                dest_path: PathBuf::from(dest_str),
                source_inode: source_inode as u64,
                status,
                operation_type,
                executed_at,
            })
        })?;

        let mut ops = Vec::new();
        for r in rows {
            ops.push(r?);
        }
        Ok(ops)
    }

    /// Queries all operations recorded for a given bundle ID.
    pub fn get_bundle_operations(&self, bundle_id: &str) -> Result<Vec<HardlinkOperation>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT op_id, run_id, bundle_id, source_path, dest_path, source_inode, status, executed_at, operation_type
             FROM hardlink_operations WHERE bundle_id = ?1 ORDER BY op_id ASC",
        )?;

        let rows = stmt.query_map(params![bundle_id], |row| {
            let op_id: i64 = row.get(0)?;
            let run_id: String = row.get(1)?;
            let b_id: Option<String> = row.get(2).ok();
            let source_str: String = row.get(3)?;
            let dest_str: String = row.get(4)?;
            let source_inode: i64 = row.get(5)?;
            let status: String = row.get(6)?;
            let executed_at: Option<String> = row.get(7)?;
            let operation_type: String = row.get(8).unwrap_or_else(|_| "hardlink".to_string());

            Ok(HardlinkOperation {
                op_id: Some(op_id),
                run_id,
                bundle_id: b_id.unwrap_or_default(),
                source_path: PathBuf::from(source_str),
                dest_path: PathBuf::from(dest_str),
                source_inode: source_inode as u64,
                status,
                operation_type,
                executed_at,
            })
        })?;

        let mut ops = Vec::new();
        for r in rows {
            ops.push(r?);
        }
        Ok(ops)
    }

    /// Updates the destination path and status for a hardlink operation by op_id.
    pub fn update_operation_dest(&self, op_id: i64, new_dest: &Path) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE hardlink_operations SET dest_path = ?1, status = 'linked' WHERE op_id = ?2",
            params![new_dest.to_string_lossy().to_string(), op_id],
        )?;
        Ok(())
    }

    /// Returns the organization mode configured for the latest run, if any.
    pub fn get_latest_run_mode(&self) -> Result<Option<OrganizationMode>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT config_json FROM runs ORDER BY started_at DESC LIMIT 1")?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            let config_str: String = row.get(0)?;
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&config_str) {
                if let Some(mode_str) = val.get("mode").and_then(|m| m.as_str()) {
                    if mode_str == "in_place" {
                        return Ok(Some(OrganizationMode::InPlaceFileRenameToDescriptiveNames));
                    } else if mode_str == "hierarchy" {
                        return Ok(Some(OrganizationMode::HierarchicalHardlinkHierarchy));
                    }
                }
            }
        }
        Ok(None)
    }

    /// Records an in-place file rename in the database for tracking and rollback.
    pub fn record_inplace_rename(
        &self,
        run_id: &str,
        original_path: &Path,
        renamed_path: &Path,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();
        let source_inode = fs::metadata(renamed_path)
            .or_else(|_| fs::metadata(original_path))
            .map(|m| m.ino())
            .unwrap_or(0);

        let original_stem = original_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();

        let bundle_id: Option<String> = {
            let mut stmt = conn.prepare(
                "SELECT bundle_id FROM bundles WHERE run_id = ?1 AND original_stem = ?2 LIMIT 1",
            )?;
            let mut rows = stmt.query(params![run_id, original_stem])?;
            if let Some(row) = rows.next()? {
                Some(row.get(0)?)
            } else {
                None
            }
        };

        // If bundle_id column is NOT NULL in this DB table and bundle_id is None,
        // create a minimal bundle record to satisfy foreign key constraint.
        let bundle_id = match bundle_id {
            Some(b) => Some(b),
            None => {
                let is_not_null = conn
                    .prepare("PRAGMA table_info(hardlink_operations)")?
                    .query_map([], |row| {
                        let col_name: String = row.get(1)?;
                        let not_null: i32 = row.get(3)?;
                        Ok((col_name, not_null))
                    })?
                    .any(|res| {
                        res.map(|(col, nn)| col == "bundle_id" && nn == 1)
                            .unwrap_or(false)
                    });

                if is_not_null {
                    let dummy_id = format!("{}_inplace", original_stem);
                    let _ = conn.execute(
                        "INSERT OR IGNORE INTO bundles (
                            bundle_id, run_id, source_inode, trip_folder, original_stem,
                            assigned_species_slug, frame_conf, final_conf, routing_tier,
                            top5_predictions_json, bundle_json
                        ) VALUES (?1, ?2, 0, '', ?3, '', 0.0, 0.0, 'NoBirdDetected', '[]', '{}')",
                        params![dummy_id, run_id, original_stem],
                    );
                    Some(dummy_id)
                } else {
                    None
                }
            }
        };

        conn.execute(
            "INSERT INTO hardlink_operations (run_id, bundle_id, source_path, dest_path, source_inode, status, operation_type, executed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                run_id,
                bundle_id,
                original_path.to_string_lossy().to_string(),
                renamed_path.to_string_lossy().to_string(),
                source_inode as i64,
                "renamed",
                "in_place_rename",
                now,
            ],
        )?;

        Ok(())
    }

    /// Reverts all in-place renames for the given run ID in reverse order.
    /// Renames files back from their current renamed path to their original path,
    /// ensuring zero data deletion and updating their operation status to 'reverted'.
    pub fn rollback_inplace_renames(&self, run_id: &str) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT op_id, source_path, dest_path
             FROM hardlink_operations
             WHERE run_id = ?1 AND operation_type = 'in_place_rename' AND status = 'renamed'
             ORDER BY op_id DESC",
        )?;

        let rows = stmt.query_map(params![run_id], |row| {
            let op_id: i64 = row.get(0)?;
            let source_str: String = row.get(1)?;
            let dest_str: String = row.get(2)?;
            Ok((op_id, PathBuf::from(source_str), PathBuf::from(dest_str)))
        })?;

        let mut renames = Vec::new();
        for r in rows {
            renames.push(r?);
        }
        drop(stmt);

        let mut rolled_back = 0usize;
        for (op_id, source_path, dest_path) in renames {
            if dest_path == source_path {
                conn.execute(
                    "UPDATE hardlink_operations SET status = 'reverted' WHERE op_id = ?1",
                    params![op_id],
                )?;
                continue;
            }

            if dest_path.exists() {
                fs::rename(&dest_path, &source_path).with_context(|| {
                    format!(
                        "Failed to rollback in-place rename from '{}' to '{}'",
                        dest_path.display(),
                        source_path.display()
                    )
                })?;
                rolled_back += 1;
            }

            conn.execute(
                "UPDATE hardlink_operations SET status = 'reverted' WHERE op_id = ?1",
                params![op_id],
            )?;
        }

        Ok(rolled_back)
    }

    /// Updates status of hardlink operations (e.g. marking as "unlinked").
    pub fn update_operation_status(&self, op_id: i64, new_status: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE hardlink_operations SET status = ?1 WHERE op_id = ?2",
            params![new_status, op_id],
        )?;
        Ok(())
    }

    /// Returns the most recent run ID recorded in the database.
    pub fn get_latest_run_id(&self) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT run_id FROM runs ORDER BY started_at DESC LIMIT 1")?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            Ok(Some(id))
        } else {
            Ok(None)
        }
    }

    /// Queries all bundle decisions and original bundle metadata for relinking and reports.
    pub fn get_all_bundles(
        &self,
        run_id: Option<&str>,
    ) -> Result<Vec<(BundleDecision, AssetBundle)>> {
        let conn = self.conn.lock().unwrap();
        let query = if run_id.is_some() {
            "SELECT bundle_id, burst_id, detection_conf, bbox_json, assigned_species_slug, frame_conf, final_conf, routing_tier, top5_predictions_json, bundle_json, is_user_override, relative_path
             FROM bundles WHERE run_id = ?1"
        } else {
            "SELECT bundle_id, burst_id, detection_conf, bbox_json, assigned_species_slug, frame_conf, final_conf, routing_tier, top5_predictions_json, bundle_json, is_user_override, relative_path
             FROM bundles"
        };

        let mut stmt = conn.prepare(query)?;
        let row_mapper = |row: &rusqlite::Row| {
            let bundle_id: String = row.get(0)?;
            let burst_id: Option<String> = row.get(1)?;
            let detection_confidence: f32 = row.get(2)?;
            let bbox_json: String = row.get(3)?;
            let assigned_species_slug: String = row.get(4)?;
            let frame_confidence: f32 = row.get(5)?;
            let final_confidence: f32 = row.get(6)?;
            let routing_tier_str: String = row.get(7)?;
            let top5_json: String = row.get(8)?;
            let bundle_json: String = row.get(9)?;
            let is_user_override_val: i64 = row.get(10).unwrap_or(0);
            let relative_path_str: Option<String> = row.get(11).unwrap_or(None);

            let bbox_xyxy_norm: Option<[f32; 4]> = serde_json::from_str(&bbox_json).ok();
            let top_predictions: Vec<SpeciesPrediction> =
                serde_json::from_str(&top5_json).unwrap_or_default();
            let mut asset_bundle: AssetBundle =
                serde_json::from_str(&bundle_json).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(e),
                    )
                })?;

            if asset_bundle.relative_path.is_none() && relative_path_str.is_some() {
                asset_bundle.relative_path = relative_path_str.map(PathBuf::from);
            }

            let routing_tier = match routing_tier_str.as_str() {
                "ConfirmedSpecies" => RoutingTier::ConfirmedSpecies,
                "NeedsReview" => RoutingTier::NeedsReview,
                _ => RoutingTier::NoBirdDetected,
            };

            let decision = BundleDecision {
                bundle_id,
                burst_id,
                detection_confidence,
                bbox_xyxy_norm,
                top_predictions,
                frame_confidence,
                final_confidence,
                assigned_species_slug,
                routing_tier,
                is_user_override: is_user_override_val != 0,
            };

            Ok((decision, asset_bundle))
        };

        let mut results = Vec::new();
        if let Some(id) = run_id {
            let rows = stmt.query_map(params![id], row_mapper)?;
            for r in rows {
                results.push(r?);
            }
        } else {
            let rows = stmt.query_map([], row_mapper)?;
            for r in rows {
                results.push(r?);
            }
        }

        Ok(results)
    }

    /// Returns a map of bundle_id, source_inode, relative_path, and portable key -> (BundleDecision, AssetBundle) of all previously recorded bundles.
    pub fn get_existing_bundle_map(&self) -> Result<ExistingBundleMap> {
        let all = self.get_all_bundles(None)?;
        let mut by_id = std::collections::HashMap::new();
        let mut by_inode = std::collections::HashMap::new();
        let mut by_relative_path = std::collections::HashMap::new();
        let mut by_portable_key = std::collections::HashMap::new();

        for (dec, bundle) in all {
            by_id.insert(bundle.bundle_id.clone(), (dec.clone(), bundle.clone()));
            if bundle.source_inode != 0 {
                by_inode.insert(bundle.source_inode, (dec.clone(), bundle.clone()));
            }
            if let Some(ref rel_path) = bundle.relative_path {
                match by_relative_path.entry(rel_path.clone()) {
                    std::collections::hash_map::Entry::Vacant(e) => {
                        e.insert((dec.clone(), bundle.clone()));
                    }
                    std::collections::hash_map::Entry::Occupied(mut e) => {
                        if dec.is_user_override && !e.get().0.is_user_override {
                            e.insert((dec.clone(), bundle.clone()));
                        }
                    }
                }
            }
            let key = (
                bundle.trip_folder_name.clone(),
                bundle.original_stem.clone(),
            );
            match by_portable_key.entry(key) {
                std::collections::hash_map::Entry::Vacant(e) => {
                    e.insert((dec, bundle));
                }
                std::collections::hash_map::Entry::Occupied(mut e) => {
                    if dec.is_user_override && !e.get().0.is_user_override {
                        e.insert((dec, bundle));
                    }
                }
            }
        }
        Ok(ExistingBundleMap {
            by_id,
            by_inode,
            by_relative_path,
            by_portable_key,
        })
    }

    /// Retrieves the cached resolved location for a trip directory path.
    pub fn get_directory_location(
        &self,
        path_key: &str,
    ) -> Result<Option<crate::location::resolver::ResolvedLocation>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT resolved_name, region_code, latitude, longitude, source
             FROM directory_locations WHERE path_key = ?1",
        )?;
        let mut rows = stmt.query(params![path_key])?;
        if let Some(row) = rows.next()? {
            let name: String = row.get(0)?;
            let region_code: String = row.get(1)?;
            let latitude: f64 = row.get(2)?;
            let longitude: f64 = row.get(3)?;
            let source: String = row.get(4)?;
            Ok(Some(crate::location::resolver::ResolvedLocation {
                name,
                region_code,
                latitude,
                longitude,
                source,
            }))
        } else {
            Ok(None)
        }
    }

    /// Retrieves the resolved location for a trip directory path or any of its parent ancestors,
    /// selecting the most specific (deepest) match.
    pub fn get_hierarchical_directory_location(
        &self,
        path: &Path,
    ) -> Result<Option<crate::location::resolver::ResolvedLocation>> {
        let mut curr = Some(path);
        while let Some(p) = curr {
            let s = p.to_string_lossy();
            if !s.is_empty() && s != "." {
                if let Some(loc) = self.get_directory_location(&s)? {
                    return Ok(Some(loc));
                }
            }
            curr = p.parent();
        }
        // Finally check "." or empty root if present
        self.get_directory_location(".")
    }

    /// Saves or updates the resolved location for a trip directory path.
    pub fn set_directory_location(
        &self,
        path_key: &str,
        loc: &crate::location::resolver::ResolvedLocation,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let updated_at = Utc::now().timestamp_millis();
        conn.execute(
            "INSERT OR REPLACE INTO directory_locations (
                path_key, resolved_name, region_code, latitude, longitude, source, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                path_key,
                loc.name,
                loc.region_code,
                loc.latitude,
                loc.longitude,
                loc.source,
                updated_at,
            ],
        )?;
        Ok(())
    }

    /// Retrieves the resolved location for a specific photo bundle, if explicitly overridden.
    pub fn get_bundle_location(
        &self,
        bundle_id: &str,
    ) -> Result<Option<crate::location::resolver::ResolvedLocation>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT resolved_name, region_code, latitude, longitude, source
             FROM bundle_locations WHERE bundle_id = ?1",
        )?;
        let mut rows = stmt.query(params![bundle_id])?;
        if let Some(row) = rows.next()? {
            let name: String = row.get(0)?;
            let region_code: String = row.get(1)?;
            let latitude: f64 = row.get(2)?;
            let longitude: f64 = row.get(3)?;
            let source: String = row.get(4)?;
            Ok(Some(crate::location::resolver::ResolvedLocation {
                name,
                region_code,
                latitude,
                longitude,
                source,
            }))
        } else {
            Ok(None)
        }
    }

    /// Finds a bundle by bundle_id, original_stem, or primary filename.
    pub fn find_bundle_by_query(&self, query: &str) -> Result<Option<crate::types::AssetBundle>> {
        let clean_query = query.trim().strip_prefix("./").unwrap_or(query.trim());
        let stem = std::path::Path::new(clean_query)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(clean_query);

        let all = self.get_all_bundles(None)?;
        for (_, bundle) in all {
            if bundle.bundle_id == clean_query
                || bundle.original_stem == stem
                || bundle.original_stem == clean_query
                || bundle
                    .primary_image
                    .file_name()
                    .map(|f| f.to_string_lossy() == clean_query)
                    .unwrap_or(false)
            {
                return Ok(Some(bundle));
            }
        }
        Ok(None)
    }

    /// Saves or updates the explicit location override for a photo bundle.
    pub fn set_bundle_location(
        &self,
        bundle_id: &str,
        loc: &crate::location::resolver::ResolvedLocation,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let updated_at = Utc::now().timestamp_millis();
        conn.execute(
            "INSERT OR REPLACE INTO bundle_locations (
                bundle_id, resolved_name, region_code, latitude, longitude, source, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                bundle_id,
                loc.name,
                loc.region_code,
                loc.latitude,
                loc.longitude,
                loc.source,
                updated_at,
            ],
        )?;
        Ok(())
    }

    /// Retrieves the cached base64 micro-thumbnail data URI for a bundle if present.
    pub fn get_thumbnail_data_uri(&self, bundle_id: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT data_uri FROM bundle_thumbnails WHERE bundle_id = ?1")?;
        let mut rows = stmt.query(params![bundle_id])?;
        if let Some(row) = rows.next()? {
            let uri: String = row.get(0)?;
            Ok(Some(uri))
        } else {
            Ok(None)
        }
    }

    /// Retrieves all cached micro-thumbnails as a map of `bundle_id -> data_uri`.
    pub fn get_all_thumbnails(&self) -> Result<std::collections::HashMap<String, String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT bundle_id, data_uri FROM bundle_thumbnails")?;
        let mut rows = stmt.query([])?;
        let mut map = std::collections::HashMap::new();
        while let Some(row) = rows.next()? {
            let bundle_id: String = row.get(0)?;
            let data_uri: String = row.get(1)?;
            map.insert(bundle_id, data_uri);
        }
        Ok(map)
    }

    /// Stores or replaces the micro-thumbnail data URI for a specific bundle.
    pub fn store_thumbnail(&self, bundle_id: &str, data_uri: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let updated_at = Utc::now().timestamp_millis();
        conn.execute(
            "INSERT OR REPLACE INTO bundle_thumbnails (bundle_id, data_uri, updated_at) VALUES (?1, ?2, ?3)",
            params![bundle_id, data_uri, updated_at],
        )?;
        Ok(())
    }

    /// Bulk stores or replaces micro-thumbnail data URIs in a single transaction.
    pub fn store_thumbnails_bulk(&self, items: &[(&str, &str)]) -> Result<()> {
        if items.is_empty() {
            return Ok(());
        }
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let updated_at = Utc::now().timestamp_millis();
        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO bundle_thumbnails (bundle_id, data_uri, updated_at) VALUES (?1, ?2, ?3)",
            )?;
            for (bundle_id, data_uri) in items {
                stmt.execute(params![bundle_id, data_uri, updated_at])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Imports bundle decisions, locations, and cached thumbnails from a child manifest database
    /// into this manifest database, prefixing bundle metadata if relative paths need adjustments.
    /// Returns the number of imported bundles.
    pub fn import_child_manifest(&self, child_db_path: &Path, rel_subdir: &Path) -> Result<usize> {
        let child_db = Self::open(child_db_path)?;
        let child_bundles = child_db.get_all_bundles(None)?;
        if child_bundles.is_empty() {
            return Ok(0);
        }

        let import_run_id = format!("import_{}", Utc::now().timestamp_millis());
        self.record_run(
            &import_run_id,
            rel_subdir,
            rel_subdir,
            false,
            r#"{"source":"child_manifest_import"}"#,
        )?;

        let mut decisions = Vec::with_capacity(child_bundles.len());
        let mut bundles = Vec::with_capacity(child_bundles.len());

        for (dec, mut bundle) in child_bundles {
            // Adjust relative_path if present to include the child subdir relative to the parent
            if let Some(ref rel) = bundle.relative_path {
                if !rel.starts_with(rel_subdir) {
                    bundle.relative_path = Some(rel_subdir.join(rel));
                }
            } else {
                bundle.relative_path = Some(rel_subdir.join(&bundle.original_stem));
            }

            decisions.push(dec);
            bundles.push(bundle);
        }

        self.record_bundles(&import_run_id, &decisions, &bundles)?;
        self.complete_run(&import_run_id)?;

        // Import child directory locations
        {
            let dir_locs = {
                let conn = child_db.conn.lock().unwrap();
                let mut stmt = conn.prepare(
                    "SELECT path_key, resolved_name, region_code, latitude, longitude, source, updated_at FROM directory_locations",
                )?;
                let mut rows = stmt.query([])?;
                let mut locs = Vec::new();
                while let Some(row) = rows.next()? {
                    let key: String = row.get(0)?;
                    let name: String = row.get(1)?;
                    let region: String = row.get(2)?;
                    let lat: f64 = row.get(3)?;
                    let lon: f64 = row.get(4)?;
                    let src: String = row.get(5)?;
                    let full_key = if key == "." || key.is_empty() {
                        rel_subdir.to_string_lossy().to_string()
                    } else if !Path::new(&key).starts_with(rel_subdir) {
                        rel_subdir.join(&key).to_string_lossy().to_string()
                    } else {
                        key
                    };
                    locs.push((full_key, name, region, lat, lon, src));
                }
                locs
            };

            for (k, name, region, lat, lon, src) in dir_locs {
                let loc = crate::location::resolver::ResolvedLocation {
                    name,
                    region_code: region,
                    latitude: lat,
                    longitude: lon,
                    source: src,
                };
                let _ = self.set_directory_location(&k, &loc);
            }
        }

        // Import child thumbnails
        if let Ok(child_thumbs) = child_db.get_all_thumbnails() {
            if !child_thumbs.is_empty() {
                let thumb_slice: Vec<(&str, &str)> = child_thumbs
                    .iter()
                    .map(|(b, u)| (b.as_str(), u.as_str()))
                    .collect();
                let _ = self.store_thumbnails_bulk(&thumb_slice);
            }
        }

        Ok(bundles.len())
    }
}
