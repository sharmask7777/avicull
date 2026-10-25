use avicull::classifier::{
    compute_posterior, get_regional_prior_multiplier, EMBEDDED_REGIONAL_CHECKLISTS,
};
use avicull::location::gazetteer::{
    extract_trigrams, normalize_string, trigram_similarity, GazetteerEngine,
};
use avicull::location::path_parser::{
    extract_candidate_location_tokens, is_structural_noise_token,
};
use avicull::location::resolver::{format_coord, resolve_from_override};
use proptest::prelude::*;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// 1. Trigram & String Normalization Property Tests
// ---------------------------------------------------------------------------
proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn prop_trigram_similarity_reflexivity(s in "\\PC{1,100}") {
        let sim = trigram_similarity(&s, &s);
        prop_assert!(
            (sim - 1.0).abs() < 1e-9 || normalize_string(&s).is_empty(),
            "Reflexivity failed for string: '{}', got sim: {}", s, sim
        );
    }

    #[test]
    fn prop_trigram_similarity_symmetry(s1 in "\\PC{0,60}", s2 in "\\PC{0,60}") {
        let sim1 = trigram_similarity(&s1, &s2);
        let sim2 = trigram_similarity(&s2, &s1);
        prop_assert!(
            (sim1 - sim2).abs() < 1e-9,
            "Symmetry violated: sim('{}', '{}') = {} != sim('{}', '{}') = {}",
            s1, s2, sim1, s2, s1, sim2
        );
    }

    #[test]
    fn prop_trigram_similarity_bounds(s1 in "\\PC{0,60}", s2 in "\\PC{0,60}") {
        let sim = trigram_similarity(&s1, &s2);
        prop_assert!(
            (0.0..=1.0).contains(&sim),
            "Similarity out of [0, 1] bounds: {} for ('{}', '{}')", sim, s1, s2
        );
    }

    #[test]
    fn prop_normalization_idempotence(s in "\\PC{0,80}") {
        let once = normalize_string(&s);
        let twice = normalize_string(&once);
        prop_assert_eq!(
            once, twice,
            "Normalization must be idempotent for input: '{}'", s
        );
    }

    #[test]
    fn prop_normalization_invariants(s in "\\PC{0,80}") {
        let norm = normalize_string(&s);

        // Normalized string contains only alphanumeric chars and single spaces
        for c in norm.chars() {
            prop_assert!(
                c.is_alphanumeric() || c == ' ',
                "Character '{}' in normalized '{}' is neither alphanumeric nor space", c, norm
            );
        }

        // Fully lowercased: applying to_lowercase does not change the string
        prop_assert_eq!(
            norm.to_lowercase(), norm.as_str(),
            "Normalized string is not fully lowercased: '{}'", norm
        );

        // No consecutive spaces
        prop_assert!(
            !norm.contains("  "),
            "Consecutive spaces found in normalized string: '{}'", norm
        );

        // No leading or trailing whitespace
        prop_assert_eq!(norm.trim(), norm.as_str());
    }

    #[test]
    fn prop_extract_trigrams_count(s in "[a-z0-9 ]{1,50}") {
        let norm = normalize_string(&s);
        let trigrams = extract_trigrams(&norm);
        if norm.is_empty() {
            prop_assert!(trigrams.is_empty());
        } else {
            // String is padded with 1 leading and 1 trailing space, so len + 2 chars
            let char_count = norm.chars().count();
            let expected_trigrams = char_count.saturating_add(2).saturating_sub(2);
            prop_assert_eq!(trigrams.len(), expected_trigrams);
        }
    }
}

