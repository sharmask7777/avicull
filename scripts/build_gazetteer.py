#!/usr/bin/env python3
"""
Compiles curated global birding hotspots (eBird + IBA + national parks/reserves)
and major global cities into data/gazetteer.json, and maps regional codes
to confirmed species taxon IDs from models/taxonomy.json into data/regional_checklists.json.
"""

import json
import math
import os
import re
from collections.abc import Sequence
from dataclasses import dataclass
from typing import Any

# Root directory of avian_rearranger
REPO_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
TAXONOMY_PATH = os.path.join(REPO_ROOT, "models", "taxonomy.json")
DATA_DIR = os.path.join(REPO_ROOT, "data")
GAZETTEER_PATH = os.path.join(DATA_DIR, "gazetteer.json")
CHECKLISTS_PATH = os.path.join(DATA_DIR, "regional_checklists.json")


@dataclass(frozen=True)
class GazetteerHotspot:
    """Immutable representation of a gazetteer geographic waypoint or hotspot.

    Attributes:
        name: Primary display name of the hotspot or locality.
        lat: Latitude in decimal degrees ([-90.0, 90.0]).
        lon: Longitude in decimal degrees ([-180.0, 180.0]).
        region: ISO 3166-1/2 or eBird regional code (e.g., 'IN-KA', 'US-FL').
        aliases: Alternative names or transliterations for fuzzy resolution.
        category: Place category ('hotspot', 'city', 'reserve').
    """

    name: str
    lat: float
    lon: float
    region: str
    aliases: tuple[str, ...] = ()
    category: str = "hotspot"


def validate_coordinates(lat: float, lon: float) -> bool:
    """Validate whether latitude and longitude coordinates fall within terrestrial bounds.

    Returns:
        True if lat in [-90.0, 90.0] and lon in [-180.0, 180.0] and not NaN/infinite,
        otherwise False.
    """
    try:
        f_lat = float(lat)
        f_lon = float(lon)
    except (ValueError, TypeError):
        return False
    if math.isnan(f_lat) or math.isnan(f_lon) or math.isinf(f_lat) or math.isinf(f_lon):
        return False
    return -90.0 <= f_lat <= 90.0 and -180.0 <= f_lon <= 180.0


def normalize_hotspot_name(name: str) -> str:
    """Normalize a hotspot name by stripping whitespace, collapsing spaces, and lowercasing."""
    cleaned = re.sub(r"\s+", " ", name.strip())
    return cleaned.lower()


def deduplicate_hotspots(hotspots: Sequence[GazetteerHotspot]) -> tuple[GazetteerHotspot, ...]:
    """Deduplicate a sequence of hotspots by normalized name and coordinates.

    When duplicate entries are encountered, aliases are merged into the original entry.
    """
    deduped: list[GazetteerHotspot] = []
    index_by_key: dict[tuple[str, float, float], int] = {}

    for spot in hotspots:
        norm_name = normalize_hotspot_name(spot.name)
        key = (norm_name, round(spot.lat, 4), round(spot.lon, 4))
        if key in index_by_key:
            idx = index_by_key[key]
            existing = deduped[idx]
            combined_aliases = list(existing.aliases)
            for alias in spot.aliases:
                if alias not in combined_aliases and normalize_hotspot_name(alias) != norm_name:
                    combined_aliases.append(alias)
            deduped[idx] = GazetteerHotspot(
                name=existing.name,
                lat=existing.lat,
                lon=existing.lon,
                region=existing.region,
                aliases=tuple(combined_aliases),
                category=existing.category,
            )
        else:
            index_by_key[key] = len(deduped)
            deduped.append(spot)

    return tuple(deduped)


def build_gazetteer_data(raw_entries: Sequence[dict[str, Any]]) -> tuple[GazetteerHotspot, ...]:
    """Parse raw dictionary entries into validated, immutable GazetteerHotspot instances.

    Raises:
        ValueError: If coordinates are out of bounds or required fields are missing.
    """
    hotspots: list[GazetteerHotspot] = []
    for entry in raw_entries:
        name = entry.get("name")
        if not name or not isinstance(name, str):
            raise ValueError(f"Missing or invalid name in entry: {entry}")

        lat_val = entry.get("latitude") if "latitude" in entry else entry.get("lat")
        lon_val = entry.get("longitude") if "longitude" in entry else entry.get("lon")
        if lat_val is None or lon_val is None:
            raise ValueError(f"Missing latitude/longitude in entry for {name}")

        lat = float(lat_val)
        lon = float(lon_val)
        if not validate_coordinates(lat, lon):
            raise ValueError(f"Invalid coordinates ({lat}, {lon}) for {name}")

        region = entry.get("region_code") or entry.get("region", "")
        category = entry.get("category", "hotspot")
        aliases_raw = entry.get("aliases", ())
        aliases = tuple(aliases_raw) if isinstance(aliases_raw, (list, tuple)) else ()

        hotspots.append(
            GazetteerHotspot(
                name=name,
                lat=lat,
                lon=lon,
                region=region,
                aliases=aliases,
                category=category,
            )
        )

    return tuple(hotspots)


def serialize_gazetteer(hotspots: Sequence[GazetteerHotspot]) -> list[dict[str, Any]]:
    """Serialize a sequence of GazetteerHotspot instances to schema-compliant dictionary format.

    Emits both `lat`/`lon` and `latitude`/`longitude` for downstream cross-system compatibility.
    """
    serialized: list[dict[str, Any]] = []
    for spot in hotspots:
        item: dict[str, Any] = {
            "name": spot.name,
            "region_code": spot.region,
            "lat": spot.lat,
            "lon": spot.lon,
            "latitude": spot.lat,
            "longitude": spot.lon,
            "category": spot.category,
        }
        if spot.aliases:
            item["aliases"] = list(spot.aliases)
        serialized.append(item)
    return serialized


