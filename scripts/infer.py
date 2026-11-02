#!/usr/bin/env python3
"""
High-Performance Local Avian Inference Engine for avicull.
Executes:
1. Ultralytics YOLO11 bird localization & context-padded cropping
2. BioCLIP visual embedding extraction on Apple Silicon MPS
3. Fast matrix cosine similarity across 11,167 precomputed avian species embeddings
4. Bayesian spatio-temporal prior weighting across full species space and top-k ranking
"""

import argparse
import json
import logging
import os
import sys
import warnings
from collections.abc import Sequence
from dataclasses import dataclass
from typing import Any

# Suppress noisy logs & warnings
warnings.filterwarnings("ignore")
os.environ["HF_HUB_DISABLE_SYMLINKS_WARNING"] = "1"
os.environ["TRANSFORMERS_VERBOSITY"] = "error"
logging.getLogger("huggingface_hub").setLevel(logging.ERROR)
logging.getLogger("transformers").setLevel(logging.ERROR)

# ruff: noqa: E402
import numpy as np
import open_clip
import torch
from PIL import Image, ImageOps
from safetensors.torch import load_file
from ultralytics import YOLO

COCO_BIRD_CLASS_ID = 14


@dataclass(frozen=True)
class DetectionBox:
    """Immutable representation of a 2D bounding box and its detector confidence."""

    x1: float
    y1: float
    x2: float
    y2: float
    confidence: float
    class_id: int = 0

    @property
    def area(self) -> float:
        """Computes the 2D bounding box area in squared coordinate units."""
        return max(0.0, self.x2 - self.x1) * max(0.0, self.y2 - self.y1)

    def clip(self, width: float, height: float) -> "DetectionBox":
        """Clamps coordinate boundaries within image dimensions [0, width] and [0, height]."""
        cx1 = max(0.0, min(float(width), self.x1))
        cy1 = max(0.0, min(float(height), self.y1))
        cx2 = max(0.0, min(float(width), self.x2))
        cy2 = max(0.0, min(float(height), self.y2))
        return DetectionBox(
            x1=cx1,
            y1=cy1,
            x2=cx2,
            y2=cy2,
            confidence=self.confidence,
            class_id=self.class_id,
        )

    def to_xyxy(self) -> tuple[float, float, float, float]:
        """Returns coordinate quad tuple (x1, y1, x2, y2)."""
        return (self.x1, self.y1, self.x2, self.y2)

    def to_dict(self) -> dict[str, Any]:
        """Serializes box for JSON serialization and legacy dictionary compatibility."""
        return {
            "class_id": self.class_id,
            "confidence": self.confidence,
            "bbox_xyxy": [self.x1, self.y1, self.x2, self.y2],
        }


@dataclass(frozen=True)
class InferencePrediction:
    """Immutable avian classification prediction record."""

    species_code: str
    common_name: str
    scientific_name: str
    confidence: float
    raw_logit: float = 0.0
    regional_prior_applied: bool = False
    bbox: DetectionBox | None = None
    folder_slug: str = ""
    visual_prob: float = 0.0
    geo_prior: float = 1.0

    def to_dict(self) -> dict[str, Any]:
        """Exports prediction record to serialized dictionary structure expected by avicull."""
        slug = self.folder_slug or self.common_name.replace(" ", "_")
        return {
            "taxon_id": self.species_code,
            "common_name": self.common_name,
            "folder_slug": slug,
            "scientific_name": self.scientific_name,
            "visual_prob": round(self.visual_prob, 4),
            "geo_prior": round(self.geo_prior, 4),
            "combined_score": round(self.confidence, 4),
        }


def compute_iou(box_a: DetectionBox, box_b: DetectionBox) -> float:
    """Computes the intersection-over-union metric between two bounding boxes."""
    inter_x1 = max(box_a.x1, box_b.x1)
    inter_y1 = max(box_a.y1, box_b.y1)
    inter_x2 = min(box_a.x2, box_b.x2)
    inter_y2 = min(box_a.y2, box_b.y2)

    inter_w = max(0.0, inter_x2 - inter_x1)
    inter_h = max(0.0, inter_y2 - inter_y1)
    intersection = inter_w * inter_h

    if intersection <= 0.0:
        return 0.0

    area_a = box_a.area
    area_b = box_b.area
    union = area_a + area_b - intersection
    if union <= 0.0:
        return 0.0

    return float(intersection / union)


