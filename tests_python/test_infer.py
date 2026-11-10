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
    COCO_ANIMAL_CLASS_IDS,
    COCO_BIRD_CLASS_ID,
    COCO_PERSON_CLASS_ID,
    AssetCategory,
    AvianInferenceEngine,
    CategoryAnchorResult,
    DetectionBox,
    InferencePrediction,
    apply_regional_prior,
    classify_category_anchors,
    compute_category_centroids,
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

    def test_positional_args_with_category_first(self):
        """Verify InferencePrediction accepts (category, species_code, ...) positionally."""
        emb = [0.1] * 512
        pred = InferencePrediction(
            "Mammalia",
            "mamm_sq1",
            "Malabar Giant Squirrel",
            "Ratufa indica",
            0.92,
            emb,
        )
        self.assertEqual(pred.category, "Mammalia")
        self.assertEqual(pred.species_code, "mamm_sq1")
        self.assertEqual(pred.common_name, "Malabar Giant Squirrel")
        self.assertEqual(pred.scientific_name, "Ratufa indica")
        self.assertEqual(pred.confidence, 0.92)
        self.assertEqual(pred.embedding, emb)

        d = pred.to_dict()
        self.assertEqual(d["category"], "Mammalia")
        self.assertEqual(d["embedding"], emb)

    def test_positional_args_with_species_code_first(self):
        """Verify InferencePrediction accepts (species_code, common_name, ...) positionally."""
        emb = [0.2] * 512
        pred = InferencePrediction(
            "ins_peacock1",
            "Malabar Banded Peacock",
            "Papilio buddha",
            0.95,
            "Insecta",
            emb,
        )
        self.assertEqual(pred.category, "Insecta")
        self.assertEqual(pred.species_code, "ins_peacock1")
        self.assertEqual(pred.common_name, "Malabar Banded Peacock")
        self.assertEqual(pred.scientific_name, "Papilio buddha")
        self.assertEqual(pred.confidence, 0.95)
        self.assertEqual(pred.embedding, emb)


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

    def test_batch_inference_emits_verbose_progress_to_stderr(self):
        import io

        engine = self._create_mock_engine()
        img_path = os.path.join(self.models_dir, "bird_progress.jpg")
        Image.new("RGB", (100, 100), color="blue").save(img_path)

        batch_tasks = [
            {
                "bundle_id": "bundle_test_1",
                "primary_image": img_path,
                "trip_folder_name": "TripTest",
                "region_code": "IN-KA",
            }
        ]
        batch_json = os.path.join(self.models_dir, "batch_progress.json")
        with open(batch_json, "w", encoding="utf-8") as f:
            json.dump(batch_tasks, f)

        batch_out = os.path.join(self.models_dir, "batch_progress_out.json")
        cmd_batch = [
            "infer.py",
            "--models-dir",
            self.models_dir,
            "--batch-file",
            batch_json,
            "--output-file",
            batch_out,
        ]
        fake_stderr = io.StringIO()
        with (
            patch("scripts.infer.AvianInferenceEngine", return_value=engine),
            patch.object(sys, "argv", cmd_batch),
            patch.object(sys, "stderr", fake_stderr),
        ):
            main()
            output = fake_stderr.getvalue()
            self.assertIn("[inference] Progress: 1/1 (100%)", output)