# -------------------------------------------------------------------------
# Curated Global Gazetteer: Hotspots, Reserves, and Cities
# -------------------------------------------------------------------------
RAW_GAZETTEER: list[dict[str, Any]] = [
    {
        "name": "Agumbe",
        "region_code": "IN-KA",
        "lat": 13.5074,
        "lon": 75.0931,
        "category": "reserve",
    },
    {
        "name": "Bhadra Wildlife Sanctuary",
        "region_code": "IN-KA",
        "lat": 13.6966,
        "lon": 75.6344,
        "category": "reserve",
    },
    {"name": "Kabini", "region_code": "IN-KA", "lat": 11.9167, "lon": 76.35, "category": "reserve"},
    {
        "name": "Kabini Wildlife Sanctuary",
        "region_code": "IN-KA",
        "lat": 11.9167,
        "lon": 76.35,
        "category": "reserve",
    },
    {
        "name": "K Gudi",
        "region_code": "IN-KA",
        "lat": 11.9833,
        "lon": 77.1333,
        "category": "reserve",
    },
    {
        "name": "BR Hills",
        "region_code": "IN-KA",
        "lat": 11.9833,
        "lon": 77.1333,
        "category": "reserve",
    },
    {
        "name": "Biligiriranga Hills",
        "region_code": "IN-KA",
        "lat": 11.9833,
        "lon": 77.1333,
        "category": "reserve",
    },
    {
        "name": "Madikeri",
        "region_code": "IN-KA",
        "lat": 12.4244,
        "lon": 75.7382,
        "category": "city",
    },
    {
        "name": "Kudremukh National Park",
        "region_code": "IN-KA",
        "lat": 13.2167,
        "lon": 75.25,
        "category": "reserve",
    },
    {
        "name": "Brahmagiri Wildlife Sanctuary",
        "region_code": "IN-KA",
        "lat": 12.0167,
        "lon": 75.9833,
        "category": "reserve",
    },
    {
        "name": "Pushpagiri Wildlife Sanctuary",
        "region_code": "IN-KA",
        "lat": 12.5833,
        "lon": 75.6833,
        "category": "reserve",
    },
    {
        "name": "Talakaveri Wildlife Sanctuary",
        "region_code": "IN-KA",
        "lat": 12.3833,
        "lon": 75.4833,
        "category": "reserve",
    },
    {
        "name": "Lalbagh Botanical Garden",
        "region_code": "IN-KA",
        "lat": 12.9507,
        "lon": 77.5848,
        "category": "hotspot",
    },
    {
        "name": "Cubbon Park",
        "region_code": "IN-KA",
        "lat": 12.9763,
        "lon": 77.5929,
        "category": "hotspot",
    },
    {
        "name": "Madiwala Lake",
        "region_code": "IN-KA",
        "lat": 12.915,
        "lon": 77.6186,
        "category": "hotspot",
    },
    {
        "name": "Agara Lake",
        "region_code": "IN-KA",
        "lat": 12.9242,
        "lon": 77.6464,
        "category": "hotspot",
    },
    {
        "name": "Puttenahalli Lake",
        "region_code": "IN-KA",
        "lat": 12.8906,
        "lon": 77.5864,
        "category": "hotspot",
    },
    {
        "name": "Bellandur Lake",
        "region_code": "IN-KA",
        "lat": 12.9366,
        "lon": 77.6666,
        "category": "hotspot",
    },
    {
        "name": "Jakkur Lake",
        "region_code": "IN-KA",
        "lat": 13.0769,
        "lon": 77.6083,
        "category": "hotspot",
    },
    {
        "name": "Hesaraghatta Grasslands",
        "region_code": "IN-KA",
        "lat": 13.155,
        "lon": 77.485,
        "category": "hotspot",
    },
    {
        "name": "Manchinbele Dam",
        "region_code": "IN-KA",
        "lat": 12.875,
        "lon": 77.334,
        "category": "hotspot",
    },
    {"name": "Gokarna", "region_code": "IN-KA", "lat": 14.5479, "lon": 74.3188, "category": "city"},
    {"name": "Karwar", "region_code": "IN-KA", "lat": 14.8167, "lon": 74.1333, "category": "city"},
    {
        "name": "Thattekad Bird Sanctuary",
        "region_code": "IN-KL",
        "lat": 10.1333,
        "lon": 76.7,
        "category": "reserve",
    },
    {
        "name": "Salim Ali Bird Sanctuary Thattekad",
        "region_code": "IN-KL",
        "lat": 10.1333,
        "lon": 76.7,
        "category": "reserve",
    },
    {
        "name": "Munnar",
        "region_code": "IN-KL",
        "lat": 10.0889,
        "lon": 77.0595,
        "category": "hotspot",
    },
    {
        "name": "Eravikulam National Park",
        "region_code": "IN-KL",
        "lat": 10.2,
        "lon": 77.0667,
        "category": "reserve",
    },
    {
        "name": "Periyar Tiger Reserve",
        "region_code": "IN-KL",
        "lat": 9.4667,
        "lon": 77.1667,
        "category": "reserve",
    },
    {
        "name": "Wayanad Wildlife Sanctuary",
        "region_code": "IN-KL",
        "lat": 11.6854,
        "lon": 76.3686,
        "category": "reserve",
    },
    {
        "name": "Silent Valley National Park",
        "region_code": "IN-KL",
        "lat": 11.1333,
        "lon": 76.4333,
        "category": "reserve",
    },
    {
        "name": "Parambikulam Tiger Reserve",
        "region_code": "IN-KL",
        "lat": 10.4333,
        "lon": 76.7833,
        "category": "reserve",
    },
    {
        "name": "Kumarakom Bird Sanctuary",
        "region_code": "IN-KL",
        "lat": 9.6167,
        "lon": 76.4167,
        "category": "reserve",
    },
    {
        "name": "Vembanad Lake",
        "region_code": "IN-KL",
        "lat": 9.6,
        "lon": 76.4,
        "category": "hotspot",
    },
    {
        "name": "Mudumalai National Park",
        "region_code": "IN-TN",
        "lat": 11.5622,
        "lon": 76.5342,
        "category": "reserve",
    },
    {
        "name": "Anamalai Tiger Reserve",
        "region_code": "IN-TN",
        "lat": 10.5,
        "lon": 76.9667,
        "category": "reserve",
    },
    {
        "name": "Valparai",
        "region_code": "IN-TN",
        "lat": 10.3204,
        "lon": 76.9554,
        "category": "hotspot",
    },
    {"name": "Ooty", "region_code": "IN-TN", "lat": 11.4102, "lon": 76.695, "category": "city"},
    {
        "name": "Kodaikanal",
        "region_code": "IN-TN",
        "lat": 10.2381,
        "lon": 77.4892,
        "category": "city",
    },
    {
        "name": "Vedanthangal Bird Sanctuary",
        "region_code": "IN-TN",
        "lat": 12.5444,
        "lon": 79.8556,
        "category": "reserve",
    },
    {
        "name": "Point Calimere Wildlife and Bird Sanctuary",
        "region_code": "IN-TN",
        "lat": 10.3,
        "lon": 79.8667,
        "category": "reserve",
    },
    {
        "name": "Pulicat Lake",
        "region_code": "IN-TN",
        "lat": 13.4167,
        "lon": 80.1667,
        "category": "hotspot",
    },
    {
        "name": "Shey Marshes",
        "region_code": "IN-LA",
        "lat": 34.0722,
        "lon": 77.6361,
        "category": "hotspot",
    },
    {
        "name": "Sindhu Ghat",
        "region_code": "IN-LA",
        "lat": 34.095,
        "lon": 77.618,
        "category": "hotspot",
    },
    {
        "name": "Thiksey",
        "region_code": "IN-LA",
        "lat": 34.0583,
        "lon": 77.6667,
        "category": "hotspot",
    },
    {
        "name": "Spituk",
        "region_code": "IN-LA",
        "lat": 34.1297,
        "lon": 77.5258,
        "category": "hotspot",
    },
    {
        "name": "Hemis National Park",
        "region_code": "IN-LA",
        "lat": 33.9167,
        "lon": 77.4167,
        "category": "reserve",
    },
    {
        "name": "Pangong Tso",
        "region_code": "IN-LA",
        "lat": 33.7289,
        "lon": 78.7189,
        "category": "hotspot",
    },
    {
        "name": "Tso Moriri",
        "region_code": "IN-LA",
        "lat": 32.9167,
        "lon": 78.3167,
        "category": "hotspot",
    },
    {
        "name": "Tso Kar",
        "region_code": "IN-LA",
        "lat": 33.3167,
        "lon": 77.9833,
        "category": "hotspot",
    },
    {
        "name": "Hanle",
        "region_code": "IN-LA",
        "lat": 32.7667,
        "lon": 78.9667,
        "category": "hotspot",
    },
    {
        "name": "Nubra Valley",
        "region_code": "IN-LA",
        "lat": 34.6833,
        "lon": 77.5667,
        "category": "hotspot",
    },
    {"name": "Diskit", "region_code": "IN-LA", "lat": 34.5428, "lon": 77.5583, "category": "city"},
    {
        "name": "Hunder",
        "region_code": "IN-LA",
        "lat": 34.5833,
        "lon": 77.4667,
        "category": "hotspot",
    },
    {
        "name": "Khardung La",
        "region_code": "IN-LA",
        "lat": 34.2789,
        "lon": 77.6044,
        "category": "hotspot",
    },
    {
        "name": "Chang La",
        "region_code": "IN-LA",
        "lat": 34.0489,
        "lon": 77.9303,
        "category": "hotspot",
    },
    {"name": "Tangtse", "region_code": "IN-LA", "lat": 34.02, "lon": 78.18, "category": "hotspot"},
    {
        "name": "Puga Valley",
        "region_code": "IN-LA",
        "lat": 33.2333,
        "lon": 78.3167,
        "category": "hotspot",
    },
    {
        "name": "Tal Chhapar Sanctuary",
        "region_code": "IN-RJ",
        "lat": 27.8,
        "lon": 74.4333,
        "category": "reserve",
    },
    {
        "name": "Desert National Park",
        "region_code": "IN-RJ",
        "lat": 26.5,
        "lon": 70.6667,
        "category": "reserve",
    },
    {
        "name": "Sam Sand Dunes",
        "region_code": "IN-RJ",
        "lat": 26.8333,
        "lon": 70.5,
        "category": "hotspot",
    },
    {
        "name": "Khichan Bird Sanctuary",
        "region_code": "IN-RJ",
        "lat": 27.1333,
        "lon": 72.4167,
        "category": "reserve",
    },
    {
        "name": "Sambhar Salt Lake",
        "region_code": "IN-RJ",
        "lat": 26.9,
        "lon": 75.2,
        "category": "hotspot",
    },
    {
        "name": "Chandlai Lake",
        "region_code": "IN-RJ",
        "lat": 26.68,
        "lon": 75.87,
        "category": "hotspot",
    },
    {"name": "Barkheda", "region_code": "IN-RJ", "lat": 26.7, "lon": 75.9, "category": "hotspot"},
    {
        "name": "Kumbhalgarh Wildlife Sanctuary",
        "region_code": "IN-RJ",
        "lat": 25.15,
        "lon": 73.5833,
        "category": "reserve",
    },
    {
        "name": "Mount Abu Wildlife Sanctuary",
        "region_code": "IN-RJ",
        "lat": 24.5925,
        "lon": 72.7156,
        "category": "reserve",
    },
    {
        "name": "Sariska Tiger Reserve",
        "region_code": "IN-RJ",
        "lat": 27.3167,
        "lon": 76.4333,
        "category": "reserve",
    },
    {
        "name": "Mukundra Hills National Park",
        "region_code": "IN-RJ",
        "lat": 24.8167,
        "lon": 75.9833,
        "category": "reserve",
    },
    {
        "name": "Gir National Park",
        "region_code": "IN-GJ",
        "lat": 21.1244,
        "lon": 70.8242,
        "category": "reserve",
    },
    {
        "name": "Velavadar Blackbuck National Park",
        "region_code": "IN-GJ",
        "lat": 21.9333,
        "lon": 72.0333,
        "category": "reserve",
    },
    {
        "name": "Little Rann of Kutch",
        "region_code": "IN-GJ",
        "lat": 23.3333,
        "lon": 71.5,
        "category": "reserve",
    },
    {
        "name": "Wild Ass Sanctuary",
        "region_code": "IN-GJ",
        "lat": 23.3333,
        "lon": 71.5,
        "category": "reserve",
    },
    {
        "name": "Greater Rann of Kutch",
        "region_code": "IN-GJ",
        "lat": 24.0833,
        "lon": 70.4167,
        "category": "reserve",
    },
    {
        "name": "Banni Grasslands",
        "region_code": "IN-GJ",
        "lat": 23.6667,
        "lon": 69.5,
        "category": "hotspot",
    },
    {
        "name": "Chari-Dhand Wetland Conservation Reserve",
        "region_code": "IN-GJ",
        "lat": 23.5833,
        "lon": 69.35,
        "category": "reserve",
    },
    {
        "name": "Nalsarovar Bird Sanctuary",
        "region_code": "IN-GJ",
        "lat": 22.8,
        "lon": 72.05,
        "category": "reserve",
    },
    {
        "name": "Khijadiya Bird Sanctuary",
        "region_code": "IN-GJ",
        "lat": 22.5167,
        "lon": 70.1667,
        "category": "reserve",
    },
    {
        "name": "Marine National Park Jamnagar",
        "region_code": "IN-GJ",
        "lat": 22.4667,
        "lon": 69.95,
        "category": "reserve",
    },
    {
        "name": "Thol Bird Sanctuary",
        "region_code": "IN-GJ",
        "lat": 23.1333,
        "lon": 72.4,
        "category": "reserve",
    },
    {
        "name": "Jim Corbett National Park",
        "region_code": "IN-UT",
        "lat": 29.53,
        "lon": 78.7747,
        "category": "reserve",
    },
    {
        "name": "Corbett National Park",
        "region_code": "IN-UT",
        "lat": 29.53,
        "lon": 78.7747,
        "category": "reserve",
    },
    {
        "name": "Pangot",
        "region_code": "IN-UT",
        "lat": 29.4167,
        "lon": 79.4333,
        "category": "hotspot",
    },
    {"name": "Sattal", "region_code": "IN-UT", "lat": 29.35, "lon": 79.5333, "category": "hotspot"},
    {
        "name": "Chopta",
        "region_code": "IN-UT",
        "lat": 30.4833,
        "lon": 79.1833,
        "category": "hotspot",
    },
    {
        "name": "Kedarnath Wildlife Sanctuary",
        "region_code": "IN-UT",
        "lat": 30.55,
        "lon": 79.15,
        "category": "reserve",
    },
    {
        "name": "Valley of Flowers National Park",
        "region_code": "IN-UT",
        "lat": 30.728,
        "lon": 79.6053,
        "category": "reserve",
    },
    {
        "name": "Rajaji National Park",
        "region_code": "IN-UT",
        "lat": 30.05,
        "lon": 78.1833,
        "category": "reserve",
    },
    {
        "name": "Asan Barrage Bird Sanctuary",
        "region_code": "IN-UT",
        "lat": 30.4333,
        "lon": 77.6667,
        "category": "reserve",
    },
    {
        "name": "Milford Sound",
        "region_code": "NZ",
        "lat": -44.6714,
        "lon": 167.9264,
        "category": "hotspot",
    },
    {
        "name": "Fiordland National Park",
        "region_code": "NZ",
        "lat": -45.4167,
        "lon": 167.7167,
        "category": "reserve",
    },
    {
        "name": "Stewart Island",
        "region_code": "NZ",
        "lat": -46.9,
        "lon": 167.8333,
        "category": "hotspot",
    },
    {
        "name": "Rakiura National Park",
        "region_code": "NZ",
        "lat": -46.9,
        "lon": 167.8333,
        "category": "reserve",
    },
    {
        "name": "Ulva Island",
        "region_code": "NZ",
        "lat": -46.9286,
        "lon": 168.1283,
        "category": "reserve",
    },
    {
        "name": "Arthur's Pass National Park",
        "region_code": "NZ",
        "lat": -42.9419,
        "lon": 171.5583,
        "category": "reserve",
    },
    {
        "name": "Aoraki Mount Cook National Park",
        "region_code": "NZ",
        "lat": -43.7333,
        "lon": 170.1,
        "category": "reserve",
    },
    {
        "name": "Mount Cook",
        "region_code": "NZ",
        "lat": -43.7333,
        "lon": 170.1,
        "category": "hotspot",
    },
    {
        "name": "Lake Tekapo",
        "region_code": "NZ",
        "lat": -44.0047,
        "lon": 170.4771,
        "category": "hotspot",
    },
    {"name": "Kaikoura", "region_code": "NZ", "lat": -42.4, "lon": 173.6833, "category": "hotspot"},
    {
        "name": "Tiritiri Matangi Island",
        "region_code": "NZ",
        "lat": -36.6,
        "lon": 174.8833,
        "category": "reserve",
    },
    {
        "name": "Zealandia",
        "region_code": "NZ",
        "lat": -41.2906,
        "lon": 174.7533,
        "category": "reserve",
    },
    {
        "name": "Karori Sanctuary",
        "region_code": "NZ",
        "lat": -41.2906,
        "lon": 174.7533,
        "category": "reserve",
    },
    {
        "name": "Miranda Shorebird Centre",
        "region_code": "NZ",
        "lat": -37.1833,
        "lon": 175.3167,
        "category": "hotspot",
    },
    {
        "name": "Otago Peninsula",
        "region_code": "NZ",
        "lat": -45.8667,
        "lon": 170.65,
        "category": "hotspot",
    },
    {
        "name": "Taiaroa Head",
        "region_code": "NZ",
        "lat": -45.7725,
        "lon": 170.7289,
        "category": "reserve",
    },
    {
        "name": "Cape Kidnappers",
        "region_code": "NZ",
        "lat": -39.6433,
        "lon": 177.0917,
        "category": "reserve",
    },
    {
        "name": "Tongariro National Park",
        "region_code": "NZ",
        "lat": -39.2,
        "lon": 175.5833,
        "category": "reserve",
    },
    {
        "name": "Abel Tasman National Park",
        "region_code": "NZ",
        "lat": -40.9167,
        "lon": 172.9667,
        "category": "reserve",
    },
    {
        "name": "Merritt Island National Wildlife Refuge",
        "region_code": "US-FL",
        "lat": 28.6333,
        "lon": -80.7167,
        "category": "reserve",
    },
    {
        "name": "J.N. Ding Darling National Wildlife Refuge",
        "region_code": "US-FL",
        "lat": 26.4469,
        "lon": -82.1128,
        "category": "reserve",
    },
    {
        "name": "Corkscrew Swamp Sanctuary",
        "region_code": "US-FL",
        "lat": 26.3756,
        "lon": -81.6033,
        "category": "reserve",
    },
    {
        "name": "Dry Tortugas National Park",
        "region_code": "US-FL",
        "lat": 24.6285,
        "lon": -82.8732,
        "category": "reserve",
    },
    {
        "name": "Biscayne National Park",
        "region_code": "US-FL",
        "lat": 25.4667,
        "lon": -80.1833,
        "category": "reserve",
    },
    {
        "name": "Big Cypress National Preserve",
        "region_code": "US-FL",
        "lat": 25.8583,
        "lon": -81.0333,
        "category": "reserve",
    },
    {
        "name": "St. Marks National Wildlife Refuge",
        "region_code": "US-FL",
        "lat": 30.15,
        "lon": -84.15,
        "category": "reserve",
    },
    {
        "name": "Arthur R. Marshall Loxahatchee National Wildlife Refuge",
        "region_code": "US-FL",
        "lat": 26.5,
        "lon": -80.2667,
        "category": "reserve",
    },
    {
        "name": "Yosemite National Park",
        "region_code": "US-CA",
        "lat": 37.8651,
        "lon": -119.5383,
        "category": "reserve",
    },
    {
        "name": "Point Reyes National Seashore",
        "region_code": "US-CA",
        "lat": 38.0694,
        "lon": -122.8869,
        "category": "reserve",
    },
    {
        "name": "Monterey Bay",
        "region_code": "US-CA",
        "lat": 36.8,
        "lon": -121.9,
        "category": "hotspot",
    },
    {
        "name": "Santa Cruz Island",
        "region_code": "US-CA",
        "lat": 34.02,
        "lon": -119.77,
        "category": "reserve",
    },
    {
        "name": "Masai Mara National Reserve",
        "region_code": "KE",
        "lat": -1.4917,
        "lon": 35.1444,
        "category": "reserve",
    },
    {
        "name": "Amboseli National Park",
        "region_code": "KE",
        "lat": -2.6528,
        "lon": 37.2606,
        "category": "reserve",
    },
    {
        "name": "Lake Nakuru National Park",
        "region_code": "KE",
        "lat": -0.3667,
        "lon": 36.0833,
        "category": "reserve",
    },
    {
        "name": "Lake Naivasha",
        "region_code": "KE",
        "lat": -0.7667,
        "lon": 36.35,
        "category": "hotspot",
    },
    {
        "name": "Tsavo East National Park",
        "region_code": "KE",
        "lat": -2.7667,
        "lon": 38.7667,
        "category": "reserve",
    },
    {
        "name": "Tsavo West National Park",
        "region_code": "KE",
        "lat": -3.0,
        "lon": 38.0,
        "category": "reserve",
    },
    {
        "name": "Samburu National Reserve",
        "region_code": "KE",
        "lat": 0.6167,
        "lon": 37.5333,
        "category": "reserve",
    },
    {
        "name": "Serengeti National Park",
        "region_code": "TZ",
        "lat": -2.3333,
        "lon": 34.8333,
        "category": "reserve",
    },
    {
        "name": "Ngorongoro Conservation Area",
        "region_code": "TZ",
        "lat": -3.2,
        "lon": 35.5,
        "category": "reserve",
    },
    {
        "name": "Tarangire National Park",
        "region_code": "TZ",
        "lat": -3.8333,
        "lon": 36.0,
        "category": "reserve",
    },
    {
        "name": "Lake Manyara National Park",
        "region_code": "TZ",
        "lat": -3.5,
        "lon": 35.8333,
        "category": "reserve",
    },
    {
        "name": "Chobe National Park",
        "region_code": "BW",
        "lat": -18.6667,
        "lon": 24.5,
        "category": "reserve",
    },
    {
        "name": "Okavango Delta",
        "region_code": "BW",
        "lat": -19.2833,
        "lon": 22.9,
        "category": "reserve",
    },
    {
        "name": "Moremi Game Reserve",
        "region_code": "BW",
        "lat": -19.1667,
        "lon": 23.4167,
        "category": "reserve",
    },
    {
        "name": "Boulders Beach African Penguin Colony",
        "region_code": "ZA",
        "lat": -34.1972,
        "lon": 18.4514,
        "category": "reserve",
    },
    {
        "name": "Cape Point Nature Reserve",
        "region_code": "ZA",
        "lat": -34.3569,
        "lon": 18.4975,
        "category": "reserve",
    },
    {
        "name": "Kirstenbosch National Botanical Garden",
        "region_code": "ZA",
        "lat": -33.9875,
        "lon": 18.4328,
        "category": "hotspot",
    },
    {
        "name": "Hluhluwe-iMfolozi Park",
        "region_code": "ZA",
        "lat": -28.2167,
        "lon": 31.95,
        "category": "reserve",
    },
    {
        "name": "iSimangaliso Wetland Park",
        "region_code": "ZA",
        "lat": -28.0,
        "lon": 32.5,
        "category": "reserve",
    },
    {
        "name": "Daintree Rainforest",
        "region_code": "AU-QLD",
        "lat": -16.2833,
        "lon": 145.4167,
        "category": "reserve",
    },
    {
        "name": "Cairns Esplanade",
        "region_code": "AU-QLD",
        "lat": -16.9167,
        "lon": 145.7667,
        "category": "hotspot",
    },
    {
        "name": "Kakadu National Park",
        "region_code": "AU-NT",
        "lat": -12.8333,
        "lon": 132.8333,
        "category": "reserve",
    },
    {
        "name": "Blue Mountains National Park",
        "region_code": "AU-NSW",
        "lat": -33.7167,
        "lon": 150.3167,
        "category": "reserve",
    },
    {
        "name": "Royal National Park",
        "region_code": "AU-NSW",
        "lat": -34.1333,
        "lon": 151.0667,
        "category": "reserve",
    },
    {
        "name": "Fuji-Hakone-Izu National Park",
        "region_code": "JP",
        "lat": 35.3333,
        "lon": 138.8333,
        "category": "reserve",
    },
    {
        "name": "Kushiro Shitsugen National Park",
        "region_code": "JP",
        "lat": 43.1,
        "lon": 144.4,
        "category": "reserve",
    },
    {
        "name": "Shiretoko National Park",
        "region_code": "JP",
        "lat": 44.1167,
        "lon": 145.1333,
        "category": "reserve",
    },
    {
        "name": "Khao Yai National Park",
        "region_code": "TH",
        "lat": 14.4333,
        "lon": 101.3667,
        "category": "reserve",
    },
    {
        "name": "Kaeng Krachan National Park",
        "region_code": "TH",
        "lat": 12.9,
        "lon": 99.3,
        "category": "reserve",
    },
    # --- India: Karnataka (IN-KA) ---
    {
        "name": "Saul Kere",
        "region_code": "IN-KA",
        "lat": 12.9123,
        "lon": 77.6845,
        "category": "hotspot",
    },
    {
        "name": "Saul Kere Lake",
        "region_code": "IN-KA",
        "lat": 12.9123,
        "lon": 77.6845,
        "category": "hotspot",
    },
    {
        "name": "Bengaluru",
        "region_code": "IN-KA",
        "lat": 12.9716,
        "lon": 77.5946,
        "category": "city",
    },
    {
        "name": "Bangalore",
        "region_code": "IN-KA",
        "lat": 12.9716,
        "lon": 77.5946,
        "category": "city",
    },
    {
        "name": "Kaikondrahalli Lake",
        "region_code": "IN-KA",
        "lat": 12.9135,
        "lon": 77.6744,
        "category": "hotspot",
    },
    {
        "name": "Hebbal Lake",
        "region_code": "IN-KA",
        "lat": 13.0456,
        "lon": 77.5901,
        "category": "hotspot",
    },
    {
        "name": "Bannerghatta National Park",
        "region_code": "IN-KA",
        "lat": 12.8006,
        "lon": 77.5772,
        "category": "reserve",
    },
    {
        "name": "Ranganathittu Bird Sanctuary",
        "region_code": "IN-KA",
        "lat": 12.4244,
        "lon": 76.6575,
        "category": "reserve",
    },
    {"name": "Mysore", "region_code": "IN-KA", "lat": 12.2958, "lon": 76.6394, "category": "city"},
    {"name": "Mysuru", "region_code": "IN-KA", "lat": 12.2958, "lon": 76.6394, "category": "city"},
    {
        "name": "Nagarhole National Park",
        "region_code": "IN-KA",
        "lat": 12.0311,
        "lon": 76.1557,
        "category": "reserve",
    },
    {
        "name": "Bandipur National Park",
        "region_code": "IN-KA",
        "lat": 11.6664,
        "lon": 76.6291,
        "category": "reserve",
    },
    {
        "name": "Dandeli Wildlife Sanctuary",
        "region_code": "IN-KA",
        "lat": 15.2427,
        "lon": 74.6289,
        "category": "reserve",
    },
    {
        "name": "Anshi National Park",
        "region_code": "IN-KA",
        "lat": 14.9981,
        "lon": 74.3586,
        "category": "reserve",
    },
    {
        "name": "K Gudi",
        "region_code": "IN-KA",
        "lat": 11.9961,
        "lon": 77.1350,
        "category": "hotspot",
    },
    {
        "name": "Kyathadevara Gudi",
        "region_code": "IN-KA",
        "lat": 11.9961,
        "lon": 77.1350,
        "category": "hotspot",
    },
    {
        "name": "BR Hills",
        "region_code": "IN-KA",
        "lat": 11.9961,
        "lon": 77.1350,
        "category": "reserve",
    },
    {
        "name": "Biligiriranga Hills",
        "region_code": "IN-KA",
        "lat": 11.9961,
        "lon": 77.1350,
        "category": "reserve",
    },
    {
        "name": "Coorg",
        "region_code": "IN-KA",
        "lat": 12.4244,
        "lon": 75.7382,
        "category": "hotspot",
    },
    {
        "name": "Kodagu",
        "region_code": "IN-KA",
        "lat": 12.4244,
        "lon": 75.7382,
        "category": "hotspot",
    },
    {
        "name": "Madikeri",
        "region_code": "IN-KA",
        "lat": 12.4244,
        "lon": 75.7382,
        "category": "city",
    },
    {
        "name": "Kanaka Sarovara",
        "region_code": "IN-KA",
        "lat": 12.9716,
        "lon": 77.5946,
        "category": "hotspot",
    },
    # --- India: Rajasthan (IN-RJ) ---
    {
        "name": "Ranthambore",
        "region_code": "IN-RJ",
        "lat": 26.0173,
        "lon": 76.5026,
        "category": "reserve",
    },
    {
        "name": "Ranthambore National Park",
        "region_code": "IN-RJ",
        "lat": 26.0173,
        "lon": 76.5026,
        "category": "reserve",
    },
    {
        "name": "Keoladeo National Park",
        "region_code": "IN-RJ",
        "lat": 27.1594,
        "lon": 77.5218,
        "category": "reserve",
    },
    {
        "name": "Bharatpur",
        "region_code": "IN-RJ",
        "lat": 27.2152,
        "lon": 77.5030,
        "category": "city",
    },
    {"name": "Jaipur", "region_code": "IN-RJ", "lat": 26.9124, "lon": 75.7873, "category": "city"},
    {
        "name": "Desert National Park",
        "region_code": "IN-RJ",
        "lat": 27.0397,
        "lon": 70.8872,
        "category": "reserve",
    },
    {
        "name": "Jaisalmer",
        "region_code": "IN-RJ",
        "lat": 26.9157,
        "lon": 70.9083,
        "category": "city",
    },
    {
        "name": "Jhalana",
        "region_code": "IN-RJ",
        "lat": 26.8580,
        "lon": 75.8230,
        "category": "reserve",
    },
    {
        "name": "Jhalana Safari Park",
        "region_code": "IN-RJ",
        "lat": 26.8580,
        "lon": 75.8230,
        "category": "reserve",
    },
    {
        "name": "Jhalana Leopard Safari Park",
        "region_code": "IN-RJ",
        "lat": 26.8580,
        "lon": 75.8230,
        "category": "reserve",
    },
    # --- India: Gujarat (IN-GJ) ---
    {"name": "Gir", "region_code": "IN-GJ", "lat": 21.1243, "lon": 70.8242, "category": "reserve"},
    {
        "name": "Gir National Park",
        "region_code": "IN-GJ",
        "lat": 21.1243,
        "lon": 70.8242,
        "category": "reserve",
    },
    {
        "name": "Sasan Gir",
        "region_code": "IN-GJ",
        "lat": 21.1243,
        "lon": 70.8242,
        "category": "reserve",
    },
    {
        "name": "Velavadar Blackbuck National Park",
        "region_code": "IN-GJ",
        "lat": 21.9333,
        "lon": 72.0333,
        "category": "reserve",
    },
    {
        "name": "Little Rann of Kutch",
        "region_code": "IN-GJ",
        "lat": 23.2500,
        "lon": 71.2500,
        "category": "reserve",
    },
    {
        "name": "Ahmedabad",
        "region_code": "IN-GJ",
        "lat": 23.0225,
        "lon": 72.5714,
        "category": "city",
    },
    # --- India: Ladakh (IN-LA) ---
    {"name": "Leh", "region_code": "IN-LA", "lat": 34.1526, "lon": 77.5771, "category": "city"},
    {
        "name": "Pangong Tso",
        "region_code": "IN-LA",
        "lat": 33.7595,
        "lon": 78.6674,
        "category": "hotspot",
    },
    {
        "name": "Tso Moriri",
        "region_code": "IN-LA",
        "lat": 32.9000,
        "lon": 78.3167,
        "category": "hotspot",
    },
    {
        "name": "Shey Marshes",
        "region_code": "IN-LA",
        "lat": 34.0722,
        "lon": 77.6339,
        "category": "hotspot",
    },
    {
        "name": "Hemis National Park",
        "region_code": "IN-LA",
        "lat": 33.9870,
        "lon": 77.3450,
        "category": "reserve",
    },
    # --- India: Uttarakhand (IN-UT) ---
    {
        "name": "Jim Corbett National Park",
        "region_code": "IN-UT",
        "lat": 29.5300,
        "lon": 78.7747,
        "category": "reserve",
    },
    {
        "name": "Corbett",
        "region_code": "IN-UT",
        "lat": 29.5300,
        "lon": 78.7747,
        "category": "reserve",
    },
    {
        "name": "Pangot",
        "region_code": "IN-UT",
        "lat": 29.4167,
        "lon": 79.4333,
        "category": "hotspot",
    },
    {
        "name": "Dehradun",
        "region_code": "IN-UT",
        "lat": 30.3165,
        "lon": 78.0322,
        "category": "city",
    },
    # --- India: Assam (IN-AS) ---
    {
        "name": "Kaziranga National Park",
        "region_code": "IN-AS",
        "lat": 26.5775,
        "lon": 93.1711,
        "category": "reserve",
    },
    {
        "name": "Manas National Park",
        "region_code": "IN-AS",
        "lat": 26.7167,
        "lon": 91.0167,
        "category": "reserve",
    },
    {
        "name": "Guwahati",
        "region_code": "IN-AS",
        "lat": 26.1445,
        "lon": 91.7362,
        "category": "city",
    },
    # --- India: West Bengal (IN-WB) ---
    {
        "name": "Sundarbans National Park",
        "region_code": "IN-WB",
        "lat": 21.9497,
        "lon": 89.1833,
        "category": "reserve",
    },
    {"name": "Kolkata", "region_code": "IN-WB", "lat": 22.5726, "lon": 88.3639, "category": "city"},
    # --- India: Kerala (IN-KL) ---
    {
        "name": "Thattekad Bird Sanctuary",
        "region_code": "IN-KL",
        "lat": 10.1333,
        "lon": 76.6833,
        "category": "reserve",
    },
    {
        "name": "Periyar National Park",
        "region_code": "IN-KL",
        "lat": 9.4667,
        "lon": 77.2333,
        "category": "reserve",
    },
    {
        "name": "Munnar",
        "region_code": "IN-KL",
        "lat": 10.0889,
        "lon": 77.0595,
        "category": "hotspot",
    },
    {"name": "Kochi", "region_code": "IN-KL", "lat": 9.9312, "lon": 76.2673, "category": "city"},
    # --- India: Tamil Nadu (IN-TN) ---
    {
        "name": "Vedanthangal Bird Sanctuary",
        "region_code": "IN-TN",
        "lat": 12.5444,
        "lon": 79.8550,
        "category": "reserve",
    },
    {
        "name": "Mudumalai National Park",
        "region_code": "IN-TN",
        "lat": 11.5622,
        "lon": 76.5342,
        "category": "reserve",
    },
    {
        "name": "Coonoor",
        "region_code": "IN-TN",
        "lat": 11.3530,
        "lon": 76.7959,
        "category": "hotspot",
    },
    {
        "name": "Nilgiris",
        "region_code": "IN-TN",
        "lat": 11.4102,
        "lon": 76.6950,
        "category": "hotspot",
    },
    {"name": "Ooty", "region_code": "IN-TN", "lat": 11.4102, "lon": 76.6950, "category": "city"},
    {"name": "Chennai", "region_code": "IN-TN", "lat": 13.0827, "lon": 80.2707, "category": "city"},
    # --- India: Maharashtra (IN-MH) ---
    {
        "name": "Tadoba Andhari Tiger Reserve",
        "region_code": "IN-MH",
        "lat": 20.2450,
        "lon": 79.3050,
        "category": "reserve",
    },
    {
        "name": "Sanjay Gandhi National Park",
        "region_code": "IN-MH",
        "lat": 19.2288,
        "lon": 72.9182,
        "category": "reserve",
    },
    {"name": "Mumbai", "region_code": "IN-MH", "lat": 19.0760, "lon": 72.8777, "category": "city"},
    # --- India: Delhi / NCR (IN-DL, IN-HR) ---
    {"name": "Delhi", "region_code": "IN-DL", "lat": 28.6139, "lon": 77.2090, "category": "city"},
    {
        "name": "New Delhi",
        "region_code": "IN-DL",
        "lat": 28.6139,
        "lon": 77.2090,
        "category": "city",
    },
    {
        "name": "Okhla Bird Sanctuary",
        "region_code": "IN-DL",
        "lat": 28.5630,
        "lon": 77.3060,
        "category": "hotspot",
    },
    {
        "name": "Sultanpur National Park",
        "region_code": "IN-HR",
        "lat": 28.4628,
        "lon": 76.8942,
        "category": "reserve",
    },
    # --- United States: Florida (US-FL) ---
    {
        "name": "Everglades",
        "region_code": "US-FL",
        "lat": 25.2866,
        "lon": -80.8987,
        "category": "reserve",
    },
    {
        "name": "Everglades National Park",
        "region_code": "US-FL",
        "lat": 25.2866,
        "lon": -80.8987,
        "category": "reserve",
    },
    {
        "name": "Corkscrew Swamp Sanctuary",
        "region_code": "US-FL",
        "lat": 26.3756,
        "lon": -81.6033,
        "category": "reserve",
    },
    {
        "name": "Dry Tortugas National Park",
        "region_code": "US-FL",
        "lat": 24.6285,
        "lon": -82.8732,
        "category": "reserve",
    },
    {"name": "Miami", "region_code": "US-FL", "lat": 25.7617, "lon": -80.1918, "category": "city"},
    {
        "name": "Orlando",
        "region_code": "US-FL",
        "lat": 28.5383,
        "lon": -81.3792,
        "category": "city",
    },
    # --- United States: New York (US-NY) ---
    {
        "name": "Central Park",
        "region_code": "US-NY",
        "lat": 40.7851,
        "lon": -73.9683,
        "category": "hotspot",
    },
    {
        "name": "Jamaica Bay Wildlife Refuge",
        "region_code": "US-NY",
        "lat": 40.6175,
        "lon": -73.8344,
        "category": "hotspot",
    },
    {
        "name": "New York",
        "region_code": "US-NY",
        "lat": 40.7128,
        "lon": -74.0060,
        "category": "city",
    },
    {
        "name": "New York City",
        "region_code": "US-NY",
        "lat": 40.7128,
        "lon": -74.0060,
        "category": "city",
    },
    # --- United States: California (US-CA) ---
    {
        "name": "Yosemite National Park",
        "region_code": "US-CA",
        "lat": 37.8651,
        "lon": -119.5383,
        "category": "reserve",
    },
    {
        "name": "Point Reyes National Seashore",
        "region_code": "US-CA",
        "lat": 38.0694,
        "lon": -122.8805,
        "category": "reserve",
    },
    {
        "name": "Monterey Bay",
        "region_code": "US-CA",
        "lat": 36.6002,
        "lon": -121.8947,
        "category": "hotspot",
    },
    {
        "name": "San Francisco",
        "region_code": "US-CA",
        "lat": 37.7749,
        "lon": -122.4194,
        "category": "city",
    },
    {
        "name": "Los Angeles",
        "region_code": "US-CA",
        "lat": 34.0522,
        "lon": -118.2437,
        "category": "city",
    },
    # --- United States: Texas (US-TX) ---
    {
        "name": "Santa Ana National Wildlife Refuge",
        "region_code": "US-TX",
        "lat": 26.0792,
        "lon": -98.1383,
        "category": "reserve",
    },
    {
        "name": "High Island",
        "region_code": "US-TX",
        "lat": 29.5667,
        "lon": -94.3833,
        "category": "hotspot",
    },
    {"name": "Austin", "region_code": "US-TX", "lat": 30.2672, "lon": -97.7431, "category": "city"},
    {
        "name": "Houston",
        "region_code": "US-TX",
        "lat": 29.7604,
        "lon": -95.3698,
        "category": "city",
    },
    # --- United States: Arizona (US-AZ) ---
    {
        "name": "Ramsey Canyon Preserve",
        "region_code": "US-AZ",
        "lat": 31.4483,
        "lon": -110.3117,
        "category": "hotspot",
    },
    {
        "name": "Tucson",
        "region_code": "US-AZ",
        "lat": 32.2226,
        "lon": -110.9747,
        "category": "city",
    },
    # --- United States: Alaska (US-AK) ---
    {
        "name": "Denali National Park",
        "region_code": "US-AK",
        "lat": 63.1148,
        "lon": -151.1926,
        "category": "reserve",
    },
    {
        "name": "Anchorage",
        "region_code": "US-AK",
        "lat": 61.2181,
        "lon": -149.9003,
        "category": "city",
    },
    # --- United States: Washington (US-WA) ---
    {
        "name": "Olympic National Park",
        "region_code": "US-WA",
        "lat": 47.8021,
        "lon": -123.6044,
        "category": "reserve",
    },
    {
        "name": "Seattle",
        "region_code": "US-WA",
        "lat": 47.6062,
        "lon": -122.3321,
        "category": "city",
    },
    # --- United States: Colorado (US-CO) ---
    {
        "name": "Rocky Mountain National Park",
        "region_code": "US-CO",
        "lat": 40.3428,
        "lon": -105.6836,
        "category": "reserve",
    },
    {
        "name": "Denver",
        "region_code": "US-CO",
        "lat": 39.7392,
        "lon": -104.9903,
        "category": "city",
    },
    # --- South Africa (ZA) ---
    {"name": "Kruger", "region_code": "ZA", "lat": -23.9884, "lon": 31.5547, "category": "reserve"},
    {
        "name": "Kruger National Park",
        "region_code": "ZA",
        "lat": -23.9884,
        "lon": 31.5547,
        "category": "reserve",
    },
    {"name": "Cape Town", "region_code": "ZA", "lat": -33.9249, "lon": 18.4241, "category": "city"},
    {
        "name": "Johannesburg",
        "region_code": "ZA",
        "lat": -26.2041,
        "lon": 28.0473,
        "category": "city",
    },
    {
        "name": "Kirstenbosch National Botanical Garden",
        "region_code": "ZA",
        "lat": -33.9881,
        "lon": 18.4323,
        "category": "hotspot",
    },
    {
        "name": "Kgalagadi Transfrontier Park",
        "region_code": "ZA",
        "lat": -25.7667,
        "lon": 20.3833,
        "category": "reserve",
    },
    # --- Kenya (KE / KE-01) ---
    {
        "name": "Maasai Mara National Reserve",
        "region_code": "KE",
        "lat": -1.4933,
        "lon": 35.1444,
        "category": "reserve",
    },
    {
        "name": "Lake Nakuru National Park",
        "region_code": "KE",
        "lat": -0.3667,
        "lon": 36.0833,
        "category": "reserve",
    },
    {"name": "Nairobi", "region_code": "KE", "lat": -1.2921, "lon": 36.8219, "category": "city"},
    {
        "name": "Nairobi National Park",
        "region_code": "KE",
        "lat": -1.3733,
        "lon": 36.8589,
        "category": "reserve",
    },
    # --- Tanzania (TZ) ---
    {
        "name": "Serengeti National Park",
        "region_code": "TZ",
        "lat": -2.3333,
        "lon": 34.8333,
        "category": "reserve",
    },
    {
        "name": "Ngorongoro Crater",
        "region_code": "TZ",
        "lat": -3.1667,
        "lon": 35.5833,
        "category": "reserve",
    },
    # --- Botswana (BW) ---
    {
        "name": "Okavango Delta",
        "region_code": "BW",
        "lat": -19.2833,
        "lon": 22.9000,
        "category": "reserve",
    },
    {
        "name": "Chobe National Park",
        "region_code": "BW",
        "lat": -18.6667,
        "lon": 24.5000,
        "category": "reserve",
    },
    # --- Costa Rica (CR) ---
    {
        "name": "Monteverde Cloud Forest",
        "region_code": "CR",
        "lat": 10.3000,
        "lon": -84.8000,
        "category": "reserve",
    },
    {
        "name": "Manuel Antonio National Park",
        "region_code": "CR",
        "lat": 9.3853,
        "lon": -84.1436,
        "category": "reserve",
    },
    {"name": "San Jose", "region_code": "CR", "lat": 9.9281, "lon": -84.0907, "category": "city"},
    # --- Ecuador (EC) ---
    {
        "name": "Galapagos National Park",
        "region_code": "EC",
        "lat": -0.6667,
        "lon": -90.5500,
        "category": "reserve",
    },
    {
        "name": "Mindo Cloud Forest",
        "region_code": "EC",
        "lat": -0.0500,
        "lon": -78.7833,
        "category": "hotspot",
    },
    {"name": "Quito", "region_code": "EC", "lat": -0.1807, "lon": -78.4678, "category": "city"},
    # --- Peru (PE) ---
    {
        "name": "Manu National Park",
        "region_code": "PE",
        "lat": -12.2500,
        "lon": -71.5000,
        "category": "reserve",
    },
    {"name": "Lima", "region_code": "PE", "lat": -12.0464, "lon": -77.0428, "category": "city"},
    # --- Brazil (BR) ---
    {
        "name": "Pantanal",
        "region_code": "BR",
        "lat": -17.5000,
        "lon": -56.5000,
        "category": "reserve",
    },
    {
        "name": "Iguazu Falls",
        "region_code": "BR",
        "lat": -25.6953,
        "lon": -54.4367,
        "category": "reserve",
    },
    # --- United Kingdom (GB) ---
    {
        "name": "RSPB Minsmere",
        "region_code": "GB",
        "lat": 52.2472,
        "lon": 1.6153,
        "category": "reserve",
    },
    {"name": "London", "region_code": "GB", "lat": 51.5074, "lon": -0.1278, "category": "city"},
    # --- Spain (ES) ---
    {
        "name": "Donana National Park",
        "region_code": "ES",
        "lat": 36.9667,
        "lon": -6.4167,
        "category": "reserve",
    },
    {"name": "Madrid", "region_code": "ES", "lat": 40.4168, "lon": -3.7038, "category": "city"},
    # --- France (FR) ---
    {"name": "Camargue", "region_code": "FR", "lat": 43.5333, "lon": 4.6333, "category": "reserve"},
    {"name": "Paris", "region_code": "FR", "lat": 48.8566, "lon": 2.3522, "category": "city"},
    # --- Australia (AU, AU-NT, AU-QLD, AU-NSW, AU-VIC) ---
    {
        "name": "Kakadu National Park",
        "region_code": "AU-NT",
        "lat": -12.8333,
        "lon": 132.8333,
        "category": "reserve",
    },
    {
        "name": "Daintree Rainforest",
        "region_code": "AU-QLD",
        "lat": -16.2833,
        "lon": 145.4167,
        "category": "reserve",
    },
    {
        "name": "Sydney",
        "region_code": "AU-NSW",
        "lat": -33.8688,
        "lon": 151.2093,
        "category": "city",
    },
    {
        "name": "Melbourne",
        "region_code": "AU-VIC",
        "lat": -37.8136,
        "lon": 144.9631,
        "category": "city",
    },
    {
        "name": "Cairns",
        "region_code": "AU-QLD",
        "lat": -16.9186,
        "lon": 145.7781,
        "category": "city",
    },
    # --- New Zealand (NZ) ---
    {
        "name": "New Zealand",
        "region_code": "NZ",
        "lat": -40.9006,
        "lon": 174.8860,
        "category": "hotspot",
    },
    {"name": "Rotorua", "region_code": "NZ", "lat": -38.1368, "lon": 176.2497, "category": "city"},
    {
        "name": "Queenstown",
        "region_code": "NZ",
        "lat": -45.0312,
        "lon": 168.6626,
        "category": "city",
    },
    {
        "name": "Milford Sound",
        "region_code": "NZ",
        "lat": -44.6716,
        "lon": 167.9256,
        "category": "reserve",
    },
    {"name": "Auckland", "region_code": "NZ", "lat": -36.8485, "lon": 174.7633, "category": "city"},
    {
        "name": "Christchurch",
        "region_code": "NZ",
        "lat": -43.5321,
        "lon": 172.6362,
        "category": "city",
    },
    # --- Japan (JP) ---
    {
        "name": "Arasaki Crane Sanctuary",
        "region_code": "JP",
        "lat": 32.1000,
        "lon": 130.1833,
        "category": "hotspot",
    },
    {"name": "Tokyo", "region_code": "JP", "lat": 35.6762, "lon": 139.6503, "category": "city"},
    # --- Thailand (TH) ---
    {
        "name": "Khao Yai National Park",
        "region_code": "TH",
        "lat": 14.4333,
        "lon": 101.3667,
        "category": "reserve",
    },
    {"name": "Bangkok", "region_code": "TH", "lat": 13.7563, "lon": 100.5018, "category": "city"},
]


