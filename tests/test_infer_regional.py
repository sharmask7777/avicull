import json
import os
import sys
import pytest
import torch

from scripts.infer import AvianInferenceEngine

def test_regional_checklist_loading_and_tensor(monkeypatch):
    # Mock open_clip and YOLO to avoid heavy weights download in unit test
    class MockModel:
        visual = torch.nn.Identity()
    monkeypatch.setattr("open_clip.create_model_and_transforms", lambda *a, **kw: (MockModel(), None, lambda x: x))
    monkeypatch.setattr("scripts.infer.YOLO", lambda *a, **kw: None)

    engine = AvianInferenceEngine(models_dir="models")
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
