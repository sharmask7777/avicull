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
import math
import os
import sys
import warnings
from collections.abc import Sequence
from dataclasses import dataclass
from enum import Enum
from typing import Any, NamedTuple

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

COCO_PERSON_CLASS_ID = 0
COCO_BIRD_CLASS_ID = 14
COCO_MAMMAL_CLASS_IDS: frozenset[int] = frozenset({15, 16, 17, 18, 19, 20, 21, 22, 23})
COCO_ANIMAL_CLASS_IDS: frozenset[int] = frozenset(
    {
        0,  # person (portraits)
        14,  # bird
        15,  # cat
        16,  # dog
        17,  # horse
        18,  # sheep
        19,  # cow
        20,  # elephant
        21,  # bear
        22,  # zebra
        23,  # giraffe
    }
)


class AssetCategory(str, Enum):
    """High-level taxonomic domain or photographic genre category."""

    BIRD = "Bird"
    MAMMALIA = "Mammalia"
    INSECTA = "Insecta"
    HERPETOFAUNA = "Herpetofauna"
    REPTILIA = "Reptilia"
    AMPHIBIA = "Amphibia"
    SCENERY = "Scenery"
    PORTRAIT = "Portrait"
    ARCHITECTURE = "Architecture"
    ABSTRACT = "Abstract"
    NO_SUBJECT_DETECTED = "No_Subject_Detected"
    UNKNOWN = "Unknown"


CANONICAL_CATEGORIES: tuple[str, ...] = tuple(c.value for c in AssetCategory)


class CategoryAnchorResult(NamedTuple):
    """Immutable result from Stage 1 category anchor gating."""

    category: AssetCategory
    confidence: float
    scores: dict[str, float]
    is_ambiguous: bool = False


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
    """Immutable taxon classification prediction record."""

    species_code: str
    common_name: str
    scientific_name: str
    confidence: float
    category: AssetCategory | str = AssetCategory.BIRD
    embedding: list[float] | None = None
    raw_logit: float = 0.0
    regional_prior_applied: bool = False
    bbox: DetectionBox | None = None
    folder_slug: str = ""
    visual_prob: float = 0.0
    geo_prior: float = 1.0

    def __init__(
        self,
        *args: Any,
        species_code: str = "",
        common_name: str = "",
        scientific_name: str = "",
        confidence: float = 0.0,
        category: AssetCategory | str = AssetCategory.BIRD,
        embedding: list[float] | None = None,
        raw_logit: float = 0.0,
        regional_prior_applied: bool = False,
        bbox: DetectionBox | None = None,
        folder_slug: str = "",
        visual_prob: float = 0.0,
        geo_prior: float = 1.0,
    ) -> None:
        if args:
            if len(args) >= 1 and args[0] in CANONICAL_CATEGORIES:
                cat = args[0]
                sp_code = str(args[1]) if len(args) > 1 else species_code
                c_name = str(args[2]) if len(args) > 2 else common_name
                sc_name = str(args[3]) if len(args) > 3 else scientific_name
                conf = float(args[4]) if len(args) > 4 else confidence
                emb = args[5] if len(args) > 5 else embedding
            else:
                sp_code = str(args[0])
                c_name = str(args[1]) if len(args) > 1 else common_name
                sc_name = str(args[2]) if len(args) > 2 else scientific_name
                conf = float(args[3]) if len(args) > 3 else confidence
                cat = args[4] if len(args) > 4 else category
                emb = args[5] if len(args) > 5 else embedding
        else:
            sp_code = species_code
            c_name = common_name
            sc_name = scientific_name
            conf = confidence
            cat = category
            emb = embedding

        cat_val = (
            cat
            if isinstance(cat, AssetCategory)
            else (
                AssetCategory(cat)
                if isinstance(cat, str) and cat in AssetCategory._value2member_map_
                else AssetCategory.UNKNOWN
            )
        )

        object.__setattr__(self, "species_code", sp_code)
        object.__setattr__(self, "common_name", c_name)
        object.__setattr__(self, "scientific_name", sc_name)
        object.__setattr__(self, "confidence", conf)
        object.__setattr__(self, "category", cat_val)
        object.__setattr__(self, "embedding", emb)
        object.__setattr__(self, "raw_logit", raw_logit)
        object.__setattr__(self, "regional_prior_applied", regional_prior_applied)
        object.__setattr__(self, "bbox", bbox)
        object.__setattr__(self, "folder_slug", folder_slug)
        object.__setattr__(self, "visual_prob", visual_prob)
        object.__setattr__(self, "geo_prior", geo_prior)

    def to_dict(self) -> dict[str, Any]:
        """Exports prediction record to serialized dictionary structure expected by avicull."""
        slug = self.folder_slug or self.common_name.replace(" ", "_")
        cat_str = (
            self.category.value if isinstance(self.category, AssetCategory) else str(self.category)
        )
        d: dict[str, Any] = {
            "taxon_id": self.species_code,
            "category": cat_str,
            "common_name": self.common_name,
            "folder_slug": slug,
            "scientific_name": self.scientific_name,
            "visual_prob": round(self.visual_prob, 4),
            "geo_prior": round(self.geo_prior, 4),
            "combined_score": round(self.confidence, 4),
        }
        if self.embedding is not None:
            d["embedding"] = self.embedding
        return d


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


