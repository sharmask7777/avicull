import json
import os
import sys
import tempfile
import unittest
from dataclasses import FrozenInstanceError
from unittest.mock import MagicMock, patch

import pytest

# Ensure repository root is on sys.path
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from scripts.export_models import (
    ExportConfig,
    compute_sha256,
    export_mock,
    export_real,
    main,
    parse_export_args,
    validate_export_config,
    write_safetensors,
)


class TestExportModels(unittest.TestCase):
    def test_export_config_immutability(self):
        config = ExportConfig(
            output_dir="models",
            detector_model="yolo11m",
            clip_model="hf-hub:imageomics/bioclip",
            format="safetensors",
            opset=17,
            mock=True,
        )
        self.assertEqual(config.output_dir, "models")
        self.assertEqual(config.detector_model, "yolo11m")
        self.assertEqual(config.clip_model, "hf-hub:imageomics/bioclip")
        self.assertEqual(config.format, "safetensors")
        self.assertEqual(config.opset, 17)
        self.assertTrue(config.mock)

        with pytest.raises(FrozenInstanceError):
            config.output_dir = "other"  # type: ignore[misc]

    def test_validate_export_config_valid(self):
        config = ExportConfig(output_dir="my_models", format="safetensors", opset=17)
        validate_export_config(config)

    def test_validate_export_config_invalid_dir(self):
        with pytest.raises(ValueError, match="output_dir must be a non-empty path"):
            validate_export_config(ExportConfig(output_dir=""))
        with pytest.raises(ValueError, match="output_dir must be a non-empty path"):
            validate_export_config(ExportConfig(output_dir="   "))

    def test_validate_export_config_invalid_opset(self):
        with pytest.raises(ValueError, match="ONNX opset must be in range"):
            validate_export_config(ExportConfig(output_dir="models", opset=5))
        with pytest.raises(ValueError, match="ONNX opset must be in range"):
            validate_export_config(ExportConfig(output_dir="models", opset=25))

    def test_validate_export_config_invalid_format(self):
        with pytest.raises(ValueError, match="Unsupported export format"):
            validate_export_config(ExportConfig(output_dir="models", format="invalid_fmt"))

    def test_parse_export_args_defaults(self):
        config = parse_export_args([])
        self.assertEqual(config.output_dir, "models")
        self.assertEqual(config.detector_model, "yolo11m")
        self.assertEqual(config.clip_model, "hf-hub:imageomics/bioclip")
        self.assertEqual(config.format, "safetensors")
        self.assertEqual(config.opset, 17)
        self.assertFalse(config.mock)

    def test_parse_export_args_custom(self):
        args = [
            "--output-dir",
            "custom_dir",
            "--detector-model",
            "yolo11x.pt",
            "--clip-model",
            "custom_clip",
            "--format",
            "onnx",
            "--opset",
            "18",
            "--mock",
        ]
        config = parse_export_args(args)
        self.assertEqual(config.output_dir, "custom_dir")
        self.assertEqual(config.detector_model, "yolo11x.pt")
        self.assertEqual(config.clip_model, "custom_clip")
        self.assertEqual(config.format, "onnx")
        self.assertEqual(config.opset, 18)
        self.assertTrue(config.mock)

    def test_compute_sha256_and_write_safetensors(self):
        with tempfile.TemporaryDirectory() as tmp_dir:
            f_path = os.path.join(tmp_dir, "sample.safetensors")
            sample_bytes = b"\x00\x00\x80?\x00\x00\x00@"  # two floats: 1.0, 2.0
            write_safetensors(f_path, "test_tensor", sample_bytes, [1, 2])

            self.assertTrue(os.path.exists(f_path))
            sha = compute_sha256(f_path)
            self.assertEqual(len(sha), 64)

    def test_export_mock_and_manifest(self):
        with tempfile.TemporaryDirectory() as tmp_dir:
            export_mock(tmp_dir)
            expected_files = [
                "yolo11m.onnx",
                "bioclip_vision.onnx",
                "sinr_geoprior.onnx",
                "species_embeddings.safetensors",
                "taxonomy.json",
                "manifest.json",
            ]
            for f in expected_files:
                p = os.path.join(tmp_dir, f)
                self.assertTrue(os.path.exists(p), f"Missing {f}")

            with open(os.path.join(tmp_dir, "manifest.json")) as mf:
                manifest = json.load(mf)
            self.assertEqual(len(manifest), 5)
            self.assertIn("yolo11m.onnx", manifest)

    def test_main_cli_dispatch_mock(self):
        with tempfile.TemporaryDirectory() as tmp_dir:
            main(["--output-dir", tmp_dir, "--mock"])
            self.assertTrue(os.path.exists(os.path.join(tmp_dir, "manifest.json")))

    def test_export_real_mocked_dependencies(self):
        with tempfile.TemporaryDirectory() as tmp_dir:
            mock_yolo_instance = MagicMock()
            mock_yolo_class = MagicMock(return_value=mock_yolo_instance)

            # Create dummy yolo11m.onnx in cwd when export is called
            def fake_export(**kwargs):
                with open("yolo11m.onnx", "wb") as f:
                    f.write(b"mock_onnx")

            mock_yolo_instance.export.side_effect = fake_export

            mock_spec = MagicMock()
            mock_onnx = MagicMock(__spec__=mock_spec)
            mock_torch = MagicMock(__spec__=mock_spec)
            mock_ultralytics = MagicMock(__spec__=mock_spec, YOLO=mock_yolo_class)

            with patch.dict(
                "sys.modules",
                {
                    "onnx": mock_onnx,
                    "torch": mock_torch,
                    "ultralytics": mock_ultralytics,
                },
            ):
                try:
                    export_real(tmp_dir)
                    self.assertTrue(os.path.exists(os.path.join(tmp_dir, "taxonomy.json")))
                    self.assertTrue(os.path.exists(os.path.join(tmp_dir, "manifest.json")))
                finally:
                    if os.path.exists("yolo11m.onnx"):
                        os.remove("yolo11m.onnx")


if __name__ == "__main__":
    unittest.main()
