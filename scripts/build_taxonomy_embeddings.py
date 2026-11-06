#!/usr/bin/env python3
"""Downloads Cornell eBird global bird taxonomy (11,167 species) and multi-taxa categories
(Mammalia, Insecta, Scenery), and computes precomputed normalized text embeddings
using BioCLIP (imageomics/bioclip) on Apple Silicon MPS/GPU.

Saves:
- models/taxonomy.json
- models/species_embeddings.safetensors
- models/manifest.json
"""

from __future__ import annotations

import hashlib
import json
import logging
import os
import re
import sys
import urllib.request
from collections.abc import Callable, Sequence
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any, TypeVar

if TYPE_CHECKING:
    import torch

T = TypeVar("T")

CANONICAL_CATEGORIES: tuple[str, ...] = (
    "Bird",
    "Mammalia",
    "Insecta",
    "Reptilia",
    "Amphibia",
    "Botanical",
    "Scenery",
    "Architecture",
    "Portrait",
    "Abstract",
)

CATEGORY_MAPPING: dict[str, str] = {
    "bird": "Bird",
    "birds": "Bird",
    "mammal": "Mammalia",
    "mammalia": "Mammalia",
    "mammals": "Mammalia",
    "insect": "Insecta",
    "insecta": "Insecta",
    "insects": "Insecta",
    "reptile": "Reptilia",
    "reptiles": "Reptilia",
    "reptilia": "Reptilia",
    "amphibian": "Amphibia",
    "amphibians": "Amphibia",
    "amphibia": "Amphibia",
    "botanical": "Botanical",
    "plant": "Botanical",
    "plants": "Botanical",
    "flora": "Botanical",
    "scenery": "Scenery",
    "landscape": "Scenery",
    "architecture": "Architecture",
    "building": "Architecture",
    "buildings": "Architecture",
    "portrait": "Portrait",
    "portraits": "Portrait",
    "abstract": "Abstract",
}

CATEGORY_PROMPT_NOUNS: dict[str, str] = {
    "Bird": "bird",
    "Mammalia": "mammal",
    "Insecta": "insect",
    "Reptilia": "reptile",
    "Amphibia": "amphibian",
    "Botanical": "plant",
    "Scenery": "scenery",
    "Architecture": "building",
    "Portrait": "portrait",
    "Abstract": "abstract texture",
}


def normalize_category(category: str) -> str:
    """Normalize a category string to canonical singular form."""
    cleaned = category.strip()
    if cleaned in CANONICAL_CATEGORIES:
        return cleaned
    lower = cleaned.lower()
    return CATEGORY_MAPPING.get(lower, cleaned)


@dataclass(frozen=True)
class TaxonomyEntry:
    """Immutable representation of a taxonomy record across biological and photographic domains.

    Attributes:
        species_code: Unique alphanumeric taxon identifier (e.g. 'brakit1', 'mamm_mal_sq1').
        scientific_name: Binomial scientific nomenclature (e.g. 'Ratufa indica').
        common_name: English common vernacular name (e.g. 'Malabar Giant Squirrel').
        category: Singular taxonomic/genre category ('Bird', 'Mammalia', 'Insecta', 'Scenery').
        aliases: Alternative common vernacular or subspecies names.
    """

    species_code: str
    scientific_name: str
    common_name: str
    category: str = "Bird"
    aliases: tuple[str, ...] = ()

    @property
    def folder_slug(self) -> str:
        """Sanitized folder-friendly name slug derived from the common name."""
        return sanitize_species_slug(self.common_name)

    def to_dict(self) -> dict[str, Any]:
        """Convert entry into serializable dictionary format for taxonomy.json."""
        record: dict[str, Any] = {
            "taxon_id": self.species_code,
            "category": self.category,
            "common_name": self.common_name,
            "folder_slug": self.folder_slug,
            "scientific_name": self.scientific_name,
        }
        if self.aliases:
            record["aliases"] = list(self.aliases)
        return record


def generate_taxonomy_prompt(entry: TaxonomyEntry) -> str:
    """Generate category-specific BioCLIP visual text prompt for a taxonomy record.

    Examples:
        - Bird: 'a photo of a Brahminy Kite (Haliastur indus), a type of bird.'
        - Mammalia: 'a photo of a Malabar Giant Squirrel (Ratufa indica), a type of mammal.'
        - Insecta: 'a photo of a Malabar Banded Peacock (Papilio buddha), a type of insect.'
        - Scenery: 'a photo of a Aurora Borealis (Aurora Borealis), a type of scenery.'
          or without scientific name: 'a photo of a Mountain Landscape, a type of scenery.'
    """
    category_noun = CATEGORY_PROMPT_NOUNS.get(entry.category, entry.category.lower())
    if entry.scientific_name:
        return (
            f"a photo of a {entry.common_name} ({entry.scientific_name}), "
            f"a type of {category_noun}."
        )
    return f"a photo of a {entry.common_name}, a type of {category_noun}."


def generate_prompt(entry: TaxonomyEntry) -> str:
    """Backward-compatible alias for generate_taxonomy_prompt."""
    return generate_taxonomy_prompt(entry)


def format_prompt(entry: TaxonomyEntry) -> str:
    """Backward-compatible alias for generate_taxonomy_prompt."""
    return generate_taxonomy_prompt(entry)


def sanitize_species_slug(name: str) -> str:
    """Sanitize a common name for directory naming by replacing spaces and hyphens with underscores,

    and removing non-alphanumeric characters.
    """
    cleaned = re.sub(r"[^\w\s-]", "", name)
    cleaned = re.sub(r"[\s-]+", "_", cleaned)
    return cleaned.strip("_")


def sanitize_slug(name: str) -> str:
    """Backward-compatible alias for sanitize_species_slug."""
    return sanitize_species_slug(name)


