import json
import os
import sys
import tempfile
import unittest
from dataclasses import FrozenInstanceError
from unittest.mock import MagicMock, patch

import numpy as np
import pytest
import torch
from PIL import Image
from safetensors.torch import save_file

# Ensure repository root is on sys.path
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from scripts.infer import (
    AvianInferenceEngine,
    DetectionBox,
    InferencePrediction,
    apply_regional_prior,
    compute_iou,
    main,
    softmax,
)


class TestDetectionBox(unittest.TestCase):
    def test_creation_and_immutability(self):
        box = DetectionBox(x1=10.0, y1=20.0, x2=50.0, y2=80.0, confidence=0.95, class_id=14)
        self.assertEqual(box.x1, 10.0)
        self.assertEqual(box.y1, 20.0)
        self.assertEqual(box.x2, 50.0)
        self.assertEqual(box.y2, 80.0)
        self.assertEqual(box.confidence, 0.95)
        self.assertEqual(box.class_id, 14)
        self.assertAlmostEqual(box.area, 2400.0)
        self.assertEqual(box.to_xyxy(), (10.0, 20.0, 50.0, 80.0))

        with pytest.raises(FrozenInstanceError):
            box.x1 = 0.0  # type: ignore[misc]

    def test_zero_or_negative_area(self):
        box = DetectionBox(x1=50.0, y1=80.0, x2=10.0, y2=20.0, confidence=0.5)
        self.assertEqual(box.area, 0.0)

    def test_clip(self):
        box = DetectionBox(x1=-10.0, y1=-5.0, x2=150.0, y2=120.0, confidence=0.8)
        clipped = box.clip(width=100.0, height=80.0)
        self.assertEqual(clipped.x1, 0.0)
        self.assertEqual(clipped.y1, 0.0)
        self.assertEqual(clipped.x2, 100.0)
        self.assertEqual(clipped.y2, 80.0)
        self.assertEqual(clipped.confidence, 0.8)

    def test_to_dict(self):
        box = DetectionBox(x1=1.0, y1=2.0, x2=3.0, y2=4.0, confidence=0.9, class_id=14)
        d = box.to_dict()
        self.assertEqual(d["class_id"], 14)
        self.assertEqual(d["confidence"], 0.9)
        self.assertEqual(d["bbox_xyxy"], [1.0, 2.0, 3.0, 4.0])


class TestComputeIoU(unittest.TestCase):
    def test_identical_boxes(self):
        box_a = DetectionBox(0.0, 0.0, 10.0, 10.0, 0.9)
        box_b = DetectionBox(0.0, 0.0, 10.0, 10.0, 0.9)
        self.assertAlmostEqual(compute_iou(box_a, box_b), 1.0)

    def test_disjoint_boxes(self):
        box_a = DetectionBox(0.0, 0.0, 10.0, 10.0, 0.9)
        box_b = DetectionBox(20.0, 20.0, 30.0, 30.0, 0.9)
        self.assertEqual(compute_iou(box_a, box_b), 0.0)

    def test_overlapping_boxes(self):
        box_a = DetectionBox(0.0, 0.0, 10.0, 10.0, 0.9)
        box_b = DetectionBox(5.0, 0.0, 15.0, 10.0, 0.9)
        self.assertAlmostEqual(compute_iou(box_a, box_b), 50.0 / 150.0)

    def test_zero_area_box(self):
        box_a = DetectionBox(0.0, 0.0, 0.0, 0.0, 0.9)
        box_b = DetectionBox(0.0, 0.0, 10.0, 10.0, 0.9)
        self.assertEqual(compute_iou(box_a, box_b), 0.0)