# -------------------------------------------------------------------------
# Curated Regional Checklists: Region Code -> Confirmed Taxon IDs
# -------------------------------------------------------------------------
# Local birds for Karnataka (IN-KA)
KARNATAKA_TAXON_IDS = [
    "brakit1",  # Brahminy Kite
    "whtkin2",  # White-throated Kingfisher
    "indcor1",  # Indian Cormorant
    "greegr",  # Great Egret
    "graher1",  # Gray Heron
    "purher1",  # Purple Heron
    "litcor1",  # Little Cormorant
    "darter2",  # Oriental Darter
    "bkwsti",  # Black-winged Stilt
    "rewbul",  # Red-whiskered Bulbul
    "revbul",  # Red-vented Bulbul
    "magrob",  # Oriental Magpie-Robin
    "compea",  # Indian Peafowl
    "asikoe2",  # Asian Koel
    "commyn",  # Common Myna
    "junbab2",  # Jungle Babbler
    "ashpri1",  # Ashy Prinia
    "plapri1",  # Plain Prinia
    "pursun3",  # Purple-rumped Sunbird
    "pursun4",  # Purple Sunbird
    "lobsun2",  # Loten's Sunbird
    "blakit1",  # Black Kite
    "shikra1",  # Shikra
    "crseag1",  # Crested Serpent-Eagle
    "spodov",  # Spotted Dove
    "piekin1",  # Pied Kingfisher
    "comkin1",  # Common Kingfisher
    "grnbee3",  # Asian Green Bee-eater
    "whbwat1",  # White-breasted Waterhen
    "spbpel1",  # Spot-billed Pelican
    "paisto1",  # Painted Stork
    "asiope1",  # Asian Openbill
    "blhibi1",  # Black-headed Ibis
    "brwjac1",  # Bronze-winged Jacana
    "phtjac1",  # Pheasant-tailed Jacana
    "rorpar",  # Rose-ringed Parakeet
    "copbar1",  # Coppersmith Barbet
    "indrob1",  # Indian Robin
    "bladro1",  # Black Drongo
    "whbwag1",  # White-browed Wagtail
    "nutman",  # Scaly-breasted Munia
    "indsil",  # Indian Silverbill
    "baywea1",  # Baya Weaver
    "litegr",  # Little Egret
    "categr2",  # Eastern Cattle-Egret
    "inpher1",  # Indian Pond-Heron
    "bcnher",  # Black-crowned Night Heron
    "grecor",  # Great Cormorant
    "oripip1",  # Paddyfield Pipit
    "rewlap1",  # Red-wattled Lapwing
    "wonsto1",  # Asian Woolly-necked Stork
    "bkrfla1",  # Black-rumped Flameback
    "indrol2",  # Indian Roller
    "stbkin1",  # Stork-billed Kingfisher
    "ruftre2",  # Rufous Treepie
    "maghor2",  # Malabar Gray Hornbill
    "mawthr1",  # Malabar Whistling-Thrush
    "scamin3",  # Orange Minivet
    "brfowl1",  # Brown Fish-Owl
    "tibfly3",  # Tickell's Blue Flycatcher
    "yebbul3",  # Yellow-browed Bulbul
    "putbab1",  # Puff-throated Babbler
    "aspfly1",  # Indian Paradise-Flycatcher
    "isbduc1",  # Indian Spot-billed Duck
    "lewduc1",  # Lesser Whistling-Duck
    "grecou1",  # Greater Coucal
    "junmyn1",  # Jungle Myna
    "purswa3",  # Gray-headed Swamphen
]