def calculate_visual_entropy(img: Image.Image) -> float:
    """Computes the Shannon entropy of grayscale pixel distribution.

    Uniform, pure black, or synthetic solid frames return entropy ~ 0.0,
    whereas natural photographic scenes exhibit entropy > 2.0.
    """
    gray = img.convert("L")
    hist = gray.histogram()
    total = sum(hist)
    if total == 0:
        return 0.0
    probs = [c / total for c in hist if c > 0]
    return float(-sum(p * math.log2(p) for p in probs))


def compute_category_centroids(
    embeddings: torch.Tensor | np.ndarray,
    taxonomy: Sequence[dict[str, Any] | Any],
) -> dict[str, torch.Tensor]:
    """Computes normalized category centroid vectors for multi-taxa hierarchical gating.

    Args:
        embeddings: Precomputed normalized species embeddings matrix [N, 512].
        taxonomy: Sequence of taxonomy records containing category field.

    Returns:
        Dictionary mapping canonical category names ('Bird', 'Mammalia', 'Insecta', 'Scenery')
        to their 512-dimensional unit-norm centroid vector.
    """
    if not isinstance(embeddings, torch.Tensor):
        emb_tensor = torch.tensor(embeddings, dtype=torch.float32)
    else:
        emb_tensor = embeddings.float()

    centroids: dict[str, torch.Tensor] = {}
    cat_indices: dict[str, list[int]] = {}

    for idx, item in enumerate(taxonomy):
        if isinstance(item, dict):
            raw_cat = item.get("category", "Bird")
        else:
            raw_cat = getattr(item, "category", "Bird")
        cat = raw_cat if raw_cat in CANONICAL_CATEGORIES else "Bird"
        cat_indices.setdefault(cat, []).append(idx)

    for cat, indices in cat_indices.items():
        if indices:
            sub_emb = emb_tensor[indices]
            mean_vec = sub_emb.mean(dim=0)
            norm = mean_vec.norm(dim=-1, keepdim=True)
            if float(norm.item()) > 1e-6:
                centroids[cat] = mean_vec / norm
            else:
                centroids[cat] = mean_vec

    return centroids


