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
import time
import warnings

# Suppress noisy logs & warnings
warnings.filterwarnings("ignore")
os.environ["HF_HUB_DISABLE_SYMLINKS_WARNING"] = "1"
os.environ["TRANSFORMERS_VERBOSITY"] = "error"
logging.getLogger("huggingface_hub").setLevel(logging.ERROR)
logging.getLogger("transformers").setLevel(logging.ERROR)

from PIL import Image, ImageOps
import open_clip
from safetensors.torch import load_file
import torch
from ultralytics import YOLO

COCO_BIRD_CLASS_ID = 14


class AvianInferenceEngine:
    def __init__(self, models_dir: str):
        self.models_dir = models_dir
        self.device = "mps" if torch.backends.mps.is_available() else "cpu"

        # 1. Load Taxonomy
        tax_path = os.path.join(models_dir, "taxonomy.json")
        with open(tax_path, "r", encoding="utf-8") as f:
            self.taxonomy = json.load(f)

        # 2. Load Precomputed Embeddings [N, 512]
        emb_path = os.path.join(models_dir, "species_embeddings.safetensors")
        emb_data = load_file(emb_path)
        self.species_embeddings = emb_data["embeddings"].to(self.device)  # [11167, 512]

        # 3. Load Regional Checklists & Spatio-Temporal Prior Tensors
        self.load_regional_checklists()
        self.build_regional_priors()

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
            os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "data", "regional_checklists.json"),
            os.path.join("data", "regional_checklists.json"),
            os.path.join(self.models_dir, "regional_checklists.json"),
        ]
        for path in candidates:
            if os.path.exists(path):
                try:
                    with open(path, "r", encoding="utf-8") as f:
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
        weights = [
            1.0 if sp.get("taxon_id") in checklist else 0.001
            for sp in self.taxonomy
        ]
        tensor = torch.tensor(weights, dtype=torch.float32, device=self.device)
        self.regional_prior_tensors[region_code] = tensor
        return tensor

    def build_regional_priors(self):
        """Constructs prior tensors across all 11,167 species for fast vector multiplication."""
        num_sp = len(self.taxonomy)
        ladakh_weights = [1.0] * num_sp

        lowland_birds = [
            "painted stork", "spot-billed pelican", "little cormorant",
            "darter", "oriental darter", "eurasian spoonbill", "asian openbill",
            "woolly-necked stork", "lesser whistling-duck", "cotton pygmy-goose",
            "jacana", "moorhen", "swamphen", "white-breasted waterhen",
            "parakeet", "drongo", "hornbill", "sunbird", "flowerpecker", "barbet"
        ]

        himalayan_ladakh_birds = [
            "snowcock", "partridge", "chukar", "redstart", "accentor", "snowfinch",
            "rosefinch", "mountain finch", "chough", "bar-headed goose", "brown-headed gull",
            "common merganser", "black-necked crane", "citrine wagtail", "white wagtail",
            "horned lark", "desert lark", "crag martin", "rubythroat", "sandpiper",
            "bearded vulture", "lammergeier", "himalayan griffon", "golden eagle"
        ]

        for i, sp in enumerate(self.taxonomy):
            c_name = sp["common_name"].lower()
            if any(lb in c_name for lb in lowland_birds):
                ladakh_weights[i] = 0.001
            elif any(hb in c_name for hb in himalayan_ladakh_birds):
                ladakh_weights[i] = 2.0

        self.ladakh_prior = torch.tensor(ladakh_weights, dtype=torch.float32, device=self.device)

    def detect_and_crop(self, img: Image.Image, min_detect: float = 0.25):
        w, h = img.size
        results = self.detector(img, verbose=False, imgsz=640)
        best_candidate = None
        best_score = -1.0
        all_detections = []

        for r in results:
            for box in r.boxes:
                cls_id = int(box.cls[0].item())
                conf = float(box.conf[0].item())
                xyxy = [float(v) for v in box.xyxy[0].tolist()]

                if cls_id == COCO_BIRD_CLASS_ID:
                    all_detections.append({
                        "class_id": cls_id,
                        "confidence": conf,
                        "bbox_xyxy": xyxy,
                    })
                    if conf >= min_detect:
                        bx1, by1, bx2, by2 = xyxy
                        area = max(0.0, bx2 - bx1) * max(0.0, by2 - by1)
                        score = conf * area
                        if score > best_score:
                            best_score = score
                            best_candidate = (conf, xyxy)

        if best_candidate is None:
            return None, 0.0, all_detections

        conf, (x1, y1, x2, y2) = best_candidate
        bw = x2 - x1
        bh = y2 - y1
        pad_x = bw * 0.15
        pad_y = bh * 0.15

        cx1 = max(0.0, x1 - pad_x)
        cy1 = max(0.0, y1 - pad_y)
        cx2 = min(float(w), x2 + pad_x)
        cy2 = min(float(h), y2 + pad_y)

        crop = img.crop((int(cx1), int(cy1), int(cx2), int(cy2)))
        norm_bbox = [cx1 / w, cy1 / h, cx2 / w, cy2 / h]
        return crop, conf, all_detections

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
            is_ladakh = (
                region_code == "IN-LA"
                or "leh" in trip_name.lower()
                or "ladakh" in trip_name.lower()
                or (lat is not None and lat > 32.0 and lon is not None and lon > 76.0)
            )

            if geo_prior_weight > 0.0:
                priors = torch.ones_like(visual_probs)
                if regional_prior is not None:
                    priors = priors * regional_prior
                if is_ladakh:
                    priors = priors * self.ladakh_prior

                effective_priors = priors ** geo_prior_weight
                posterior_scores = visual_probs * effective_priors
            else:
                effective_priors = torch.ones_like(visual_probs)
                posterior_scores = visual_probs

            # Select Top 5 species based on fused posterior scores
            top_posteriors, top_indices = torch.topk(posterior_scores, k=5)

        top_predictions = []
        raw_scores = []

        for post_val, idx_val in zip(top_posteriors, top_indices):
            idx = int(idx_val.item())
            post_score = float(post_val.item())
            vis_prob = float(visual_probs[idx].item())
            geo_pr = float(effective_priors[idx].item())
            sp = self.taxonomy[idx]

            raw_scores.append(post_score)
            top_predictions.append({
                "taxon_id": sp["taxon_id"],
                "common_name": sp["common_name"],
                "folder_slug": sp["folder_slug"],
                "scientific_name": sp["scientific_name"],
                "visual_prob": round(vis_prob, 4),
                "geo_prior": round(geo_pr, 4),
                "combined_score": round(post_score, 4),
            })

        # Normalize top-5 posterior distribution
        score_sum = sum(raw_scores)
        if score_sum > 0:
            for i, p in enumerate(top_predictions):
                p["combined_score"] = round(raw_scores[i] / score_sum, 4)

        top1 = top_predictions[0] if top_predictions else None
        final_conf = top1["combined_score"] if top1 and not is_fallback_crop else det_conf

        return {
            "image_path": image_path,
            "detected": not is_fallback_crop and det_conf >= min_detect,
            "detection_confidence": round(det_conf, 4),
            "assigned_species_slug": top1["folder_slug"] if top1 else "Unknown",
            "frame_confidence": top1["combined_score"] if top1 else 0.0,
            "final_confidence": round(final_conf, 4),
            "top_predictions": top_predictions,
            "embedding": [round(float(v), 5) for v in img_emb[0].tolist()[:512]],
        }


def main():
    parser = argparse.ArgumentParser(description="Avicull Local Inference Engine")
    parser.add_argument("--models-dir", default="models", help="Directory with models & taxonomy")
    parser.add_argument("--batch-file", help="JSON file containing array of photo bundle tasks")
    parser.add_argument("--output-file", help="File to write JSON results to (avoids stdout noise)")
    parser.add_argument("--image", help="Single image path for testing")
    parser.add_argument("--trip-name", default="", help="Trip folder name for gazetteer prior")
    parser.add_argument("--region-code", default="", help="eBird region code (e.g. IN-KA) for regional prior")
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
        with open(args.batch_file, "r") as f:
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
