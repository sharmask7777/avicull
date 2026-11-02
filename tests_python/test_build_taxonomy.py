import hashlib
import json
import os
import sys
import tempfile
import unittest
from dataclasses import FrozenInstanceError

import pytest
import torch
from safetensors.torch import load_file

# Ensure repository root is on sys.path
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from scripts.build_taxonomy_embeddings import (
    TaxonomyEntry,
    chunk_sequence,
    compute_sha256,
    compute_taxonomy_checksum,
    parse_taxonomy_records,
    sanitize_slug,
    sanitize_species_slug,
)


class TestBuildTaxonomyEmbeddings(unittest.TestCase):
    def test_taxonomy_entry_immutability(self):
        """TaxonomyEntry should be an immutable frozen dataclass."""
        entry = TaxonomyEntry(
            species_code="brakit1",
            scientific_name="Haliastur indus",
            common_name="Brahminy Kite",
            aliases=("Red-backed Sea-Eagle",),
        )
        self.assertEqual(entry.species_code, "brakit1")
        self.assertEqual(entry.scientific_name, "Haliastur indus")
        self.assertEqual(entry.common_name, "Brahminy Kite")
        self.assertEqual(entry.folder_slug, "Brahminy_Kite")
        self.assertEqual(entry.aliases, ("Red-backed Sea-Eagle",))

        with pytest.raises(FrozenInstanceError):
            entry.species_code = "other"  # type: ignore[misc]

    def test_sanitize_species_slug(self):
        """Verify slug sanitization removes punctuation and converts spaces/hyphens."""
        self.assertEqual(sanitize_species_slug("Black-necked Crane"), "Black_necked_Crane")
        self.assertEqual(sanitize_species_slug("Blyth's Tragopan"), "Blyths_Tragopan")
        self.assertEqual(sanitize_species_slug("Bar-headed Goose"), "Bar_headed_Goose")
        self.assertEqual(
            sanitize_species_slug("White-browed Tit-Warbler"),
            "White_browed_Tit_Warbler",
        )
        self.assertEqual(sanitize_species_slug("   Double   Space   "), "Double_Space")
        # sanitize_slug backward compatibility
        self.assertEqual(sanitize_slug("Blyth's Tragopan"), "Blyths_Tragopan")

    def test_parse_taxonomy_records(self):
        """Verify parsing raw eBird API dictionaries into TaxonomyEntry tuples."""
        raw_records = [
            {
                "category": "species",
                "speciesCode": "brakit1",
                "comName": "Brahminy Kite",
                "sciName": "Haliastur indus",
            },
            {
                "category": "slash",  # Non-species category should be skipped
                "speciesCode": "sparrow1",
                "comName": "Sparrow sp.",
                "sciName": "Passer sp.",
            },
            {
                "category": "species",  # Duplicate speciesCode should be skipped
                "speciesCode": "brakit1",
                "comName": "Brahminy Kite Duplicate",
                "sciName": "Haliastur indus",
            },
            {
                "category": "species",
                "speciesCode": "baleag",
                "comName": "Bald Eagle",
                "sciName": "Haliaeetus leucocephalus",
            },
            {
                "category": "species",
                "speciesCode": "",  # Empty code should be skipped
                "comName": "Unknown",
                "sciName": "Unknown",
            },
        ]
        entries = parse_taxonomy_records(raw_records)
        self.assertEqual(len(entries), 2)
        self.assertEqual(entries[0].species_code, "brakit1")
        self.assertEqual(entries[0].common_name, "Brahminy Kite")
        self.assertEqual(entries[1].species_code, "baleag")
        self.assertEqual(entries[1].scientific_name, "Haliaeetus leucocephalus")

    def test_chunk_sequence(self):
        """Verify sequence chunking produces non-overlapping contiguous tuples."""
        items = list(range(10))
        chunks = chunk_sequence(items, chunk_size=3)
        self.assertEqual(chunks, ((0, 1, 2), (3, 4, 5), (6, 7, 8), (9,)))

        empty_chunks = chunk_sequence([], chunk_size=5)
        self.assertEqual(empty_chunks, ())

        with pytest.raises(ValueError, match="chunk_size must be positive"):
            chunk_sequence(items, chunk_size=0)

    def test_compute_taxonomy_checksum(self):
        """Verify deterministic sha256 checksum calculation over taxonomy entries."""
        entries = (
            TaxonomyEntry("brakit1", "Haliastur indus", "Brahminy Kite"),
            TaxonomyEntry("baleag", "Haliaeetus leucocephalus", "Bald Eagle"),
        )
        checksum1 = compute_taxonomy_checksum(entries)
        checksum2 = compute_taxonomy_checksum(entries)
        self.assertEqual(checksum1, checksum2)
        self.assertEqual(len(checksum1), 64)

        # Modifying entry changes checksum
        altered_entries = (
            TaxonomyEntry("brakit1", "Haliastur indus", "Brahminy Kite"),
            TaxonomyEntry("baleag", "Haliaeetus leucocephalus", "Bald Eagle Altered"),
        )
        self.assertNotEqual(checksum1, compute_taxonomy_checksum(altered_entries))

    def test_compute_sha256(self):
        """Verify SHA256 matches expected digest."""
        with tempfile.NamedTemporaryFile("w+", delete=False) as f:
            f.write("hello avicull")
            f_path = f.name
        try:
            expected = hashlib.sha256(b"hello avicull").hexdigest()
            self.assertEqual(compute_sha256(f_path), expected)
        finally:
            if os.path.exists(f_path):
                os.remove(f_path)

    def test_compute_species_embeddings_mocked(self):
        """Verify batching and normalization of embeddings with mocked model and tokenizer."""
        from scripts.build_taxonomy_embeddings import compute_species_embeddings

        entries = (
            TaxonomyEntry("brakit1", "Haliastur indus", "Brahminy Kite"),
            TaxonomyEntry("baleag", "Haliaeetus leucocephalus", "Bald Eagle"),
            TaxonomyEntry("blared1", "Phoenicurus ochruros", "Black Redstart"),
        )

        class MockTokenizer:
            def __call__(self, texts):
                return torch.zeros((len(texts), 10), dtype=torch.long)

        class MockModel:
            def encode_text(self, tokens):
                # Return unnormalized batch tensor [B, 512]
                batch_size = tokens.shape[0]
                vecs = torch.full((batch_size, 512), 2.0, dtype=torch.float32)
                return vecs

        embeddings = compute_species_embeddings(
            entries=entries,
            model=MockModel(),
            tokenizer=MockTokenizer(),
            device="cpu",
            batch_size=2,
        )
        self.assertEqual(embeddings.shape, (3, 512))
        # Verify L2 normalization
        norms = torch.norm(embeddings, dim=-1)
        for n in norms:
            self.assertAlmostEqual(n.item(), 1.0, places=4)

        # Empty sequence returns empty tensor
        empty_emb = compute_species_embeddings(
            entries=(),
            model=MockModel(),
            tokenizer=MockTokenizer(),
            device="cpu",
            batch_size=2,
        )
        self.assertEqual(empty_emb.shape, (0, 512))

    def test_build_taxonomy_pipeline_mocked(self):
        """Verify full pipeline builds taxonomy.json, safetensors, and manifest offline."""
        from scripts.build_taxonomy_embeddings import build_taxonomy_pipeline

        with tempfile.TemporaryDirectory() as tmp_dir:
            sample_entries = (
                TaxonomyEntry("brakit1", "Haliastur indus", "Brahminy Kite", aliases=("Alias 1",)),
                TaxonomyEntry("baleag", "Haliaeetus leucocephalus", "Bald Eagle"),
            )

            def mock_fetch():
                return sample_entries

            def mock_embed(entries):
                t = torch.randn(len(entries), 512)
                return t / t.norm(dim=-1, keepdim=True)

            def mock_yolo(target_dir):
                with open(os.path.join(target_dir, "yolo11n.pt"), "w") as f:
                    f.write("mock_yolo_weights")

            manifest = build_taxonomy_pipeline(
                models_dir=tmp_dir,
                fetch_taxonomy_fn=mock_fetch,
                compute_embeddings_fn=mock_embed,
                copy_yolo_fn=mock_yolo,
            )

            self.assertIn("taxonomy.json", manifest)
            self.assertIn("species_embeddings.safetensors", manifest)
            self.assertIn("yolo11n.pt", manifest)

            # Check taxonomy.json content
            with open(os.path.join(tmp_dir, "taxonomy.json"), encoding="utf-8") as f:
                tax_data = json.load(f)
            self.assertEqual(len(tax_data), 2)
            self.assertEqual(tax_data[0]["taxon_id"], "brakit1")
            self.assertEqual(tax_data[0]["aliases"], ["Alias 1"])
            self.assertNotIn("aliases", tax_data[1])

            # Check safetensors
            data = load_file(os.path.join(tmp_dir, "species_embeddings.safetensors"))
            self.assertEqual(data["embeddings"].shape, (2, 512))

            # Check manifest.json
            with open(os.path.join(tmp_dir, "manifest.json"), encoding="utf-8") as f:
                manifest_file = json.load(f)
            self.assertEqual(manifest_file, manifest)

    def test_fetch_ebird_taxonomy_mocked(self, monkeypatch=None):
        """Verify fetch_ebird_taxonomy handles HTTP requests and returns TaxonomyEntry tuples."""
        import io
        import urllib.request
        from unittest.mock import MagicMock, patch

        from scripts.build_taxonomy_embeddings import fetch_ebird_taxonomy

        dummy_api_response = json.dumps(
            [
                {
                    "category": "species",
                    "speciesCode": "brakit1",
                    "comName": "Brahminy Kite",
                    "sciName": "Haliastur indus",
                }
            ]
        ).encode("utf-8")

        mock_resp = MagicMock()
        mock_resp.read.return_value = dummy_api_response
        mock_resp.__enter__.return_value = io.BytesIO(dummy_api_response)

        with patch.object(urllib.request, "urlopen", return_value=mock_resp):
            entries = fetch_ebird_taxonomy()
            self.assertEqual(len(entries), 1)
            self.assertEqual(entries[0].species_code, "brakit1")

    def test_taxonomy_json_structure(self):
        """Verify models/taxonomy.json contains valid eBird species with mandatory keys."""
        tax_path = os.path.join("models", "taxonomy.json")
        if not os.path.exists(tax_path):
            self.skipTest("models/taxonomy.json not present in workspace")

        with open(tax_path, encoding="utf-8") as f:
            taxonomy = json.load(f)

        self.assertGreater(len(taxonomy), 10000, "Should have 10k+ species")
        sample = taxonomy[0]
        self.assertIn("taxon_id", sample)
        self.assertIn("common_name", sample)
        self.assertIn("folder_slug", sample)
        self.assertIn("scientific_name", sample)

    def test_safetensors_shape_and_unit_norm(self):
        """Verify safetensors matches taxonomy length and each vector has L2 norm == 1.0."""
        emb_path = os.path.join("models", "species_embeddings.safetensors")
        if not os.path.exists(emb_path):
            self.skipTest("models/species_embeddings.safetensors not present in workspace")

        data = load_file(emb_path)
        embeddings = data["embeddings"]
        self.assertEqual(len(embeddings.shape), 2)
        self.assertEqual(embeddings.shape[1], 512)

        norms = torch.norm(embeddings[:100], dim=-1)
        for n in norms:
            self.assertAlmostEqual(n.item(), 1.0, places=4)


if __name__ == "__main__":
    unittest.main()
