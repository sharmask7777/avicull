#!/usr/bin/env python3
"""
scripts/create_local_mini_dataset.py

Constructs a minimal, reproducible, local end-to-end testing dataset from a user's
local photo tree (e.g. Pictures/pictures_bangalore-2025- or any user photo directory).

The dataset is created under `tests/fixtures/local_user_samples/` (git-ignored),
preserving the exact multi-level trip hierarchy, RAW/JPEG bundles, and sidecar .xmp files.
"""

import argparse
import os
import shutil
import sys
from pathlib import Path

# Curated relative paths representing distinct trips, nested subdirectories,
# species diversity, and bundled sidecars (.xmp).
DEFAULT_CURATED_PATHS = [
    # Trip 1: Ranthambore (nested 'processed - oct 2 safari' with .xmp sidecar)
    (
        "ranthambore oct 2025/processed - oct 2 safari/1X7A0806_Needs_Review_Red-vented_Bulbul_20251002_Ranthambore.jpg",
        "ranthambore oct 2025/processed - oct 2 safari/1X7A0806_Needs_Review_Red-vented_Bulbul_20251002_Ranthambore.jpg.xmp",
    ),
    # Trip 2: Coorg (nested 'r7/processed' with .xmp sidecar)
    (
        "coorg Dec 2025/r7/processed/1X7A8099_Needs_Review_Purple_Finch_20251221_Coorg.jpg",
        "coorg Dec 2025/r7/processed/1X7A8099_Needs_Review_Purple_Finch_20251221_Coorg.jpg.xmp",
    ),
    # Trip 3: K Gudi (nested 'day 3 birding walk/processed' with .xmp sidecar)
    (
        "K Gudi 2025/day 3 birding walk/processed/1X7A7827_Rose-ringed_Parakeet_20250518_K_Gudi.jpg",
        "K Gudi 2025/day 3 birding walk/processed/1X7A7827_Rose-ringed_Parakeet_20250518_K_Gudi.jpg.xmp",
    ),
    # Trip 4: New Zealand (nested 'rotorua waterfront day 1' without .xmp sidecar, ultra-lightweight)
    (
        "New zealand 2025/rotorua waterfront day 1/1X7A4566_Great_Black-backed_Gull_20251124_New_Zealand.jpg",
        None,
    ),
]

# Gazetteers defining explicit coordinates for non-interactive local testing
DEFAULT_GAZETTEERS = {
    "ranthambore oct 2025": ('trip_name = "Ranthambore"\nlatitude = 26.0173\nlongitude = 76.5026\n',),
    "coorg Dec 2025": ('trip_name = "Coorg"\nlatitude = 12.3375\nlongitude = 75.8069\n',),
    "K Gudi 2025": ('trip_name = "K Gudi"\nlatitude = 11.9960\nlongitude = 77.1350\n',),
    "New zealand 2025": ('trip_name = "Rotorua"\nlatitude = -38.1368\nlongitude = 176.2497\n',),
}



def create_mini_dataset(source_root: Path, target_dir: Path, force: bool = False) -> int:
    """Copies minimal representative samples from source_root into target_dir."""
    if not source_root.exists():
        print(f"Error: source directory '{source_root}' does not exist.", file=sys.stderr)
        return 1

    if target_dir.exists() and force:
        print(f"Cleaning existing target directory '{target_dir}'...")
        shutil.rmtree(target_dir)

    target_dir.mkdir(parents=True, exist_ok=True)
    copied_count = 0
    total_bytes = 0

    print(f"Creating minimal local test dataset in '{target_dir}' from '{source_root}'...")

    for img_rel, xmp_rel in DEFAULT_CURATED_PATHS:
        src_img = source_root / img_rel
        if not src_img.exists():
            print(f"Warning: Candidate image '{src_img}' not found, skipping.", file=sys.stderr)
            continue

        dst_img = target_dir / img_rel
        dst_img.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src_img, dst_img)
        copied_count += 1
        total_bytes += dst_img.stat().st_size
        print(f"  + Copied photo: {img_rel} ({dst_img.stat().st_size} bytes)")

        if xmp_rel:
            src_xmp = source_root / xmp_rel
            if src_xmp.exists():
                dst_xmp = target_dir / xmp_rel
                dst_xmp.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(src_xmp, dst_xmp)
                copied_count += 1
                total_bytes += dst_xmp.stat().st_size
                print(f"  + Copied sidecar: {xmp_rel} ({dst_xmp.stat().st_size} bytes)")
            else:
                print(f"  ! Sidecar not found: {xmp_rel}", file=sys.stderr)

    for trip_rel, (content,) in DEFAULT_GAZETTEERS.items():
        gazetteer_path = target_dir / trip_rel / "gazetteer.toml"
        gazetteer_path.parent.mkdir(parents=True, exist_ok=True)
        gazetteer_path.write_text(content)
        copied_count += 1
        print(f"  + Created gazetteer: {trip_rel}/gazetteer.toml")

    print(f"\nSuccessfully created mini dataset with {copied_count} files ({total_bytes / 1024 / 1024:.2f} MB).")

    return 0


def main():
    parser = argparse.ArgumentParser(
        description="Create a minimal local test dataset from a user photo directory hierarchy."
    )
    default_src = Path.home() / "Library/CloudStorage/ProtonDrive-mail@shaleen.in/Pictures/pictures_bangalore-2025-"
    default_dst = Path(__file__).resolve().parent.parent / "tests/fixtures/local_user_samples"

    parser.add_argument(
        "--source",
        type=Path,
        default=default_src,
        help=f"Source photo directory (default: {default_src})",
    )
    parser.add_argument(
        "--target",
        type=Path,
        default=default_dst,
        help=f"Target fixture directory (default: {default_dst})",
    )
    parser.add_argument(
        "--force",
        action="store_true",
        help="Overwrite target directory if it already exists",
    )

    args = parser.parse_args()
    sys.exit(create_mini_dataset(args.source, args.target, args.force))


if __name__ == "__main__":
    main()
