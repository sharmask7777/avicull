import json
import os
from dataclasses import FrozenInstanceError

import pytest


def test_gazetteer_contains_key_global_and_regional_hotspots():
    gazetteer_path = os.path.join(os.path.dirname(__file__), "..", "data", "gazetteer.json")
    assert os.path.exists(gazetteer_path), "gazetteer.json must exist"

    with open(gazetteer_path, encoding="utf-8") as f:
        data = json.load(f)

    names = {entry["name"].lower(): entry for entry in data}
    # Check Indian & Bangalore spots
    assert "saul kere" in names or "saul kere lake" in names
    assert "bengaluru" in names or "bangalore" in names
    assert "ranthambore" in names or "ranthambhore" in names
    assert "leh" in names
    assert "gir" in names or "gir national park" in names
    assert "k gudi" in names or "br hills" in names
    assert "coonoor" in names
    assert "coorg" in names or "kodagu" in names
    assert "jhalana" in names
    assert "kanaka sarovara" in names
    assert "new zealand" in names
    # Check Global spots
    assert "everglades" in names or "everglades national park" in names
    assert "kruger" in names or "kruger national park" in names
    assert "central park" in names


def test_gazetteer_schema_and_types():
    gazetteer_path = os.path.join(os.path.dirname(__file__), "..", "data", "gazetteer.json")
    assert os.path.exists(gazetteer_path), "gazetteer.json must exist"

    with open(gazetteer_path, encoding="utf-8") as f:
        data = json.load(f)

    assert isinstance(data, list)
    assert len(data) >= 20, "Should contain at least 20 curated hotspots and cities"

    valid_categories = {"hotspot", "city", "reserve"}
    for entry in data:
        assert "name" in entry and isinstance(entry["name"], str) and len(entry["name"]) > 0
        assert (
            "region_code" in entry
            and isinstance(entry["region_code"], str)
            and len(entry["region_code"]) > 0
        )
        assert "lat" in entry and isinstance(entry["lat"], (int, float))
        assert -90.0 <= entry["lat"] <= 90.0
        assert "lon" in entry and isinstance(entry["lon"], (int, float))
        assert -180.0 <= entry["lon"] <= 180.0
        assert "category" in entry and entry["category"] in valid_categories


def test_regional_checklists_integrity_and_species_filtering():
    checklists_path = os.path.join(
        os.path.dirname(__file__), "..", "data", "regional_checklists.json"
    )
    taxonomy_path = os.path.join(os.path.dirname(__file__), "..", "models", "taxonomy.json")
    assert os.path.exists(checklists_path), "regional_checklists.json must exist"

    with open(checklists_path, encoding="utf-8") as f:
        checklists = json.load(f)

    valid_taxon_ids = None
    if os.path.exists(taxonomy_path):
        with open(taxonomy_path, encoding="utf-8") as f:
            taxonomy = json.load(f)
        valid_taxon_ids = {s["taxon_id"] for s in taxonomy}

    # Required regional codes
    required_regions = ["IN-KA", "IN-RJ", "IN-LA", "IN-GJ", "IN-TN", "US-FL", "ZA", "NZ"]
    for reg in required_regions:
        assert reg in checklists, f"Region {reg} must be present in regional_checklists.json"
        assert len(checklists[reg]) > 0, f"Checklist for {reg} must not be empty"
        if valid_taxon_ids is not None:
            for tid in checklists[reg]:
                assert tid in valid_taxon_ids, f"Taxon ID {tid} in {reg} not found in taxonomy.json"

    # For Karnataka (IN-KA):
    # Brahminy Kite (brakit1), White-throated Kingfisher (whtkin2), Indian Cormorant (indcor1),
    # Great Egret (greegr), Gray Heron (graher1), Asian Woolly-necked Stork (wonsto1),
    # White-browed Wagtail (whbwag1)
    karnataka = set(checklists["IN-KA"])
    assert "brakit1" in karnataka, "Brahminy Kite (brakit1) must be in IN-KA checklist"
    assert "whtkin2" in karnataka, "White-throated Kingfisher (whtkin2) must be in IN-KA checklist"
    assert "indcor1" in karnataka, "Indian Cormorant (indcor1) must be in IN-KA checklist"
    assert "greegr" in karnataka, "Great Egret (greegr) must be in IN-KA checklist"
    assert "graher1" in karnataka, "Gray Heron (graher1) must be in IN-KA checklist"
    assert "wonsto1" in karnataka, "Asian Woolly-necked Stork (wonsto1) must be in IN-KA checklist"
    assert "whbwag1" in karnataka, "White-browed Wagtail (whbwag1) must be in IN-KA checklist"

    # Out of range birds in Karnataka: Bald Eagle (baleag), Black Swan (blkswa),
    # African Woolly-necked Stork (wonsto2), African Pied Wagtail (afpwag1)
    assert "baleag" not in karnataka, "Bald Eagle (baleag) must not be in IN-KA checklist"
    assert "blkswa" not in karnataka, "Black Swan (blkswa) must not be in IN-KA checklist"
    assert "wonsto2" not in karnataka, (
        "African Woolly-necked Stork (wonsto2) must not be in IN-KA checklist"
    )
    assert "afpwag1" not in karnataka, (
        "African Pied Wagtail (afpwag1) must not be in IN-KA checklist"
    )

    # Florida should have Bald Eagle
    florida = set(checklists["US-FL"])
    assert "baleag" in florida, "Bald Eagle (baleag) should be in US-FL checklist"

    # New Zealand (NZ) should have native birds and introduced Black Swan
    nz = set(checklists["NZ"])
    assert "kea1" in nz, "Kea (kea1) should be in NZ checklist"
    assert "nezpig2" in nz, "New Zealand Pigeon (nezpig2) should be in NZ checklist"
    assert "nezsca1" in nz, "New Zealand Scaup (nezsca1) should be in NZ checklist"
    assert "weka1" in nz, "Weka (weka1) should be in NZ checklist"
    assert "blkswa" in nz, "Black Swan (blkswa) should be in NZ checklist"
    assert "bramer1" not in nz, "Brazilian Merganser (bramer1) must not be in NZ checklist"
    assert "madpoc1" not in nz, "Madagascar Pochard (madpoc1) must not be in NZ checklist"


