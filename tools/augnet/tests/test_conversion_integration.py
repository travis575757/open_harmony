from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess

import numpy as np
import pytest

from tools.augnet import convert_to_onnx as cto


FIXED_TIME_STEPS = 8
INPUT_DIMS = {
    "X_Bass19": 19,
    "X_Chromagram19": 19,
    "X_MeasureNoteOnset14": 14,
}
OUTPUT_HEADS = [
    ("Alto35", 35),
    ("Bass35", 35),
    ("RomanNumeral31", 31),
]


def _build_fixture_model(tf):
    const = tf.keras.initializers.Constant(0.125)
    bias = tf.keras.initializers.Constant(0.0)

    inputs = [
        tf.keras.Input(shape=(FIXED_TIME_STEPS, dim), dtype=tf.float32, name=name)
        for name, dim in INPUT_DIMS.items()
    ]
    x = tf.keras.layers.Concatenate(axis=-1, name="concat_inputs")(inputs)
    x = tf.keras.layers.Dense(
        24,
        name="shared_dense",
        kernel_initializer=const,
        bias_initializer=bias,
    )(x)
    outputs = [
        tf.keras.layers.Dense(
            units,
            name=head_name,
            kernel_initializer=const,
            bias_initializer=bias,
        )(x)
        for head_name, units in OUTPUT_HEADS
    ]
    return tf.keras.Model(inputs=inputs, outputs=outputs, name="augnet_fixture")


def _write_fixture_h5(tmp_path: Path, tf) -> Path:
    model = _build_fixture_model(tf)
    out = tmp_path / "fixture_model.hdf5"
    model.save(str(out), include_optimizer=False)
    return out


def _run_conversion(input_h5: Path, tmp_path: Path, suffix: str):
    onnx_path = tmp_path / f"fixture_{suffix}.onnx"
    manifest_path = tmp_path / f"fixture_{suffix}.manifest.json"
    rc = cto.main(
        [
            "--input-h5",
            str(input_h5),
            "--output-onnx",
            str(onnx_path),
            "--manifest",
            str(manifest_path),
            "--model-id",
            f"fixture-{suffix}",
            "--overwrite",
        ]
    )
    assert rc == 0
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    return onnx_path, manifest_path, manifest


def _run_rust_ort_smoke(onnx_path: Path, tmp_path: Path):
    repo_root = Path(__file__).resolve().parents[3]
    tool_manifest = repo_root / "tools" / "augnet" / "rust_ort_smoke" / "Cargo.toml"
    if not tool_manifest.exists():
        raise AssertionError(f"Rust ORT smoke tool not found: {tool_manifest}")

    env = os.environ.copy()
    env.setdefault("CARGO_HOME", str(tmp_path / "cargo-home"))
    env.setdefault("CARGO_TARGET_DIR", str(tmp_path / "cargo-target"))
    env.setdefault("ORT_CACHE_DIR", str(tmp_path / "ort-cache"))
    env.setdefault("XDG_CACHE_HOME", str(tmp_path / "xdg-cache"))
    cmd = [
        "cargo",
        "run",
        "--quiet",
        "--manifest-path",
        str(tool_manifest),
        "--",
        "--model",
        str(onnx_path),
    ]
    proc = subprocess.run(cmd, capture_output=True, text=True, env=env, check=True)
    return json.loads(proc.stdout)


def _deterministic_inputs(batch_size: int = 1):
    values = {}
    for idx, (name, dim) in enumerate(INPUT_DIMS.items()):
        arr = np.arange(batch_size * FIXED_TIME_STEPS * dim, dtype=np.float32).reshape(
            batch_size, FIXED_TIME_STEPS, dim
        )
        values[name] = (arr + idx) / 100.0
    return values


@pytest.fixture(scope="module")
def runtime_deps():
    tf = pytest.importorskip("tensorflow")
    pytest.importorskip("onnx")
    ort = pytest.importorskip("onnxruntime")
    return tf, ort


