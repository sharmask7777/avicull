import json
import os
import sys
from typing import ClassVar

# Ensure repository root is on sys.path
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

import pytest
import torch
from safetensors.torch import save_file

from scripts.infer import (
    AssetCategory,
    AvianInferenceEngine,
)


def test_guardrail_us_wa_checklist_contains_pileated_woodpecker():
    """Guardrail invariant: US-WA checklist MUST contain native Pileated Woodpecker (pilwoo)

    and other common Pacific Northwest avifauna.
    """
    checklists_path = os.path.join(
        os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
        "data",
        "regional_checklists.json",
    )
    with open(checklists_path, encoding="utf-8") as f:
        checklists = json.load(f)

    assert "US-WA" in checklists, "US-WA must be in regional_checklists.json"
    wa_species = set(checklists["US-WA"])

    # Pileated Woodpecker (pilwoo) is a permanent breeding resident of WA state
    assert "pilwoo" in wa_species, (
        "Pileated Woodpecker (pilwoo) must be in US-WA checklist! "
        "Missing native species causes severe 1000x penalty distortions."
    )
    # Varied Thrush (varthr) is a native Pacific Northwest thrush
    assert "varthr" in wa_species, "Varied Thrush (varthr) must be in US-WA checklist"
    # American Robin (amerob)
    assert "amerob" in wa_species, "American Robin (amerob) must be in US-WA checklist"
    # Great Blue Heron (grbher3)
    assert "grbher3" in wa_species, "Great Blue Heron (grbher3) must be in US-WA checklist"


def test_guardrail_prior_bounded_penalty_ratio(tmp_path, monkeypatch):
    """Guardrail invariant: Regional prior penalty must be bounded.

    An out-of-region species (e.g. captive zoo Toucan or rare vagrant) with strong visual
    evidence (prob >= 0.20) must NOT be suppressed by a 1000x penalty in favor of a
    0.001 noise candidate.
    """
    models_dir = tmp_path / "models"
    models_dir.mkdir(parents=True, exist_ok=True)

    taxonomy = [
        {
            "taxon_id": "brnpel",
            "common_name": "Brown Pelican",
            "folder_slug": "Brown_Pelican",
            "category": "Bird",
        },
        {
            "taxon_id": "toctou1",
            "common_name": "Toco Toucan",
            "folder_slug": "Toco_Toucan",
            "category": "Bird",
        },
    ]
    with open(models_dir / "taxonomy.json", "w", encoding="utf-8") as f:
        json.dump(taxonomy, f)

    # 2 synthetic embeddings
    embs = torch.randn(len(taxonomy), 512, dtype=torch.float32)
    embs /= embs.norm(dim=-1, keepdim=True)
    save_file({"embeddings": embs}, str(models_dir / "species_embeddings.safetensors"))

    # US-WA checklist has Brown Pelican (brnpel), but NOT Toucan (toctou1)
    checklists = {"US-WA": ["brnpel"]}
    with open(models_dir / "regional_checklists.json", "w", encoding="utf-8") as f:
        json.dump(checklists, f)

    class MockModel:
        visual = torch.nn.Identity()

    monkeypatch.setattr(
        "open_clip.create_model_and_transforms", lambda *a, **kw: (MockModel(), None, lambda x: x)
    )
    monkeypatch.setattr("scripts.infer.YOLO", lambda *a, **kw: None)

    engine = AvianInferenceEngine(models_dir=str(models_dir))

    # Check prior tensor: Out-of-region penalty must NOT be as harsh as 0.001 (1000x)
    wa_prior = engine.get_regional_prior_tensor("US-WA")
    assert wa_prior is not None
    pelican_prior = float(wa_prior[0].item())
    toucan_prior = float(wa_prior[1].item())

    assert pelican_prior == pytest.approx(1.0)
    # The penalty ratio should be soft (e.g. >= 0.05 or 0.10, NOT 0.001)
    assert toucan_prior >= 0.02, (
        f"Out-of-region prior penalty too harsh ({toucan_prior}); "
        "must be >= 0.02 to avoid crushing captive/vagrant birds"
    )


