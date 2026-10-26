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

            CREATE TABLE IF NOT EXISTS directory_locations (
                path_key TEXT PRIMARY KEY,
                resolved_name TEXT NOT NULL,
                region_code TEXT NOT NULL,
                latitude REAL NOT NULL,
                longitude REAL NOT NULL,
                source TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_bundles_run ON bundles(run_id);
            CREATE INDEX IF NOT EXISTS idx_bundles_portable ON bundles(trip_folder, original_stem);
            CREATE INDEX IF NOT EXISTS idx_ops_run ON hardlink_operations(run_id);
            CREATE INDEX IF NOT EXISTS idx_dir_loc_region ON directory_locations(region_code);
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

        // Ensure bundles table contains portable relative path and user override columns
        let (has_override_col, has_relpath_col): (bool, bool) = {
            let mut stmt = conn.prepare("PRAGMA table_info(bundles)")?;
            let mut rows = stmt.query([])?;
            let mut has_override = false;
            let mut has_relpath = false;
            while let Some(row) = rows.next()? {
                let col_name: String = row.get(1)?;
                if col_name == "is_user_override" {
                    has_override = true;
                } else if col_name == "relative_path" {
                    has_relpath = true;
                }
            }
            (has_override, has_relpath)
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
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        let bundle_meta_map: std::collections::HashMap<_, _> =
            source_bundles.iter().map(|b| (&b.bundle_id, b)).collect();

        for dec in decisions {
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

                tx.execute(
                    "INSERT OR REPLACE INTO bundles (
                        bundle_id, run_id, source_inode, trip_folder, original_stem,
                        capture_timestamp_ms, lat, lon, location_source, burst_id,
                        detection_conf, bbox_json, assigned_species_slug, frame_conf,
                        final_conf, routing_tier, top5_predictions_json, bundle_json,
                        is_user_override, relative_path
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
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
}