def softmax(logits: Sequence[float] | np.ndarray, temperature: float = 1.0) -> np.ndarray:
    """Pure mathematical softmax with numerical stability subtraction and temperature scaling."""
    if temperature <= 0.0:
        raise ValueError("Softmax temperature must be strictly positive.")
    arr = np.asarray(logits, dtype=np.float64) / float(temperature)
    shifted = arr - np.max(arr, axis=-1, keepdims=True)
    exp_vals = np.exp(shifted)
    sum_exp = np.sum(exp_vals, axis=-1, keepdims=True)
    return exp_vals / sum_exp


def apply_regional_prior(
    logits: Sequence[float] | np.ndarray,
    species_codes: Sequence[str],
    regional_candidates: set[str],
    boost_factor: float = 2.0,
    penalty_factor: float = 0.1,
) -> np.ndarray:
    """Applies regional prior weighting factors to species logit or probability vectors."""
    scores = np.asarray(logits, dtype=np.float64).copy()
    for idx, code in enumerate(species_codes):
        if code in regional_candidates:
            scores[idx] *= boost_factor
        else:
            scores[idx] *= penalty_factor
    return scores


class AvianInferenceEngine:
    def __init__(self, models_dir: str):
        self.models_dir = models_dir
        self.device = "mps" if torch.backends.mps.is_available() else "cpu"

        # 1. Load Taxonomy
        tax_path = os.path.join(models_dir, "taxonomy.json")
        with open(tax_path, encoding="utf-8") as f:
            self.taxonomy = json.load(f)

        # 2. Load Precomputed Embeddings [N, 512]
        emb_path = os.path.join(models_dir, "species_embeddings.safetensors")
        emb_data = load_file(emb_path)
        self.species_embeddings = emb_data["embeddings"].to(self.device)  # [11167, 512]

        # 3. Load Regional Checklists & Spatio-Temporal Prior Tensors
        self.load_regional_checklists()

        # 4. Load YOLO Detector
        yolo_path = os.path.join(models_dir, "yolo11n.pt")
        if not os.path.exists(yolo_path):
            yolo_path = "yolo11n.pt"
        self.detector = YOLO(yolo_path)

        # 5. Load BioCLIP Vision Model
        model, _, preprocess = open_clip.create_model_and_transforms("hf-hub:imageomics/bioclip")
        self.vision_model = model.visual.to(self.device)
        self.vision_model.eval()
        self.preprocess = preprocess

    def load_regional_checklists(self):
        """Loads eBird regional checklist species sets from data/regional_checklists.json."""
        self.regional_checklists = {}
        self.regional_prior_tensors = {}
        candidates = [
            os.path.join(
                os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
                "data",
                "regional_checklists.json",
            ),
            os.path.join("data", "regional_checklists.json"),
            os.path.join(self.models_dir, "regional_checklists.json"),
        ]
        for path in candidates:
            if os.path.exists(path):
                try:
                    with open(path, encoding="utf-8") as f:
                        data = json.load(f)
                        self.regional_checklists = {k: set(v) for k, v in data.items()}
                        break
                except Exception as e:
                    logging.warning(f"Failed to load regional checklists from {path}: {e}")

    def get_regional_prior_tensor(self, region_code: str):
        """Returns or constructs a 11,167-dim prior tensor for the given eBird region code."""
        if not region_code or region_code not in self.regional_checklists:
            return None
        if region_code in self.regional_prior_tensors:
            return self.regional_prior_tensors[region_code]

        checklist = self.regional_checklists[region_code]
        weights = [1.0 if sp.get("taxon_id") in checklist else 0.001 for sp in self.taxonomy]
        tensor = torch.tensor(weights, dtype=torch.float32, device=self.device)
        self.regional_prior_tensors[region_code] = tensor
        return tensor

    def detect_and_crop(self, img: Image.Image, min_detect: float = 0.25):
        w, h = img.size
        results = self.detector(img, verbose=False, imgsz=640)
        best_candidate: tuple[float, DetectionBox] | None = None
        best_score = -1.0
        all_detections: list[DetectionBox] = []

        for r in results:
            for box in r.boxes:
                cls_id = int(box.cls[0].item())
                conf = float(box.conf[0].item())
                xyxy = [float(v) for v in box.xyxy[0].tolist()]

                if cls_id == COCO_BIRD_CLASS_ID:
                    det_box = DetectionBox(
                        x1=xyxy[0],
                        y1=xyxy[1],
                        x2=xyxy[2],
                        y2=xyxy[3],
                        confidence=conf,
                        class_id=cls_id,
                    )
                    all_detections.append(det_box)
                    if conf >= min_detect:
                        score = conf * det_box.area
                        if score > best_score:
                            best_score = score
                            best_candidate = (conf, det_box)

        dict_detections = [d.to_dict() for d in all_detections]

        if best_candidate is None:
            return None, 0.0, dict_detections

        conf, box = best_candidate
        bw = box.x2 - box.x1
        bh = box.y2 - box.y1
        pad_x = bw * 0.15
        pad_y = bh * 0.15

        padded_box = DetectionBox(
            x1=box.x1 - pad_x,
            y1=box.y1 - pad_y,
            x2=box.x2 + pad_x,
            y2=box.y2 + pad_y,
            confidence=conf,
            class_id=box.class_id,
        ).clip(width=float(w), height=float(h))

        crop = img.crop(
            (
                int(padded_box.x1),
                int(padded_box.y1),
                int(padded_box.x2),
                int(padded_box.y2),
            )
        )
        return crop, conf, dict_detections

    def process_image(
        self,
        image_path: str,
        min_detect: float = 0.25,
        geo_prior_weight: float = 1.0,
        trip_name: str = "",
        lat: float = None,
        lon: float = None,
        region_code: str = "",
    ):
        if not os.path.exists(image_path):
            return {
                "error": f"File not found: {image_path}",
                "detected": False,
                "detection_confidence": 0.0,
                "assigned_species_slug": "Unknown",
                "final_confidence": 0.0,
                "top_predictions": [],
                "embedding": [0.0] * 512,
            }

        try:
            # Handle EXIF orientation properly
            raw_img = Image.open(image_path)
            img = ImageOps.exif_transpose(raw_img).convert("RGB")
            # Downsample huge full-frame camera RAWs/JPEGs to 1600 max dim for fast localization
            max_dim = max(img.size)
            if max_dim > 1600:
                img.thumbnail((1600, 1600), Image.Resampling.BILINEAR)
        except Exception as e:
            return {
                "error": f"Failed to open image: {e}",
                "detected": False,
                "detection_confidence": 0.0,
                "assigned_species_slug": "Unknown",
                "final_confidence": 0.0,
                "top_predictions": [],
                "embedding": [0.0] * 512,
            }

        crop, det_conf, detections = self.detect_and_crop(img, min_detect=min_detect)

        is_fallback_crop = False
        if crop is None:
            crop = img
            det_conf = 0.10
            is_fallback_crop = True

        crop_tensor = self.preprocess(crop).unsqueeze(0).to(self.device)
        with torch.no_grad():
            img_emb = self.vision_model(crop_tensor)
            img_emb = img_emb / img_emb.norm(dim=-1, keepdim=True)
            # Cosine similarity logits with temperature scaling 100.0
            logits = 100.0 * (img_emb @ self.species_embeddings.T)
            visual_probs = torch.softmax(logits, dim=-1)[0]

            # Apply Bayesian Spatio-Temporal Prior across FULL 11,167 species BEFORE top-k!
            regional_prior = self.get_regional_prior_tensor(region_code) if region_code else None

            if geo_prior_weight > 0.0:
                priors = torch.ones_like(visual_probs)
                if regional_prior is not None:
                    priors = priors * regional_prior

                effective_priors = priors**geo_prior_weight
                posterior_scores = visual_probs * effective_priors
            else:
                effective_priors = torch.ones_like(visual_probs)
                posterior_scores = visual_probs

            # Select Top species based on fused posterior scores (capped at taxonomy size)
            k = min(5, len(self.taxonomy))
            top_posteriors, top_indices = torch.topk(posterior_scores, k=k)

        predictions: list[InferencePrediction] = []
        raw_scores = []

        for post_val, idx_val in zip(top_posteriors, top_indices, strict=False):
            idx = int(idx_val.item())
            post_score = float(post_val.item())
            vis_prob = float(visual_probs[idx].item())
            geo_pr = float(effective_priors[idx].item())
            sp = self.taxonomy[idx]

            raw_val = (
                float(logits[idx].item()) if logits.ndim == 1 else float(logits[0, idx].item())
            )
            raw_scores.append(post_score)
            predictions.append(
                InferencePrediction(
                    species_code=sp.get("taxon_id", ""),
                    common_name=sp.get("common_name", ""),
                    scientific_name=sp.get("scientific_name", ""),
                    confidence=post_score,
                    raw_logit=raw_val,
                    regional_prior_applied=regional_prior is not None,
                    folder_slug=sp.get("folder_slug", ""),
                    visual_prob=vis_prob,
                    geo_prior=geo_pr,
                )
            )

        # Normalize top-5 posterior distribution
        score_sum = sum(raw_scores)
        if score_sum > 0:
            normalized_predictions = []
            for i, p in enumerate(predictions):
                norm_score = raw_scores[i] / score_sum
                normalized_predictions.append(
                    InferencePrediction(
                        species_code=p.species_code,
                        common_name=p.common_name,
                        scientific_name=p.scientific_name,
                        confidence=norm_score,
                        raw_logit=p.raw_logit,
                        regional_prior_applied=p.regional_prior_applied,
                        bbox=p.bbox,
                        folder_slug=p.folder_slug,
                        visual_prob=p.visual_prob,
                        geo_prior=p.geo_prior,
                    )
                )
            predictions = normalized_predictions

        top_predictions_dict = [p.to_dict() for p in predictions]
        top1 = predictions[0] if predictions else None
        top1_slug = top1.folder_slug or top1.common_name.replace(" ", "_") if top1 else "Unknown"
        top1_conf = top1.confidence if top1 else 0.0
        final_conf = top1_conf if top1 and not is_fallback_crop else det_conf

        return {
            "image_path": image_path,
            "detected": not is_fallback_crop and det_conf >= min_detect,
            "detection_confidence": round(det_conf, 4),
            "assigned_species_slug": top1_slug,
            "frame_confidence": round(top1_conf, 4),
            "final_confidence": round(final_conf, 4),
            "top_predictions": top_predictions_dict,
            "embedding": [round(float(v), 5) for v in img_emb[0].tolist()[:512]],
        }