def test_validate_coordinates_valid_and_invalid():
    from scripts.build_gazetteer import validate_coordinates

    # Valid coordinates
    assert validate_coordinates(0.0, 0.0) is True
    assert validate_coordinates(90.0, 180.0) is True
    assert validate_coordinates(-90.0, -180.0) is True
    assert validate_coordinates(12.9123, 77.6845) is True

    # Invalid coordinates
    assert validate_coordinates(90.0001, 0.0) is False
    assert validate_coordinates(-90.0001, 0.0) is False
    assert validate_coordinates(0.0, 180.0001) is False
    assert validate_coordinates(0.0, -180.0001) is False
    assert validate_coordinates(float("nan"), 0.0) is False
    assert validate_coordinates(0.0, float("nan")) is False
    assert validate_coordinates(float("inf"), 0.0) is False
    assert validate_coordinates(0.0, float("inf")) is False


def test_normalize_hotspot_name():
    from scripts.build_gazetteer import normalize_hotspot_name

    assert normalize_hotspot_name("  Saul Kere  ") == "saul kere"
    assert normalize_hotspot_name("Ranthambore   National   Park") == "ranthambore national park"
    assert normalize_hotspot_name("Keoladeo - National Park!") == "keoladeo - national park!"
    assert normalize_hotspot_name("  BANGALORE  ") == "bangalore"


def test_gazetteer_hotspot_dataclass_frozen():
    from scripts.build_gazetteer import GazetteerHotspot

    hotspot = GazetteerHotspot(
        name="Saul Kere",
        lat=12.9123,
        lon=77.6845,
        region="IN-KA",
        aliases=("Saul Kere Lake",),
    )
    assert hotspot.name == "Saul Kere"
    assert hotspot.lat == 12.9123
    assert hotspot.lon == 77.6845
    assert hotspot.region == "IN-KA"
    assert hotspot.aliases == ("Saul Kere Lake",)

    # Immutability check
    with pytest.raises(FrozenInstanceError):
        hotspot.name = "New Name"  # type: ignore[misc]


def test_deduplicate_hotspots():
    from scripts.build_gazetteer import GazetteerHotspot, deduplicate_hotspots

    h1 = GazetteerHotspot("Saul Kere", 12.9123, 77.6845, "IN-KA")
    h2 = GazetteerHotspot("saul kere", 12.9123, 77.6845, "IN-KA", aliases=("Saul Kere Lake",))
    h3 = GazetteerHotspot("Bengaluru", 12.9716, 77.5946, "IN-KA")

    deduped = deduplicate_hotspots([h1, h2, h3])
    assert len(deduped) == 2
    assert deduped[0].name == "Saul Kere"
    assert "Saul Kere Lake" in deduped[0].aliases
    assert deduped[1].name == "Bengaluru"