# Birds for Rajasthan (IN-RJ)
RAJASTHAN_TAXON_IDS = [
    "compea",  # Indian Peafowl
    "paisto1",  # Painted Stork
    "sarcra1",  # Sarus Crane
    "blnsto1",  # Black-necked Stork
    "eurspo1",  # Eurasian Spoonbill
    "bahgoo",  # Bar-headed Goose
    "gragoo",  # Graylag Goose
    "rudshe",  # Ruddy Shelduck
    "shikra1",  # Shikra
    "crseag1",  # Crested Serpent-Eagle
    "indvul1",  # Indian Vulture
    "whrvul1",  # White-rumped Vulture
    "egyvul1",  # Egyptian Vulture
    "commyn",  # Common Myna
    "banmyn1",  # Bank Myna
    "brasta1",  # Brahminy Starling
    "grefla1",  # Greater Flamingo
    "lesfla1",  # Lesser Flamingo
    "indrol2",  # Indian Roller
    "indbus1",  # Great Indian Bustard
    "demcra1",  # Demoiselle Crane
    "brakit1",  # Brahminy Kite
    "blakit1",  # Black Kite
    "whtkin2",  # White-throated Kingfisher
    "piekin1",  # Pied Kingfisher
    "comkin1",  # Common Kingfisher
    "categr2",  # Eastern Cattle-Egret
    "graher1",  # Gray Heron
    "greegr",  # Great Egret
    "bkwsti",  # Black-winged Stilt
    "bcnher",  # Black-crowned Night Heron
    "spodov",  # Spotted Dove
    "rorpar",  # Rose-ringed Parakeet
    "copbar1",  # Coppersmith Barbet
    "bladro1",  # Black Drongo
    "rewlap1",  # Red-wattled Lapwing
    "wonsto1",  # Asian Woolly-necked Stork
    "bkrfla1",  # Black-rumped Flameback
    "ruftre2",  # Rufous Treepie
    "aspfly1",  # Indian Paradise-Flycatcher
    "junbab2",  # Jungle Babbler
    "grecou1",  # Greater Coucal
]

