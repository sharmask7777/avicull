import os
import sys
import unittest
from dataclasses import FrozenInstanceError

import pytest

# Ensure repository root is on sys.path
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from scripts.build_taxonomy_embeddings import (
    TaxonomyEntry,
    fetch_multi_taxa_taxonomy,
    format_prompt,
    generate_prompt,
    generate_taxonomy_prompt,
    load_curated_non_avian_taxa,
    normalize_category,
    parse_taxonomy_records,
)


class TestMultiTaxaTaxonomyEmbeddings(unittest.TestCase):
    def test_taxonomy_entry_default_category(self):
        """TaxonomyEntry should default category to 'Bird' and include it in to_dict()."""
        entry = TaxonomyEntry(
            species_code="brakit1",
            scientific_name="Haliastur indus",
            common_name="Brahminy Kite",
            aliases=("Red-backed Sea-Eagle",),
        )
        self.assertEqual(entry.species_code, "brakit1")
        self.assertEqual(entry.scientific_name, "Haliastur indus")
        self.assertEqual(entry.common_name, "Brahminy Kite")
        self.assertEqual(entry.category, "Bird")
        self.assertEqual(entry.folder_slug, "Brahminy_Kite")
        self.assertEqual(entry.aliases, ("Red-backed Sea-Eagle",))

        # Check serialization dictionary format
        record = entry.to_dict()
        self.assertEqual(record["taxon_id"], "brakit1")
        self.assertEqual(record["category"], "Bird")
        self.assertEqual(record["common_name"], "Brahminy Kite")
        self.assertEqual(record["folder_slug"], "Brahminy_Kite")
        self.assertEqual(record["scientific_name"], "Haliastur indus")
        self.assertEqual(record["aliases"], ["Red-backed Sea-Eagle"])

        with pytest.raises(FrozenInstanceError):
            entry.category = "Mammalia"  # type: ignore[misc]

    def test_taxonomy_entry_custom_categories(self):
        """TaxonomyEntry should support Mammalia, Insecta, and Scenery singular categories."""
        mammal = TaxonomyEntry(
            species_code="mamm_mal_sq1",
            scientific_name="Ratufa indica",
            common_name="Malabar Giant Squirrel",
            category="Mammalia",
        )
        self.assertEqual(mammal.category, "Mammalia")
        self.assertEqual(mammal.folder_slug, "Malabar_Giant_Squirrel")
        self.assertEqual(mammal.to_dict()["category"], "Mammalia")
        self.assertEqual(mammal.to_dict()["taxon_id"], "mamm_mal_sq1")

        insect = TaxonomyEntry(
            species_code="ins_peacock1",
            scientific_name="Papilio buddha",
            common_name="Malabar Banded Peacock",
            category="Insecta",
        )
        self.assertEqual(insect.category, "Insecta")
        self.assertEqual(insect.folder_slug, "Malabar_Banded_Peacock")
        self.assertEqual(insect.to_dict()["category"], "Insecta")

        scenery = TaxonomyEntry(
            species_code="scen_aurora",
            scientific_name="Aurora Borealis",
            common_name="Aurora Borealis",
            category="Scenery",
        )
        self.assertEqual(scenery.category, "Scenery")
        self.assertEqual(scenery.folder_slug, "Aurora_Borealis")
        self.assertEqual(scenery.to_dict()["category"], "Scenery")

    def test_parse_multi_taxa_records(self):
        """Verify parse_taxonomy_records ingests Bird, Mammalia, Insecta, and Scenery."""
        raw_records = [
            # eBird species record (implicitly Bird)
            {
                "category": "species",
                "speciesCode": "whbkin1",
                "comName": "White-throated Kingfisher",
                "sciName": "Halcyon smyrnensis",
            },
            # Explicit category Bird
            {
                "category": "Bird",
                "taxon_id": "brakit1",
                "common_name": "Brahminy Kite",
                "scientific_name": "Haliastur indus",
            },
            # Mammalia record
            {
                "category": "Mammalia",
                "taxon_id": "mamm_mal_sq1",
                "common_name": "Malabar Giant Squirrel",
                "scientific_name": "Ratufa indica",
            },
            # Insecta record
            {
                "category": "Insecta",
                "speciesCode": "ins_peacock1",
                "comName": "Malabar Banded Peacock",
                "sciName": "Papilio buddha",
            },
            # Scenery record
            {
                "category": "Scenery",
                "taxon_id": "scen_aurora",
                "common_name": "Aurora Borealis",
                "scientific_name": "Aurora Borealis",
            },
            # eBird non-species slash should be skipped
            {
                "category": "slash",
                "speciesCode": "ducksp1",
                "comName": "duck sp.",
                "sciName": "Anas sp.",
            },
            # Duplicate code should be skipped
            {
                "category": "Mammalia",
                "taxon_id": "mamm_mal_sq1",
                "common_name": "Duplicate Squirrel",
                "scientific_name": "Ratufa indica",
            },
        ]

        entries = parse_taxonomy_records(raw_records)
        self.assertEqual(len(entries), 5)

        by_code = {e.species_code: e for e in entries}
        self.assertEqual(by_code["whbkin1"].category, "Bird")
        self.assertEqual(by_code["brakit1"].category, "Bird")
        self.assertEqual(by_code["mamm_mal_sq1"].category, "Mammalia")
        self.assertEqual(by_code["ins_peacock1"].category, "Insecta")
        self.assertEqual(by_code["scen_aurora"].category, "Scenery")

    def test_prompt_generation_per_category(self):
        """Verify prompt generation produces category-specific BioCLIP text prompts."""
        # Bird
        bird = TaxonomyEntry(
            species_code="brakit1",
            scientific_name="Haliastur indus",
            common_name="Brahminy Kite",
            category="Bird",
        )
        self.assertEqual(
            generate_taxonomy_prompt(bird),
            "a photo of a Brahminy Kite (Haliastur indus), a type of bird.",
        )

        # Mammal
        mammal = TaxonomyEntry(
            species_code="mamm_mal_sq1",
            scientific_name="Ratufa indica",
            common_name="Malabar Giant Squirrel",
            category="Mammalia",
        )
        self.assertEqual(
            generate_taxonomy_prompt(mammal),
            "a photo of a Malabar Giant Squirrel (Ratufa indica), a type of mammal.",
        )

        # Insect
        insect = TaxonomyEntry(
            species_code="ins_peacock1",
            scientific_name="Papilio buddha",
            common_name="Malabar Banded Peacock",
            category="Insecta",
        )
        self.assertEqual(
            generate_taxonomy_prompt(insect),
            "a photo of a Malabar Banded Peacock (Papilio buddha), a type of insect.",
        )

        # Scenery with scientific name
        scenery = TaxonomyEntry(
            species_code="scen_aurora",
            scientific_name="Aurora Borealis",
            common_name="Aurora Borealis",
            category="Scenery",
        )
        self.assertEqual(
            generate_taxonomy_prompt(scenery),
            "a photo of a Aurora Borealis (Aurora Borealis), a type of scenery.",
        )

        # Scenery without scientific name
        scenery_plain = TaxonomyEntry(
            species_code="scen_mountain",
            scientific_name="",
            common_name="Mountain Landscape",
            category="Scenery",
        )
        self.assertEqual(
            generate_taxonomy_prompt(scenery_plain),
            "a photo of a Mountain Landscape, a type of scenery.",
        )

    def test_curated_non_avian_taxa(self):
        """Verify curated non-avian taxonomy contains Mammalia, Insecta, and Scenery."""
        entries = load_curated_non_avian_taxa()
        self.assertGreaterEqual(len(entries), 10)

        categories = {e.category for e in entries}
        self.assertIn("Mammalia", categories)
        self.assertIn("Insecta", categories)
        self.assertIn("Scenery", categories)

        # Ensure all categories in curated dataset are canonical singular
        for entry in entries:
            self.assertIn(entry.category, ("Bird", "Mammalia", "Insecta", "Scenery"))

    def test_normalize_category(self):
        """Verify normalize_category handles canonical and informal variations."""
        self.assertEqual(normalize_category("Bird"), "Bird")
        self.assertEqual(normalize_category("bird"), "Bird")
        self.assertEqual(normalize_category("mammal"), "Mammalia")
        self.assertEqual(normalize_category("mammals"), "Mammalia")
        self.assertEqual(normalize_category("insect"), "Insecta")
        self.assertEqual(normalize_category("insects"), "Insecta")
        self.assertEqual(normalize_category("scenery"), "Scenery")
        self.assertEqual(normalize_category("landscape"), "Scenery")
        self.assertEqual(normalize_category("Unknown"), "Unknown")

    def test_prompt_aliases(self):
        """Verify generate_prompt and format_prompt aliases match generate_taxonomy_prompt."""
        entry = TaxonomyEntry("brakit1", "Haliastur indus", "Brahminy Kite", category="Bird")
        expected = "a photo of a Brahminy Kite (Haliastur indus), a type of bird."
        self.assertEqual(generate_prompt(entry), expected)
        self.assertEqual(format_prompt(entry), expected)

    def test_fetch_multi_taxa_taxonomy_merged(self):
        """Verify fetch_multi_taxa_taxonomy combines avian species with curated non-avian taxa."""
        mock_avian = (
            TaxonomyEntry("brakit1", "Haliastur indus", "Brahminy Kite", category="Bird"),
        )

        def mock_fetch():
            return mock_avian

        merged = fetch_multi_taxa_taxonomy(fetch_ebird_fn=mock_fetch, include_curated=True)
        self.assertGreater(len(merged), 1)
        self.assertEqual(merged[0].species_code, "brakit1")
        categories = {e.category for e in merged}
        self.assertIn("Bird", categories)
        self.assertIn("Mammalia", categories)
        self.assertIn("Insecta", categories)
        self.assertIn("Scenery", categories)

        unmerged = fetch_multi_taxa_taxonomy(fetch_ebird_fn=mock_fetch, include_curated=False)
        self.assertEqual(len(unmerged), 1)
        self.assertEqual(unmerged[0].species_code, "brakit1")


if __name__ == "__main__":
    unittest.main()