def test_build_gazetteer_data():
    from scripts.build_gazetteer import build_gazetteer_data

    raw = [
        {
            "name": "Saul Kere",
            "lat": 12.9123,
            "lon": 77.6845,
            "region_code": "IN-KA",
            "category": "hotspot",
        },
        {
            "name": "Bengaluru",
            "lat": 12.9716,
            "lon": 77.5946,
            "region_code": "IN-KA",
            "category": "city",
        },
    ]
    hotspots = build_gazetteer_data(raw)
    assert len(hotspots) == 2
    assert hotspots[0].name == "Saul Kere"
    assert hotspots[0].lat == 12.9123
    assert hotspots[0].lon == 77.6845
    assert hotspots[0].region == "IN-KA"


def test_build_gazetteer_data_invalid_coordinates():
    from scripts.build_gazetteer import build_gazetteer_data

    raw = [
        {"name": "Bad Lat", "lat": 100.0, "lon": 77.6845, "region_code": "IN-KA"},
    ]
    with pytest.raises(ValueError, match="Invalid coordinates"):
        build_gazetteer_data(raw)


def test_serialize_gazetteer():
    from scripts.build_gazetteer import GazetteerHotspot, serialize_gazetteer

    hotspots = [
        GazetteerHotspot(
            "Saul Kere", 12.9123, 77.6845, "IN-KA", aliases=("Saul Kere Lake",), category="hotspot"
        ),
    ]
    serialized = serialize_gazetteer(hotspots)
    assert len(serialized) == 1
    assert serialized[0]["name"] == "Saul Kere"
    assert serialized[0]["region_code"] == "IN-KA"
    assert serialized[0]["lat"] == 12.9123
    assert serialized[0]["lon"] == 77.6845
    assert serialized[0]["latitude"] == 12.9123
    assert serialized[0]["longitude"] == 77.6845
    assert serialized[0]["category"] == "hotspot"
    assert serialized[0]["aliases"] == ["Saul Kere Lake"]


def test_build_gazetteer_data_missing_fields():
    from scripts.build_gazetteer import build_gazetteer_data

    with pytest.raises(ValueError, match="Missing or invalid name"):
        build_gazetteer_data([{"lat": 10.0, "lon": 10.0}])

    with pytest.raises(ValueError, match="Missing or invalid name"):
        build_gazetteer_data([{"name": "", "lat": 10.0, "lon": 10.0}])

    with pytest.raises(ValueError, match="Missing latitude/longitude"):
        build_gazetteer_data([{"name": "No Coords"}])


def test_validate_coordinates_non_numeric():
    from scripts.build_gazetteer import validate_coordinates

    assert validate_coordinates("invalid", 10.0) is False  # type: ignore[arg-type]
    assert validate_coordinates(10.0, "invalid") is False  # type: ignore[arg-type]


def test_build_gazetteer_execution(tmp_path, monkeypatch):
    import scripts.build_gazetteer as bg

    taxonomy_file = tmp_path / "taxonomy.json"
    gazetteer_out = tmp_path / "gazetteer.json"
    checklists_out = tmp_path / "regional_checklists.json"

    # Create dummy taxonomy covering all taxon IDs in build_gazetteer
    all_taxon_ids = set()
    for tids in bg.REGIONAL_CHECKLISTS_MAP.values():
        all_taxon_ids.update(tids)

    dummy_taxonomy = [{"taxon_id": tid, "common_name": tid} for tid in all_taxon_ids]
    with open(taxonomy_file, "w", encoding="utf-8") as f:
        json.dump(dummy_taxonomy, f)

    monkeypatch.setattr(bg, "GAZETTEER_PATH", str(gazetteer_out))
    monkeypatch.setattr(bg, "CHECKLISTS_PATH", str(checklists_out))
    monkeypatch.setattr(bg, "DATA_DIR", str(tmp_path))

    bg.build_gazetteer(str(taxonomy_file))

    assert os.path.exists(gazetteer_out)
    assert os.path.exists(checklists_out)

    # Test file not found error
    with pytest.raises(FileNotFoundError):
        bg.build_gazetteer(str(tmp_path / "nonexistent.json"))

    # Test invalid taxon id error
    invalid_taxonomy = [{"taxon_id": "different"}]
    inv_file = tmp_path / "invalid_tax.json"
    with open(inv_file, "w", encoding="utf-8") as f:
        json.dump(invalid_taxonomy, f)

    with pytest.raises(ValueError, match="Taxon ID .* not found in taxonomy.json"):
        bg.build_gazetteer(str(inv_file))