def test_guardrail_visual_relevance_floor_rejects_noise_crops(tmp_path, monkeypatch):
    """Guardrail invariant: When a crop's peak cosine similarity against all species

    is below the visual relevance floor (< 0.29), it must NOT be classified as a confirmed bird!
    It must be rejected to No_Subject_Detected or natural scenery genres.
    """
    models_dir = tmp_path / "models"
    models_dir.mkdir(parents=True, exist_ok=True)

    taxonomy = [
        {
            "taxon_id": "goleag",
            "common_name": "Golden Eagle",
            "folder_slug": "Golden_Eagle",
            "category": "Bird",
        },
    ]
    with open(models_dir / "taxonomy.json", "w", encoding="utf-8") as f:
        json.dump(taxonomy, f)

    # 1 synthetic embedding
    embs = torch.randn(len(taxonomy), 512, dtype=torch.float32)
    embs /= embs.norm(dim=-1, keepdim=True)
    save_file({"embeddings": embs}, str(models_dir / "species_embeddings.safetensors"))

    checklists = {"US-WA": ["goleag"]}
    with open(models_dir / "regional_checklists.json", "w", encoding="utf-8") as f:
        json.dump(checklists, f)

    class MockVisual(torch.nn.Module):
        def forward(self, x):
            # Return an embedding that is orthogonal to the species embedding (sim ~ 0.0)
            # This simulates a background noise crop (statue, waterfall, foliage)
            out = torch.randn(x.shape[0], 512, device=x.device)
            # Make orthogonal
            out = out - (out @ embs.to(x.device).T) * embs.to(x.device)
            return out / out.norm(dim=-1, keepdim=True)

    class MockModel:
        visual = MockVisual()

    monkeypatch.setattr(
        "open_clip.create_model_and_transforms",
        lambda *a, **kw: (MockModel(), None, lambda img: torch.zeros(3, 224, 224)),
    )

    class MockBox:
        def __init__(self):
            self.cls = torch.tensor([14.0])  # bird
            self.conf = torch.tensor([0.27])  # weak false positive
            self.xyxy = torch.tensor([[10.0, 10.0, 100.0, 100.0]])

    class MockResult:
        def __init__(self):
            self.boxes = [MockBox()]

    class MockYOLO:
        names: ClassVar[dict[int, str]] = {14: "bird"}

        def __call__(self, img, **kw):
            return [MockResult()]

    monkeypatch.setattr("scripts.infer.YOLO", lambda *a, **kw: MockYOLO())

    engine = AvianInferenceEngine(models_dir=str(models_dir))

    # Create a synthetic image
    from PIL import Image

    img_path = tmp_path / "statue.jpg"
    Image.new("RGB", (200, 200), color=(128, 128, 128)).save(img_path)

    res = engine.process_image(str(img_path), min_detect=0.25, region_code="US-WA")

    # Guardrail Invariant: A noise crop with near-zero similarity to any species
    # must be rejected and NEVER classified as a confirmed bird!
    valid_rejections = (
        AssetCategory.NO_SUBJECT_DETECTED.value,
        AssetCategory.UNKNOWN.value,
        "No_Subject_Detected",
    )
    assert res["category"] in valid_rejections, (
        f"Expected rejection, but got category '{res['category']}' "
        f"and slug '{res['assigned_species_slug']}'"
    )
    assert res["assigned_species_slug"] in ("_No_Subject_Detected", "Unknown")


