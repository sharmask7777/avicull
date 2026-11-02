import json
import os
import sys

# Ensure repository root is on sys.path
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

import pytest
import torch
from safetensors.torch import save_file

from scripts.infer import AvianInferenceEngine


@pytest.fixture
def mock_models_dir(tmp_path):
    """Create a temporary models directory with synthetic taxonomy, embeddings, and checklists."""
    models_dir = tmp_path / "models"
    models_dir.mkdir(parents=True, exist_ok=True)

    taxonomy = [
        {"taxon_id": "brakit1", "common_name": "Brahminy Kite", "folder_slug": "Brahminy_Kite"},
        {"taxon_id": "baleag", "common_name": "Bald Eagle", "folder_slug": "Bald_Eagle"},
        {"taxon_id": "blared1", "common_name": "Black Redstart", "folder_slug": "Black_Redstart"},
    ]
    with open(models_dir / "taxonomy.json", "w", encoding="utf-8") as f:
        json.dump(taxonomy, f)

    # Save synthetic 3-species embeddings
    synthetic_embeddings = torch.randn(len(taxonomy), 512, dtype=torch.float32)
    synthetic_embeddings /= synthetic_embeddings.norm(dim=-1, keepdim=True)
    safetensors_path = str(models_dir / "species_embeddings.safetensors")
    save_file({"embeddings": synthetic_embeddings}, safetensors_path)

    # Also save synthetic regional_checklists in models_dir as candidate
    checklists = {
        "IN-KA": ["brakit1"],
        "IN-LA": ["blared1"],
        "US-FL": ["baleag"],
    }
    with open(models_dir / "regional_checklists.json", "w", encoding="utf-8") as f:
        json.dump(checklists, f)

    return str(models_dir)


def test_regional_checklist_loading_and_tensor(mock_models_dir, monkeypatch):
    # Mock open_clip and YOLO to avoid heavy weights download in unit test
    class MockModel:
        visual = torch.nn.Identity()

    monkeypatch.setattr(
        "open_clip.create_model_and_transforms", lambda *a, **kw: (MockModel(), None, lambda x: x)
    )
    monkeypatch.setattr("scripts.infer.YOLO", lambda *a, **kw: None)

    engine = AvianInferenceEngine(models_dir=mock_models_dir)
    assert not hasattr(engine, "ladakh_prior"), "Ad-hoc ladakh_prior must be purged"
    assert not hasattr(engine, "build_regional_priors"), "build_regional_priors must be purged"
    assert "IN-KA" in engine.regional_checklists
    assert "IN-LA" in engine.regional_checklists
    assert "US-FL" in engine.regional_checklists

    # Test regional prior tensor generation
    ka_tensor = engine.get_regional_prior_tensor("IN-KA")
    assert ka_tensor is not None
    assert ka_tensor.shape[0] == len(engine.taxonomy)

    # Brahminy Kite (brakit1) should have multiplier 1.0
    brakit1_idx = next(i for i, sp in enumerate(engine.taxonomy) if sp["taxon_id"] == "brakit1")
    assert float(ka_tensor[brakit1_idx].item()) == pytest.approx(1.0)

    # Bald Eagle (baleag) should have multiplier 0.001
    baleag_idx = next(i for i, sp in enumerate(engine.taxonomy) if sp["taxon_id"] == "baleag")
    assert float(ka_tensor[baleag_idx].item()) == pytest.approx(0.001)

    # Non-existent region code
    none_tensor = engine.get_regional_prior_tensor("UNKNOWN-REGION")
    assert none_tensor is None

    # Empty region code
    empty_tensor = engine.get_regional_prior_tensor("")
    assert empty_tensor is None
