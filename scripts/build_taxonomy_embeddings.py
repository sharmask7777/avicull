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

CANONICAL_CATEGORIES: tuple[str, ...] = ("Bird", "Mammalia", "Insecta", "Scenery")

CATEGORY_MAPPING: dict[str, str] = {
    "bird": "Bird",
    "birds": "Bird",
    "mammal": "Mammalia",
    "mammalia": "Mammalia",
    "mammals": "Mammalia",
    "insect": "Insecta",
    "insecta": "Insecta",
    "insects": "Insecta",
    "scenery": "Scenery",
    "landscape": "Scenery",
}

CATEGORY_PROMPT_NOUNS: dict[str, str] = {
    "Bird": "bird",
    "Mammalia": "mammal",
    "Insecta": "insect",
    "Scenery": "scenery",
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
            resolved_category = normalize_category(raw_cat)
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
