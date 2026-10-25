import json
import os
import pytest

def test_gazetteer_contains_key_global_and_regional_hotspots():
    gazetteer_path = os.path.join(os.path.dirname(__file__), "..", "data", "gazetteer.json")
    assert os.path.exists(gazetteer_path), "gazetteer.json must exist"
    
    with open(gazetteer_path, "r", encoding="utf-8") as f:
        data = json.load(f)
        
    names = {entry["name"].lower(): entry for entry in data}
    # Check Indian & Bangalore spots
    assert "saul kere" in names or "saul kere lake" in names
    assert "bengaluru" in names or "bangalore" in names
    assert "ranthambore" in names or "ranthambhore" in names
    assert "leh" in names
    # Check Global spots
    assert "everglades" in names or "everglades national park" in names
    assert "kruger" in names or "kruger national park" in names
    assert "central park" in names

def test_gazetteer_schema_and_types():
    gazetteer_path = os.path.join(os.path.dirname(__file__), "..", "data", "gazetteer.json")
    assert os.path.exists(gazetteer_path), "gazetteer.json must exist"
    
    with open(gazetteer_path, "r", encoding="utf-8") as f:
        data = json.load(f)
        
    assert isinstance(data, list)
    assert len(data) >= 20, "Should contain at least 20 curated hotspots and cities"
    
    valid_categories = {"hotspot", "city", "reserve"}
    for entry in data:
        assert "name" in entry and isinstance(entry["name"], str) and len(entry["name"]) > 0
        assert "region_code" in entry and isinstance(entry["region_code"], str) and len(entry["region_code"]) > 0
        assert "lat" in entry and isinstance(entry["lat"], (int, float))
        assert -90.0 <= entry["lat"] <= 90.0
        assert "lon" in entry and isinstance(entry["lon"], (int, float))
        assert -180.0 <= entry["lon"] <= 180.0
        assert "category" in entry and entry["category"] in valid_categories

def test_regional_checklists_integrity_and_species_filtering():
    checklists_path = os.path.join(os.path.dirname(__file__), "..", "data", "regional_checklists.json")
    taxonomy_path = os.path.join(os.path.dirname(__file__), "..", "models", "taxonomy.json")
    assert os.path.exists(checklists_path), "regional_checklists.json must exist"
    assert os.path.exists(taxonomy_path), "taxonomy.json must exist"
    
    with open(checklists_path, "r", encoding="utf-8") as f:
        checklists = json.load(f)
        
    with open(taxonomy_path, "r", encoding="utf-8") as f:
        taxonomy = json.load(f)
        
    valid_taxon_ids = {s["taxon_id"] for s in taxonomy}
    
    # Required regional codes
    required_regions = ["IN-KA", "IN-RJ", "IN-LA", "US-FL", "ZA"]
    for reg in required_regions:
        assert reg in checklists, f"Region {reg} must be present in regional_checklists.json"
        assert len(checklists[reg]) > 0, f"Checklist for {reg} must not be empty"
        for tid in checklists[reg]:
            assert tid in valid_taxon_ids, f"Taxon ID {tid} in {reg} not found in taxonomy.json"
            
    # For Karnataka (IN-KA):
    # Brahminy Kite (brakit1), White-throated Kingfisher (whtkin2), Indian Cormorant (indcor1),
    # Great Egret (greegr), Gray Heron (graher1)
    karnataka = set(checklists["IN-KA"])
    assert "brakit1" in karnataka, "Brahminy Kite (brakit1) must be in IN-KA checklist"
    assert "whtkin2" in karnataka, "White-throated Kingfisher (whtkin2) must be in IN-KA checklist"
    assert "indcor1" in karnataka, "Indian Cormorant (indcor1) must be in IN-KA checklist"
    assert "greegr" in karnataka, "Great Egret (greegr) must be in IN-KA checklist"
    assert "graher1" in karnataka, "Gray Heron (graher1) must be in IN-KA checklist"
    
    # Out of range birds in Karnataka: Bald Eagle (baleag), Black Swan (blkswa)
    assert "baleag" not in karnataka, "Bald Eagle (baleag) must not be in IN-KA checklist"
    assert "blkswa" not in karnataka, "Black Swan (blkswa) must not be in IN-KA checklist"
    
    # Florida should have Bald Eagle
    florida = set(checklists["US-FL"])
    assert "baleag" in florida, "Bald Eagle (baleag) should be in US-FL checklist"
