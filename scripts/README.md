# Model Exporter for avicull

This directory contains `export_models.py`, which is responsible for exporting pre-trained neural networks to static ONNX graphs and precomputing taxonomy embeddings for Apple Silicon hardware acceleration.

## Usage

### Fast Local Testing & CI
Generate lightweight mock models with verified schemas, safetensors, and manifest hashes:
```bash
python3 scripts/export_models.py --output-dir models/ --mock
```

### Full Production Export
Using `uv`:
```bash
uv run --project scripts python scripts/export_models.py --output-dir models/
```

### Exported Artifacts
- `models/yolo11m.onnx`: YOLO11 bird detector (COCO class 14)
- `models/bioclip_vision.onnx`: BioCLIP visual embedding tower
- `models/sinr_geoprior.onnx`: SINR spatio-temporal prior network
- `models/species_embeddings.safetensors`: Precomputed normalized text embeddings
- `models/taxonomy.json`: eBird/AviList species metadata mapping
- `models/manifest.json`: Cryptographic SHA-256 hashes of all exported files
