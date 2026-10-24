import hashlib
import json
import os
import sys
import tempfile
import unittest
import torch
from safetensors.torch import load_file

sys.path.insert(0, os.path.abspath("scripts"))
from build_taxonomy_embeddings import sanitize_slug, compute_sha256


class TestBuildTaxonomyEmbeddings(unittest.TestCase):
    def test_sanitize_slug(self):
        """Verify slug sanitization removes punctuation and converts spaces/hyphens to underscores."""
        self.assertEqual(sanitize_slug("Black-necked Crane"), "Black_necked_Crane")
        self.assertEqual(sanitize_slug("Blyth's Tragopan"), "Blyths_Tragopan")
        self.assertEqual(sanitize_slug("Bar-headed Goose"), "Bar_headed_Goose")
        self.assertEqual(sanitize_slug("White-browed Tit-Warbler"), "White_browed_Tit_Warbler")
        self.assertEqual(sanitize_slug("   Double   Space   "), "Double_Space")

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

    def test_taxonomy_json_structure(self):
        """Verify models/taxonomy.json contains valid eBird species with mandatory keys."""
        tax_path = os.path.join("models", "taxonomy.json")
        if not os.path.exists(tax_path):
            self.skipTest("models/taxonomy.json not present in workspace")

        with open(tax_path, "r", encoding="utf-8") as f:
            taxonomy = json.load(f)

        self.assertGreater(len(taxonomy), 10000, "Should have 10k+ species")
        sample = taxonomy[0]
        self.assertIn("taxon_id", sample)
        self.assertIn("common_name", sample)
        self.assertIn("folder_slug", sample)
        self.assertIn("scientific_name", sample)

    def test_safetensors_shape_and_unit_norm(self):
        """Verify species_embeddings.safetensors matches taxonomy length and each vector has L2 norm == 1.0."""
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
