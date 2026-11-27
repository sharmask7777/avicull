# SDD ledger — plan: docs/superpowers/plans/2026-10-24-avicull.md

Pre-flight:
- Task 1 Produces: Core domain contracts (AssetBundle, GpsCoordinate, LocationSource, SpeciesPrediction, BundleDecision, RoutingTier, ConfidenceMode) -> Consumed by Tasks 2, 3, 5, 6, 7, 8, 9, 10
- Task 2 Produces: validate_volume_and_disjointness, scan_input_tree, extract_embedded_jpeg_preview -> Consumed by Task 10
- Task 3 Produces: LocationResolver, resolve_locations -> Consumed by Task 10
- Task 4 Produces: One-time Python model exporter script & manifest verification
- Task 5 Produces: BirdDetector, MockBirdDetector, OrtBirdDetector, select_and_pad_bird_crop -> Consumed by Task 10
- Task 6 Produces: SpeciesClassifier, MockSpeciesClassifier, OrtSpeciesClassifier, compute_posterior -> Consumed by Task 7, 10
- Task 7 Produces: cluster_bursts, apply_burst_consensus -> Consumed by Task 10
- Task 8 Produces: ManifestDb -> Consumed by Tasks 9, 10
- Task 9 Produces: Hard-link organizer & unlinker -> Consumed by Task 10
- Task 10 Produces: CLI binary & subcommands (scan, organize, relink, clean, report)
- Task 11 Produces: Verification harness with synthetic data + Real-world test against /Users/shaleensharma/Pictures/Leh + README
No interface conflicts found.

Task 1: complete (commits f7f1d03..4e32e1d, tests: cargo test --test test_types -> 2/2 pass)
Task 2: complete (commits 4e32e1d..c7d1f0e, tests: cargo test --test test_scanner -> 4/4 pass)
Task 3: complete (commits c7d1f0e..6858065, tests: cargo test --test test_geo -> 4/4 pass)
Task 4: complete (commits 6858065..4271674, tests: python3 scripts/test_export_mock.py -> pass)
Task 5: complete (commits 4271674..18cedd5, tests: cargo test --test test_detector -> 5/5 pass)
Task 6: complete (commits 18cedd5..bb9b7ee, tests: cargo test --test test_classifier -> 4/4 pass)
Task 7: complete (commits bb9b7ee..a7fd3c7, tests: cargo test --test test_burst -> 2/2 pass)
Task 8: complete (commits a7fd3c7..aac327e, tests: cargo test --test test_db -> 1/1 pass)
Task 9: complete (commits aac327e..7180cbc, tests: cargo test --test test_organizer -> 4/4 pass)
Task 10: complete (commits 7180cbc..fcfd0f8, tests: cargo test -> 23/23 pass)
Task 11: complete (commits fcfd0f8..5e8990f, tests: real-world validation on /Users/shaleensharma/Pictures/Leh with 405 bundles and 442 links verified on APFS; cargo test -> 23/23 pass)
