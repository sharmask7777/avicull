use crate::types::{AssetBundle, BundleDecision, RoutingTier, SpeciesPrediction};
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
        input_root: &Path,
        output_root: &Path,
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
                input_root.to_string_lossy().to_string(),
                output_root.to_string_lossy().to_string(),
                if dry_run { 1 } else { 0 },
                config_json,
            ],
        )?;

        Ok(())
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
                        final_conf, routing_tier, top5_predictions_json, bundle_json
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
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
            "SELECT bundle_id, burst_id, detection_conf, bbox_json, assigned_species_slug, frame_conf, final_conf, routing_tier, top5_predictions_json, bundle_json
             FROM bundles WHERE run_id = ?1"
        } else {
            "SELECT bundle_id, burst_id, detection_conf, bbox_json, assigned_species_slug, frame_conf, final_conf, routing_tier, top5_predictions_json, bundle_json
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

            let bbox_xyxy_norm: Option<[f32; 4]> = serde_json::from_str(&bbox_json).ok();
            let top_predictions: Vec<SpeciesPrediction> =
                serde_json::from_str(&top5_json).unwrap_or_default();
            let asset_bundle: AssetBundle = serde_json::from_str(&bundle_json).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?;

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

    /// Returns a map of bundle_id -> (BundleDecision, AssetBundle) of all previously recorded bundles.
    pub fn get_existing_bundle_map(
        &self,
    ) -> Result<std::collections::HashMap<String, (BundleDecision, AssetBundle)>> {
        let all = self.get_all_bundles(None)?;
        let mut map = std::collections::HashMap::new();
        for (dec, bundle) in all {
            map.insert(bundle.bundle_id.clone(), (dec, bundle));
        }
        Ok(map)
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
