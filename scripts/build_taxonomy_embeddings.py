#!/usr/bin/env python3
"""Downloads Cornell eBird global bird taxonomy (11,167 species) and computes
precomputed normalized text embeddings using BioCLIP (imageomics/bioclip) on Apple Silicon MPS/GPU.

Saves:
- models/taxonomy.json
- models/species_embeddings.safetensors
- models/manifest.json
"""

import hashlib
import json
import logging
import os
import re
import sys
import urllib.request
from collections.abc import Sequence
from dataclasses import dataclass
from typing import Any, TypeVar

import open_clip
import torch
from safetensors.torch import save_file

T = TypeVar("T")


@dataclass(frozen=True)
class TaxonomyEntry:
    """Immutable representation of an eBird species taxonomy record.

    Attributes:
        species_code: eBird unique alphanumeric species identifier (e.g. 'brakit1').
        scientific_name: Binomial scientific nomenclature (e.g. 'Haliastur indus').
        common_name: English common vernacular name (e.g. 'Brahminy Kite').
        aliases: Alternative common vernacular or subspecies names.
    """

    species_code: str
    scientific_name: str
    common_name: str
    aliases: tuple[str, ...] = ()

    @property
    def folder_slug(self) -> str:
        """Sanitized folder-friendly name slug derived from the common name."""
        return sanitize_species_slug(self.common_name)

    def to_dict(self) -> dict[str, Any]:
        """Convert entry into serializable dictionary format for taxonomy.json."""
        record: dict[str, Any] = {
            "taxon_id": self.species_code,
            "common_name": self.common_name,
            "folder_slug": self.folder_slug,
            "scientific_name": self.scientific_name,
        }
        if self.aliases:
            record["aliases"] = list(self.aliases)
        return record


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


def parse_taxonomy_records(raw_records: Sequence[dict[str, Any]]) -> tuple[TaxonomyEntry, ...]:
    """Parse and validate raw eBird taxonomy API dictionaries into immutable TaxonomyEntry records.

    Filters to confirmed bird species, validates required fields, and deduplicates by species code.
    """
    entries: list[TaxonomyEntry] = []
    seen_codes: set[str] = set()

    for item in raw_records:
        if item.get("category") != "species":
            continue

        sp_code = item.get("speciesCode")
        com_name = item.get("comName")
        sci_name = item.get("sciName", "")

        if not sp_code or not isinstance(sp_code, str):
            continue
        if not com_name or not isinstance(com_name, str):
            continue

        clean_code = sp_code.strip()
        if not clean_code or clean_code in seen_codes:
            continue

        seen_codes.add(clean_code)
        aliases = tuple(item.get("aliases", ()))
        entries.append(
            TaxonomyEntry(
                species_code=clean_code,
                scientific_name=sci_name.strip() if isinstance(sci_name, str) else "",
                common_name=com_name.strip(),
                aliases=aliases,
            )
        )

    return tuple(entries)


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
        token = f"{entry.species_code}|{entry.scientific_name}|{entry.common_name}\n"
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


def compute_species_embeddings(
    entries: Sequence[TaxonomyEntry],
    model: Any,
    tokenizer: Any,
    device: str,
    batch_size: int = 256,
) -> torch.Tensor:
    """Compute normalized visual text embeddings using BioCLIP for taxonomy entries."""
    prompts = [
        f"a photo of a {entry.common_name} ({entry.scientific_name}), a type of bird."
        for entry in entries
    ]

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
    fetch_taxonomy_fn: Any = fetch_ebird_taxonomy,
    compute_embeddings_fn: Any = None,
    copy_yolo_fn: Any = None,
) -> dict[str, Any]:
    """Execute complete taxonomy and embedding pipeline, writing artifacts into models_dir."""
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
        # Default BioCLIP loading
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