class TestCategoryAnchorsAndGating(unittest.TestCase):
    def setUp(self):
        # 4 orthogonal unit vectors for 4 categories in 512 dimensions
        self.bird_vec = torch.zeros(512, dtype=torch.float32)
        self.bird_vec[0] = 1.0

        self.mammal_vec = torch.zeros(512, dtype=torch.float32)
        self.mammal_vec[1] = 1.0

        self.insect_vec = torch.zeros(512, dtype=torch.float32)
        self.insect_vec[2] = 1.0

        self.scenery_vec = torch.zeros(512, dtype=torch.float32)
        self.scenery_vec[3] = 1.0

        self.centroids = {
            "Bird": self.bird_vec,
            "Mammalia": self.mammal_vec,
            "Insecta": self.insect_vec,
            "Scenery": self.scenery_vec,
        }

    def test_classify_category_anchors_identifies_all_centroids(self):
        """Verify classify_category_anchors identifies mammal, insect, bird, and scenery."""
        for cat_name, vec in self.centroids.items():
            result = classify_category_anchors(vec, self.centroids, margin=0.02)
            self.assertIsInstance(result, CategoryAnchorResult)
            self.assertEqual(result.category, cat_name)
            self.assertAlmostEqual(result.confidence, 1.0, places=4)
            self.assertFalse(result.is_ambiguous)
            self.assertAlmostEqual(result.scores[cat_name], 1.0, places=4)

    def test_classify_category_anchors_ambiguous_margin(self):
        """Verify ambiguous difference between top categories is flagged with is_ambiguous=True."""
        # Vector equidistant between Bird (dim 0) and Mammalia (dim 1)
        ambiguous_vec = torch.zeros(512, dtype=torch.float32)
        ambiguous_vec[0] = 0.7071
        ambiguous_vec[1] = 0.7071

        result = classify_category_anchors(ambiguous_vec, self.centroids, margin=0.02)
        self.assertTrue(result.is_ambiguous)
        self.assertIn(result.category, ("Bird", "Mammalia"))

    def test_compute_category_centroids(self):
        """Verify compute_category_centroids computes normalized centroids across taxa records."""
        taxonomy = [
            {"taxon_id": "sp1", "category": "Bird"},
            {"taxon_id": "sp2", "category": "Bird"},
            {"taxon_id": "sp3", "category": "Mammalia"},
            {"taxon_id": "sp4", "category": "Insecta"},
            {"taxon_id": "sp5", "category": "Scenery"},
        ]
        emb = torch.zeros((5, 512), dtype=torch.float32)
        emb[0, 0] = 1.0
        emb[1, 0] = 1.0  # Both birds on dim 0
        emb[2, 1] = 1.0  # Mammal on dim 1
        emb[3, 2] = 1.0  # Insect on dim 2
        emb[4, 3] = 1.0  # Scenery on dim 3

        centroids = compute_category_centroids(emb, taxonomy)
        self.assertIn("Bird", centroids)
        self.assertIn("Mammalia", centroids)
        self.assertIn("Insecta", centroids)
        self.assertIn("Scenery", centroids)

        # Bird centroid should have unit norm
        self.assertAlmostEqual(float(torch.norm(centroids["Bird"])), 1.0, places=4)
        self.assertAlmostEqual(float(centroids["Bird"][0]), 1.0, places=4)

    def test_classify_category_anchors_empty_centroids(self):
        """Verify empty centroids fallback cleanly without errors."""
        res = classify_category_anchors(torch.zeros(512), {})
        self.assertEqual(res.category, "Bird")
        self.assertEqual(res.confidence, 1.0)
        self.assertFalse(res.is_ambiguous)

    def test_classify_category_anchors_numpy_inputs(self):
        """Verify numpy array inputs are handled transparently."""
        z = np.zeros(512, dtype=np.float32)
        z[1] = 1.0
        mamm_c = np.zeros(512, dtype=np.float32)
        mamm_c[1] = 1.0
        bird_c = np.zeros(512, dtype=np.float32)
        bird_c[0] = 1.0

        res = classify_category_anchors(z, {"Bird": bird_c, "Mammalia": mamm_c})
        self.assertEqual(res.category, "Mammalia")
        self.assertAlmostEqual(res.confidence, 1.0, places=4)

    def test_compute_category_centroids_with_objects(self):
        """Verify compute_category_centroids works with objects having category attribute."""

        class MockTaxon:
            def __init__(self, category: str):
                self.category = category

        emb = np.zeros((2, 512), dtype=np.float32)
        emb[0, 0] = 1.0
        emb[1, 1] = 1.0
        taxa = [MockTaxon("Bird"), MockTaxon("Mammalia")]
        centroids = compute_category_centroids(emb, taxa)
        self.assertIn("Bird", centroids)
        self.assertIn("Mammalia", centroids)