class TestPureMathHelpers(unittest.TestCase):
    def test_softmax_distribution_and_temperature(self):
        logits = [2.0, 1.0, 0.1]
        probs = softmax(logits, temperature=1.0)
        self.assertIsInstance(probs, np.ndarray)
        self.assertAlmostEqual(float(np.sum(probs)), 1.0)
        self.assertGreater(probs[0], probs[1])
        self.assertGreater(probs[1], probs[2])

        probs_high_t = softmax(logits, temperature=100.0)
        self.assertTrue(np.allclose(probs_high_t, 1.0 / 3.0, atol=0.05))

        with pytest.raises(ValueError, match="temperature must be strictly positive"):
            softmax(logits, temperature=0.0)

    def test_apply_regional_prior(self):
        species = ["brakit1", "baleag", "blared1"]
        logits = [10.0, 10.0, 10.0]
        candidates = {"brakit1"}
        adjusted = apply_regional_prior(
            logits,
            species_codes=species,
            regional_candidates=candidates,
            boost_factor=2.0,
            penalty_factor=0.1,
        )
        self.assertAlmostEqual(adjusted[0], 20.0)
        self.assertAlmostEqual(adjusted[1], 1.0)
        self.assertAlmostEqual(adjusted[2], 1.0)


class TestInferencePrediction(unittest.TestCase):
    def test_immutability_and_to_dict(self):
        pred = InferencePrediction(
            species_code="brakit1",
            common_name="Brahminy Kite",
            scientific_name="Haliastur indus",
            confidence=0.85,
            raw_logit=12.4,
            regional_prior_applied=True,
            folder_slug="Brahminy_Kite",
            visual_prob=0.8,
            geo_prior=1.0,
        )
        self.assertEqual(pred.species_code, "brakit1")
        self.assertEqual(pred.common_name, "Brahminy Kite")
        self.assertEqual(pred.scientific_name, "Haliastur indus")
        self.assertEqual(pred.confidence, 0.85)
        self.assertEqual(pred.raw_logit, 12.4)
        self.assertTrue(pred.regional_prior_applied)
        self.assertIsNone(pred.bbox)

        d = pred.to_dict()
        self.assertEqual(d["taxon_id"], "brakit1")
        self.assertEqual(d["common_name"], "Brahminy Kite")
        self.assertEqual(d["folder_slug"], "Brahminy_Kite")
        self.assertEqual(d["scientific_name"], "Haliastur indus")
        self.assertEqual(d["combined_score"], 0.85)

        with pytest.raises(FrozenInstanceError):
            pred.confidence = 0.99  # type: ignore[misc]