# Comprehensive iNaturalist Place IDs mapped to administrative region codes
INATURALIST_PLACE_IDS: dict[str, int] = {
    # India States & Union Territories
    "IN-LA": 184748,  # Ladakh Union Territory
    "IN-KA": 7043,  # Karnataka
    "IN-RJ": 7215,  # Rajasthan
    "IN-GJ": 6682,  # Gujarat
    "IN-TN": 6842,  # Tamil Nadu
    "IN-KL": 7088,  # Kerala
    "IN-MH": 7143,  # Maharashtra
    "IN-UT": 7323,  # Uttarakhand
    "IN-AS": 7017,  # Assam
    "IN-WB": 7334,  # West Bengal
    "IN-DL": 7035,  # Delhi NCT
    "IN-HR": 7048,  # Haryana
    "IN-HP": 7050,  # Himachal Pradesh
    "IN-JK": 184749,  # Jammu and Kashmir
    "IN-GA": 7040,  # Goa
    "IN-MP": 7142,  # Madhya Pradesh
    "IN-AP": 7016,  # Andhra Pradesh
    "IN-OR": 7183,  # Odisha
    "IN-AR": 7015,  # Arunachal Pradesh
    "IN-SK": 7250,  # Sikkim
    "IN-AN": 7014,  # Andaman and Nicobar Islands
    "IN-ML": 7150,  # Meghalaya
    "IN-MZ": 7160,  # Mizoram
    "IN-NL": 7164,  # Nagaland
    "IN-MN": 7144,  # Manipur
    "IN-TR": 7311,  # Tripura
    "IN-JH": 7084,  # Jharkhand
    "IN-BR": 7027,  # Bihar
    "IN-UP": 7322,  # Uttar Pradesh
    "IN-PB": 7187,  # Punjab
    "IN-CT": 7034,  # Chhattisgarh
    "IN-TS": 77138,  # Telangana
    # United States
    "US-FL": 14,  # Florida
    "US-NY": 48,  # New York
    "US-CA": 14,  # California
    "US-TX": 18,  # Texas
    "US-AZ": 40,  # Arizona
    "US-CO": 34,  # Colorado
    "US-WA": 46,  # Washington
    "US-AK": 6,  # Alaska
    "US-OR": 10,  # Oregon
    "US-NM": 9,  # New Mexico
    "US-MA": 2,  # Massachusetts
    "US-NC": 30,  # North Carolina
    "US-GA": 23,  # Georgia
    "US-IL": 35,  # Illinois
    "US-NJ": 51,  # New Jersey
    "US-PA": 42,  # Pennsylvania
    "US-OH": 31,  # Ohio
    "US-MI": 29,  # Michigan
    "US-VA": 7,  # Virginia
    "US-HI": 11,  # Hawaii
    "US-MT": 17,  # Montana
    "US-WY": 16,  # Wyoming
    "US-UT": 52,  # Utah
    "US-NV": 50,  # Nevada
    "US-ID": 22,  # Idaho
    # Australia
    "AU-QLD": 6681,  # Queensland
    "AU-NSW": 6684,  # New South Wales
    "AU-VIC": 6683,  # Victoria
    "AU-NT": 6680,  # Northern Territory
    "AU-WA": 6682,  # Western Australia
    "AU-SA": 6685,  # South Australia
    "AU-TAS": 6686,  # Tasmania
    "AU-ACT": 6687,  # Australian Capital Territory
    # Global Countries
    "NZ": 6803,  # New Zealand
    "ZA": 6986,  # South Africa
    "KE": 6964,  # Kenya
    "KE-01": 6964,  # Kenya (Nairobi / national)
    "TZ": 6984,  # Tanzania
    "BW": 6985,  # Botswana
    "GB": 6857,  # United Kingdom
    "CR": 7214,  # Costa Rica
    "EC": 7184,  # Ecuador
    "PE": 7186,  # Peru
    "BR": 6878,  # Brazil
    "CO": 6924,  # Colombia
    "ES": 6774,  # Spain
    "FR": 6753,  # France
    "JP": 6737,  # Japan
    "TH": 7147,  # Thailand
    "NA": 7163,  # Namibia
    "UG": 7000,  # Uganda
    "MG": 7009,  # Madagascar
    "IS": 6768,  # Iceland
    "NO": 6754,  # Norway
    "SE": 6756,  # Sweden
    "DE": 6757,  # Germany
    "IT": 6755,  # Italy
    "PT": 6777,  # Portugal
    "GR": 6763,  # Greece
    "MX": 6793,  # Mexico
    "CA": 6712,  # Canada
    "CL": 7182,  # Chile
    "AR": 7180,  # Argentina
    "NP": 6980,  # Nepal
    "BT": 7132,  # Bhutan
    "LK": 7136,  # Sri Lanka
    "ID": 6973,  # Indonesia
    "MY": 7155,  # Malaysia
    "VN": 7181,  # Vietnam
}


# Broad avifauna of the Indian Subcontinent (~240 species)
INDIA_BASE_TAXON_IDS = [
    "paisto1",  # Painted Stork
    "spbpel1",  # Spot-billed Pelican
    "blhibi1",  # Black-headed Ibis
    "asiope1",  # Asian Openbill
    "wonsto1",  # Asian Woolly-necked Stork
    "renibi1",  # Red-naped Ibis
    "gloibi",  # Glossy Ibis
    "eurspo1",  # Eurasian Spoonbill
    "inpher1",  # Indian Pond-Heron
    "categr1",  # Western Cattle-Egret
    "integr1",  # Medium Egret
    "litegr",  # Little Egret
    "greegr",  # Great Egret
    "purher1",  # Purple Heron
    "graher1",  # Gray Heron
    "darter2",  # Oriental Darter
    "litcor1",  # Little Cormorant
    "indcor1",  # Indian Cormorant
    "grecor",  # Great Cormorant
    "bcnher",  # Black-crowned Night Heron
    "whbwat1",  # White-breasted Waterhen
    "commoo3",  # Eurasian Moorhen
    "eurcoo",  # Eurasian Coot
    "brwjac1",  # Bronze-winged Jacana
    "phtjac1",  # Pheasant-tailed Jacana
    "bkwsti",  # Black-winged Stilt
    "pieavo1",  # Pied Avocet
    "rewlap1",  # Red-wattled Lapwing
    "yewlap2",  # Yellow-wattled Lapwing
    "lirplo",  # Little Ringed Plover
    "comsni",  # Common Snipe
    "bktgod",  # Black-tailed Godwit
    "comred1",  # Common Redshank
    "marsan",  # Marsh Sandpiper
    "comgre",  # Common Greenshank
    "grnsan",  # Green Sandpiper
    "woosan",  # Wood Sandpiper
    "comsan",  # Common Sandpiper
    "litsti",  # Little Stint
    "temsti",  # Temminck's Stint
    "ruff",  # Ruff
    "rivter1",  # River Tern
    "whiter2",  # Whiskered Tern
    "gubter2",  # Gull-billed Tern
    "bnhgul1",  # Brown-headed Gull
    "bkhgul",  # Black-headed Gull
    "copgoo1",  # Cotton Pygmy-Goose
    "gargan",  # Garganey
    "norsho",  # Northern Shoveler
    "norpin",  # Northern Pintail
    "gnwtea",  # Green-winged Teal
    "eurwig",  # Eurasian Wigeon
    "isbduc1",  # Indian Spot-billed Duck
    "rudshe",  # Ruddy Shelduck
    "bahgoo",  # Bar-headed Goose
    "comduc3",  # Comb Duck
    "brakit1",  # Brahminy Kite
    "blakit1",  # Black Kite
    "bkskit1",  # Black-winged Kite
    "shikra1",  # Shikra
    "besra1",  # Besra
    "cregos1",  # Crested Goshawk
    "whebuz1",  # White-eyed Buzzard
    "orihob2",  # Oriental Honey-buzzard
    "crseag1",  # Crested Serpent-Eagle
    "y00839",  # Changeable Hawk-Eagle
    "boneag2",  # Bonelli's Eagle
    "booeag1",  # Booted Eagle
    "grseag1",  # Greater Spotted Eagle
    "osprey",  # Osprey
    "perfal",  # Peregrine Falcon
    "eurkes",  # Eurasian Kestrel
    "webowl1",  # Western Barn Owl
    "eabowl1",  # Eastern Barn Owl
    "spoowl1",  # Spotted Owlet
    "roeowl1",  # Rock Eagle-Owl
    "brfowl1",  # Brown Fish-Owl
    "junowl1",  # Jungle Owlet
    "mowowl1",  # Mottled Wood-Owl
    "brwowl1",  # Brown Wood-Owl
    "compea",  # Indian Peafowl
    "gryfra",  # Gray Francolin
    "jubqua1",  # Jungle Bush-Quail
    "redspu1",  # Red Spurfowl
    "paispu1",  # Painted Spurfowl
    "grejun2",  # Gray Junglefowl
    "rocpig",  # Rock Pigeon
    "eucdov",  # Eurasian Collared-Dove
    "spodov",  # Spotted Dove
    "laudov1",  # Laughing Dove
    "yefpig1",  # Yellow-footed Green-Pigeon
    "niwpig1",  # Nilgiri Wood-Pigeon
    "emedov2",  # Asian Emerald Dove
    "asikoe2",  # Asian Koel
    "grecou1",  # Greater Coucal
    "sirmal1",  # Sirkeer Malkoha
    "blfmal1",  # Blue-faced Malkoha
    "piecuc1",  # Pied Cuckoo
    "cohcuc1",  # Common Hawk-Cuckoo
    "indcuc1",  # Indian Cuckoo
    "babcuc2",  # Banded Bay Cuckoo
    "placuc3",  # Gray-bellied Cuckoo
    "aspswi1",  # Asian Palm Swift
    "y00960",  # Little Swift
    "alpswi1",  # Alpine Swift
    "cretre1",  # Crested Treeswift
    "indnig1",  # Indian Nightjar
    "jernig1",  # Jerdon's Nightjar
    "indrol2",  # Indian Roller
    "comkin1",  # Common Kingfisher
    "whtkin2",  # White-throated Kingfisher
    "piekin1",  # Pied Kingfisher
    "stbkin1",  # Stork-billed Kingfisher
    "blckin1",  # Black-capped Kingfisher
    "bkbkin1",  # Black-backed Dwarf-Kingfisher
    "grnbee3",  # Asian Green Bee-eater
    "btbeat1",  # Blue-tailed Bee-eater
    "chbeat1",  # Chestnut-headed Bee-eater
    "hoopoe",  # Common Hoopoe
    "maphor1",  # Malabar Pied-Hornbill
    "inghor2",  # Indian Gray Hornbill
    "grehor1",  # Great Hornbill
    "maghor2",  # Malabar Gray Hornbill
    "copbar1",  # Coppersmith Barbet
    "brhbar1",  # Brown-headed Barbet
    "whcbar1",  # White-cheeked Barbet
    "crfbar1",  # Malabar Barbet
    "bkrfla1",  # Black-rumped Flameback
    "grefla1",  # Greater Flameback
    "comfla1",  # Common Flameback
    "whnwoo1",  # White-naped Woodpecker
    "heswoo1",  # Heart-spotted Woodpecker
    "bncwoo3",  # Brown-capped Pygmy Woodpecker
    "yecwoo1",  # Yellow-crowned Woodpecker
    "rufwoo2",  # Rufous Woodpecker
    "indpit1",  # Indian Pitta
    "ingori1",  # Indian Golden Oriole
    "blhori1",  # Black-hooded Oriole
    "bladro1",  # Black Drongo
    "ashdro1",  # Ashy Drongo
    "whbdro1",  # White-bellied Drongo
    "brodro1",  # Bronzed Drongo
    "grtdro1",  # Greater Racket-tailed Drongo
    "aspfly1",  # Indian Paradise-Flycatcher
    "blnmon1",  # Black-naped Monarch
    "comwoo1",  # Common Woodshrike
    "bwfshr1",  # Bar-winged Flycatcher-shrike
    "comior1",  # Common Iora
    "larcuc1",  # Indian Cuckooshrike
    "bkhcus1",  # Black-headed Cuckooshrike
    "smamin1",  # Small Minivet
    "scamin1",  # Scarlet Minivet
    "goflea1",  # Golden-fronted Leafbird
    "jerlea1",  # Jerdon's Leafbird
    "brnshr",  # Brown Shrike
    "lotshr1",  # Long-tailed Shrike
    "babshr1",  # Bay-backed Shrike
    "ruftre2",  # Rufous Treepie
    "whbtre1",  # White-bellied Treepie
    "houcro1",  # House Crow
    "labcro1",  # Large-billed Crow
    "whbfan2",  # White-browed Fantail
    "spbfan1",  # Spot-breasted Fantail
    "tibfly3",  # Tickell's Blue Flycatcher
    "verfly4",  # Verditer Flycatcher
    "nilfly2",  # Nilgiri Flycatcher
    "barfly1",  # Black-and-orange Flycatcher
    "wbbfly1",  # White-bellied Blue Flycatcher
    "rutfly6",  # Rusty-tailed Flycatcher
    "asbfly",  # Asian Brown Flycatcher
    "magrob",  # Oriental Magpie-Robin
    "whrsha",  # White-rumped Shama
    "indrob1",  # Indian Robin
    "blared1",  # Black Redstart
    "piebus1",  # Pied Bushchat
    "sibsto1",  # Siberian Stonechat
    "mawthr1",  # Malabar Whistling-Thrush
    "orhthr1",  # Orange-headed Thrush
    "eurbla2",  # Indian Blackbird
    "scathr4",  # Nilgiri Thrush
    "commyn",  # Common Myna
    "junmyn1",  # Jungle Myna
    "banmyn1",  # Bank Myna
    "brasta1",  # Brahminy Starling
    "rossta2",  # Rosy Starling
    "aspsta2",  # Indian Pied Starling
    "malsta1",  # Malabar Starling
    "vefnut1",  # Velvet-fronted Nuthatch
    "cintit13",  # Asian Tit
    "indtit1",  # Indian Yellow Tit
    "whbbul2",  # White-browed Bulbul
    "revbul",  # Red-vented Bulbul
    "rewbul",  # Red-whiskered Bulbul
    "yetbul1",  # Yellow-throated Bulbul
    "sqtbul1",  # Square-tailed Bulbul
    "gyhbul1",  # Gray-headed Bulbul
    "bkcbul1",  # Flame-throated Bulbul
    "yebbul3",  # Yellow-browed Bulbul
    "ashpri1",  # Ashy Prinia
    "plapri1",  # Plain Prinia
    "junpri1",  # Jungle Prinia
    "comtai1",  # Common Tailorbird
    "blrwar1",  # Blyth's Reed Warbler
    "clrwar1",  # Clamorous Reed Warbler
    "boowar1",  # Booted Warbler
    "sykwar2",  # Sykes's Warbler
    "grewar3",  # Greenish Warbler
    "zitcis1",  # Zitting Cisticola
    "junbab2",  # Jungle Babbler
    "yebbab1",  # Yellow-billed Babbler
    "rufbab3",  # Rufous Babbler
    "putbab1",  # Puff-throated Babbler
    "insbab1",  # Indian Scimitar-Babbler
    "tabbab1",  # Tawny-bellied Babbler
    "dafbab1",  # Dark-fronted Babbler
    "wynlau1",  # Wayanad Laughingthrush
    "bkclau2",  # Nilgiri Laughingthrush
    "kerlau2",  # Palani Laughingthrush
    "indwhe1",  # Indian White-eye
    "thbflo1",  # Thick-billed Flowerpecker
    "pabflo1",  # Pale-billed Flowerpecker
    "plaflo1",  # Nilgiri Flowerpecker
    "pursun4",  # Purple Sunbird
    "pursun3",  # Purple-rumped Sunbird
    "lobsun2",  # Loten's Sunbird
    "crbsun2",  # Crimson-backed Sunbird
    "wecsun1",  # Vigors's Sunbird
    "houspa",  # House Sparrow
    "chspet1",  # Yellow-throated Sparrow
    "baywea1",  # Baya Weaver
    "strwea2",  # Streaked Weaver
    "redava",  # Red Avadavat
    "whrmun",  # White-rumped Munia
    "nutman",  # Scaly-breasted Munia
    "trimun",  # Tricolored Munia
    "indsil",  # Indian Silverbill
    "forwag1",  # Forest Wagtail
    "eaywag1",  # Western Yellow Wagtail
    "citwag",  # Citrine Wagtail
    "grywag",  # Gray Wagtail
    "whiwag",  # White Wagtail
    "whbwag1",  # White-browed Wagtail
    "oripip1",  # Paddyfield Pipit
    "blypip1",  # Blyth's Pipit
    "trepip",  # Tree Pipit
    "olbpip",  # Olive-backed Pipit
    "comros",  # Common Rosefinch
]