def test_conversion_creates_onnx_and_manifest_with_conversion_contracts(
    tmp_path: Path, runtime_deps
):
    tf, _ = runtime_deps
    import onnx

    input_h5 = _write_fixture_h5(tmp_path, tf)
    onnx_path, manifest_path, manifest = _run_conversion(input_h5, tmp_path, "single")

    assert onnx_path.exists()
    assert manifest_path.exists()
    assert manifest["onnx"]["opset"] == 13
    assert manifest["onnx"]["onnx_sha256"]
    assert manifest["source"]["hdf5_sha256"]
    assert manifest["signature"]["fixed_time_axis_contract"]["enforced"] is True
    assert manifest["signature"]["fixed_time_axis_contract"]["all_inputs_fixed"] is True
    assert manifest["signature"]["fixed_time_axis_contract"]["lengths"] == [FIXED_TIME_STEPS] * 3
    assert manifest["signature"]["output_head_order_match"] is True
    assert [item["onnx_head"] for item in manifest["signature"]["output_head_mapping"]] == [
        head for head, _ in OUTPUT_HEADS
    ]

    model = onnx.load(str(onnx_path))
    assert any(opset.version == 13 for opset in model.opset_import)


def test_runtime_shape_contract_accepts_valid_and_rejects_invalid_time_dim(
    tmp_path: Path, runtime_deps
):
    tf, ort = runtime_deps
    input_h5 = _write_fixture_h5(tmp_path, tf)
    onnx_path, _, _ = _run_conversion(input_h5, tmp_path, "shape_contract")

    session = ort.InferenceSession(str(onnx_path), providers=["CPUExecutionProvider"])
    valid_feed = _deterministic_inputs()
    outputs = session.run(None, valid_feed)
    assert len(outputs) == len(OUTPUT_HEADS)

    invalid_feed = dict(valid_feed)
    invalid_feed["X_Bass19"] = np.zeros((1, FIXED_TIME_STEPS + 1, INPUT_DIMS["X_Bass19"]), dtype=np.float32)
    with pytest.raises(Exception) as exc_info:
        session.run(None, invalid_feed)
    message = str(exc_info.value).lower()
    assert "invalid" in message or "shape" in message


def test_reproducibility_policy_enforces_functional_equivalence(tmp_path: Path, runtime_deps):
    tf, ort = runtime_deps
    input_h5 = _write_fixture_h5(tmp_path, tf)

    onnx_a, _, manifest_a = _run_conversion(input_h5, tmp_path, "a")
    onnx_b, _, manifest_b = _run_conversion(input_h5, tmp_path, "b")

    assert manifest_a["reproducibility"]["policy_id"] == cto.REPRO_POLICY_ID
    assert manifest_a["reproducibility"]["byte_identical_required"] is False
    assert manifest_a["signature"]["inputs"] == manifest_b["signature"]["inputs"]
    assert manifest_a["signature"]["onnx_input_shapes"] == manifest_b["signature"]["onnx_input_shapes"]
    assert manifest_a["signature"]["output_head_mapping"] == manifest_b["signature"]["output_head_mapping"]
    assert manifest_a["signature"]["output_head_order_match"] is True
    assert manifest_b["signature"]["output_head_order_match"] is True

    feed = _deterministic_inputs(batch_size=2)
    sess_a = ort.InferenceSession(str(onnx_a), providers=["CPUExecutionProvider"])
    sess_b = ort.InferenceSession(str(onnx_b), providers=["CPUExecutionProvider"])
    outputs_a = sess_a.run(None, feed)
    outputs_b = sess_b.run(None, feed)

    assert len(outputs_a) == len(outputs_b)
    for lhs, rhs in zip(outputs_a, outputs_b):
        np.testing.assert_allclose(lhs, rhs, rtol=0.0, atol=0.0)


def test_rust_ort_smoke_loads_model_and_matches_manifest_signature(tmp_path: Path, runtime_deps):
    tf, _ = runtime_deps
    input_h5 = _write_fixture_h5(tmp_path, tf)
    onnx_path, _, manifest = _run_conversion(input_h5, tmp_path, "rust_smoke")

    summary = _run_rust_ort_smoke(onnx_path, tmp_path)
    assert summary["input_count"] == len(INPUT_DIMS)
    assert summary["output_count"] == len(OUTPUT_HEADS)
    assert [item["name"] for item in summary["inputs"]] == manifest["signature"]["onnx_input_names"]
    assert [item["name"] for item in summary["outputs"]] == manifest["signature"]["onnx_output_names"]
    assert [item["shape"] for item in summary["inputs"]] == manifest["signature"]["onnx_input_shapes"]