class TestAvianInferenceEngineMocked(unittest.TestCase):
    def setUp(self):
        self.tmp_dir = tempfile.TemporaryDirectory()
        self.models_dir = self.tmp_dir.name

        self.taxonomy = [
            {
                "taxon_id": "brakit1",
                "common_name": "Brahminy Kite",
                "folder_slug": "Brahminy_Kite",
                "scientific_name": "Haliastur indus",
            },
            {
                "taxon_id": "baleag",
                "common_name": "Bald Eagle",
                "folder_slug": "Bald_Eagle",
                "scientific_name": "Haliaeetus leucocephalus",
            },
        ]
        with open(os.path.join(self.models_dir, "taxonomy.json"), "w", encoding="utf-8") as f:
            json.dump(self.taxonomy, f)

        emb = torch.zeros(len(self.taxonomy), 512, dtype=torch.float32)
        # Give Brahminy Kite (index 0) and Bald Eagle (index 1) distinct orthogonal unit vectors
        emb[0, 0] = 1.0
        emb[1, 1] = 1.0
        safetensors_path = os.path.join(self.models_dir, "species_embeddings.safetensors")
        save_file({"embeddings": emb}, safetensors_path)

        checklists_path = os.path.join(self.models_dir, "regional_checklists.json")
        with open(checklists_path, "w", encoding="utf-8") as f:
            json.dump({"IN-KA": ["brakit1"]}, f)

    def tearDown(self):
        self.tmp_dir.cleanup()

    def _create_mock_engine(self):
        class MockModel:
            visual = torch.nn.Identity()

        # Fixed preprocess vector so crop embedding is deterministic
        # (aligned with index 0: Brahminy Kite)
        def mock_preprocess(_img):
            v = torch.zeros(512, dtype=torch.float32)
            v[0] = 1.0
            return v

        with (
            patch(
                "open_clip.create_model_and_transforms",
                return_value=(MockModel(), None, mock_preprocess),
            ),
            patch("scripts.infer.YOLO") as mock_yolo_cls,
        ):
            mock_yolo = MagicMock()
            mock_yolo_cls.return_value = mock_yolo
            engine = AvianInferenceEngine(self.models_dir)
            engine.detector = mock_yolo
            return engine

    def test_detect_and_crop_with_bird_box(self):
        engine = self._create_mock_engine()

        mock_box = MagicMock()
        mock_box.cls = [torch.tensor(14)]
        mock_box.conf = [torch.tensor(0.9)]
        mock_box.xyxy = [torch.tensor([50.0, 50.0, 200.0, 200.0])]

        mock_result = MagicMock()
        mock_result.boxes = [mock_box]
        engine.detector.return_value = [mock_result]

        img = Image.new("RGB", (640, 480), color="blue")
        crop, conf, all_detections = engine.detect_and_crop(img, min_detect=0.25)

        self.assertIsNotNone(crop)
        self.assertAlmostEqual(conf, 0.9, places=5)
        self.assertEqual(len(all_detections), 1)
        self.assertEqual(all_detections[0]["class_id"], 14)

    def test_detect_and_crop_no_bird(self):
        engine = self._create_mock_engine()
        engine.detector.return_value = []

        img = Image.new("RGB", (640, 480), color="blue")
        crop, conf, all_detections = engine.detect_and_crop(img, min_detect=0.25)

        self.assertIsNone(crop)
        self.assertEqual(conf, 0.0)
        self.assertEqual(len(all_detections), 0)

    def test_process_image_nonexistent(self):
        engine = self._create_mock_engine()
        res = engine.process_image("/nonexistent/file.jpg")
        self.assertIn("error", res)
        self.assertFalse(res["detected"])

    def test_process_image_mock_full_flow(self):
        engine = self._create_mock_engine()

        mock_box = MagicMock()
        mock_box.cls = [torch.tensor(14)]
        mock_box.conf = [torch.tensor(0.95)]
        mock_box.xyxy = [torch.tensor([20.0, 20.0, 100.0, 100.0])]

        mock_res = MagicMock()
        mock_res.boxes = [mock_box]
        engine.detector.return_value = [mock_res]

        img_path = os.path.join(self.models_dir, "test_bird.jpg")
        Image.new("RGB", (200, 200), color="green").save(img_path)

        res = engine.process_image(
            img_path,
            min_detect=0.25,
            geo_prior_weight=1.0,
            region_code="IN-KA",
        )
        self.assertTrue(res["detected"])
        self.assertGreater(res["detection_confidence"], 0.9)
        self.assertIn("top_predictions", res)
        self.assertEqual(len(res["top_predictions"]), 2)
        self.assertEqual(res["assigned_species_slug"], "Brahminy_Kite")

    def test_main_cli_single_image_and_batch(self):
        engine = self._create_mock_engine()

        img_path = os.path.join(self.models_dir, "bird.jpg")
        Image.new("RGB", (100, 100), color="red").save(img_path)

        out_json = os.path.join(self.models_dir, "out.json")

        cmd_single = [
            "infer.py",
            "--models-dir",
            self.models_dir,
            "--image",
            img_path,
            "--output-file",
            out_json,
        ]
        with (
            patch("scripts.infer.AvianInferenceEngine", return_value=engine),
            patch.object(sys, "argv", cmd_single),
        ):
            main()
            self.assertTrue(os.path.exists(out_json))

        batch_tasks = [
            {
                "bundle_id": "b1",
                "primary_image": img_path,
                "trip_folder_name": "Trip1",
                "region_code": "IN-KA",
            }
        ]
        batch_json = os.path.join(self.models_dir, "batch.json")
        with open(batch_json, "w", encoding="utf-8") as f:
            json.dump(batch_tasks, f)

        batch_out = os.path.join(self.models_dir, "batch_out.json")
        cmd_batch = [
            "infer.py",
            "--models-dir",
            self.models_dir,
            "--batch-file",
            batch_json,
            "--output-file",
            batch_out,
        ]
        with (
            patch("scripts.infer.AvianInferenceEngine", return_value=engine),
            patch.object(sys, "argv", cmd_batch),
        ):
            main()
            self.assertTrue(os.path.exists(batch_out))
            with open(batch_out) as f:
                data = json.load(f)
            self.assertEqual(len(data), 1)
            self.assertEqual(data[0]["bundle_id"], "b1")


if __name__ == "__main__":
    unittest.main()