# Arid and wetland species characteristic of Thar / Ranthambore / Keoladeo
RAJASTHAN_SPECIFIC_TAXON_IDS = [
    "indbus1",  # Great Indian Bustard
    "macbus1",  # Asian Houbara
    "demcra1",  # Demoiselle Crane
    "comcra",  # Common Crane
    "sarcra1",  # Sarus Crane
    "indcou1",  # Indian Courser
    "crccou1",  # Cream-colored Courser
    "chbsan",  # Chestnut-bellied Sandgrouse
    "sposan1",  # Spotted Sandgrouse
    "paisan1",  # Painted Sandgrouse
    "blbsan1",  # Black-bellied Sandgrouse
    "deslar1",  # Desert Lark
    "bimlar1",  # Bimaculated Lark
    "grhlar1",  # Greater Hoopoe-Lark
    "sinbus6",  # Singing Bushlark
    "indbus2",  # Indian Bushlark
    "rutscr1",  # Rufous-tailed Scrub-Robin
    "deswhe1",  # Desert Wheatear
    "isawhe1",  # Isabelline Wheatear
    "varwhe1",  # Variable Wheatear
    "retwhe2",  # Persian Wheatear
    "eutkne1",  # Eurasian Thick-knee
    "grtkne1",  # Great Thick-knee
    "taweag1",  # Tawny Eagle
    "steeag1",  # Steppe Eagle
    "impeag1",  # Eastern Imperial Eagle
    "shteag1",  # Short-toed Snake-Eagle
    "lolbuz1",  # Long-legged Buzzard
    "renfal1",  # Red-necked Falcon
    "lagfal1",  # Laggar Falcon
    "palhar1",  # Pallid Harrier
    "monhar1",  # Montagu's Harrier
    "wemhar1",  # Western Marsh Harrier
    "whrvul1",  # White-rumped Vulture
    "indvul1",  # Indian Vulture
    "rehvul1",  # Red-headed Vulture
    "egyvul1",  # Egyptian Vulture
    "cinvul1",  # Cinereous Vulture
    "gragoo",  # Graylag Goose
    "comshe",  # Common Shelduck
    "gadwal",  # Gadwall
    "compoc",  # Common Pochard
    "ferduc",  # Ferruginous Duck
    "tufduc",  # Tufted Duck
    "grefla3",  # Greater Flamingo
    "lesfla1",  # Lesser Flamingo
    "grwpel1",  # Great White Pelican
    "dalpel1",  # Dalmatian Pelican
    "blnsto1",  # Black-necked Stork
    "isashr1",  # Isabelline Shrike
    "norshr1",  # Great Gray Shrike
    "whebul1",  # White-eared Bulbul
    "combab3",  # Common Babbler
    "lagbab2",  # Large Gray Babbler
    "yeebab1",  # Yellow-eyed Babbler
    "strbab1",  # Striated Babbler
    "spaspa1",  # Spanish Sparrow
    "leswhi4",  # Lesser Whitethroat
    "asdwar1",  # Asian Desert Warbler
]

# High-altitude Tibetan plateau and Trans-Himalayan specialists
LADAKH_SPECIFIC_TAXON_IDS = [
    "blared1",  # Black Redstart
    "blueth",  # Bluethroat
    "chukar",  # Chukar
    "himrub1",  # Himalayan Rubythroat
    "bahgoo",  # Bar-headed Goose
    "bnhgul1",  # Brown-headed Gull
    "rudshe",  # Ruddy Shelduck
    "blncra1",  # Black-necked Crane
    "tibsno1",  # Tibetan Snowcock
    "himsno",  # Himalayan Snowcock
    "tibpar1",  # Tibetan Partridge
    "tibsan1",  # Tibetan Sandgrouse
    "ibisbi1",  # Ibisbill
    "whwred2",  # White-winged Redstart
    "robacc1",  # Robin Accentor
    "broacc1",  # Brown Accentor
    "goleag",  # Golden Eagle
    "lammer1",  # Bearded Vulture
    "rebcho1",  # Red-billed Chough
    "yebcho1",  # Yellow-billed Chough
    "horlar",  # Horned Lark
    "greros1",  # Great Rosefinch
    "comrav",  # Common Raven
    "eurmag1",  # Eurasian Magpie
    "norpin",  # Northern Pintail
    "commer",  # Common Merganser
    "gargan",  # Garganey
    "tufduc",  # Tufted Duck
    "compoc",  # Common Pochard
    "comgre",  # Common Greenshank
    "woosan",  # Wood Sandpiper
    "comsan",  # Common Sandpiper
    "citwag",  # Citrine Wagtail
    "whiwag",  # White Wagtail
    "deswhe1",  # Desert Wheatear
    "wallcr1",  # Wallcreeper
    "himgri1",  # Himalayan Griffon
    "uplbuz1",  # Upland Buzzard
    "bhmfin1",  # Black-headed Mountain Finch
    "plmfin1",  # Plain Mountain Finch
    "monfin2",  # Mongolian Finch
    "fifser1",  # Fire-fronted Serin
    "strros1",  # Streaked Rosefinch
    "alpacc1",  # Alpine Accentor
    "y00989",  # Tickell's Leaf Warbler
    "subwar2",  # Sulphur-bellied Warbler
    "humwar1",  # Hume's Leaf Warbler
]

# Comprehensive avifauna of New Zealand (forest endemics, seabirds, penguins, waders)
NEW_ZEALAND_TAXON_IDS = [
    "sobkiw1",  # Southern Brown Kiwi
    "liskiw1",  # Little Spotted Kiwi
    "grskiw1",  # Great Spotted Kiwi
    "kea1",  # Kea
    "nezkak1",  # New Zealand Kaka
    "kakapo2",  # Kakapo
    "refpar4",  # Red-crowned Kakariki
    "yefpar3",  # Yellow-crowned Kakariki
    "takahe3",  # South Island Takahe
    "weka1",  # Weka
    "purswa6",  # Australasian Swamphen
    "bubrai1",  # Buff-banded Rail
    "spocra2",  # Spotless Crake
    "soioys1",  # South Island Oystercatcher
    "varoys1",  # Variable Oystercatcher
    "piesti1",  # Pied Stilt
    "blasti1",  # Black Stilt
    "wrybil1",  # Wrybill
    "pagplo",  # Pacific Golden-Plover
    "dobplo1",  # Double-banded Plover
    "batgod",  # Bar-tailed Godwit
    "redkno",  # Red Knot
    "bluduc1",  # Blue Duck
    "parshe1",  # Paradise Shelduck
    "nezsca1",  # New Zealand Scaup
    "brotea1",  # Brown Teal
    "pabduc1",  # Pacific Black Duck
    "mallar3",  # Mallard
    "blkswa",  # Black Swan
    "aussho1",  # Australasian Shoveler
    "yeepen1",  # Yellow-eyed Penguin
    "fiopen1",  # Fiordland Penguin
    "litpen1",  # Little Penguin
    "ausgan1",  # Australasian Gannet
    "piecor1",  # Pied Cormorant
    "lipcor1",  # Little Pied Cormorant
    "grecor",  # Great Cormorant
    "libcor1",  # Little Black Cormorant
    "sposha1",  # Spotted Shag
    "royalb2",  # Southern Royal Albatross
    "royalb3",  # Northern Royal Albatross
    "wanalb1",  # Snowy Albatross
    "bulalb2",  # Buller's Albatross
    "whcalb1",  # White-capped Albatross
    "salalb1",  # Salvin's Albatross
    "faipri1",  # Fairy Prion
    "sooshe",  # Sooty Shearwater
    "flushe1",  # Fluttering Shearwater
    "wespet1",  # Westland Petrel
    "parpet1",  # Parkinson's Petrel
    "coopet",  # Cook's Petrel
    "whfter1",  # White-fronted Tern
    "blfter1",  # Black-fronted Tern
    "caster1",  # Caspian Tern
    "kelgul",  # Kelp Gull
    "blbgul1",  # Black-billed Gull
    "swahar1",  # Swamp Harrier
    "nezfal1",  # New Zealand Falcon
    "morepo2",  # Morepork
    "nezpig2",  # New Zealand Pigeon
    "nezbel1",  # New Zealand Bellbird
    "tui1",  # Tui
    "nezfan1",  # New Zealand Fantail
    "tomtit1",  # Tomtit
    "nezrob3",  # South Island Robin
    "nezrob2",  # North Island Robin
    "whiteh1",  # Whitehead
    "yellow3",  # Yellowhead
    "pipipi1",  # Pipipi
    "saddle2",  # North Island Saddleback
    "saddle3",  # South Island Saddleback
    "stitch1",  # Stitchbird
    "riflem1",  # Rifleman
    "gryger1",  # Gray Gerygone
    "silver3",  # Silvereye
    "welswa1",  # Welcome Swallow
    "auspip3",  # New Zealand Pipit
    "skylar",  # Eurasian Skylark
    "sonthr1",  # Song Thrush
    "eurbla",  # Eurasian Blackbird
    "eurgre1",  # European Greenfinch
    "eurgol",  # European Goldfinch
    "redpol1",  # Redpoll
    "yellow2",  # Yellowhammer
]

# Avifauna of the United Kingdom and Western Europe
UK_EUROPE_TAXON_IDS = [
    "mallar3",  # Mallard
    "grecor",  # Great Cormorant
    "eurmag1",  # Eurasian Magpie
    "eursta",  # European Starling
    "houspa",  # House Sparrow
    "comrav",  # Common Raven
    "eurrob1",  # European Robin
    "sonthr1",  # Song Thrush
    "blutit",  # Eurasian Blue Tit
    "gretit1",  # Great Tit
    "comcha",  # Common Chaffinch
    "comchi1",  # Common Chiffchaff
    "eurbla",  # Eurasian Blackbird
    "comkin1",  # Common Kingfisher
    "graher1",  # Gray Heron
    "osprey",  # Osprey
    "perfal",  # Peregrine Falcon
    "bahgoo",  # Bar-headed Goose
    "eurjay1",  # Eurasian Jay
    "eurgre1",  # European Greenfinch
    "eurgol",  # European Goldfinch
    "eurbul",  # Eurasian Bullfinch
    "dunnoc1",  # Dunnock
    "winwre4",  # Eurasian Wren
    "misthr1",  # Mistle Thrush
    "redwin",  # Redwing
    "fieldf",  # Fieldfare
    "goldcr1",  # Goldcrest
    "firecr1",  # Common Firecrest
    "coatit2",  # Coal Tit
    "martit2",  # Marsh Tit
    "lottit1",  # Long-tailed Tit
    "eurnut2",  # Eurasian Nuthatch
    "eurtre1",  # Eurasian Treecreeper
    "eurkes",  # Eurasian Kestrel
    "eurspa1",  # Eurasian Sparrowhawk
    "combuz1",  # Common Buzzard
    "redkit1",  # Red Kite
    "bargoo",  # Barnacle Goose
    "brant",  # Brant
    "mutswa",  # Mute Swan
    "whoswa",  # Whooper Swan
    "comshe",  # Common Shelduck
    "eurwig",  # Eurasian Wigeon
    "gadwal",  # Gadwall
    "gnwtea",  # Green-winged Teal
    "norpin",  # Northern Pintail
    "norsho",  # Northern Shoveler
    "tufduc",  # Tufted Duck
    "comgol",  # Common Goldeneye
    "commer",  # Common Merganser
    "rebmer",  # Red-breasted Merganser
    "litgre1",  # Little Grebe
    "grcgre1",  # Great Crested Grebe
    "watrai1",  # Water Rail
    "commoo3",  # Eurasian Moorhen
    "eurcoo",  # Eurasian Coot
    "euroys1",  # Eurasian Oystercatcher
    "pieavo1",  # Pied Avocet
    "norlap",  # Northern Lapwing
    "eugplo",  # European Golden-Plover
    "corplo",  # Common Ringed Plover
    "eurcur",  # Eurasian Curlew
    "batgod",  # Bar-tailed Godwit
    "bktgod",  # Black-tailed Godwit
    "rudtur",  # Ruddy Turnstone
    "redkno",  # Red Knot
    "sander",  # Sanderling
    "dunlin",  # Dunlin
    "comsni",  # Common Snipe
    "comred1",  # Common Redshank
    "comgre",  # Common Greenshank
    "bkhgul",  # Black-headed Gull
    "mewgul",  # Common Gull
    "euhgul1",  # European Herring Gull
    "lbbgul",  # Lesser Black-backed Gull
    "gbbgul",  # Great Black-backed Gull
    "santer1",  # Sandwich Tern
    "comter",  # Common Tern
]

# Avifauna of Florida, Everglades, and Atlantic coastal wetlands
FLORIDA_TAXON_IDS = [
    "baleag",  # Bald Eagle
    "osprey",  # Osprey
    "snakit",  # Snail Kite
    "swtkit",  # Swallow-tailed Kite
    "reshaw",  # Red-shouldered Hawk
    "brdowl",  # Barred Owl
    "pilwoo",  # Pileated Woodpecker
    "rebwoo",  # Red-bellied Woodpecker
    "anhing",  # Anhinga
    "doccor",  # Double-crested Cormorant
    "brnpel",  # Brown Pelican
    "amwpel",  # American White Pelican
    "grbher3",  # Great Blue Heron
    "greegr",  # Great Egret
    "snoegr",  # Snowy Egret
    "libher",  # Little Blue Heron
    "triher",  # Tricolored Heron
    "redegr",  # Reddish Egret
    "categr1",  # Western Cattle-Egret
    "grnher",  # Green Heron
    "bcnher",  # Black-crowned Night Heron
    "ycnher",  # Yellow-crowned Night Heron
    "whiibi",  # White Ibis
    "gloibi",  # Glossy Ibis
    "rosspo1",  # Roseate Spoonbill
    "woosto",  # Wood Stork
    "limpki",  # Limpkin
    "purgal2",  # Purple Gallinule
    "comgal1",  # Common Gallinule
    "blkski",  # Black Skimmer
    "laugul",  # Laughing Gull
    "royter1",  # Royal Tern
    "flsjay",  # Florida Scrub-Jay
    "blujay",  # Blue Jay
    "normoc",  # Northern Mockingbird
    "botgra",  # Boat-tailed Grackle
    "comgra",  # Common Grackle
    "rewbla",  # Red-winged Blackbird
    "carwre",  # Carolina Wren
    "norcar",  # Northern Cardinal
    "paibun",  # Painted Bunting
    "coohaw",  # Cooper's Hawk
    "perfal",  # Peregrine Falcon
    "belkin1",  # Belted Kingfisher
    "dowwoo",  # Downy Woodpecker
    "easpho",  # Eastern Phoebe
    "grcfly",  # Great Crested Flycatcher
    "whevir",  # White-eyed Vireo
    "buhvir",  # Blue-headed Vireo
    "treswa",  # Tree Swallow
    "buggna",  # Blue-gray Gnatcatcher
    "grycat",  # Gray Catbird
    "brnthr",  # Brown Thrasher
    "cedwax",  # Cedar Waxwing
    "palwar",  # Palm Warbler
    "yerwar",  # Yellow-rumped Warbler
    "pinwar",  # Pine Warbler
    "bawwar",  # Black-and-white Warbler
    "comyel",  # Common Yellowthroat
    "norpar",  # Northern Parula
    "sumtan",  # Summer Tanager
]

