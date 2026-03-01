from __future__ import annotations

from pathlib import Path
from types import SimpleNamespace

import pytest

from tools.augnet import convert_to_onnx as cto


class _FakeTensorSpec:
    def __init__(self, shape, dtype, name):
        self.shape = shape
        self.dtype = dtype
        self.name = name


class _FakeTF:
    TensorSpec = _FakeTensorSpec


class _FakeTensor:
    def __init__(self, name: str, shape, dtype):
        self.name = name
        self.shape = shape
        self.dtype = dtype


class _FakeModel:
    def __init__(self, inputs):
        self.inputs = inputs


def _fake_spec(name: str, shape):
    return SimpleNamespace(name=name, shape=shape, dtype=SimpleNamespace(name="float32"))


def test_parse_args_defaults_enforce_conversion_contracts():
    args = cto.parse_args(["--input-h5", "in.h5", "--output-onnx", "out.onnx"])
    assert args.opset == cto.DEFAULT_OPSET
    assert args.enforce_fixed_time_axis is True
    assert args.allow_opset_override is False


def test_parse_args_allows_dynamic_override():
    args = cto.parse_args(
        [
            "--input-h5",
            "in.h5",
            "--output-onnx",
            "out.onnx",
            "--allow-dynamic-time-axis",
        ]
    )
    assert args.enforce_fixed_time_axis is False


def test_validate_opset_policy_rejects_non_default_without_override():
    with pytest.raises(RuntimeError, match="Phase 1 requires opset=13"):
        cto._validate_opset_policy(14, allow_override=False)


def test_validate_opset_policy_allows_override():
    cto._validate_opset_policy(14, allow_override=True)


def test_normalize_head_name_strips_tf_suffixes():
    assert cto._normalize_head_name("RomanNumeral31/BiasAdd:0") == "RomanNumeral31"
    assert cto._normalize_head_name("LocalKey38:0") == "LocalKey38"


def test_build_input_signature_rejects_dynamic_time_axis_when_enforced():
    model = _FakeModel(
        [
            _FakeTensor(
                name="X_Bass19:0",
                shape=[None, None, 19],
                dtype=SimpleNamespace(name="float32"),
            )
        ]
    )
    with pytest.raises(RuntimeError, match="dynamic time axis"):
        cto._build_input_signature(_FakeTF(), model, enforce_fixed_time_axis=True)


def test_build_input_signature_allows_dynamic_time_axis_when_override_enabled():
    model = _FakeModel(
        [
            _FakeTensor(
                name="X_Bass19:0",
                shape=[None, None, 19],
                dtype=SimpleNamespace(name="float32"),
            )
        ]
    )
    sig = cto._build_input_signature(_FakeTF(), model, enforce_fixed_time_axis=False)
    assert len(sig) == 1
    assert sig[0].name == "X_Bass19"


def test_build_head_mapping_tracks_order_and_mismatch():
    keras_names = ["HeadA", "HeadB", "HeadC"]
    onnx_names = ["HeadA", "HeadX", "HeadC"]
    keras_heads, onnx_heads, mapping, match = cto._build_head_mapping(keras_names, onnx_names)
    assert keras_heads == ["HeadA", "HeadB", "HeadC"]
    assert onnx_heads == ["HeadA", "HeadX", "HeadC"]
    assert mapping[1]["match"] is False
    assert match is False


def test_build_manifest_contains_required_conversion_fields():
    args = cto.parse_args(["--input-h5", "in.h5", "--output-onnx", "out.onnx"])
    manifest = cto._build_manifest(
        args=args,
        input_h5=Path("in.h5"),
        output_onnx=Path("out.onnx"),
        source_sha256="a" * 64,
        onnx_sha256="b" * 64,
        tf=SimpleNamespace(__version__="2.x"),
        tf2onnx=SimpleNamespace(__version__="1.x"),
        onnx=SimpleNamespace(__version__="1.y"),
        ort=SimpleNamespace(__version__="1.z"),
        input_signature=[_fake_spec("X_Bass19", [None, 640, 19])],
        onnx_input_names=["X_Bass19"],
        onnx_input_shapes=[[None, 640, 19]],
        keras_output_names=["HeadA"],
        onnx_output_names=["HeadA"],
        keras_heads=["HeadA"],
        onnx_heads=["HeadA"],
        head_mapping=[
            {
                "index": 0,
                "keras_output_name": "HeadA",
                "keras_head": "HeadA",
                "onnx_output_name": "HeadA",
                "onnx_head": "HeadA",
                "match": True,
            }
        ],
        head_order_match=True,
    )

    assert manifest["model_id"] == "out"
    assert manifest["source"]["hdf5_sha256"] == "a" * 64
    assert manifest["onnx"]["onnx_sha256"] == "b" * 64
    assert manifest["onnx"]["opset"] == cto.DEFAULT_OPSET
    assert manifest["signature"]["output_head_order_match"] is True
    assert manifest["signature"]["output_head_mapping"][0]["match"] is True
    assert manifest["reproducibility"]["policy_id"] == cto.REPRO_POLICY_ID
