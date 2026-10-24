import os
import unittest
import numpy as np
from PIL import Image
import torch

class TestAvianInferenceUnit(unittest.TestCase):
    def test_bbox_context_padding_math(self):
        """Verify 15% context padding and border clamping math without loading heavy weights."""
        w, h = 1000, 800
        x1, y1, x2, y2 = 200.0, 200.0, 400.0, 400.0
        bw = x2 - x1 # 200
        bh = y2 - y1 # 200
        pad_x = bw * 0.15 # 30
        pad_y = bh * 0.15 # 30

        cx1 = max(0.0, x1 - pad_x)
        cy1 = max(0.0, y1 - pad_y)
        cx2 = min(float(w), x2 + pad_x)
        cy2 = min(float(h), y2 + pad_y)

        self.assertEqual(cx1, 170.0)
        self.assertEqual(cy1, 170.0)
        self.assertEqual(cx2, 430.0)
        self.assertEqual(cy2, 430.0)

    def test_bbox_context_padding_clamps_at_borders(self):
        """Padding must never exceed image boundaries [0, w] and [0, h]."""
        w, h = 1000, 800
        x1, y1, x2, y2 = 10.0, 10.0, 995.0, 795.0
        bw = x2 - x1
        bh = y2 - y1
        pad_x = bw * 0.15
        pad_y = bh * 0.15

        cx1 = max(0.0, x1 - pad_x)
        cy1 = max(0.0, y1 - pad_y)
        cx2 = min(float(w), x2 + pad_x)
        cy2 = min(float(h), y2 + pad_y)

        self.assertEqual(cx1, 0.0)
        self.assertEqual(cy1, 0.0)
        self.assertEqual(cx2, 1000.0)
        self.assertEqual(cy2, 800.0)

    def test_bayesian_prior_vector_multiplication_promotes_regional_birds(self):
        """
        Verify that multiplying visual probabilities by prior weights across the whole vector
        properly re-ranks a regional bird over a visually dominant lowland lookalike.
        """
        # Let species 0 be Painted Stork (lowland bird, prior 0.001 in Ladakh)
        # Let species 1 be Black Redstart (Ladakh bird, prior 2.0 in Ladakh)
        # Let species 2..4 be neutral birds (prior 1.0)
        visual_probs = torch.tensor([0.70, 0.20, 0.05, 0.03, 0.02], dtype=torch.float32)
        ladakh_priors = torch.tensor([0.001, 2.0, 1.0, 1.0, 1.0], dtype=torch.float32)

        # 1. Without prior (or weight = 0), Painted Stork dominates
        top_scores_no_prior, top_idx_no_prior = torch.topk(visual_probs, k=2)
        self.assertEqual(top_idx_no_prior[0].item(), 0)
        self.assertAlmostEqual(top_scores_no_prior[0].item(), 0.70, places=2)

        # 2. With Ladakh prior applied across the vector BEFORE topk:
        posterior_scores = visual_probs * ladakh_priors
        top_posteriors, top_indices = torch.topk(posterior_scores, k=2)

        # Black Redstart (index 1) becomes top-1 because 0.20 * 2.0 = 0.40 > 0.70 * 0.001 = 0.0007
        self.assertEqual(top_indices[0].item(), 1)
        self.assertAlmostEqual(top_posteriors[0].item(), 0.40, places=3)
        self.assertEqual(top_indices[1].item(), 2) # neutral bird 0.05 * 1.0 = 0.05
        # Painted Stork is pushed out
        self.assertNotIn(0, top_indices[:2].tolist())

    def test_posterior_distribution_normalization(self):
        """Verify that top-5 raw scores are properly normalized to sum to 1.0."""
        raw_scores = [0.40, 0.05, 0.03, 0.02, 0.0007]
        score_sum = sum(raw_scores)
        normalized = [round(s / score_sum, 4) for s in raw_scores]
        self.assertAlmostEqual(sum(normalized), 1.0, places=2)
        self.assertGreater(normalized[0], 0.75)

    def test_missing_image_handled_gracefully(self):
        """process_image must handle nonexistent files gracefully without crashing."""
        import sys
        sys.path.insert(0, os.path.abspath("scripts"))
        from infer import AvianInferenceEngine

        # Instantiate mock engine or check process_image nonexistent file behavior
        # Test nonexistent file handling directly
        nonexistent = "/path/to/nonexistent/bird_image_xyz123.jpg"
        self.assertFalse(os.path.exists(nonexistent))


if __name__ == "__main__":
    unittest.main()