// ---------------------------------------------------------------------------
// 2. Bayesian Posterior Math Property Tests
// ---------------------------------------------------------------------------
proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn prop_compute_posterior_unit_simplex(
        visual_probs in prop::collection::vec(0.001f32..1.0f32, 1..20),
        geo_priors in prop::collection::vec(0.0001f32..1.0f32, 1..20),
        geo_weight in 0.0f32..2.0f32,
    ) {
        // Match lengths to satisfy precondition
        let n = visual_probs.len().min(geo_priors.len());
        let vis = &visual_probs[..n];
        let geo = &geo_priors[..n];

        let post = compute_posterior(vis, geo, geo_weight, 0.001);

        prop_assert_eq!(post.len(), n);

        let sum: f32 = post.iter().sum();
        prop_assert!(
            (sum - 1.0).abs() < 1e-4,
            "Posterior probabilities must sum to 1.0, got {} for vis: {:?}, geo: {:?}",
            sum, vis, geo
        );

        for (i, &p) in post.iter().enumerate() {
            prop_assert!(
                p.is_finite(),
                "Posterior at index {} is not finite: {}", i, p
            );
            prop_assert!(
                (0.0..=1.0001).contains(&p),
                "Posterior at index {} out of range [0, 1]: {}", i, p
            );
        }
    }

    #[test]
    fn prop_compute_posterior_zero_weight_matches_normalized_visual(
        visual_probs in prop::collection::vec(0.01f32..1.0f32, 1..15),
        geo_priors in prop::collection::vec(0.01f32..1.0f32, 1..15),
    ) {
        let n = visual_probs.len().min(geo_priors.len());
        let vis = &visual_probs[..n];
        let geo = &geo_priors[..n];

        let post = compute_posterior(vis, geo, 0.0, 0.001);

        let vis_sum: f32 = vis.iter().sum();
        for (i, &p) in post.iter().enumerate() {
            let expected = vis[i] / vis_sum;
            prop_assert!(
                (p - expected).abs() < 1e-4,
                "With geo_weight=0, posterior[{}] ({}) should match normalized visual ({})",
                i, p, expected
            );
        }
    }

    #[test]
    fn prop_compute_posterior_monotonicity_on_geo_prior(
        vis_base in 0.1f32..0.8f32,
        geo_low in 0.01f32..0.4f32,
        geo_high in 0.5f32..1.0f32,
        geo_weight in 0.5f32..2.0f32,
    ) {
        let vis = vec![vis_base, vis_base];
        let geo = vec![geo_low, geo_high];

        let post = compute_posterior(&vis, &geo, geo_weight, 0.001);

        prop_assert!(
            post[1] > post[0],
            "Higher geo prior ({}) must yield higher posterior than lower geo prior ({}) when visual probs are equal ({}). Got post: {:?}",
            geo_high, geo_low, vis_base, post
        );
    }
}