def main():
    parser = argparse.ArgumentParser(description="Avicull Local Inference Engine")
    parser.add_argument("--models-dir", default="models", help="Directory with models & taxonomy")
    parser.add_argument("--batch-file", help="JSON file containing array of photo bundle tasks")
    parser.add_argument("--output-file", help="File to write JSON results to (avoids stdout noise)")
    parser.add_argument("--image", help="Single image path for testing")
    parser.add_argument("--trip-name", default="", help="Trip folder name for gazetteer prior")
    parser.add_argument(
        "--region-code", default="", help="eBird region code (e.g. IN-KA) for regional prior"
    )
    parser.add_argument("--min-detect", type=float, default=0.25)
    parser.add_argument("--geo-prior-weight", type=float, default=1.0)
    args = parser.parse_args()

    engine = AvianInferenceEngine(args.models_dir)

    if args.image:
        result = engine.process_image(
            args.image,
            min_detect=args.min_detect,
            geo_prior_weight=args.geo_prior_weight,
            trip_name=args.trip_name,
            region_code=args.region_code,
        )
        if args.output_file:
            with open(args.output_file, "w", encoding="utf-8") as f:
                json.dump(result, f, indent=2)
        else:
            print(json.dumps(result, indent=2))
        return

    if args.batch_file:
        with open(args.batch_file) as f:
            tasks = json.load(f)

        results = []
        for i, task in enumerate(tasks):
            img_path = task.get("primary_image")
            trip = task.get("trip_folder_name", "")
            lat = task.get("latitude")
            lon = task.get("longitude")
            region_code = task.get("region_code", "")
            res = engine.process_image(
                img_path,
                min_detect=args.min_detect,
                geo_prior_weight=args.geo_prior_weight,
                trip_name=trip,
                lat=lat,
                lon=lon,
                region_code=region_code,
            )
            res["bundle_id"] = task.get("bundle_id")
            results.append(res)
            if (i + 1) % 50 == 0 or (i + 1) == len(tasks):
                print(f"Inference progress: {i + 1}/{len(tasks)} processed...", file=sys.stderr)

        if args.output_file:
            with open(args.output_file, "w", encoding="utf-8") as f:
                json.dump(results, f)
        else:
            print(json.dumps(results))


if __name__ == "__main__":
    main()