def parse_taxonomy_records(
    raw_records: Sequence[dict[str, Any]],
    default_category: str = "Bird",
) -> tuple[TaxonomyEntry, ...]:
    """Parse and validate raw taxonomy API dictionaries into immutable TaxonomyEntry records.

    Supports Cornell eBird API responses, IUCN/iNaturalist multi-taxa entries, and curated
    photographic categories (Bird, Mammalia, Insecta, Scenery).
    """
    entries: list[TaxonomyEntry] = []
    seen_codes: set[str] = set()
    skip_ebird_categories = {"slash", "spuh", "hybrid", "issf", "domestic", "form"}

    for item in raw_records:
        raw_cat = item.get("category")
        if isinstance(raw_cat, str) and raw_cat in skip_ebird_categories:
            continue

        sp_code = item.get("speciesCode") or item.get("taxon_id") or item.get("species_code")
        com_name = item.get("comName") or item.get("common_name")
        sci_name = item.get("sciName") or item.get("scientific_name") or ""

        if not sp_code or not isinstance(sp_code, str):
            continue
        if not com_name or not isinstance(com_name, str):
            continue

        clean_code = sp_code.strip()
        if not clean_code or clean_code in seen_codes:
            continue

        seen_codes.add(clean_code)

        if raw_cat == "species" or raw_cat is None:
            resolved_category = default_category
        elif isinstance(raw_cat, str):
            norm_cat = normalize_category(raw_cat)
            resolved_category = norm_cat if norm_cat in CANONICAL_CATEGORIES else default_category
        else:
            resolved_category = default_category

        aliases = tuple(item.get("aliases", ()))
        entries.append(
            TaxonomyEntry(
                species_code=clean_code,
                scientific_name=sci_name.strip() if isinstance(sci_name, str) else "",
                common_name=com_name.strip(),
                category=resolved_category,
                aliases=aliases,
            )
        )

    return tuple(entries)


