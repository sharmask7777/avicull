### Task 2: Re-evaluation of Mock-Failed and Location-Updated Bundles
- In `src/main.rs`:
  - When evaluating `existing_bundle_map` against `all_discovered`:
    - If `args.reclassify` is true, reclassify all non-override bundles.
    - If a bundle has `detection_confidence == 0.0` and `top_predictions.is_empty()` and `!is_user_override` (i.e. previously failed/mocked without real inference) and real models/inference are available, treat it as requiring classification rather than caching it as a valid decision.
    - If a bundle's resolved `gps_coords` or region differs from the cached bundle coordinates in the database, queue it for re-classification.
  - Add `--reclassify` flag to `avicull organize` in `src/cli.rs`.
- Add test coverage in `tests/test_suite/test_reclassify_unclassified_bundles.rs`.

