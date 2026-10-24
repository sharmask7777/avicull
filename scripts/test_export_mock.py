import json
import os
import subprocess
import sys
import tempfile

def test_mock_export():
    with tempfile.TemporaryDirectory() as tmpdir:
        cmd = [
            sys.executable,
            "scripts/export_models.py",
            "--output-dir",
            tmpdir,
            "--mock"
        ]
        res = subprocess.run(cmd, capture_output=True, text=True)
        assert res.returncode == 0, f"export failed: stdout={res.stdout}\nstderr={res.stderr}"

        # Verify output files
        expected_files = [
            "yolo11m.onnx",
            "bioclip_vision.onnx",
            "sinr_geoprior.onnx",
            "species_embeddings.safetensors",
            "taxonomy.json",
            "manifest.json"
        ]
        for f in expected_files:
            path = os.path.join(tmpdir, f)
            assert os.path.exists(path), f"Missing expected output file: {f}"

        # Verify manifest.json format
        manifest_path = os.path.join(tmpdir, "manifest.json")
        with open(manifest_path) as mf:
            data = json.load(mf)
        for f in expected_files:
            if f != "manifest.json":
                assert f in data, f"Manifest missing entry for {f}"
                assert "sha256" in data[f]
                assert len(data[f]["sha256"]) == 64

        # Verify taxonomy.json
        tax_path = os.path.join(tmpdir, "taxonomy.json")
        with open(tax_path) as tf:
            tax = json.load(tf)
        assert isinstance(tax, list)
        assert len(tax) > 0
        assert "taxon_id" in tax[0]
        assert "common_name" in tax[0]
        assert "folder_slug" in tax[0]

    print("test_mock_export: PASS")

if __name__ == "__main__":
    test_mock_export()