def test_guardrail_visual_dominance_overrides_prior_for_captive_or_vagrant(tmp_path, monkeypatch):
    """Guardrail invariant: When a bird has overwhelming visual evidence (e.g.

    Toco Toucan in a Seattle zoo or rare vagrant), it must NOT be overridden by
    a local candidate that has near-zero visual probability (e.g. Brown Pelican 0.001).
    """
    models_dir = tmp_path / "models"
    models_dir.mkdir(parents=True, exist_ok=True)

    taxonomy = [
        {
            "taxon_id": "brnpel",
            "common_name": "Brown Pelican",
            "folder_slug": "Brown_Pelican",
            "category": "Bird",
        },
        {
            "taxon_id": "toctou1",
            "common_name": "Toco Toucan",
            "folder_slug": "Toco_Toucan",
            "category": "Bird",
        },
        {
            "taxon_id": "sp3",
            "common_name": "Species 3",
            "folder_slug": "Species_3",
            "category": "Bird",
        },
        {
            "taxon_id": "sp4",
            "common_name": "Species 4",
            "folder_slug": "Species_4",
            "category": "Bird",
        },
        {
            "taxon_id": "sp5",
            "common_name": "Species 5",
            "folder_slug": "Species_5",
            "category": "Bird",
        },
    ]
    with open(models_dir / "taxonomy.json", "w", encoding="utf-8") as f:
        json.dump(taxonomy, f)

    # 5 embeddings: orthogonal basis vectors
    embs = torch.zeros(5, 512, dtype=torch.float32)
    for i in range(5):
        embs[i, i] = 1.0
    save_file({"embeddings": embs}, str(models_dir / "species_embeddings.safetensors"))

    # US-WA has Brown Pelican (index 0), NOT Toucan (index 1) or other 3
    checklists = {"US-WA": ["brnpel"]}
    with open(models_dir / "regional_checklists.json", "w", encoding="utf-8") as f:
        json.dump(checklists, f)

    class MockVisual(torch.nn.Module):
        def forward(self, x):
            # Logits = 100 * sim.
            # We want Toucan (idx 1) logit = 35.0 (vis prob ~ 0.25)
            # and Pelican (idx 0) logit = 32.7 (vis prob ~ 0.024)
            # and sp3, sp4, sp5 logit = 32.7
            # In softmax:
            # exp(35.0 - 32.7) = exp(2.3) = ~10.
            # sum = 10 + 1 + 1 + 1 + 1 = 14.
            # Toucan prob = 10/14 = 71%?
            # Let's adjust so Toucan is ~25%:
            # If Toucan logit = 35.0, other 15 classes logit = 34.0:
            # Here with 5 classes:
            # If Toucan = 0.350, Pelican = 0.336, sp3 = 0.336, sp4 = 0.336, sp5 = 0.336:
            # diff = 0.014 -> 100 * 0.014 = 1.4 -> exp(1.4) = 4.05.
            # sum = 4.05 + 4 = 8.05.
            # Toucan prob = 4.05 / 8.05 = 0.50.
            # Pelican prob = 1 / 8.05 = 0.124.
            # If Toucan gets 0.05 prior: 0.50 * 0.05 = 0.025.
            # Pelican gets 1.0 prior: 0.124 * 1.0 = 0.124.
            # Without visual dominance: Pelican (0.124) WINS over Toucan (0.025) by 5x!
            v = torch.zeros(x.shape[0], 512, device=x.device)
            v[:, 1] = 0.350  # Toucan
            v[:, 0] = 0.336  # Pelican
            v[:, 2] = 0.336
            v[:, 3] = 0.336
            v[:, 4] = 0.336
            return v / v.norm(dim=-1, keepdim=True)

    class MockModel:
        visual = MockVisual()

    monkeypatch.setattr(
        "open_clip.create_model_and_transforms",
        lambda *a, **kw: (MockModel(), None, lambda img: torch.zeros(3, 224, 224)),
    )

    class MockBox:
        def __init__(self):
            self.cls = torch.tensor([14.0])  # bird
            self.conf = torch.tensor([0.85])  # strong detection
            self.xyxy = torch.tensor([[10.0, 10.0, 100.0, 100.0]])

    class MockResult:
        def __init__(self):
            self.boxes = [MockBox()]

    class MockYOLO:
        names: ClassVar[dict[int, str]] = {14: "bird"}

        def __call__(self, img, **kw):
            return [MockResult()]

    monkeypatch.setattr("scripts.infer.YOLO", lambda *a, **kw: MockYOLO())

    engine = AvianInferenceEngine(models_dir=str(models_dir))

    from PIL import Image

    img_path = tmp_path / "zoo_toucan.jpg"
    Image.new("RGB", (200, 200), color=(100, 100, 100)).save(img_path)

    res = engine.process_image(str(img_path), min_detect=0.25, region_code="US-WA")

    # Guardrail Invariant: Strong visual match for Toucan MUST win over local Pelican
    assert res["category"] == "Bird"
    assert res["assigned_species_slug"] == "Toco_Toucan", (
        f"Expected Toco_Toucan, but prior forced '{res['assigned_species_slug']}'!"
    )
