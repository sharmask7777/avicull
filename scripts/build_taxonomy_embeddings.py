#!/usr/bin/env python3
"""
Downloads Cornell eBird global bird taxonomy (11,167 species) and computes
precomputed normalized text embeddings using BioCLIP (imageomics/bioclip) on Apple Silicon MPS/GPU.
Saves:
- models/taxonomy.json
- models/species_embeddings.safetensors
- models/manifest.json
"""

import hashlib
import json
import os
import re
import struct
import sys
import time
import urllib.request

import open_clip
import torch
from safetensors.torch import save_file


def sanitize_slug(name: str) -> str:
    cleaned = re.sub(r"[^\w\s-]", "", name)
    cleaned = re.sub(r"[\s-]+", "_", cleaned)
    return cleaned.strip("_")


def compute_sha256(filepath: str) -> str:
    h = hashlib.sha256()
    with open(filepath, "rb") as f:
        while chunk := f.read(65536):
            h.update(chunk)
    return h.hexdigest()


def fetch_ebird_taxonomy() -> list:
    url = "https://api.ebird.org/v2/ref/taxonomy/ebird?fmt=json"
    print(f"Fetching global eBird taxonomy from {url}...")
    req = urllib.request.Request(url, headers={"User-Agent": "avicull/1.0"})
    with urllib.request.urlopen(req, timeout=30) as resp:
        data = json.load(resp)

    species_list = []
    seen = set()
    for item in data:
        if item.get("category") == "species":
            sp_code = item.get("speciesCode")
            com_name = item.get("comName")
            sci_name = item.get("sciName")
            if sp_code and com_name and sp_code not in seen:
                seen.add(sp_code)
                species_list.append({
                    "taxon_id": sp_code,
                    "common_name": com_name,
                    "folder_slug": sanitize_slug(com_name),
                    "scientific_name": sci_name,
                })
    print(f"Loaded {len(species_list)} valid avian species.")
    return species_list


def main():
    models_dir = sys.argv[1] if len(sys.argv) > 1 else "models"
    os.makedirs(models_dir, exist_ok=True)

    taxonomy = fetch_ebird_taxonomy()

    taxonomy_path = os.path.join(models_dir, "taxonomy.json")
    with open(taxonomy_path, "w", encoding="utf-8") as f:
        json.dump(taxonomy, f, indent=2)
    print(f"Saved taxonomy to {taxonomy_path}")

    # Load BioCLIP model
    print("Loading BioCLIP model...")
    device = "mps" if torch.backends.mps.is_available() else "cpu"
    model, _, _ = open_clip.create_model_and_transforms("hf-hub:imageomics/bioclip")
    tokenizer = open_clip.get_tokenizer("hf-hub:imageomics/bioclip")
    model.to(device).eval()

    # Create prompts for all species
    # Format: "a photo of a {common_name} ({scientific_name}), a type of bird."
    prompts = [
        f"a photo of a {item['common_name']} ({item['scientific_name']}), a type of bird."
        for item in taxonomy
    ]

    print(f"Computing embeddings for {len(prompts)} species on {device}...")
    batch_size = 256
    all_embeddings = []
    t0 = time.time()
    with torch.no_grad():
        for i in range(0, len(prompts), batch_size):
            batch = prompts[i:i + batch_size]
            tokens = tokenizer(batch).to(device)
            emb = model.encode_text(tokens)
            emb /= emb.norm(dim=-1, keepdim=True)
            all_embeddings.append(emb.cpu())
            if (i // batch_size) % 10 == 0:
                print(f"  Processed {min(i + batch_size, len(prompts))}/{len(prompts)} species...")

    embeddings_tensor = torch.cat(all_embeddings, dim=0).float()
    print(f"Finished embedding calculation in {time.time() - t0:.2f}s. Shape: {embeddings_tensor.shape}")

    safetensors_path = os.path.join(models_dir, "species_embeddings.safetensors")
    save_file({"embeddings": embeddings_tensor}, safetensors_path)
    print(f"Saved embeddings to {safetensors_path} ({os.path.getsize(safetensors_path) / (1024*1024):.2f} MB)")

    # Download yolo11n.pt if not present
    from ultralytics import YOLO
    yolo_target = os.path.join(models_dir, "yolo11n.pt")
    if not os.path.exists(yolo_target):
        print("Downloading YOLO11n weights...")
        y = YOLO("yolo11n.pt")
        import shutil
        if os.path.exists("yolo11n.pt"):
            shutil.copy("yolo11n.pt", yolo_target)

    # Update manifest.json
    manifest = {}
    for fname in os.listdir(models_dir):
        p = os.path.join(models_dir, fname)
        if os.path.isfile(p) and fname != "manifest.json":
            manifest[fname] = {
                "size_bytes": os.path.getsize(p),
                "sha256": compute_sha256(p),
            }
    with open(os.path.join(models_dir, "manifest.json"), "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=2)
    print(f"Updated manifest at {os.path.join(models_dir, 'manifest.json')}")
    print("Done!")


if __name__ == "__main__":
    main()