# Curated representative multi-taxa records for open-set classification
CURATED_MULTI_TAXA_RECORDS: tuple[dict[str, Any], ...] = (
    # --- Mammalia (IUCN / iNaturalist prominent wild mammals) ---
    {
        "category": "Mammalia",
        "taxon_id": "mamm_mal_sq1",
        "common_name": "Malabar Giant Squirrel",
        "scientific_name": "Ratufa indica",
        "aliases": ["Indian Giant Squirrel"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_tiger1",
        "common_name": "Bengal Tiger",
        "scientific_name": "Panthera tigris tigris",
        "aliases": ["Tiger", "Royal Bengal Tiger"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_lion1",
        "common_name": "Asiatic Lion",
        "scientific_name": "Panthera leo leo",
        "aliases": ["Lion", "Persian Lion"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_ind_leop1",
        "common_name": "Indian Leopard",
        "scientific_name": "Panthera pardus fusca",
        "aliases": ["Leopard"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_snow_leop1",
        "common_name": "Snow Leopard",
        "scientific_name": "Panthera uncia",
        "aliases": ["Ounce"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_eleph1",
        "common_name": "Asian Elephant",
        "scientific_name": "Elephas maximus",
        "aliases": ["Indian Elephant"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_chital1",
        "common_name": "Chital",
        "scientific_name": "Axis axis",
        "aliases": ["Spotted Deer", "Axis Deer"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_sambar1",
        "common_name": "Sambar",
        "scientific_name": "Rusa unicolor",
        "aliases": ["Sambar Deer"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_red_panda1",
        "common_name": "Red Panda",
        "scientific_name": "Ailurus fulgens",
        "aliases": ["Lesser Panda"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_rhesus1",
        "common_name": "Rhesus Macaque",
        "scientific_name": "Macaca mulatta",
        "aliases": ["Rhesus Monkey"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_langur1",
        "common_name": "Gray Langur",
        "scientific_name": "Semnopithecus entellus",
        "aliases": ["Hanuman Langur"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_wild_yak1",
        "common_name": "Wild Yak",
        "scientific_name": "Bos mutus",
        "aliases": ["Wild Yak", "Yak"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_dom_yak1",
        "common_name": "Domestic Yak",
        "scientific_name": "Bos grunniens",
        "aliases": ["Domestic Yak", "Himalayan Yak"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_wool_hare1",
        "common_name": "Woolly Hare",
        "scientific_name": "Lepus oiostolus",
        "aliases": ["Woolly Hare", "Tibetan Hare", "Mountain Hare"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_him_marmot1",
        "common_name": "Himalayan Marmot",
        "scientific_name": "Marmota himalayana",
        "aliases": ["Himalayan Marmot"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_bharal1",
        "common_name": "Bharal",
        "scientific_name": "Pseudois nayaur",
        "aliases": ["Blue Sheep", "Bharal"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_lion2",
        "common_name": "African Lion",
        "scientific_name": "Panthera leo melanochaita",
        "aliases": ["Lion", "Simba"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_jaguar1",
        "common_name": "Jaguar",
        "scientific_name": "Panthera onca",
        "aliases": ["El Tigre"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_cheetah1",
        "common_name": "Cheetah",
        "scientific_name": "Acinonyx jubatus",
        "aliases": ["Hunting Leopard"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_cougar1",
        "common_name": "Cougar",
        "scientific_name": "Puma concolor",
        "aliases": ["Mountain Lion", "Puma", "Panther"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_caracal1",
        "common_name": "Caracal",
        "scientific_name": "Caracal caracal",
        "aliases": ["Desert Lynx"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_serval1",
        "common_name": "Serval",
        "scientific_name": "Leptailurus serval",
        "aliases": ["Tierboskat"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_eur_lynx1",
        "common_name": "Eurasian Lynx",
        "scientific_name": "Lynx lynx",
        "aliases": ["Northern Lynx"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_can_lynx1",
        "common_name": "Canada Lynx",
        "scientific_name": "Lynx canadensis",
        "aliases": [],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_bobcat1",
        "common_name": "Bobcat",
        "scientific_name": "Lynx rufus",
        "aliases": ["Red Lynx"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_ocelot1",
        "common_name": "Ocelot",
        "scientific_name": "Leopardus pardalis",
        "aliases": ["Painted Leopard"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_cloud_leop1",
        "common_name": "Clouded Leopard",
        "scientific_name": "Neofelis nebulosa",
        "aliases": [],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_gray_wolf1",
        "common_name": "Gray Wolf",
        "scientific_name": "Canis lupus",
        "aliases": ["Timber Wolf", "Wolf"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_coyote1",
        "common_name": "Coyote",
        "scientific_name": "Canis latrans",
        "aliases": ["Prairie Wolf"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_dhole1",
        "common_name": "Dhole",
        "scientific_name": "Cuon alpinus",
        "aliases": ["Asiatic Wild Dog", "Indian Wild Dog"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_af_wild_dog1",
        "common_name": "African Wild Dog",
        "scientific_name": "Lycaon pictus",
        "aliases": ["Painted Dog", "Cape Hunting Dog"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_red_fox1",
        "common_name": "Red Fox",
        "scientific_name": "Vulpes vulpes",
        "aliases": ["Fox"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_arctic_fox1",
        "common_name": "Arctic Fox",
        "scientific_name": "Vulpes lagopus",
        "aliases": ["Polar Fox", "White Fox"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_fennec_fox1",
        "common_name": "Fennec Fox",
        "scientific_name": "Vulpes zerda",
        "aliases": ["Desert Fox"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_bl_jackal1",
        "common_name": "Black-backed Jackal",
        "scientific_name": "Lupulella mesomelas",
        "aliases": ["Silver-backed Jackal"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_gold_jackal1",
        "common_name": "Golden Jackal",
        "scientific_name": "Canis aureus",
        "aliases": ["Common Jackal"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_dingo1",
        "common_name": "Dingo",
        "scientific_name": "Canis dingo",
        "aliases": ["Australian Wild Dog"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_grizzly1",
        "common_name": "Brown Bear",
        "scientific_name": "Ursus arctos",
        "aliases": ["Grizzly Bear", "Kodiak Bear"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_polar_bear1",
        "common_name": "Polar Bear",
        "scientific_name": "Ursus maritimus",
        "aliases": ["White Bear", "Ice Bear"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_am_black_bear1",
        "common_name": "American Black Bear",
        "scientific_name": "Ursus americanus",
        "aliases": ["Black Bear"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_as_black_bear1",
        "common_name": "Asian Black Bear",
        "scientific_name": "Ursus thibetanus",
        "aliases": ["Moon Bear"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_giant_panda1",
        "common_name": "Giant Panda",
        "scientific_name": "Ailuropoda melanoleuca",
        "aliases": ["Panda Bear"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_sloth_bear1",
        "common_name": "Sloth Bear",
        "scientific_name": "Melursus ursinus",
        "aliases": ["Labiatated Bear"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_sun_bear1",
        "common_name": "Sun Bear",
        "scientific_name": "Helarctos malayanus",
        "aliases": ["Malayan Sun Bear", "Honey Bear"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_spect_bear1",
        "common_name": "Spectacled Bear",
        "scientific_name": "Tremarctos ornatus",
        "aliases": ["Andean Bear"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_spotted_hyena1",
        "common_name": "Spotted Hyena",
        "scientific_name": "Crocuta crocuta",
        "aliases": ["Laughing Hyena"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_striped_hyena1",
        "common_name": "Striped Hyena",
        "scientific_name": "Hyaena hyaena",
        "aliases": [],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_meerkat1",
        "common_name": "Meerkat",
        "scientific_name": "Suricata suricatta",
        "aliases": ["Suricate"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_wolverine1",
        "common_name": "Wolverine",
        "scientific_name": "Gulo gulo",
        "aliases": ["Glutton", "Carcajou"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_eur_badger1",
        "common_name": "Eurasian Badger",
        "scientific_name": "Meles meles",
        "aliases": ["Badger"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_honey_badger1",
        "common_name": "Honey Badger",
        "scientific_name": "Mellivora capensis",
        "aliases": ["Ratel"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_sea_otter1",
        "common_name": "Sea Otter",
        "scientific_name": "Enhydra lutris",
        "aliases": [],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_giant_otter1",
        "common_name": "Giant Otter",
        "scientific_name": "Pteronura brasiliensis",
        "aliases": ["Giant River Otter"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_eur_otter1",
        "common_name": "Eurasian Otter",
        "scientific_name": "Lutra lutra",
        "aliases": ["European Otter"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_raccoon1",
        "common_name": "Raccoon",
        "scientific_name": "Procyon lotor",
        "aliases": ["Common Raccoon"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_coati1",
        "common_name": "White-nosed Coati",
        "scientific_name": "Nasua narica",
        "aliases": ["Coatimundi"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_walrus1",
        "common_name": "Walrus",
        "scientific_name": "Odobenus rosmarus",
        "aliases": ["Morsea"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_ca_sea_lion1",
        "common_name": "California Sea Lion",
        "scientific_name": "Zalophus californianus",
        "aliases": ["Sea Lion"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_harbor_seal1",
        "common_name": "Harbor Seal",
        "scientific_name": "Phoca vitulina",
        "aliases": ["Common Seal"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_leopard_seal1",
        "common_name": "Leopard Seal",
        "scientific_name": "Hydrurga leptonyx",
        "aliases": ["Sea Leopard"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_s_elephant_seal1",
        "common_name": "Southern Elephant Seal",
        "scientific_name": "Mirounga leonina",
        "aliases": ["Elephant Seal"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_af_bush_eleph1",
        "common_name": "African Bush Elephant",
        "scientific_name": "Loxodonta africana",
        "aliases": ["Savanna Elephant"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_af_for_eleph1",
        "common_name": "African Forest Elephant",
        "scientific_name": "Loxodonta cyclotis",
        "aliases": [],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_white_rhino1",
        "common_name": "White Rhinoceros",
        "scientific_name": "Ceratotherium simum",
        "aliases": ["Square-lipped Rhinoceros"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_black_rhino1",
        "common_name": "Black Rhinoceros",
        "scientific_name": "Diceros bicornis",
        "aliases": ["Hook-lipped Rhinoceros"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_ind_rhino1",
        "common_name": "Indian Rhinoceros",
        "scientific_name": "Rhinoceros unicornis",
        "aliases": ["Greater One-horned Rhinoceros"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_plains_zebra1",
        "common_name": "Plains Zebra",
        "scientific_name": "Equus quagga",
        "aliases": ["Common Zebra"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_mountain_zebra1",
        "common_name": "Mountain Zebra",
        "scientific_name": "Equus zebra",
        "aliases": [],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_grevys_zebra1",
        "common_name": "Grévy's Zebra",
        "scientific_name": "Equus grevyi",
        "aliases": ["Imperial Zebra"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_onager1",
        "common_name": "Onager",
        "scientific_name": "Equus hemionus",
        "aliases": ["Asiatic Wild Ass", "Khur"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_kiang1",
        "common_name": "Kiang",
        "scientific_name": "Equus kiang",
        "aliases": ["Tibetan Wild Ass"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_bairds_tapir1",
        "common_name": "Baird's Tapir",
        "scientific_name": "Tapirus bairdii",
        "aliases": ["Central American Tapir"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_malayan_tapir1",
        "common_name": "Malayan Tapir",
        "scientific_name": "Tapirus indicus",
        "aliases": ["Asian Tapir"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_giraffe1",
        "common_name": "Giraffe",
        "scientific_name": "Giraffa camelopardalis",
        "aliases": ["Northern Giraffe", "Southern Giraffe"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_okapi1",
        "common_name": "Okapi",
        "scientific_name": "Okapia johnstoni",
        "aliases": ["Forest Giraffe"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_hippo1",
        "common_name": "Hippopotamus",
        "scientific_name": "Hippopotamus amphibius",
        "aliases": ["Common Hippo", "River Horse"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_pygmy_hippo1",
        "common_name": "Pygmy Hippopotamus",
        "scientific_name": "Choeropsis liberiensis",
        "aliases": [],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_bact_camel1",
        "common_name": "Wild Bactrian Camel",
        "scientific_name": "Camelus ferus",
        "aliases": ["Two-humped Camel"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_drom_camel1",
        "common_name": "Dromedary",
        "scientific_name": "Camelus dromedarius",
        "aliases": ["Arabian Camel", "One-humped Camel"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_llama1",
        "common_name": "Llama",
        "scientific_name": "Lama glama",
        "aliases": [],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_alpaca1",
        "common_name": "Alpaca",
        "scientific_name": "Vicugna pacos",
        "aliases": [],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_guanaco1",
        "common_name": "Guanaco",
        "scientific_name": "Lama guanicoe",
        "aliases": [],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_vicuna1",
        "common_name": "Vicuña",
        "scientific_name": "Vicugna vicugna",
        "aliases": [],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_warthog1",
        "common_name": "Common Warthog",
        "scientific_name": "Phacochoerus africanus",
        "aliases": ["Warthog"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_wild_boar1",
        "common_name": "Wild Boar",
        "scientific_name": "Sus scrofa",
        "aliases": ["Wild Pig", "Razorback"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_af_buffalo1",
        "common_name": "African Buffalo",
        "scientific_name": "Syncerus caffer",
        "aliases": ["Cape Buffalo"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_am_bison1",
        "common_name": "American Bison",
        "scientific_name": "Bison bison",
        "aliases": ["Buffalo", "Plains Bison"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_wisent1",
        "common_name": "European Bison",
        "scientific_name": "Bison bonasus",
        "aliases": ["Wisent"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_gaur1",
        "common_name": "Gaur",
        "scientific_name": "Bos gaurus",
        "aliases": ["Indian Bison"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_water_buffalo1",
        "common_name": "Wild Water Buffalo",
        "scientific_name": "Bubalus arnee",
        "aliases": ["Asian Buffalo"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_blue_wildebeest1",
        "common_name": "Blue Wildebeest",
        "scientific_name": "Connochaetes taurinus",
        "aliases": ["Brindled Gnu"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_black_wildebeest1",
        "common_name": "Black Wildebeest",
        "scientific_name": "Connochaetes gnou",
        "aliases": ["White-tailed Gnu"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_impala1",
        "common_name": "Impala",
        "scientific_name": "Aepyceros melampus",
        "aliases": [],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_springbok1",
        "common_name": "Springbok",
        "scientific_name": "Antidorcas marsupialis",
        "aliases": [],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_gemsbok1",
        "common_name": "Gemsbok",
        "scientific_name": "Oryx gazella",
        "aliases": ["South African Oryx"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_scimitar_oryx1",
        "common_name": "Scimitar-horned Oryx",
        "scientific_name": "Oryx dammah",
        "aliases": ["Sahara Oryx"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_greater_kudu1",
        "common_name": "Greater Kudu",
        "scientific_name": "Tragelaphus strepsiceros",
        "aliases": ["Kudu"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_common_eland1",
        "common_name": "Common Eland",
        "scientific_name": "Taurotragus oryx",
        "aliases": ["Eland"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_blackbuck1",
        "common_name": "Blackbuck",
        "scientific_name": "Antilope cervicapra",
        "aliases": ["Indian Antelope"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_chinkara1",
        "common_name": "Chinkara",
        "scientific_name": "Gazella bennettii",
        "aliases": ["Indian Gazelle"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_nilgai1",
        "common_name": "Nilgai",
        "scientific_name": "Boselaphus tragocamelus",
        "aliases": ["Blue Bull"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_four_horn1",
        "common_name": "Four-horned Antelope",
        "scientific_name": "Tetracerus quadricornis",
        "aliases": ["Chousingha"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_tib_antelope1",
        "common_name": "Tibetan Antelope",
        "scientific_name": "Pantholops hodgsonii",
        "aliases": ["Chiru"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_tib_gazelle1",
        "common_name": "Tibetan Gazelle",
        "scientific_name": "Procapra picticaudata",
        "aliases": ["Goa"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_saiga1",
        "common_name": "Saiga Antelope",
        "scientific_name": "Saiga tatarica",
        "aliases": ["Saiga"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_muskox1",
        "common_name": "Muskox",
        "scientific_name": "Ovibos moschatus",
        "aliases": ["Musk Ox"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_bighorn_sheep1",
        "common_name": "Bighorn Sheep",
        "scientific_name": "Ovis canadensis",
        "aliases": ["Mountain Sheep"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_dall_sheep1",
        "common_name": "Dall Sheep",
        "scientific_name": "Ovis dalli",
        "aliases": ["Thinhorn Sheep"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_argali1",
        "common_name": "Argali",
        "scientific_name": "Ovis ammon",
        "aliases": ["Mountain Sheep"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_mountain_goat1",
        "common_name": "Mountain Goat",
        "scientific_name": "Oreamnos americanus",
        "aliases": ["Rocky Mountain Goat"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_alpine_ibex1",
        "common_name": "Alpine Ibex",
        "scientific_name": "Capra ibex",
        "aliases": ["Steinbock"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_siberian_ibex1",
        "common_name": "Siberian Ibex",
        "scientific_name": "Capra sibirica",
        "aliases": ["Asiatic Ibex"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_markhor1",
        "common_name": "Markhor",
        "scientific_name": "Capra falconeri",
        "aliases": [],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_chamois1",
        "common_name": "Alpine Chamois",
        "scientific_name": "Rupicapra rupicapra",
        "aliases": ["Chamois"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_nilgiri_tahr1",
        "common_name": "Nilgiri Tahr",
        "scientific_name": "Nilgiritragus hylocrius",
        "aliases": ["Ibex of Nilgiris"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_him_tahr1",
        "common_name": "Himalayan Tahr",
        "scientific_name": "Hemitragus jemlahicus",
        "aliases": [],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_moose1",
        "common_name": "Moose",
        "scientific_name": "Alces alces",
        "aliases": ["Eurasian Elk"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_elk1",
        "common_name": "Elk",
        "scientific_name": "Cervus canadensis",
        "aliases": ["Wapiti"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_red_deer1",
        "common_name": "Red Deer",
        "scientific_name": "Cervus elaphus",
        "aliases": ["European Red Deer"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_wh_deer1",
        "common_name": "White-tailed Deer",
        "scientific_name": "Odocoileus virginianus",
        "aliases": ["Virginia Deer"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_mule_deer1",
        "common_name": "Mule Deer",
        "scientific_name": "Odocoileus hemionus",
        "aliases": ["Black-tailed Deer"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_reindeer1",
        "common_name": "Reindeer",
        "scientific_name": "Rangifer tarandus",
        "aliases": ["Caribou"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_fallow_deer1",
        "common_name": "Fallow Deer",
        "scientific_name": "Dama dama",
        "aliases": [],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_roe_deer1",
        "common_name": "European Roe Deer",
        "scientific_name": "Capreolus capreolus",
        "aliases": ["Roe Deer"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_barasingha1",
        "common_name": "Barasingha",
        "scientific_name": "Rucervus duvaucelii",
        "aliases": ["Swamp Deer"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_muntjac1",
        "common_name": "Indian Muntjac",
        "scientific_name": "Muntiacus muntjak",
        "aliases": ["Barking Deer"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_blue_whale1",
        "common_name": "Blue Whale",
        "scientific_name": "Balaenoptera musculus",
        "aliases": ["Sulphur-bottom Whale"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_humpback1",
        "common_name": "Humpback Whale",
        "scientific_name": "Megaptera novaeangliae",
        "aliases": [],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_orca1",
        "common_name": "Orca",
        "scientific_name": "Orcinus orca",
        "aliases": ["Killer Whale"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_sperm_whale1",
        "common_name": "Sperm Whale",
        "scientific_name": "Physeter macrocephalus",
        "aliases": ["Cachalot"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_bottlenose_dol1",
        "common_name": "Common Bottlenose Dolphin",
        "scientific_name": "Tursiops truncatus",
        "aliases": ["Bottlenose Dolphin"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_spinner_dol1",
        "common_name": "Spinner Dolphin",
        "scientific_name": "Stenella longirostris",
        "aliases": [],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_ganga_dol1",
        "common_name": "Ganges River Dolphin",
        "scientific_name": "Platanista gangetica",
        "aliases": ["Susu"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_chimpanzee1",
        "common_name": "Chimpanzee",
        "scientific_name": "Pan troglodytes",
        "aliases": ["Common Chimpanzee", "Chimp"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_bonobo1",
        "common_name": "Bonobo",
        "scientific_name": "Pan paniscus",
        "aliases": ["Pygmy Chimpanzee"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_w_gorilla1",
        "common_name": "Western Gorilla",
        "scientific_name": "Gorilla gorilla",
        "aliases": ["Western Lowland Gorilla"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_e_gorilla1",
        "common_name": "Eastern Gorilla",
        "scientific_name": "Gorilla beringei",
        "aliases": ["Mountain Gorilla"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_bor_orang1",
        "common_name": "Bornean Orangutan",
        "scientific_name": "Pongo pygmaeus",
        "aliases": ["Orangutan"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_sum_orang1",
        "common_name": "Sumatran Orangutan",
        "scientific_name": "Pongo abelii",
        "aliases": [],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_lar_gibbon1",
        "common_name": "Lar Gibbon",
        "scientific_name": "Hylobates lar",
        "aliases": ["White-handed Gibbon"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_hoolock1",
        "common_name": "Western Hoolock Gibbon",
        "scientific_name": "Hoolock hoolock",
        "aliases": ["Hoolock"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_olive_baboon1",
        "common_name": "Olive Baboon",
        "scientific_name": "Papio anubis",
        "aliases": ["Anubis Baboon"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_chacma_baboon1",
        "common_name": "Chacma Baboon",
        "scientific_name": "Papio ursinus",
        "aliases": ["Cape Baboon"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_gelada1",
        "common_name": "Gelada",
        "scientific_name": "Theropithecus gelada",
        "aliases": ["Bleeding-heart Baboon"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_mandrill1",
        "common_name": "Mandrill",
        "scientific_name": "Mandrillus sphinx",
        "aliases": [],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_japanese_mac1",
        "common_name": "Japanese Macaque",
        "scientific_name": "Macaca fuscata",
        "aliases": ["Snow Monkey"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_lion_tail_mac1",
        "common_name": "Lion-tailed Macaque",
        "scientific_name": "Macaca silenus",
        "aliases": ["Wanderoo"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_bonnet_mac1",
        "common_name": "Bonnet Macaque",
        "scientific_name": "Macaca radiata",
        "aliases": [],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_proboscis_monk1",
        "common_name": "Proboscis Monkey",
        "scientific_name": "Nasalis larvatus",
        "aliases": ["Long-nosed Monkey"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_howler_monk1",
        "common_name": "Mantled Howler",
        "scientific_name": "Alouatta palliata",
        "aliases": ["Howler Monkey"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_capuchin1",
        "common_name": "White-headed Capuchin",
        "scientific_name": "Cebus capucinus",
        "aliases": ["Capuchin Monkey"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_squirrel_monk1",
        "common_name": "Common Squirrel Monkey",
        "scientific_name": "Saimiri sciureus",
        "aliases": [],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_ring_tail_lem1",
        "common_name": "Ring-tailed Lemur",
        "scientific_name": "Lemur catta",
        "aliases": ["Lemur"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_indri1",
        "common_name": "Indri",
        "scientific_name": "Indri indri",
        "aliases": ["Babakoto"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_am_beaver1",
        "common_name": "North American Beaver",
        "scientific_name": "Castor canadensis",
        "aliases": ["Beaver"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_eur_beaver1",
        "common_name": "Eurasian Beaver",
        "scientific_name": "Castor fiber",
        "aliases": [],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_capybara1",
        "common_name": "Capybara",
        "scientific_name": "Hydrochoerus hydrochaeris",
        "aliases": ["Giant Cavy"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_crest_porcupine1",
        "common_name": "Indian Crested Porcupine",
        "scientific_name": "Hystrix indica",
        "aliases": ["Porcupine"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_na_porcupine1",
        "common_name": "North American Porcupine",
        "scientific_name": "Erethizon dorsatum",
        "aliases": [],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_woodchuck1",
        "common_name": "Groundhog",
        "scientific_name": "Marmota monax",
        "aliases": ["Woodchuck"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_alpine_marmot1",
        "common_name": "Alpine Marmot",
        "scientific_name": "Marmota marmota",
        "aliases": [],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_bl_tail_prairie1",
        "common_name": "Black-tailed Prairie Dog",
        "scientific_name": "Cynomys ludovicianus",
        "aliases": ["Prairie Dog"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_eur_red_sq1",
        "common_name": "Eurasian Red Squirrel",
        "scientific_name": "Sciurus vulgaris",
        "aliases": ["Red Squirrel"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_ea_gray_sq1",
        "common_name": "Eastern Gray Squirrel",
        "scientific_name": "Sciurus carolinensis",
        "aliases": ["Gray Squirrel"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_fox_sq1",
        "common_name": "Fox Squirrel",
        "scientific_name": "Sciurus niger",
        "aliases": [],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_eur_hare1",
        "common_name": "European Hare",
        "scientific_name": "Lepus europaeus",
        "aliases": ["Brown Hare"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_snowshoe_hare1",
        "common_name": "Snowshoe Hare",
        "scientific_name": "Lepus americanus",
        "aliases": ["Varying Hare"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_am_pika1",
        "common_name": "American Pika",
        "scientific_name": "Ochotona princeps",
        "aliases": ["Pika"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_roy_pika1",
        "common_name": "Royle's Pika",
        "scientific_name": "Ochotona roylei",
        "aliases": ["Himalayan Pika"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_red_kangaroo1",
        "common_name": "Red Kangaroo",
        "scientific_name": "Osphranter rufus",
        "aliases": ["Kangaroo"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_e_gray_kang1",
        "common_name": "Eastern Gray Kangaroo",
        "scientific_name": "Macropus giganteus",
        "aliases": [],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_koala1",
        "common_name": "Koala",
        "scientific_name": "Phascolarctos cinereus",
        "aliases": ["Koala Bear"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_platypus1",
        "common_name": "Platypus",
        "scientific_name": "Ornithorhynchus anatinus",
        "aliases": ["Duck-billed Platypus"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_sh_echidna1",
        "common_name": "Short-beaked Echidna",
        "scientific_name": "Tachyglossus aculeatus",
        "aliases": ["Spiny Anteater"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_tas_devil1",
        "common_name": "Tasmanian Devil",
        "scientific_name": "Sarcophilus harrisii",
        "aliases": [],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_com_wombat1",
        "common_name": "Common Wombat",
        "scientific_name": "Vombatus ursinus",
        "aliases": ["Wombat"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_quokka1",
        "common_name": "Quokka",
        "scientific_name": "Setonix brachyurus",
        "aliases": [],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_sugar_glider1",
        "common_name": "Sugar Glider",
        "scientific_name": "Petaurus breviceps",
        "aliases": [],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_brush_possum1",
        "common_name": "Common Brushtail Possum",
        "scientific_name": "Trichosurus vulpecula",
        "aliases": ["Brushtail Possum"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_virg_opossum1",
        "common_name": "Virginia Opossum",
        "scientific_name": "Didelphis virginiana",
        "aliases": ["North American Opossum"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_giant_anteater1",
        "common_name": "Giant Anteater",
        "scientific_name": "Myrmecophaga tridactyla",
        "aliases": ["Ant Bear"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_br_sloth1",
        "common_name": "Brown-throated Sloth",
        "scientific_name": "Bradypus variegatus",
        "aliases": ["Three-toed Sloth"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_nine_band_arm1",
        "common_name": "Nine-banded Armadillo",
        "scientific_name": "Dasypus novemcinctus",
        "aliases": ["Armadillo"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_aardvark1",
        "common_name": "Aardvark",
        "scientific_name": "Orycteropus afer",
        "aliases": ["Antbear"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_indian_pang1",
        "common_name": "Indian Pangolin",
        "scientific_name": "Manis crassicaudata",
        "aliases": ["Scaly Anteater"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_fruit_bat1",
        "common_name": "Indian Flying Fox",
        "scientific_name": "Pteropus giganteus",
        "aliases": ["Greater Indian Fruit Bat"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_vamp_bat1",
        "common_name": "Common Vampire Bat",
        "scientific_name": "Desmodus rotundus",
        "aliases": ["Vampire Bat"],
    },
    {
        "category": "Mammalia",
        "taxon_id": "mamm_little_brown_bat1",
        "common_name": "Little Brown Bat",
        "scientific_name": "Myotis lucifugus",
        "aliases": [],
    },
    # --- Insecta (Prominent global Lepidoptera, Odonata, etc.) ---
    {
        "category": "Insecta",
        "taxon_id": "ins_peacock1",
        "common_name": "Malabar Banded Peacock",
        "scientific_name": "Papilio buddha",
        "aliases": ["Banded Peacock"],
    },
    {
        "category": "Insecta",
        "taxon_id": "ins_monarch1",
        "common_name": "Monarch Butterfly",
        "scientific_name": "Danaus plexippus",
        "aliases": ["Monarch"],
    },
    {
        "category": "Insecta",
        "taxon_id": "ins_mormon1",
        "common_name": "Common Mormon",
        "scientific_name": "Papilio polytes",
        "aliases": ["Mormon Butterfly"],
    },
    {
        "category": "Insecta",
        "taxon_id": "ins_blue_mormon1",
        "common_name": "Blue Mormon",
        "scientific_name": "Papilio polymnestor",
    },
    {
        "category": "Insecta",
        "taxon_id": "ins_jezebel1",
        "common_name": "Common Jezebel",
        "scientific_name": "Delias eucharis",
    },
    {
        "category": "Insecta",
        "taxon_id": "ins_blue_tiger1",
        "common_name": "Blue Tiger",
        "scientific_name": "Tirumala limniace",
    },
    {
        "category": "Insecta",
        "taxon_id": "ins_crimson_rose1",
        "common_name": "Crimson Rose",
        "scientific_name": "Pachliopta hector",
    },
    {
        "category": "Insecta",
        "taxon_id": "ins_globe_skimmer1",
        "common_name": "Globe Skimmer",
        "scientific_name": "Pantala flavescens",
        "aliases": ["Wandering Glider"],
    },
    {
        "category": "Insecta",
        "taxon_id": "ins_green_darner1",
        "common_name": "Common Green Darner",
        "scientific_name": "Anax junius",
        "aliases": ["Green Darner"],
    },
    # --- Scenery (Standard photographic landscape motifs) ---
    {
        "category": "Scenery",
        "taxon_id": "scen_aurora",
        "common_name": "Aurora Borealis",
        "scientific_name": "Aurora Borealis",
        "aliases": ["Northern Lights"],
    },
    {
        "category": "Scenery",
        "taxon_id": "scen_mountain",
        "common_name": "Mountain Landscape",
        "scientific_name": "Mountain Landscape",
        "aliases": ["Mountains", "Alpine Peak"],
    },
    {
        "category": "Scenery",
        "taxon_id": "scen_waterfall",
        "common_name": "Waterfall",
        "scientific_name": "Waterfall",
        "aliases": ["Cascades"],
    },
    {
        "category": "Scenery",
        "taxon_id": "scen_forest",
        "common_name": "Forest Canopy",
        "scientific_name": "Forest Canopy",
        "aliases": ["Woodland", "Forest"],
    },
    {
        "category": "Scenery",
        "taxon_id": "scen_sunset",
        "common_name": "Sunset Sky",
        "scientific_name": "Sunset Sky",
        "aliases": ["Sunrise", "Golden Hour"],
    },
    {
        "category": "Scenery",
        "taxon_id": "scen_desert",
        "common_name": "Desert Dunes",
        "scientific_name": "Desert Dunes",
        "aliases": ["Sand Dunes"],
    },
    {
        "category": "Scenery",
        "taxon_id": "scen_coastline",
        "common_name": "Ocean Coastline",
        "scientific_name": "Ocean Coastline",
        "aliases": ["Seashore", "Coast"],
    },
    {
        "category": "Scenery",
        "taxon_id": "scen_night_sky",
        "common_name": "Starry Night Sky",
        "scientific_name": "Starry Night Sky",
        "aliases": ["Milky Way", "Astrophotography"],
    },
    {
        "category": "Scenery",
        "taxon_id": "scen_rainbow",
        "common_name": "Rainbow over Landscape",
        "scientific_name": "Rainbow",
        "aliases": ["Rainbow", "Rainbow Sky"],
    },
    # --- Reptilia (Reptiles: crocodilians, lizards, snakes, turtles) ---
    {
        "category": "Reptilia",
        "taxon_id": "rept_mugger_croc",
        "common_name": "Mugger Crocodile",
        "scientific_name": "Crocodylus palustris",
        "aliases": ["Marsh Crocodile", "Broad-snouted Crocodile"],
    },
    {
        "category": "Reptilia",
        "taxon_id": "rept_bengal_monitor",
        "common_name": "Bengal Monitor",
        "scientific_name": "Varanus bengalensis",
        "aliases": ["Common Indian Monitor", "Monitor Lizard"],
    },
    {
        "category": "Reptilia",
        "taxon_id": "rept_indian_rock_python",
        "common_name": "Indian Rock Python",
        "scientific_name": "Python molurus",
        "aliases": ["Black-tailed Python", "Indian Python"],
    },
    # --- Amphibia (Amphibians: frogs, toads, salamanders) ---
    {
        "category": "Amphibia",
        "taxon_id": "amph_indian_bullfrog",
        "common_name": "Indian Bullfrog",
        "scientific_name": "Hoplobatrachus tigerinus",
        "aliases": ["Indus Valley Bullfrog", "Bullfrog"],
    },
    {
        "category": "Amphibia",
        "taxon_id": "amph_common_indian_toad",
        "common_name": "Common Indian Toad",
        "scientific_name": "Duttaphrynus melanostictus",
        "aliases": ["Asian Common Toad", "Black-spined Toad"],
    },
    # --- Botanical (Plants, Flowers, Trees, Fungi) ---
    {
        "category": "Botanical",
        "taxon_id": "plnt_rhododendron",
        "common_name": "Rhododendron",
        "scientific_name": "Rhododendron arboreum",
        "aliases": ["Burans", "Tree Rhododendron"],
    },
    {
        "category": "Botanical",
        "taxon_id": "plnt_lotus",
        "common_name": "Sacred Lotus",
        "scientific_name": "Nelumbo nucifera",
        "aliases": ["Indian Lotus", "Water Lily"],
    },
    {
        "category": "Botanical",
        "taxon_id": "plnt_banyan",
        "common_name": "Banyan Tree",
        "scientific_name": "Ficus benghalensis",
        "aliases": ["Indian Banyan", "Vada Tree"],
    },
    {
        "category": "Botanical",
        "taxon_id": "plnt_mushroom",
        "common_name": "Wild Mushroom",
        "scientific_name": "Agaricus campestris",
        "aliases": ["Fungus", "Field Mushroom"],
    },
    # --- Architecture (Monuments, Historic Buildings, Urban) ---
    {
        "category": "Architecture",
        "taxon_id": "arch_taj_mahal",
        "common_name": "Taj Mahal",
        "scientific_name": "Monumentum Taj Mahal",
        "aliases": ["Mausoleum", "Marble Palace"],
    },
    {
        "category": "Architecture",
        "taxon_id": "arch_hindu_temple",
        "common_name": "Historic Hindu Temple",
        "scientific_name": "Templum Sanctuarium",
        "aliases": ["Mandir", "Gopuram", "Ancient Temple"],
    },
    {
        "category": "Architecture",
        "taxon_id": "arch_fort_palace",
        "common_name": "Historic Fort and Palace",
        "scientific_name": "Castellum Fortalitia",
        "aliases": ["Fortress", "Ancient Fort", "Citadel"],
    },
    {
        "category": "Architecture",
        "taxon_id": "arch_modern_building",
        "common_name": "Modern Urban Skyscraper",
        "scientific_name": "Architectura Urbana",
        "aliases": ["Tower", "Skyscraper", "Contemporary Architecture"],
    },
    # --- Portrait (People, Human Subjects, Portraits) ---
    {
        "category": "Portrait",
        "taxon_id": "port_close_up",
        "common_name": "Close-up Human Portrait",
        "scientific_name": "Homo sapiens faciei",
        "aliases": ["Headshot", "Facial Portrait"],
    },
    {
        "category": "Portrait",
        "taxon_id": "port_cultural",
        "common_name": "Cultural Street Portrait",
        "scientific_name": "Homo sapiens cultus",
        "aliases": ["Street Portrait", "Environmental Portrait"],
    },
    # --- Abstract (Textures, Minimalist Patterns, Geometry) ---
    {
        "category": "Abstract",
        "taxon_id": "abst_water_ripples",
        "common_name": "Water Ripples and Refraction Texture",
        "scientific_name": "Textura Aquae",
        "aliases": ["Water Texture", "Fluid Reflection"],
    },
    {
        "category": "Abstract",
        "taxon_id": "abst_minimal_geometry",
        "common_name": "Minimalist Geometric Pattern",
        "scientific_name": "Pattern Geometria",
        "aliases": ["Abstract Geometry", "Architectural Shadow Pattern"],
    },
)


def load_curated_non_avian_taxa() -> tuple[TaxonomyEntry, ...]:
    """Return curated representative non-avian records (Mammalia, Insecta, Scenery)."""
    return parse_taxonomy_records(CURATED_MULTI_TAXA_RECORDS)


def chunk_sequence(seq: Sequence[T], chunk_size: int) -> tuple[tuple[T, ...], ...]:
    """Partition a sequence into contiguous immutable chunk tuples of maximum size chunk_size."""
    if chunk_size <= 0:
        raise ValueError(f"chunk_size must be positive, got {chunk_size}")
    if not seq:
        return ()
    return tuple(tuple(seq[i : i + chunk_size]) for i in range(0, len(seq), chunk_size))


def compute_taxonomy_checksum(entries: Sequence[TaxonomyEntry]) -> str:
    """Compute a deterministic SHA256 checksum across sorted taxonomy entries."""
    h = hashlib.sha256()
    for entry in entries:
        token = (
            f"{entry.species_code}|{entry.category}|{entry.scientific_name}|{entry.common_name}\n"
        )
        h.update(token.encode("utf-8"))
    return h.hexdigest()


def compute_sha256(filepath: str) -> str:
    """Compute hex-encoded SHA-256 digest of a local file on disk."""
    h = hashlib.sha256()
    with open(filepath, "rb") as f:
        while chunk := f.read(65536):
            h.update(chunk)
    return h.hexdigest()


def fetch_ebird_taxonomy() -> tuple[TaxonomyEntry, ...]:
    """Fetch global eBird taxonomy via Cornell API and parse into TaxonomyEntry records."""
    url = "https://api.ebird.org/v2/ref/taxonomy/ebird?fmt=json"
    logging.info(f"Fetching global eBird taxonomy from {url}...")
    req = urllib.request.Request(url, headers={"User-Agent": "avicull/1.0"})
    with urllib.request.urlopen(req, timeout=30) as resp:
        data = json.load(resp)

    entries = parse_taxonomy_records(data)
    logging.info(f"Loaded {len(entries)} valid avian species.")
    return entries


def fetch_multi_taxa_taxonomy(
    fetch_ebird_fn: Callable[[], Sequence[TaxonomyEntry]] = fetch_ebird_taxonomy,
    include_curated: bool = True,
) -> tuple[TaxonomyEntry, ...]:
    """Fetch global eBird taxonomy and merge with non-avian taxa (Mammalia, Insecta, Scenery)."""
    avian_entries = fetch_ebird_fn()
    if not include_curated:
        return tuple(avian_entries)

    non_avian_entries = load_curated_non_avian_taxa()
    seen_codes = {e.species_code for e in avian_entries}
    merged: list[TaxonomyEntry] = list(avian_entries)
    for entry in non_avian_entries:
        if entry.species_code not in seen_codes:
            seen_codes.add(entry.species_code)
            merged.append(entry)
    return tuple(merged)


def compute_species_embeddings(
    entries: Sequence[TaxonomyEntry],
    model: Any,
    tokenizer: Any,
    device: str,
    batch_size: int = 256,
) -> torch.Tensor:
    """Compute normalized visual text embeddings using BioCLIP for taxonomy entries."""
    import torch

    prompts = [generate_taxonomy_prompt(entry) for entry in entries]

    all_embeddings: list[torch.Tensor] = []
    chunks = chunk_sequence(prompts, batch_size)

    with torch.no_grad():
        for i, batch in enumerate(chunks):
            tokens = tokenizer(list(batch)).to(device)
            emb = model.encode_text(tokens)
            emb /= emb.norm(dim=-1, keepdim=True)
            all_embeddings.append(emb.cpu())
            if i % 10 == 0:
                processed_count = min((i + 1) * batch_size, len(prompts))
                logging.info(f"  Processed {processed_count}/{len(prompts)} species...")

    if not all_embeddings:
        return torch.empty((0, 512), dtype=torch.float32)

    return torch.cat(all_embeddings, dim=0).float()


def build_manifest(models_dir: str) -> dict[str, dict[str, Any]]:
    """Build sha256 checksum and file size manifest dictionary for a models directory."""
    manifest = {}
    for fname in sorted(os.listdir(models_dir)):
        p = os.path.join(models_dir, fname)
        if os.path.isfile(p) and fname != "manifest.json":
            manifest[fname] = {
                "size_bytes": os.path.getsize(p),
                "sha256": compute_sha256(p),
            }
    return manifest


def build_taxonomy_pipeline(
    models_dir: str,
    fetch_taxonomy_fn: Any = fetch_multi_taxa_taxonomy,
    compute_embeddings_fn: Any = None,
    copy_yolo_fn: Any = None,
) -> dict[str, Any]:
    """Execute complete taxonomy and embedding pipeline, writing artifacts into models_dir."""
    from safetensors.torch import save_file

    os.makedirs(models_dir, exist_ok=True)
    entries = fetch_taxonomy_fn()

    taxonomy_dicts = [entry.to_dict() for entry in entries]
    taxonomy_path = os.path.join(models_dir, "taxonomy.json")
    with open(taxonomy_path, "w", encoding="utf-8") as f:
        json.dump(taxonomy_dicts, f, indent=2)

    safetensors_path = os.path.join(models_dir, "species_embeddings.safetensors")
    if compute_embeddings_fn is not None:
        embeddings_tensor = compute_embeddings_fn(entries)
    else:
        import open_clip
        import torch

        device = "mps" if torch.backends.mps.is_available() else "cpu"
        model, _, _ = open_clip.create_model_and_transforms("hf-hub:imageomics/bioclip")
        tokenizer = open_clip.get_tokenizer("hf-hub:imageomics/bioclip")
        model.to(device).eval()
        embeddings_tensor = compute_species_embeddings(
            entries=entries,
            model=model,
            tokenizer=tokenizer,
            device=device,
            batch_size=256,
        )

    save_file({"embeddings": embeddings_tensor}, safetensors_path)

    if copy_yolo_fn is not None:
        copy_yolo_fn(models_dir)
    else:
        from ultralytics import YOLO

        yolo_target = os.path.join(models_dir, "yolo11n.pt")
        if not os.path.exists(yolo_target):
            YOLO("yolo11n.pt")
            import shutil

            if os.path.exists("yolo11n.pt"):
                shutil.copy("yolo11n.pt", yolo_target)

    manifest = build_manifest(models_dir)
    manifest_path = os.path.join(models_dir, "manifest.json")
    with open(manifest_path, "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=2)

    return manifest


def main() -> None:
    models_dir = sys.argv[1] if len(sys.argv) > 1 else "models"
    build_taxonomy_pipeline(models_dir)
    print("Done!")


if __name__ == "__main__":
    main()