class TestCOCOAnimalCandidateDetections(unittest.TestCase):
    def test_coco_animal_class_ids_presence(self):
        """Verify COCO animal class IDs contains birds and all standard animals."""
        self.assertIn(COCO_BIRD_CLASS_ID, COCO_ANIMAL_CLASS_IDS)
        expected_ids = {14, 15, 16, 17, 18, 19, 20, 21, 22, 23}
        self.assertTrue(expected_ids.issubset(COCO_ANIMAL_CLASS_IDS))

    def test_detect_and_crop_captures_coco_animals(self):
        """Verify detect_and_crop captures non-avian COCO animals (e.g. dog=16, elephant=20)."""
        engine = MagicMock(spec=AvianInferenceEngine)
        engine.detector = MagicMock()

        mock_box_dog = MagicMock()
        mock_box_dog.cls = [torch.tensor(16)]  # dog
        mock_box_dog.conf = [torch.tensor(0.88)]
        mock_box_dog.xyxy = [torch.tensor([10.0, 10.0, 100.0, 100.0])]

        mock_box_elephant = MagicMock()
        mock_box_elephant.cls = [torch.tensor(20)]  # elephant
        mock_box_elephant.conf = [torch.tensor(0.92)]
        mock_box_elephant.xyxy = [torch.tensor([150.0, 150.0, 400.0, 400.0])]

        mock_result = MagicMock()
        mock_result.boxes = [mock_box_dog, mock_box_elephant]
        engine.detector.return_value = [mock_result]

        # Use the actual detect_and_crop method
        img = Image.new("RGB", (640, 480), color="green")
        crop, conf, all_detections = AvianInferenceEngine.detect_and_crop(
            engine, img, min_detect=0.25
        )

        self.assertIsNotNone(crop)
        self.assertAlmostEqual(conf, 0.92, places=4)
        self.assertEqual(len(all_detections), 2)
        det_classes = {d["class_id"] for d in all_detections}
        self.assertEqual(det_classes, {16, 20})

    def test_detect_and_crop_filters_microscopic_speckle_boxes(self):
        """A detection box with area ratio < 0.001 of total image area should be rejected."""
        engine = MagicMock(spec=AvianInferenceEngine)
        engine.detector = MagicMock()

        # Image size: 1000 x 1000 = 1,000,000 area
        # Box size: 10 x 10 = 100 area (ratio 0.0001 < 0.001)
        mock_box_speckle = MagicMock()
        mock_box_speckle.cls = [torch.tensor(14)]  # bird
        mock_box_speckle.conf = [torch.tensor(0.95)]  # high confidence speckle/noise
        mock_box_speckle.xyxy = [torch.tensor([100.0, 100.0, 110.0, 110.0])]

        mock_result = MagicMock()
        mock_result.boxes = [mock_box_speckle]
        engine.detector.return_value = [mock_result]

        img = Image.new("RGB", (1000, 1000), color="blue")
        crop, conf, all_detections = AvianInferenceEngine.detect_and_crop(
            engine, img, min_detect=0.25
        )

        self.assertIsNone(crop)
        self.assertEqual(conf, 0.0)
        self.assertEqual(len(all_detections), 1)


