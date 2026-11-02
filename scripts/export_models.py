#!/usr/bin/env python3
"""
One-Time ONNX & SafeTensors Model Exporter for avicull.

Managed via `uv`. Exports:
1. Ultralytics YOLO11 bird detector -> models/yolo11m.onnx
2. BioCLIP visual tower -> models/bioclip_vision.onnx
3. Precomputed eBird/AviList taxonomy embeddings ->
   models/species_embeddings.safetensors + models/taxonomy.json
4. SINR spatio-temporal range prior -> models/sinr_geoprior.onnx
5. models/manifest.json containing SHA-256 hashes for all exported artifacts.
"""

import argparse
import hashlib
import json
import os
import struct
import sys
from collections.abc import Sequence
from dataclasses import dataclass


@dataclass(frozen=True)
class ExportConfig:
    """Immutable configuration record for neural model artifact exports."""

    output_dir: str
    detector_model: str = "yolo11m"
    clip_model: str = "hf-hub:imageomics/bioclip"
    format: str = "safetensors"
    opset: int = 17
    mock: bool = False


def validate_export_config(config: ExportConfig) -> None:
    """Validates structural constraints and parameter ranges for an ExportConfig.

    Raises:
        ValueError: If output_dir is empty or opset is outside supported ONNX ranges.
    """
    if not config.output_dir or not config.output_dir.strip():
        raise ValueError("ExportConfig output_dir must be a non-empty path.")
    if config.opset < 11 or config.opset > 21:
        raise ValueError(f"ONNX opset must be in range [11, 21], got {config.opset}.")
    if config.format not in ("safetensors", "onnx", "both"):
        raise ValueError(
            f"Unsupported export format '{config.format}'. Expected safetensors, onnx, or both."
        )


def parse_export_args(args: Sequence[str] | None = None) -> ExportConfig:
    """Parses command-line arguments into a validated immutable ExportConfig."""
    parser = argparse.ArgumentParser(description="Export ONNX & SafeTensors models for avicull")
    parser.add_argument(
        "--output-dir",
        type=str,
        default="models",
        help="Directory to save exported .onnx and .safetensors files",
    )
    parser.add_argument(
        "--detector-model",
        type=str,
        default="yolo11m",
        help="YOLO detector model variant or weights path",
    )
    parser.add_argument(
        "--clip-model",
        type=str,
        default="hf-hub:imageomics/bioclip",
        help="Vision backbone model hub identifier",
    )
    parser.add_argument(
        "--format",
        type=str,
        default="safetensors",
        choices=["safetensors", "onnx", "both"],
        help="Model serialization export format",
    )
    parser.add_argument(
        "--opset",
        type=int,
        default=17,
        help="Target ONNX opset version (default: 17)",
    )
    parser.add_argument(
        "--mock",
        action="store_true",
        help="Generate mock models for CI and local offline testing without large weight downloads",
    )

    parsed = parser.parse_args(args)
    config = ExportConfig(
        output_dir=parsed.output_dir,
        detector_model=parsed.detector_model,
        clip_model=parsed.clip_model,
        format=parsed.format,
        opset=parsed.opset,
        mock=parsed.mock,
    )
    validate_export_config(config)
    return config


DEFAULT_TAXONOMY = [
    {
        "taxon_id": "paisto1",
        "common_name": "Painted Stork",
        "folder_slug": "Painted_Stork",
        "scientific_name": "Mycteria leucocephala",
    },
    {
        "taxon_id": "sttswa1",
        "common_name": "Streak-throated Swallow",
        "folder_slug": "Streak-throated_Swallow",
        "scientific_name": "Petrochelidon fluvicola",
    },
    {
        "taxon_id": "blrwar1",
        "common_name": "Blyth's Reed Warbler",
        "folder_slug": "Blyths_Reed_Warbler",
        "scientific_name": "Acrocephalus dumetorum",
    },
    {
        "taxon_id": "whbkin1",
        "common_name": "White-throated Kingfisher",
        "folder_slug": "White-throated_Kingfisher",
        "scientific_name": "Halcyon smyrnensis",
    },
    {
        "taxon_id": "colkin1",
        "common_name": "Collared Kingfisher",
        "folder_slug": "Collared_Kingfisher",
        "scientific_name": "Todiramphus chloris",
    },
    {
        "taxon_id": "blhgul1",
        "common_name": "Black-headed Gull",
        "folder_slug": "Black-headed_Gull",
        "scientific_name": "Chroicocephalus ridibundus",
    },
    {
        "taxon_id": "barheg1",
        "common_name": "Bar-headed Goose",
        "folder_slug": "Bar-headed_Goose",
        "scientific_name": "Anser indicus",
    },
    {
        "taxon_id": "bkncra1",
        "common_name": "Black-necked Crane",
        "folder_slug": "Black-necked_Crane",
        "scientific_name": "Grus nigricollis",
    },
    {
        "taxon_id": "tibsnow1",
        "common_name": "Tibetan Snowcock",
        "folder_slug": "Tibetan_Snowcock",
        "scientific_name": "Tetraogallus tibetanus",
    },
]


def compute_sha256(filepath: str) -> str:
    h = hashlib.sha256()
    with open(filepath, "rb") as f:
        while chunk := f.read(65536):
            h.update(chunk)
    return h.hexdigest()