# Avifauna of New York, Northeast Woodlands, and migratory flyways
NEW_YORK_TAXON_IDS = [
    "rethaw",  # Red-tailed Hawk
    "coohaw",  # Cooper's Hawk
    "perfal",  # Peregrine Falcon
    "grbher3",  # Great Blue Heron
    "bcnher",  # Black-crowned Night Heron
    "mallar3",  # Mallard
    "wooduc",  # Wood Duck
    "cangoo",  # Canada Goose
    "norcar",  # Northern Cardinal
    "blujay",  # Blue Jay
    "amerob",  # American Robin
    "sonspa",  # Song Sparrow
    "whtspa",  # White-throated Sparrow
    "daejun",  # Dark-eyed Junco
    "bkcchi",  # Black-capped Chickadee
    "tuftit",  # Tufted Titmouse
    "whbnut",  # White-breasted Nuthatch
    "dowwoo",  # Downy Woodpecker
    "rebwoo",  # Red-bellied Woodpecker
    "eursta",  # European Starling
    "houspa",  # House Sparrow
    "moudov",  # Mourning Dove
    "amegfi",  # American Goldfinch
    "yelwar1",  # Northern Yellow Warbler
    "btbwar",  # Black-throated Blue Warbler
    "amered",  # American Redstart
    "comyel",  # Common Yellowthroat
    "osprey",  # Osprey
    "baleag",  # Bald Eagle
    "shshaw",  # Sharp-shinned Hawk
    "brwhaw",  # Broad-winged Hawk
    "easowl1",  # Eastern Screech-Owl
    "grhowl",  # Great Horned Owl
    "belkin1",  # Belted Kingfisher
    "haiwoo",  # Hairy Woodpecker
    "norfli",  # Northern Flicker
    "easpho",  # Eastern Phoebe
    "grcfly",  # Great Crested Flycatcher
    "easkin",  # Eastern Kingbird
    "reevir1",  # Red-eyed Vireo
    "eawvir1",  # Eastern Warbling Vireo
    "barswa",  # Barn Swallow
    "houwre",  # Northern House Wren
    "woothr",  # Wood Thrush
    "herthr",  # Hermit Thrush
    "grycat",  # Gray Catbird
    "cedwax",  # Cedar Waxwing
    "ovenbi1",  # Ovenbird
    "chswar",  # Chestnut-sided Warbler
    "magwar",  # Magnolia Warbler
    "bkbwar",  # Blackburnian Warbler
    "btnwar",  # Black-throated Green Warbler
    "scatan",  # Scarlet Tanager
    "robgro",  # Rose-breasted Grosbeak
    "indbun",  # Indigo Bunting
    "eastow",  # Eastern Towhee
    "chispa",  # Chipping Sparrow
    "balori",  # Baltimore Oriole
]

# Avifauna of California, Pacific coast, and western woodlands
CALIFORNIA_TAXON_IDS = [
    "calqua",  # California Quail
    "annhum",  # Anna's Hummingbird
    "allhum",  # Allen's Hummingbird
    "acowoo",  # Acorn Woodpecker
    "nutwoo",  # Nuttall's Woodpecker
    "cowscj1",  # California Scrub-Jay
    "yebmag",  # Yellow-billed Magpie
    "oaktit",  # Oak Titmouse
    "wrenti",  # Wrentit
    "calthr",  # California Thrasher
    "caltow",  # California Towhee
    "spotow",  # Spotted Towhee
    "whcspa",  # White-crowned Sparrow
    "gocspa",  # Golden-crowned Sparrow
    "tribla",  # Tricolored Blackbird
    "wesblu",  # Western Bluebird
    "brebla",  # Brewer's Blackbird
    "bushti",  # Bushtit
    "bewwre",  # Bewick's Wren
    "blkpho",  # Black Phoebe
    "saypho",  # Say's Phoebe
    "weskin",  # Western Kingbird
    "caskin",  # Cassin's Kingbird
    "hutvir",  # Hutton's Vireo
    "chbchi",  # Chestnut-backed Chickadee
    "pygnut",  # Pygmy Nuthatch
    "rethaw",  # Red-tailed Hawk
    "reshaw",  # Red-shouldered Hawk
    "coohaw",  # Cooper's Hawk
    "ferhaw",  # Ferruginous Hawk
    "goleag",  # Golden Eagle
    "amekes",  # American Kestrel
    "perfal",  # Peregrine Falcon
    "whtkit",  # White-tailed Kite
    "clagre",  # Clark's Grebe
    "wesgre",  # Western Grebe
    "pibgre",  # Pied-billed Grebe
    "bracor",  # Brandt's Cormorant
    "pelcor",  # Pelagic Cormorant
    "doccor",  # Double-crested Cormorant
    "brnpel",  # Brown Pelican
    "heegul",  # Heermann's Gull
    "wesgul",  # Western Gull
    "calgul",  # California Gull
    "caster1",  # Caspian Tern
    "eleter1",  # Elegant Tern
]

# Avifauna of South Africa and Botswana (Kruger, Cape, Savannas)
SOUTH_AFRICA_TAXON_IDS = [
    "ostric2",  # Common Ostrich
    "jacpen1",  # African Penguin
    "capgan1",  # Cape Gannet
    "capcor1",  # Cape Cormorant
    "bancor1",  # Bank Cormorant
    "affeag1",  # African Fish-Eagle
    "mareag1",  # Martial Eagle
    "batele1",  # Bateleur
    "taweag1",  # Tawny Eagle
    "vereag1",  # Verreaux's Eagle
    "secret2",  # Secretarybird
    "korbus1",  # Kori Bustard
    "helgui",  # Helmeted Guineafowl
    "marsto1",  # Marabou Stork
    "sabsto1",  # Saddle-billed Stork
    "yebsto1",  # Yellow-billed Stork
    "hamerk1",  # Hamerkop
    "hadibi1",  # Hadada Ibis
    "sacibi2",  # African Sacred Ibis
    "egygoo",  # Egyptian Goose
    "spwgoo1",  # Spur-winged Goose
    "librol2",  # Lilac-breasted Roller
    "sybhor1",  # Southern Yellow-billed Hornbill
    "srbhor1",  # Southern Red-billed Hornbill
    "reboxp1",  # Red-billed Oxpecker
    "yeboxp1",  # Yellow-billed Oxpecker
    "carcha1",  # Cape Robin-Chat
    "capwag1",  # Cape Wagtail
    "afpwag1",  # African Pied Wagtail
    "capsug1",  # Cape Sugarbird
    "orbsun2",  # Orange-breasted Sunbird
    "malsun1",  # Malachite Sunbird
    "sdcsun3",  # Southern Double-collared Sunbird
    "capwea1",  # Cape Weaver
    "afmwea",  # Southern Masked-Weaver
    "rebque1",  # Red-billed Quelea
    "blucra2",  # Blue Crane
    "grccra1",  # Gray Crowned-Crane
    "piekin1",  # Pied Kingfisher
    "giakin3",  # Giant Kingfisher
    "malkin1",  # Malachite Kingfisher
    "wookin1",  # Woodland Kingfisher
    "knytur1",  # Knysna Turaco
    "puctur2",  # Purple-crested Turaco
    "whbvul1",  # White-backed Vulture
    "lafvul1",  # Lappet-faced Vulture
    "capgri1",  # Cape Griffon
    "bkskit1",  # Black-winged Kite
    "jacbuz1",  # Jackal Buzzard
    "blacra1",  # Black Crake
    "afrjac1",  # African Jacana
    "blaplo1",  # Blacksmith Lapwing
    "crolap1",  # Crowned Lapwing
    "hoopoe",  # Common Hoopoe
    "grewoo2",  # Green Woodhoopoe
    "soghor1",  # Southern Ground-Hornbill
    "fotdro5",  # Fork-tailed Drongo
    "piecro1",  # Pied Crow
    "capcro2",  # Cape Crow
    "sobfly1",  # Southern Black-Flycatcher
    "fisfly1",  # Fiscal Flycatcher
    "soufis1",  # Southern Fiscal
    "bokmak1",  # Bokmakierie
    "capwhe2",  # Cape White-eye
    "capbul1",  # Cape Bulbul
    "combul2",  # Common Bulbul
]

# Avifauna of East Africa (Kenya, Tanzania, Mara, Serengeti, Rift Valley)
EAST_AFRICA_TAXON_IDS = [
    "ostric2",  # Common Ostrich
    "ostric3",  # Somali Ostrich
    "lesfla1",  # Lesser Flamingo
    "grefla3",  # Greater Flamingo
    "grwpel1",  # Great White Pelican
    "pibpel1",  # Pink-backed Pelican
    "grccra1",  # Gray Crowned-Crane
    "affeag1",  # African Fish-Eagle
    "mareag1",  # Martial Eagle
    "batele1",  # Bateleur
    "taweag1",  # Tawny Eagle
    "lafvul1",  # Lappet-faced Vulture
    "ruegri1",  # Rüppell's Griffon
    "whbvul1",  # White-backed Vulture
    "secret2",  # Secretarybird
    "korbus1",  # Kori Bustard
    "helgui",  # Helmeted Guineafowl
    "vulgui1",  # Vulturine Guineafowl
    "marsto1",  # Marabou Stork
    "sabsto1",  # Saddle-billed Stork
    "yebsto1",  # Yellow-billed Stork
    "hamerk1",  # Hamerkop
    "hadibi1",  # Hadada Ibis
    "sacibi2",  # African Sacred Ibis
    "egygoo",  # Egyptian Goose
    "spwgoo1",  # Spur-winged Goose
    "librol2",  # Lilac-breasted Roller
    "supsta1",  # Superb Starling
    "hilsta1",  # Hildebrandt's Starling
    "reboxp1",  # Red-billed Oxpecker
    "yeboxp1",  # Yellow-billed Oxpecker
    "afpwag1",  # African Pied Wagtail
    "soghor1",  # Southern Ground-Hornbill
    "vddhor1",  # Von der Decken's Hornbill
    "rebhor1",  # Northern Red-billed Hornbill
    "wbgbir1",  # White-bellied Go-away-bird
    "bfgbir1",  # Bare-faced Go-away-bird
    "hartur1",  # Hartlaub's Turaco
    "piekin1",  # Pied Kingfisher
    "giakin3",  # Giant Kingfisher
    "malkin1",  # Malachite Kingfisher
    "wookin1",  # Woodland Kingfisher
    "afpkin1",  # African Pygmy Kingfisher
    "libeat1",  # Little Bee-eater
    "wfbeat1",  # White-fronted Bee-eater
    "ncbeat1",  # Northern Carmine Bee-eater
    "raybar1",  # Red-and-yellow Barbet
    "darbar1",  # D'Arnaud's Barbet
    "blaplo1",  # Blacksmith Lapwing
    "crolap1",  # Crowned Lapwing
    "afrjac1",  # African Jacana
    "blacra1",  # Black Crake
    "fotdro5",  # Fork-tailed Drongo
    "ruegls1",  # Rüppell's Starling
    "watsta1",  # Wattled Starling
    "gobsta5",  # Golden-breasted Starling
    "whbwea1",  # White-headed Buffalo-Weaver
    "rbbwea1",  # Red-billed Buffalo-Weaver
    "spewea2",  # Speke's Weaver
    "bagwea1",  # Baglafecht Weaver
]

# Avifauna of Australia (Queensland, New South Wales, Victoria, Northern Territory)
AUSTRALIA_TAXON_IDS = [
    "emu1",  # Emu
    "ausmag2",  # Australian Magpie
    "laukoo1",  # Laughing Kookaburra
    "railor5",  # Rainbow Lorikeet
    "galah",  # Galah
    "succoc",  # Sulphur-crested Cockatoo
    "criros2",  # Crimson Rosella
    "easros1",  # Eastern Rosella
    "supfai1",  # Superb Fairywren
    "wilwag1",  # Willie-wagtail
    "maglar1",  # Magpie-lark
    "noimin1",  # Noisy Miner
    "redwat1",  # Red Wattlebird
    "silver3",  # Silvereye
    "maslap1",  # Masked Lapwing
    "blkswa",  # Black Swan
    "manduc1",  # Maned Duck
    "pabduc1",  # Pacific Black Duck
    "ausgre1",  # Australasian Grebe
    "auspel1",  # Australian Pelican
    "piecor1",  # Pied Cormorant
    "lipcor1",  # Little Pied Cormorant
    "darter4",  # Australasian Darter
    "whfher1",  # White-faced Heron
    "ausibi1",  # Australian Ibis
    "stnibi1",  # Straw-necked Ibis
    "royspo1",  # Royal Spoonbill
    "auskit1",  # Black-shouldered Kite
    "whikit1",  # Whistling Kite
    "weteag1",  # Wedge-tailed Eagle
    "brofal1",  # Brown Falcon
    "auskes1",  # Nankeen Kestrel
    "purswa6",  # Australasian Swamphen
    "dusmoo1",  # Dusky Moorhen
    "eurcoo",  # Eurasian Coot
    "silgul2",  # Silver Gull
    "grcter1",  # Greater Crested Tern
    "crepig1",  # Crested Pigeon
    "peadov1",  # Peaceful Dove
    "tawfro1",  # Tawny Frogmouth
    "sackin1",  # Sacred Kingfisher
    "dollar1",  # Oriental Dollarbird
    "welswa1",  # Welcome Swallow
    "y01193",  # Golden Whistler
    "rufwhi1",  # Rufous Whistler
    "gryfan1",  # Gray Fantail
    "rebfir1",  # Red-browed Finch
    "zebfin2",  # Zebra Finch
]

# Avifauna of Central & South America (Costa Rica, Ecuador, Peru, Brazil)
NEOTROPICS_TAXON_IDS = [
    "kebtou1",  # Keel-billed Toucan
    "toctou1",  # Toco Toucan
    "chbtou1",  # Channel-billed Toucan
    "scamac1",  # Scarlet Macaw
    "baymac",  # Blue-and-yellow Macaw
    "ragmac1",  # Red-and-green Macaw
    "sunbit1",  # Sunbittern
    "hoatzi1",  # Hoatzin
    "jabiru",  # Jabiru
    "rosspo1",  # Roseate Spoonbill
    "woosto",  # Wood Stork
    "magfri",  # Magnificent Frigatebird
    "brnpel",  # Brown Pelican
    "neocor",  # Neotropic Cormorant
    "anhing",  # Anhinga
    "capher1",  # Capped Heron
    "agaher1",  # Agami Heron
    "bobher1",  # Boat-billed Heron
    "ruther1",  # Rufescent Tiger-Heron
    "blkvul",  # Black Vulture
    "turvul",  # Turkey Vulture
    "kinvul1",  # King Vulture
    "hareag1",  # Harpy Eagle
    "roahaw",  # Roadside Hawk
    "osprey",  # Osprey
    "laufal1",  # Laughing Falcon
    "batfal1",  # Bat Falcon
    "limpki",  # Limpkin
    "gycwor1",  # Gray-cowled Wood-Rail
    "purgal2",  # Purple Gallinule
    "watjac1",  # Wattled Jacana
    "soulap1",  # Southern Lapwing
    "colplo1",  # Collared Plover
    "blkski",  # Black Skimmer
    "labter1",  # Large-billed Tern
    "rugdov",  # Ruddy Ground Dove
    "whtdov",  # White-tipped Dove
    "smbani",  # Smooth-billed Ani
    "grbani",  # Groove-billed Ani
    "squcuc1",  # Common Squirrel-Cuckoo
    "compot1",  # Common Potoo
    "grepot1",  # Great Potoo
    "rinkin1",  # Ringed Kingfisher
    "amakin1",  # Amazon Kingfisher
    "grnkin",  # Green Kingfisher
    "rufmot1",  # Rufous Motmot
    "brbmot1",  # Broad-billed Motmot
    "bucmot4",  # Amazonian Motmot
    "rutjac1",  # Rufous-tailed Jacamar
    "whnpuf2",  # White-necked Puffbird
    "colara2",  # Collared Aracari
    "linwoo1",  # Lineated Woodpecker
    "crcwoo1",  # Crimson-crested Woodpecker
    "relpar10",  # Red-lored Amazon
    "meapar",  # Mealy Amazon
    "orcpar",  # Orange-chinned Parakeet
    "barant1",  # Barred Antshrike
    "greant1",  # Great Antshrike
    "rufhor2",  # Rufous Hornero
    "strfly1",  # Streaked Flycatcher
    "socfly1",  # Social Flycatcher
    "grekis",  # Great Kiskadee
    "bobfly1",  # Boat-billed Flycatcher
    "trokin",  # Tropical Kingbird
    "fotfly",  # Fork-tailed Flycatcher
    "bawswa1",  # Blue-and-white Swallow
    "srwswa1",  # Southern Rough-winged Swallow
    "houwre",  # Northern House Wren
    "clcrob",  # Clay-colored Thrush
    "banana",  # Bananaquit
    "bugtan",  # Blue-gray Tanager
    "paltan1",  # Palm Tanager
    "sibtan2",  # Silver-beaked Tanager
    "grehon1",  # Green Honeycreeper
    "relhon1",  # Red-legged Honeycreeper
    "bludac1",  # Blue Dacnis
    "varsee3",  # Variable Seedeater
    "butsal1",  # Buff-throated Saltator
    "grasal3",  # Bluish-gray Saltator
]