class TestMultiTaxaHierarchicalInferenceFlow(unittest.TestCase):
    def setUp(self):
        self.tmp_dir = tempfile.TemporaryDirectory()
        self.models_dir = self.tmp_dir.name

        self.taxonomy = [
            {
                "taxon_id": "brakit1",
                "common_name": "Brahminy Kite",
                "folder_slug": "Brahminy_Kite",
                "scientific_name": "Haliastur indus",
                "category": "Bird",
            },
            {
                "taxon_id": "mamm_sq1",
                "common_name": "Malabar Giant Squirrel",
                "folder_slug": "Malabar_Giant_Squirrel",
                "scientific_name": "Ratufa indica",
                "category": "Mammalia",
            },
            {
                "taxon_id": "ins_peacock1",
                "common_name": "Malabar Banded Peacock",
                "folder_slug": "Malabar_Banded_Peacock",
                "scientific_name": "Papilio buddha",
                "category": "Insecta",
            },
            {
                "taxon_id": "scen_mountain1",
                "common_name": "Mountain Landscape",
                "folder_slug": "Mountain_Landscape",
                "scientific_name": "",
                "category": "Scenery",
            },
            {
                "taxon_id": "port_close_up1",
                "common_name": "Close-up Human Portrait",
                "folder_slug": "Portrait/Close-up_Human_Portrait",
                "scientific_name": "Homo sapiens faciei",
                "category": "Portrait",
            },
        ]
        with open(os.path.join(self.models_dir, "taxonomy.json"), "w", encoding="utf-8") as f:
            json.dump(self.taxonomy, f)

        # 5 distinct orthogonal vectors for each species
        emb = torch.zeros(len(self.taxonomy), 512, dtype=torch.float32)
        emb[0, 0] = 1.0  # Bird on dim 0
        emb[1, 1] = 1.0  # Mammalia on dim 1
        emb[2, 2] = 1.0  # Insecta on dim 2
        emb[3, 3] = 1.0  # Scenery on dim 3
        emb[4, 4] = 1.0  # Portrait on dim 4
        safetensors_path = os.path.join(self.models_dir, "species_embeddings.safetensors")
        save_file({"embeddings": emb}, safetensors_path)

    def tearDown(self):
        self.tmp_dir.cleanup()

    def _create_mock_engine(self, embedding_vector: torch.Tensor):
        class MockModel:
            visual = torch.nn.Identity()

        def mock_preprocess(_img):
            return embedding_vector.clone()

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

    def test_open_set_rejection_butterfly_assigned_insecta(self):
        """Verify open-set rejection: a butterfly is rejected from birds and assigned to Insecta."""
        # Vector corresponding to insect (dim 2)
        butterfly_vec = torch.zeros(512, dtype=torch.float32)
        butterfly_vec[2] = 1.0

        engine = self._create_mock_engine(butterfly_vec)

        # YOLO detects an object (e.g. animal/insect crop)
        mock_box = MagicMock()
        mock_box.cls = [torch.tensor(14)]
        mock_box.conf = [torch.tensor(0.9)]
        mock_box.xyxy = [torch.tensor([10.0, 10.0, 100.0, 100.0])]
        mock_res = MagicMock()
        mock_res.boxes = [mock_box]
        engine.detector.return_value = [mock_res]

        img_path = os.path.join(self.models_dir, "butterfly.jpg")
        Image.new("RGB", (100, 100), color="yellow").save(img_path)

        res = engine.process_image(img_path)

        # Must NOT be classified as bird! Must be assigned to Insecta
        self.assertEqual(res["category"], "Insecta")
        self.assertEqual(res["assigned_species_slug"], "Malabar_Banded_Peacock")
        self.assertGreater(res["final_confidence"], 0.8)
        self.assertTrue(res["detected"])

    def test_detector_fallback_scenery_returns_scenery_category(self):
        """Verify detector fallback: crop is None on scenery returns Scenery instead of bird."""
        # Vector corresponding to scenery (dim 3)
        scenery_vec = torch.zeros(512, dtype=torch.float32)
        scenery_vec[3] = 1.0

        engine = self._create_mock_engine(scenery_vec)
        # YOLO returns NO detections (crop is None)
        engine.detector.return_value = []

        img_path = os.path.join(self.models_dir, "landscape.jpg")
        # Natural image with texture / gradients (non-zero visual entropy)
        landscape_img = Image.effect_noise((200, 200), 20).convert("RGB")
        landscape_img.save(img_path)

        res = engine.process_image(img_path)

        # Must return Scenery category instead of guessing a random bird
        self.assertEqual(res["category"], "Scenery")
        self.assertEqual(res["assigned_species_slug"], "Mountain_Landscape")
        self.assertNotEqual(res["category"], "Bird")

    def test_detector_fallback_ambiguous_blur_returns_no_subject_detected(self):
        """Verify detector fallback: crop is None on blur returns _No_Subject_Detected."""
        # Ambiguous blur vector with negligible or ambiguous values
        ambiguous_vec = torch.zeros(512, dtype=torch.float32)
        # Low values or zero vector
        ambiguous_vec[0] = 0.05
        ambiguous_vec[1] = 0.05

        engine = self._create_mock_engine(ambiguous_vec)
        # YOLO returns NO detections (crop is None)
        engine.detector.return_value = []

        img_path = os.path.join(self.models_dir, "blur.jpg")
        Image.new("RGB", (100, 100), color="black").save(img_path)

        res = engine.process_image(img_path)

        self.assertEqual(res["assigned_species_slug"], "_No_Subject_Detected")
        self.assertAlmostEqual(res["final_confidence"], 0.0, places=4)
        self.assertFalse(res["detected"])
        self.assertEqual(res["detection_confidence"], 0.0)
        self.assertEqual(len(res["top_predictions"]), 0)

    def test_detector_bird_crop_defaults_to_bird_category(self):
        """Verify that when YOLO detects a bird (class_id == 14), category defaults to Bird."""
        # Bird vector on dim 0
        # If mammal centroid similarity is artificially higher due to centroid pooling:
        # Centroid for Bird might be dim 0 (0.5), Centroid for Mammalia is dim 1 (1.0).
        # Visual embedding has Bird similarity 0.7 on dim 0, Mammal similarity 0.72 on dim 1.
        bird_vec = torch.zeros(512, dtype=torch.float32)
        bird_vec[0] = 0.7
        bird_vec[1] = 0.72  # slightly higher mammal centroid

        engine = self._create_mock_engine(bird_vec)

        # YOLO detects a bird box (class_id = 14)
        mock_box = MagicMock()
        mock_box.cls = [torch.tensor(COCO_BIRD_CLASS_ID)]
        mock_box.conf = [torch.tensor(0.85)]
        mock_box.xyxy = [torch.tensor([10.0, 10.0, 90.0, 90.0])]
        mock_res = MagicMock()
        mock_res.boxes = [mock_box]
        engine.detector.return_value = [mock_res]

        img_path = os.path.join(self.models_dir, "bird_sample.jpg")
        Image.new("RGB", (100, 100), color="brown").save(img_path)

        res = engine.process_image(img_path)
        self.assertEqual(res["category"], "Bird")
        self.assertEqual(res["assigned_species_slug"], "Brahminy_Kite")
        self.assertTrue(res["detected"])

    def test_yolo_mammal_detection_routes_to_mammalia_not_bird(self):
        """When YOLO detects a mammal class with high confidence and no bird class,
        target_category should be Mammalia even if top raw candidate in taxonomy is a bird."""
        # Visual embedding has highest cosine similarity with Bird (dim 0 = 0.9, dim 1 = 0.8)
        bird_leaning_vec = torch.zeros(512, dtype=torch.float32)
        bird_leaning_vec[0] = 0.9  # Brahminy Kite (Bird)
        bird_leaning_vec[1] = 0.8  # Malabar Giant Squirrel (Mammalia)

        engine = self._create_mock_engine(bird_leaning_vec)

        # YOLO detects a mammal box (e.g. class 18 'sheep') with high conf and NO bird box
        mock_box = MagicMock()
        mock_box.cls = [torch.tensor(18)]  # sheep (mammal)
        mock_box.conf = [torch.tensor(0.92)]
        mock_box.xyxy = [torch.tensor([10.0, 10.0, 90.0, 90.0])]
        mock_res = MagicMock()
        mock_res.boxes = [mock_box]
        engine.detector.return_value = [mock_res]

        img_path = os.path.join(self.models_dir, "mammal_sample.jpg")
        Image.new("RGB", (100, 100), color="gray").save(img_path)

        res = engine.process_image(img_path)
        self.assertEqual(res["category"], "Mammalia")
        self.assertEqual(res["assigned_species_slug"], "Malabar_Giant_Squirrel")
        self.assertTrue(res["detected"])

    def test_detector_fallback_synthetic_black_returns_no_subject_detected(self):
        """Verify fallback on pure black/synthetic image returns _No_Subject_Detected."""
        # Even if scenery vector is high
        scenery_vec = torch.zeros(512, dtype=torch.float32)
        scenery_vec[3] = 1.0

        engine = self._create_mock_engine(scenery_vec)
        engine.detector.return_value = []

        # Synthetic pure black image (zero entropy)
        img_path = os.path.join(self.models_dir, "pure_black.jpg")
        Image.new("RGB", (100, 100), color="black").save(img_path)

        res = engine.process_image(img_path)
        self.assertEqual(res["assigned_species_slug"], "_No_Subject_Detected")
        self.assertEqual(res["category"], "No_Subject_Detected")
        self.assertFalse(res["detected"])

    def test_compute_category_centroids_normalizes_missing_or_invalid_category(self):
        """Verify compute_category_centroids normalizes invalid or intergrade categories to Bird."""
        emb = torch.zeros(3, 512, dtype=torch.float32)
        emb[0, 0] = 1.0
        emb[1, 0] = 1.0
        emb[2, 1] = 1.0
        tax = [
            {"category": "intergrade", "taxon_id": "sp1"},
            {"category": "unknown_cat", "taxon_id": "sp2"},
            {"category": "Mammalia", "taxon_id": "mamm1"},
        ]
        centroids = compute_category_centroids(emb, tax)
        self.assertIn("Bird", centroids)
        self.assertIn("Mammalia", centroids)
        self.assertNotIn("intergrade", centroids)
        self.assertNotIn("unknown_cat", centroids)

    def test_person_detection_routes_to_portrait(self):
        """Verify YOLO person detection (class_id=0) routes to Portrait category."""
        portrait_vec = torch.zeros(512, dtype=torch.float32)
        portrait_vec[4] = 1.0  # high match for portrait

        engine = self._create_mock_engine(portrait_vec)

        mock_box = MagicMock()
        mock_box.cls = [torch.tensor(COCO_PERSON_CLASS_ID)]
        mock_box.conf = [torch.tensor(0.88)]
        mock_box.xyxy = [torch.tensor([10.0, 10.0, 90.0, 90.0])]
        mock_res = MagicMock()
        mock_res.boxes = [mock_box]
        engine.detector.return_value = [mock_res]

        img_path = os.path.join(self.models_dir, "person_sample.jpg")
        Image.new("RGB", (100, 100), color="pink").save(img_path)

        res = engine.process_image(img_path)
        self.assertEqual(res["category"], "Portrait")
        self.assertTrue(res["detected"])
        self.assertIn("Portrait", res["assigned_species_slug"])

    def test_detector_fallback_natural_landscape_routes_to_scenery(self):
        """Verify detector fallback with high scenery anchor similarity routes to Scenery."""
        # Scenery embedding
        scenery_vec = torch.zeros(512, dtype=torch.float32)
        scenery_vec[3] = 1.0

        engine = self._create_mock_engine(scenery_vec)
        engine.detector.return_value = []

        # Natural textured image (non-zero entropy)
        img = Image.new("RGB", (100, 100))
        for x in range(100):
            for y in range(100):
                img.putpixel((x, y), ((x * 7) % 256, (y * 11) % 256, (x + y) % 256))
        img_path = os.path.join(self.models_dir, "natural_landscape.jpg")
        img.save(img_path)

        res = engine.process_image(img_path)
        self.assertIn(res["category"], ("Scenery", "Architecture", "Portrait", "Abstract"))
        self.assertTrue(res["detected"])
        self.assertNotEqual(res["assigned_species_slug"], "_No_Subject_Detected")