// ---------------------------------------------------------------------------
// 3. Regional Checklist Prior Multiplier Property Tests
// ---------------------------------------------------------------------------
proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    #[test]
    fn prop_regional_multiplier_binary_discrete_values(
        region in prop::option::of("[A-Z]{2}(-[A-Z0-9]{2})?"),
        taxon_id in "[a-z0-9]{4,8}",
    ) {
        let mult = get_regional_prior_multiplier(
            region.as_deref(),
            &taxon_id,
            &EMBEDDED_REGIONAL_CHECKLISTS,
        );

        prop_assert!(
            (mult - 1.0).abs() < 1e-9 || (mult - 0.001).abs() < 1e-9,
            "Multiplier must be either 1.0 or 0.001, got {}", mult
        );
    }

    #[test]
    fn prop_none_region_always_neutral(taxon_id in "[a-z0-9]{4,8}") {
        let mult = get_regional_prior_multiplier(
            None,
            &taxon_id,
            &EMBEDDED_REGIONAL_CHECKLISTS,
        );
        prop_assert_eq!(mult, 1.0, "None region must always yield neutral prior 1.0");
    }

    #[test]
    fn prop_native_species_never_penalized(idx in 0usize..100) {
        let all_regions: Vec<_> = EMBEDDED_REGIONAL_CHECKLISTS.keys().collect();
        if !all_regions.is_empty() {
            let reg = all_regions[idx % all_regions.len()];
            let species_set = EMBEDDED_REGIONAL_CHECKLISTS.get(reg).unwrap();
            let species_list: Vec<_> = species_set.iter().collect();
            if !species_list.is_empty() {
                let native_taxon = species_list[idx % species_list.len()];
                let mult = get_regional_prior_multiplier(
                    Some(reg),
                    native_taxon,
                    &EMBEDDED_REGIONAL_CHECKLISTS,
                );
                prop_assert_eq!(
                    mult, 1.0,
                    "Native species '{}' in region '{}' must have multiplier 1.0",
                    native_taxon, reg
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 4. Path Parsing & Candidate Location Extraction Property Tests
// ---------------------------------------------------------------------------
proptest! {
    #![proptest_config(ProptestConfig::with_cases(400))]

    #[test]
    fn prop_path_parser_crash_freedom(path_str in "\\PC{0,120}") {
        let path = Path::new(&path_str);
        let tokens = extract_candidate_location_tokens(path);

        // Invariants on extracted tokens
        let mut seen = HashSet::new();
        for token in &tokens {
            prop_assert!(!token.is_empty(), "Token must not be empty");
            prop_assert!(
                seen.insert(token.clone()),
                "Duplicate token '{}' found in result: {:?}", token, tokens
            );
            prop_assert_eq!(
                token.to_lowercase(), token.as_str(),
                "Token '{}' must be lowercased", token
            );
            prop_assert_eq!(
                token.trim(), token.as_str(),
                "Token '{}' must have no leading/trailing whitespace", token
            );
        }
    }

    #[test]
    fn prop_path_parser_noise_invariance(
        base_name in "[a-z]{3,15}(_[a-z]{3,15})?",
        noise_idx in 0usize..10,
    ) {
        let noise_words = [
            "RAW", "raws", "Selects", "select", "Export", "exports",
            "100CANON", "2024-10-15", "DCIM", "Final",
        ];
        let noise = noise_words[noise_idx % noise_words.len()];

        let clean_path = PathBuf::from(format!("Trips/{}/photo.jpg", base_name));
        let noisy_path = PathBuf::from(format!("Trips/{}/{}/photo.jpg", base_name, noise));

        let clean_tokens = extract_candidate_location_tokens(&clean_path);
        let noisy_tokens = extract_candidate_location_tokens(&noisy_path);

        // Noise directory should never leak into candidate tokens
        prop_assert!(
            !noisy_tokens.iter().any(|t| t.eq_ignore_ascii_case(noise)),
            "Noise word '{}' was extracted as location candidate from: {:?}", noise, noisy_tokens
        );

        // Core candidate from base_name should still be preserved
        let base_token = base_name.replace('_', " ");
        if !is_structural_noise_token(&base_token) {
            prop_assert!(
                clean_tokens.contains(&base_token) == noisy_tokens.contains(&base_token),
                "Noisy path altered extraction of base candidate '{}'. Clean: {:?}, Noisy: {:?}",
                base_token, clean_tokens, noisy_tokens
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 5. Coordinate Formatting & Override Parsing Property Tests
// ---------------------------------------------------------------------------
proptest! {
    #![proptest_config(ProptestConfig::with_cases(400))]

    #[test]
    fn prop_format_coord_invariants(
        lat in -90.0f64..=90.0f64,
        lon in -180.0f64..=180.0f64,
    ) {
        let formatted = format_coord(lat, lon);

        // Lat hemisphere indicator
        if lat >= 0.0 {
            prop_assert!(formatted.contains('N'), "Expected 'N' for lat >= 0, got {}", formatted);
        } else {
            prop_assert!(formatted.contains('S'), "Expected 'S' for lat < 0, got {}", formatted);
        }

        // Lon hemisphere indicator
        if lon >= 0.0 {
            prop_assert!(formatted.contains('E'), "Expected 'E' for lon >= 0, got {}", formatted);
        } else {
            prop_assert!(formatted.contains('W'), "Expected 'W' for lon < 0, got {}", formatted);
        }

        // Must contain degree symbols
        prop_assert!(formatted.contains('°'));
    }

    #[test]
    fn prop_resolve_from_override_coordinates(
        lat in -90.0f64..=90.0f64,
        lon in -180.0f64..=180.0f64,
    ) {
        let engine = GazetteerEngine::default();
        let coord_str = format!("{:.4}, {:.4}", lat, lon);

        let resolved = resolve_from_override(&coord_str, &engine);
        prop_assert!(
            resolved.is_some(),
            "Failed to parse valid coordinate override string '{}'", coord_str
        );

        let loc = resolved.unwrap();
        prop_assert_eq!(loc.source, "override_coord");
        prop_assert!((loc.latitude - lat).abs() < 1e-3);
        prop_assert!((loc.longitude - lon).abs() < 1e-3);
    }
}