# Avifauna of East Asia (Japan, Thailand, Eastern flyway)
EAST_ASIA_TAXON_IDS = [
    "rinphe2",  # Green Pheasant
    "copphe1",  # Copper Pheasant
    "manduc",  # Mandarin Duck
    "spbduc",  # Eastern Spot-billed Duck
    "mallar3",  # Mallard
    "norpin",  # Northern Pintail
    "gnwtea",  # Green-winged Teal
    "compoc",  # Common Pochard
    "tufduc",  # Tufted Duck
    "grcgre1",  # Great Crested Grebe
    "litgre1",  # Little Grebe
    "eurcoo",  # Eurasian Coot
    "commoo3",  # Eurasian Moorhen
    "bcnher",  # Black-crowned Night Heron
    "graher1",  # Gray Heron
    "greegr",  # Great Egret
    "litegr",  # Little Egret
    "grecor",  # Great Cormorant
    "japcor1",  # Japanese Cormorant
    "blakit1",  # Black Kite
    "combuz6",  # Eastern Buzzard
    "osprey",  # Osprey
    "perfal",  # Peregrine Falcon
    "eurkes",  # Eurasian Kestrel
    "bktgul",  # Black-tailed Gull
    "veggul1",  # Vega Gull
    "slbgul",  # Slaty-backed Gull
    "bkhgul",  # Black-headed Gull
    "ortdov",  # Oriental Turtle-Dove
    "comkin1",  # Common Kingfisher
    "pygwoo1",  # Japanese Pygmy Woodpecker
    "japwoo1",  # Japanese Woodpecker
    "grswoo",  # Great Spotted Woodpecker
    "buhshr1",  # Bull-headed Shrike
    "eurjay1",  # Eurasian Jay
    "azwmag2",  # Azure-winged Magpie
    "carcro1",  # Carrion Crow
    "labcro1",  # Large-billed Crow
    "vartit1",  # Varied Tit
    "coatit2",  # Coal Tit
    "brebul1",  # Brown-eared Bulbul
    "barswa",  # Barn Swallow
    "ashmar1",  # Asian House-Martin
    "jabwar",  # Japanese Bush Warbler
    "warwhe1",  # Warbling White-eye
    "goldcr1",  # Goldcrest
    "eurnut2",  # Eurasian Nuthatch
    "whcsta1",  # White-cheeked Starling
    "brhthr1",  # Brown-headed Thrush
    "palthr1",  # Pale Thrush
    "dusthr2",  # Dusky Thrush
    "refblu1",  # Red-flanked Bluetail
    "daured1",  # Daurian Redstart
    "burthr",  # Blue Rock-Thrush
    "eutspa",  # Eurasian Tree Sparrow
    "whiwag",  # White Wagtail
    "japwag1",  # Japanese Wagtail
    "grywag",  # Gray Wagtail
    "olbpip",  # Olive-backed Pipit
    "origre",  # Oriental Greenfinch
    "brambl",  # Brambling
    "hawfin",  # Hawfinch
    "meabun1",  # Meadow Bunting
    "rusbun",  # Rustic Bunting
    "bkfbun1",  # Black-faced Bunting
]

# Mammals, insects, and scenery of Western Ghats / South India
WESTERN_GHATS_WILDLIFE_TAXON_IDS = [
    "mamm_tiger1",  # Bengal Tiger
    "mamm_eleph1",  # Asian Elephant
    "mamm_mal_sq1",  # Malabar Giant Squirrel
    "mamm_ind_leop1",  # Indian Leopard
    "mamm_chital1",  # Chital
    "mamm_sambar1",  # Sambar
    "mamm_langur1",  # Gray Langur
    "ins_peacock1",  # Malabar Banded Peacock
    "ins_blue_mormon1",  # Blue Mormon
    "ins_mormon1",  # Common Mormon
    "ins_crimson_rose1",  # Crimson Rose
    "ins_globe_skimmer1",  # Globe Skimmer
    "scen_forest",  # Forest Canopy
    "scen_waterfall",  # Waterfall
    "scen_sunset",  # Sunset Sky
]

# Mammals, insects, and scenery of Rajasthan / Thar / Ranthambore
RAJASTHAN_WILDLIFE_TAXON_IDS = [
    "mamm_tiger1",  # Bengal Tiger
    "mamm_ind_leop1",  # Indian Leopard
    "mamm_chital1",  # Chital
    "mamm_sambar1",  # Sambar
    "mamm_langur1",  # Gray Langur
    "mamm_rhesus1",  # Rhesus Macaque
    "ins_mormon1",  # Common Mormon
    "ins_globe_skimmer1",  # Globe Skimmer
    "ins_blue_tiger1",  # Blue Tiger
    "scen_desert",  # Desert Dunes
    "scen_sunset",  # Sunset Sky
]

# Mammals, insects, and scenery of Gujarat / Gir / Kutch
GUJARAT_WILDLIFE_TAXON_IDS = [
    "mamm_lion1",  # Asiatic Lion
    "mamm_ind_leop1",  # Indian Leopard
    "mamm_chital1",  # Chital
    "mamm_sambar1",  # Sambar
    "mamm_langur1",  # Gray Langur
    "ins_mormon1",  # Common Mormon
    "ins_globe_skimmer1",  # Globe Skimmer
    "scen_desert",  # Desert Dunes
    "scen_coastline",  # Ocean Coastline
    "scen_sunset",  # Sunset Sky
]

# Mammals, insects, and scenery of Ladakh / High-altitude Himalaya
LADAKH_WILDLIFE_TAXON_IDS = [
    "mamm_wild_yak1",  # Wild Yak
    "mamm_dom_yak1",  # Domestic Yak
    "mamm_wool_hare1",  # Woolly Hare
    "mamm_him_marmot1",  # Himalayan Marmot
    "mamm_bharal1",  # Bharal
    "mamm_snow_leop1",  # Snow Leopard
    "ins_peacock1",  # Malabar Banded Peacock
    "scen_mountain",  # Mountain Landscape
    "scen_sunset",  # Sunset Sky
    "scen_night_sky",  # Starry Night Sky
    "scen_rainbow",  # Rainbow over Landscape
]

# Mammals, insects, and scenery of Uttarakhand / Eastern Himalaya
HIMALAYA_WILDLIFE_TAXON_IDS = [
    "mamm_red_panda1",  # Red Panda
    "mamm_snow_leop1",  # Snow Leopard
    "mamm_him_marmot1",  # Himalayan Marmot
    "mamm_bharal1",  # Bharal
    "mamm_langur1",  # Gray Langur
    "mamm_rhesus1",  # Rhesus Macaque
    "scen_mountain",  # Mountain Landscape
    "scen_forest",  # Forest Canopy
    "scen_waterfall",  # Waterfall
]


# Map of regional codes to taxon ID lists
REGIONAL_CHECKLISTS_MAP: dict[str, list[str]] = {
    # South India / Western Ghats / Deccan
    "IN-KA": INDIA_BASE_TAXON_IDS + WESTERN_GHATS_WILDLIFE_TAXON_IDS,
    "IN-KL": INDIA_BASE_TAXON_IDS + WESTERN_GHATS_WILDLIFE_TAXON_IDS,
    "IN-TN": INDIA_BASE_TAXON_IDS + WESTERN_GHATS_WILDLIFE_TAXON_IDS,
    "IN-MH": INDIA_BASE_TAXON_IDS + WESTERN_GHATS_WILDLIFE_TAXON_IDS,
    "IN-GA": INDIA_BASE_TAXON_IDS + WESTERN_GHATS_WILDLIFE_TAXON_IDS,
    # Northwest India / Arid Zone
    "IN-RJ": INDIA_BASE_TAXON_IDS + RAJASTHAN_SPECIFIC_TAXON_IDS + RAJASTHAN_WILDLIFE_TAXON_IDS,
    "IN-GJ": INDIA_BASE_TAXON_IDS + RAJASTHAN_SPECIFIC_TAXON_IDS + GUJARAT_WILDLIFE_TAXON_IDS,
    "IN-DL": INDIA_BASE_TAXON_IDS + RAJASTHAN_SPECIFIC_TAXON_IDS[:30],
    "IN-HR": INDIA_BASE_TAXON_IDS + RAJASTHAN_SPECIFIC_TAXON_IDS[:30],
    "IN-PB": INDIA_BASE_TAXON_IDS + RAJASTHAN_SPECIFIC_TAXON_IDS[:30],
    # Himalayas & Northern Mountain Regions
    "IN-LA": LADAKH_SPECIFIC_TAXON_IDS + LADAKH_WILDLIFE_TAXON_IDS,
    "IN-UT": INDIA_BASE_TAXON_IDS + LADAKH_SPECIFIC_TAXON_IDS[:25] + HIMALAYA_WILDLIFE_TAXON_IDS,
    "IN-HP": INDIA_BASE_TAXON_IDS + LADAKH_SPECIFIC_TAXON_IDS[:25] + HIMALAYA_WILDLIFE_TAXON_IDS,
    "IN-JK": INDIA_BASE_TAXON_IDS + LADAKH_SPECIFIC_TAXON_IDS[:25] + HIMALAYA_WILDLIFE_TAXON_IDS,
    # Eastern & Central India
    "IN-AS": INDIA_BASE_TAXON_IDS + HIMALAYA_WILDLIFE_TAXON_IDS,
    "IN-WB": INDIA_BASE_TAXON_IDS + HIMALAYA_WILDLIFE_TAXON_IDS,
    "IN-MP": INDIA_BASE_TAXON_IDS + RAJASTHAN_WILDLIFE_TAXON_IDS,
    # North America
    "US-FL": FLORIDA_TAXON_IDS,
    "US-NY": NEW_YORK_TAXON_IDS,
    "US-CA": CALIFORNIA_TAXON_IDS,
    "US-TX": FLORIDA_TAXON_IDS + CALIFORNIA_TAXON_IDS[:20],
    "US-AZ": CALIFORNIA_TAXON_IDS + NEW_YORK_TAXON_IDS[:15],
    "US-AK": LADAKH_SPECIFIC_TAXON_IDS[:20] + NEW_YORK_TAXON_IDS[:25],
    "US-WA": CALIFORNIA_TAXON_IDS + NEW_YORK_TAXON_IDS[:20],
    "US-CO": CALIFORNIA_TAXON_IDS + NEW_YORK_TAXON_IDS[:20],
    # Africa
    "ZA": SOUTH_AFRICA_TAXON_IDS,
    "BW": SOUTH_AFRICA_TAXON_IDS,
    "KE": EAST_AFRICA_TAXON_IDS,
    "KE-01": EAST_AFRICA_TAXON_IDS,
    "TZ": EAST_AFRICA_TAXON_IDS,
    # Europe & Oceania
    "GB": UK_EUROPE_TAXON_IDS,
    "NZ": NEW_ZEALAND_TAXON_IDS,
    "AU-NSW": AUSTRALIA_TAXON_IDS,
    "AU-NT": AUSTRALIA_TAXON_IDS,
    "AU-QLD": AUSTRALIA_TAXON_IDS,
    "AU-VIC": AUSTRALIA_TAXON_IDS,
    # Neotropics
    "CR": NEOTROPICS_TAXON_IDS,
    "EC": NEOTROPICS_TAXON_IDS,
    "PE": NEOTROPICS_TAXON_IDS,
    "BR": NEOTROPICS_TAXON_IDS,
    # East Asia
    "JP": EAST_ASIA_TAXON_IDS,
    "TH": EAST_ASIA_TAXON_IDS + INDIA_BASE_TAXON_IDS[:50],
    # Western Europe / Mediterranean
    "ES": UK_EUROPE_TAXON_IDS,
    "FR": UK_EUROPE_TAXON_IDS,
}


def build_gazetteer(taxonomy_path: str = TAXONOMY_PATH) -> None:
    """Validates and writes data/gazetteer.json and data/regional_checklists.json."""
    if not os.path.exists(taxonomy_path):
        raise FileNotFoundError(f"Taxonomy file not found at {taxonomy_path}")

    with open(taxonomy_path, encoding="utf-8") as f:
        taxonomy = json.load(f)

    valid_taxon_ids: set[str] = {s["taxon_id"] for s in taxonomy}
    print(f"Loaded {len(valid_taxon_ids)} valid taxon IDs from {taxonomy_path}")

    # Build and serialize gazetteer
    hotspots = build_gazetteer_data(RAW_GAZETTEER)
    gazetteer = serialize_gazetteer(hotspots)
    seen_names = {normalize_hotspot_name(spot.name) for spot in hotspots}

    # Verify key hotspots are present
    assert "saul kere" in seen_names or "saul kere lake" in seen_names
    assert "bengaluru" in seen_names or "bangalore" in seen_names
    assert "ranthambore" in seen_names or "ranthambore national park" in seen_names
    assert "leh" in seen_names
    assert "everglades" in seen_names or "everglades national park" in seen_names
    assert "kruger" in seen_names or "kruger national park" in seen_names
    assert "central park" in seen_names

    # 2. Prepare Regional Checklists
    checklists: dict[str, list[str]] = {}
    for region_code, tids in REGIONAL_CHECKLISTS_MAP.items():
        # Deduplicate while preserving order
        unique_tids = []
        seen = set()
        for tid in tids:
            if tid not in valid_taxon_ids:
                raise ValueError(
                    f"Taxon ID '{tid}' for region '{region_code}' not found in taxonomy.json"
                )
            if tid not in seen:
                seen.add(tid)
                unique_tids.append(tid)

        checklists[region_code] = unique_tids

    # Verify Karnataka (IN-KA) invariants
    ka_set = set(checklists["IN-KA"])
    assert "brakit1" in ka_set, "Brahminy Kite (brakit1) must be in IN-KA"
    assert "whtkin2" in ka_set, "White-throated Kingfisher (whtkin2) must be in IN-KA"
    assert "indcor1" in ka_set, "Indian Cormorant (indcor1) must be in IN-KA"
    assert "greegr" in ka_set, "Great Egret (greegr) must be in IN-KA"
    assert "graher1" in ka_set, "Gray Heron (graher1) must be in IN-KA"
    assert "baleag" not in ka_set, "Bald Eagle (baleag) must not be in IN-KA"
    assert "blkswa" not in ka_set, "Black Swan (blkswa) must not be in IN-KA"

    # Verify Florida (US-FL) has Bald Eagle
    assert "baleag" in checklists["US-FL"], "Bald Eagle (baleag) must be in US-FL"

    # Write outputs
    os.makedirs(DATA_DIR, exist_ok=True)

    with open(GAZETTEER_PATH, "w", encoding="utf-8") as f:
        json.dump(gazetteer, f, indent=2, ensure_ascii=False)
    print(f"Successfully wrote {len(gazetteer)} gazetteer entries to {GAZETTEER_PATH}")

    with open(CHECKLISTS_PATH, "w", encoding="utf-8") as f:
        json.dump(checklists, f, indent=2, ensure_ascii=False)
    print(f"Successfully wrote {len(checklists)} regional checklists to {CHECKLISTS_PATH}")


if __name__ == "__main__":
    build_gazetteer()