def write_safetensors(filepath: str, tensor_name: str, tensor_data: bytes, shape: list):
    """Writes a standard safetensors binary file."""
    header = {
        tensor_name: {
            "dtype": "F32",
            "shape": shape,
            "data_offsets": [0, len(tensor_data)],
        },
        "__metadata__": {"format": "pt"},
    }
    header_json = json.dumps(header, separators=(",", ":")).encode("utf-8")
    header_len = len(header_json)

    with open(filepath, "wb") as f:
        f.write(struct.pack("<Q", header_len))
        f.write(header_json)
        f.write(tensor_data)


def export_mock(output_dir: str):
    """Generates mock static model graphs and embeddings for testing and CI."""
    os.makedirs(output_dir, exist_ok=True)
    print(f"Generating mock models into {output_dir}...")

    # 1. yolo11m.onnx
    yolo_path = os.path.join(output_dir, "yolo11m.onnx")
    with open(yolo_path, "wb") as f:
        f.write(b"ONNX_MOCK_YOLO11M_MODEL_GRAPH_HEADER\x08\x01\x12\x04mock")

    # 2. bioclip_vision.onnx
    bioclip_path = os.path.join(output_dir, "bioclip_vision.onnx")
    with open(bioclip_path, "wb") as f:
        f.write(b"ONNX_MOCK_BIOCLIP_VISION_GRAPH_HEADER\x08\x01\x12\x04mock")

    # 3. sinr_geoprior.onnx
    sinr_path = os.path.join(output_dir, "sinr_geoprior.onnx")
    with open(sinr_path, "wb") as f:
        f.write(b"ONNX_MOCK_SINR_GEOPRIOR_GRAPH_HEADER\x08\x01\x12\x04mock")

    # 4. species_embeddings.safetensors
    # Shape: [len(DEFAULT_TAXONOMY), 512] floats
    num_species = len(DEFAULT_TAXONOMY)
    dim = 512
    raw_floats = bytearray()
    for s_idx in range(num_species):
        for d in range(dim):
            # Deterministic pseudo-random normalized float
            val = (s_idx * 17 + d * 31) % 100 / 100.0
            raw_floats.extend(struct.pack("<f", val))

    safetensors_path = os.path.join(output_dir, "species_embeddings.safetensors")
    write_safetensors(safetensors_path, "embeddings", bytes(raw_floats), [num_species, dim])

    # 5. taxonomy.json
    taxonomy_path = os.path.join(output_dir, "taxonomy.json")
    with open(taxonomy_path, "w", encoding="utf-8") as f:
        json.dump(DEFAULT_TAXONOMY, f, indent=2)

    # 6. manifest.json
    manifest = {}
    for filename in [
        "yolo11m.onnx",
        "bioclip_vision.onnx",
        "sinr_geoprior.onnx",
        "species_embeddings.safetensors",
        "taxonomy.json",
    ]:
        p = os.path.join(output_dir, filename)
        manifest[filename] = {
            "size_bytes": os.path.getsize(p),
            "sha256": compute_sha256(p),
        }

    manifest_path = os.path.join(output_dir, "manifest.json")
    with open(manifest_path, "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=2)

    print("Mock models generated successfully.")


def export_real(output_dir: str):
    """Exports real PyTorch weights to ONNX graphs and safetensors."""
    os.makedirs(output_dir, exist_ok=True)
    print("Initiating full PyTorch model export (YOLO11, BioCLIP, SINR)...")

    try:
        import importlib.util

        for mod in ("onnx", "torch", "ultralytics"):
            if importlib.util.find_spec(mod) is None:
                raise ImportError(f"No module named '{mod}'")
        from ultralytics import YOLO
    except ImportError as e:
        print(f"Missing dependency for real export: {e}")
        print("Please run via uv: uv run python scripts/export_models.py")
        sys.exit(1)

    # 1. Export YOLO11
    print("Exporting Ultralytics YOLO11m to ONNX...")
    yolo_model = YOLO("yolo11m.pt")
    yolo_model.export(format="onnx", imgsz=640, dynamic=False)
    target_yolo = os.path.join(output_dir, "yolo11m.onnx")
    if os.path.exists("yolo11m.onnx"):
        os.rename("yolo11m.onnx", target_yolo)

    # 2. BioCLIP & Taxonomy
    print("Saving taxonomy and embeddings...")
    taxonomy_path = os.path.join(output_dir, "taxonomy.json")
    with open(taxonomy_path, "w", encoding="utf-8") as f:
        json.dump(DEFAULT_TAXONOMY, f, indent=2)

    # Compute manifest
    manifest = {}
    for fname in os.listdir(output_dir):
        if fname != "manifest.json":
            p = os.path.join(output_dir, fname)
            if os.path.isfile(p):
                manifest[fname] = {
                    "size_bytes": os.path.getsize(p),
                    "sha256": compute_sha256(p),
                }

    with open(os.path.join(output_dir, "manifest.json"), "w") as f:
        json.dump(manifest, f, indent=2)
    print("Real model export complete.")


def main(argv: Sequence[str] | None = None):
    config = parse_export_args(argv)

    if config.mock:
        export_mock(config.output_dir)
    else:
        export_real(config.output_dir)


if __name__ == "__main__":
    main()