class TestAssetCategory(unittest.TestCase):
    def test_enum_variants_and_values(self):
        self.assertEqual(AssetCategory.BIRD, "Bird")
        self.assertEqual(AssetCategory.MAMMALIA, "Mammalia")
        self.assertEqual(AssetCategory.INSECTA, "Insecta")
        self.assertEqual(AssetCategory.HERPETOFAUNA, "Herpetofauna")
        self.assertEqual(AssetCategory.REPTILIA, "Reptilia")
        self.assertEqual(AssetCategory.AMPHIBIA, "Amphibia")
        self.assertEqual(AssetCategory.SCENERY, "Scenery")
        self.assertEqual(AssetCategory.PORTRAIT, "Portrait")
        self.assertEqual(AssetCategory.ARCHITECTURE, "Architecture")
        self.assertEqual(AssetCategory.ABSTRACT, "Abstract")
        self.assertEqual(AssetCategory.NO_SUBJECT_DETECTED, "No_Subject_Detected")
        self.assertEqual(AssetCategory.UNKNOWN, "Unknown")

    def test_prediction_uses_asset_category(self):
        pred = InferencePrediction(
            species_code="paisto1",
            common_name="Painted Stork",
            folder_slug="Painted_Stork",
            scientific_name="Mycteria leucocephala",
            confidence=0.98,
            category=AssetCategory.BIRD,
        )
        self.assertIsInstance(pred.category, AssetCategory)
        self.assertEqual(pred.category, AssetCategory.BIRD)
        self.assertEqual(pred.to_dict()["category"], "Bird")


if __name__ == "__main__":
    unittest.main()