def classify_category_anchors(
    embedding: torch.Tensor | np.ndarray,
    category_centroids: dict[str, torch.Tensor | np.ndarray],
    margin: float = 0.02,
) -> CategoryAnchorResult:
    """Stage 1 category gating: classifies visual embedding against category centroids.

    Evaluates cosine similarity against category centroids (Bird, Mammalia, Insecta, Scenery).
    Applies margin delta: if |s_top - s_second| < margin, flags result as ambiguous.
    When ambiguous and 'Bird' is among the top contenders, resolves to 'Bird' rather than
    blindly defaulting to non-bird categories.

    Args:
        embedding: 512-dimensional visual embedding vector.
        category_centroids: Dictionary of category centroid vectors.
        margin: Decision margin delta (default: 0.02).

    Returns:
        CategoryAnchorResult containing winning category, confidence, all scores,
        and is_ambiguous flag.
    """
    if not category_centroids:
        return CategoryAnchorResult(
            category=AssetCategory.BIRD, confidence=1.0, scores={}, is_ambiguous=False
        )

    if not isinstance(embedding, torch.Tensor):
        z = torch.tensor(embedding, dtype=torch.float32)
    else:
        z = embedding.detach().float()

    z = z.flatten()
    z_norm = z.norm()
    if float(z_norm.item()) > 1e-6:
        z = z / z_norm

    scores: dict[str, float] = {}
    for cat, centroid in category_centroids.items():
        if not isinstance(centroid, torch.Tensor):
            c = torch.tensor(centroid, dtype=torch.float32, device=z.device)
        else:
            c = centroid.detach().float().to(z.device).flatten()

        c_norm = c.norm()
        if float(c_norm.item()) > 1e-6:
            c = c / c_norm
        score = float(torch.dot(z, c).item())
        scores[cat] = score

    sorted_cats = sorted(scores.items(), key=lambda item: item[1], reverse=True)
    top_cat, top_score = sorted_cats[0]

    is_ambiguous = False
    if len(sorted_cats) > 1:
        _second_cat, second_score = sorted_cats[1]
        diff = top_score - second_score
        if diff < margin:
            is_ambiguous = True
            # When ambiguous, do not blindly default away from Bird if Bird is close to the top
            bird_score = scores.get("Bird")
            if bird_score is not None and (top_score - bird_score) < margin:
                top_cat = "Bird"
                top_score = bird_score

    cat_enum = (
        AssetCategory(top_cat)
        if top_cat in AssetCategory._value2member_map_
        else AssetCategory.UNKNOWN
    )
    return CategoryAnchorResult(
        category=cat_enum,
        confidence=top_score,
        scores=scores,
        is_ambiguous=is_ambiguous,
    )


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

        # 2b. Compute Category Centroid Vectors E_cat
        self.category_centroids = compute_category_centroids(self.species_embeddings, self.taxonomy)

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

    def load_regional_checklists(self) -> None:
        """Loads eBird regional checklist species sets from data/regional_checklists.json."""
        self.regional_checklists: dict[str, set[str]] = {}
        self.regional_prior_tensors: dict[str, torch.Tensor] = {}
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

    def get_regional_prior_tensor(self, region_code: str) -> torch.Tensor | None:
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

    def detect_and_crop(
        self, img: Image.Image, min_detect: float = 0.25, min_area_ratio: float = 0.001
    ) -> tuple[Image.Image | None, float, list[dict[str, Any]]]:
        w, h = img.size
        img_area = float(w * h)
        results = self.detector(img, verbose=False, imgsz=640)
        best_candidate: tuple[float, DetectionBox] | None = None
        best_score = -1.0
        all_detections: list[DetectionBox] = []

        for r in results:
            for box in r.boxes:
                cls_id = int(box.cls[0].item())
                conf = float(box.conf[0].item())
                xyxy = [float(v) for v in box.xyxy[0].tolist()]

                if cls_id in COCO_ANIMAL_CLASS_IDS:
                    det_box = DetectionBox(
                        x1=xyxy[0],
                        y1=xyxy[1],
                        x2=xyxy[2],
                        y2=xyxy[3],
                        confidence=conf,
                        class_id=cls_id,
                    )
                    all_detections.append(det_box)
                    area_ratio = det_box.area / img_area if img_area > 0 else 0.0
                    if conf >= min_detect and area_ratio >= min_area_ratio:
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
        lat: float | None = None,
        lon: float | None = None,
        region_code: str = "",
    ) -> dict[str, Any]:
        if not os.path.exists(image_path):
            return {
                "error": f"File not found: {image_path}",
                "category": AssetCategory.UNKNOWN.value,
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
                "category": AssetCategory.UNKNOWN.value,
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
            # Detector fallback: 0 animal/bird objects detected by YOLO
            # Embed full frame to distinguish scenery from empty/blurry frames
            full_tensor = self.preprocess(img).unsqueeze(0).to(self.device)
            with torch.no_grad():
                full_emb = self.vision_model(full_tensor)
                full_emb = full_emb / full_emb.norm(dim=-1, keepdim=True)

            anchor_res = classify_category_anchors(
                full_emb[0], self.category_centroids, margin=0.03
            )

            # Evaluate natural visual entropy to reject pure black/synthetic uniform images
            visual_entropy = calculate_visual_entropy(img)

            # Robust non-animal genre fallback:
            # Require higher threshold and peak confidence so unlabeled frames or low-confidence
            # noise do not falsely classify into other genres. Fall back to No Subject.
            non_animal_genres = ("Scenery", "Architecture", "Abstract")
            with torch.no_grad():
                all_sims = (full_emb @ self.species_embeddings.T)[0]

            # Evaluate the top candidate scores and peak score per non-animal genre
            genre_top_scores: dict[str, float] = {}
            genre_peak_scores: dict[str, float] = {}
            for g in non_animal_genres:
                g_indices = [i for i, sp in enumerate(self.taxonomy) if sp.get("category") == g]
                if g_indices:
                    k_top = min(2, len(g_indices))
                    top_vals = torch.topk(all_sims[g_indices], k_top).values
                    genre_top_scores[g] = float(top_vals.mean().item())
                    genre_peak_scores[g] = float(top_vals[0].item())

            best_genre = (
                max(genre_top_scores.items(), key=lambda x: x[1])[0]
                if genre_top_scores
                else anchor_res.category
            )
            best_genre_score = genre_top_scores.get(best_genre, 0.0)
            best_genre_peak = genre_peak_scores.get(best_genre, 0.0)

            # Robust classification gate:
            # - For Scenery (e.g. mountain valleys, waterfalls): top-2 mean >= 0.16, peak >= 0.18
            # - For Architecture, Abstract: top-2 mean >= 0.18 and peak >= 0.22.
            # Frames below these gates cleanly fall back to _No_Subject_Detected.
            min_mean_thresh = 0.16 if best_genre == "Scenery" else 0.18
            min_peak_thresh = 0.18 if best_genre == "Scenery" else 0.22

            is_valid_genre = (
                best_genre in non_animal_genres
                and visual_entropy >= 1.0
                and best_genre_score >= min_mean_thresh
                and best_genre_peak >= min_peak_thresh
            )

            if not is_valid_genre:
                # Ambiguous or no confident subject -> fallback to _No_Subject_Detected
                return {
                    "image_path": image_path,
                    "category": AssetCategory.NO_SUBJECT_DETECTED.value,
                    "detected": False,
                    "detection_confidence": 0.0,
                    "assigned_species_slug": "_No_Subject_Detected",
                    "frame_confidence": 0.0,
                    "final_confidence": 0.0,
                    "top_predictions": [],
                    "embedding": [round(float(v), 5) for v in full_emb[0].tolist()[:512]],
                }

            # Confirmed natural genre (Scenery, Architecture, or Abstract)
            crop = img
            det_conf = best_genre_score
            img_emb = full_emb
            is_fallback_crop = False
            best_cat_name = (
                best_genre.value if isinstance(best_genre, AssetCategory) else str(best_genre)
            )
            target_category = (
                AssetCategory(best_cat_name)
                if best_cat_name in AssetCategory._value2member_map_
                else AssetCategory(anchor_res.category)
            )
        else:
            crop_tensor = self.preprocess(crop).unsqueeze(0).to(self.device)
            with torch.no_grad():
                img_emb = self.vision_model(crop_tensor)
                img_emb = img_emb / img_emb.norm(dim=-1, keepdim=True)

            # Detect whether YOLO specifically localized a person, mammal or bird
            detected_person = any(det.get("class_id") == COCO_PERSON_CLASS_ID for det in detections)
            detected_mammal = any(
                det.get("class_id") in COCO_MAMMAL_CLASS_IDS for det in detections
            )
            detected_bird = any(det.get("class_id") == COCO_BIRD_CLASS_ID for det in detections)

            # Avoid centroid dilution: evaluate top candidates directly across all species
            with torch.no_grad():
                all_sims = (img_emb @ self.species_embeddings.T)[0]
                top_sp_idx = int(torch.argmax(all_sims).item())
                top_sp_cat = self.taxonomy[top_sp_idx].get("category", "Bird")
                if top_sp_cat not in CANONICAL_CATEGORIES:
                    top_sp_cat = "Bird"

            bird_indices = [
                i for i, sp in enumerate(self.taxonomy) if sp.get("category", "Bird") == "Bird"
            ]
            mammal_indices = [
                i for i, sp in enumerate(self.taxonomy) if sp.get("category", "Bird") == "Mammalia"
            ]
            non_bird_indices = [
                i for i, sp in enumerate(self.taxonomy) if sp.get("category", "Bird") != "Bird"
            ]

            max_bird_sim = float(all_sims[bird_indices].max().item()) if bird_indices else -1.0
            max_mammal_sim = (
                float(all_sims[mammal_indices].max().item()) if mammal_indices else -1.0
            )
            max_non_bird_sim = (
                float(all_sims[non_bird_indices].max().item()) if non_bird_indices else -1.0
            )

            if detected_person and not detected_bird and not detected_mammal:
                target_category = AssetCategory.PORTRAIT
            elif detected_mammal and not detected_bird:
                # If YOLO detected a mammal with high confidence and no bird, route
                # target_category = "Mammalia" unless a bird candidate has overwhelming
                # visual similarity margin exceeding the top mammal prediction by >= 0.15.
                target_category = (
                    AssetCategory.BIRD
                    if max_bird_sim - max_mammal_sim >= 0.15
                    else AssetCategory.MAMMALIA
                )
            elif detected_bird and not detected_mammal:
                # Default target_category = "Bird" unless non-bird has margin >= 0.05.
                top_cat_enum = (
                    AssetCategory(top_sp_cat)
                    if top_sp_cat in AssetCategory._value2member_map_
                    else AssetCategory.BIRD
                )
                target_category = (
                    top_cat_enum if max_non_bird_sim - max_bird_sim >= 0.05 else AssetCategory.BIRD
                )
            elif detected_bird and detected_mammal:
                top_cat_enum = (
                    AssetCategory(top_sp_cat)
                    if top_sp_cat in AssetCategory._value2member_map_
                    else AssetCategory.BIRD
                )
                target_category = (
                    top_cat_enum if max_non_bird_sim - max_bird_sim >= 0.05 else AssetCategory.BIRD
                )
            else:
                # If YOLO detected an animal (cat, dog, etc.) or open-set entity:
                # Never blindly default to Bird. If top candidate is not Bird, honor it.
                # If ambiguous, use category centroids.
                if any(det.get("class_id") in COCO_MAMMAL_CLASS_IDS for det in detections):
                    target_category = AssetCategory.MAMMALIA
                elif any(det.get("class_id") == COCO_BIRD_CLASS_ID for det in detections):
                    target_category = AssetCategory.BIRD
                elif top_sp_cat != "Bird" and top_sp_cat in AssetCategory._value2member_map_:
                    target_category = AssetCategory(top_sp_cat)
                else:
                    anchor_res = classify_category_anchors(
                        img_emb[0], self.category_centroids, margin=0.02
                    )
                    target_category = (
                        anchor_res.category
                        if not anchor_res.is_ambiguous
                        else AssetCategory.NO_SUBJECT_DETECTED
                    )

            is_fallback_crop = False

        # Stage 2: Fine-grained ranking within target_category
        target_cat_str = (
            target_category.value
            if isinstance(target_category, AssetCategory)
            else str(target_category)
        )
        cat_indices = [
            i for i, sp in enumerate(self.taxonomy) if sp.get("category", "Bird") == target_cat_str
        ]

        if not cat_indices:
            cat_indices = list(range(len(self.taxonomy)))

        with torch.no_grad():
            category_embeddings = self.species_embeddings[cat_indices]
            # Cosine similarity logits with temperature scaling 100.0
            logits = 100.0 * (img_emb @ category_embeddings.T)
            visual_probs = torch.softmax(logits, dim=-1)[0]

            # Apply Bayesian Spatio-Temporal Prior (for Bird category only)
            regional_prior = (
                self.get_regional_prior_tensor(region_code)
                if (region_code and target_category == "Bird")
                else None
            )

            if geo_prior_weight > 0.0 and regional_prior is not None:
                priors = regional_prior[cat_indices]
                effective_priors = priors**geo_prior_weight
                posterior_scores = visual_probs * effective_priors
            else:
                effective_priors = torch.ones_like(visual_probs)
                posterior_scores = visual_probs

            # Select Top species based on fused posterior scores (capped at category species count)
            k = min(5, len(cat_indices))
            top_posteriors, top_sub_indices = torch.topk(posterior_scores, k=k)

        predictions: list[InferencePrediction] = []
        raw_scores = []

        for post_val, sub_idx_val in zip(top_posteriors, top_sub_indices, strict=False):
            sub_idx = int(sub_idx_val.item())
            global_idx = cat_indices[sub_idx]
            post_score = float(post_val.item())
            vis_prob = float(visual_probs[sub_idx].item())
            geo_pr = float(effective_priors[sub_idx].item())
            sp = self.taxonomy[global_idx]

            raw_val = (
                float(logits[sub_idx].item())
                if logits.ndim == 1
                else float(logits[0, sub_idx].item())
            )
            raw_scores.append(post_score)
            predictions.append(
                InferencePrediction(
                    species_code=sp.get("taxon_id", ""),
                    common_name=sp.get("common_name", ""),
                    scientific_name=sp.get("scientific_name", ""),
                    confidence=post_score,
                    category=sp.get("category", target_category),
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
                        category=p.category,
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
            "category": target_cat_str,
            "detected": not is_fallback_crop and det_conf >= min_detect,
            "detection_confidence": round(det_conf, 4),
            "assigned_species_slug": top1_slug,
            "frame_confidence": round(top1_conf, 4),
            "final_confidence": round(final_conf, 4),
            "top_predictions": top_predictions_dict,
            "embedding": [round(float(v), 5) for v in img_emb[0].tolist()[:512]],
        }


def main() -> None:
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
