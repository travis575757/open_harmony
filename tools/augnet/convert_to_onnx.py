#!/usr/bin/env python3
"""Convert a Keras .h5 model to ONNX with Phase 1 validation and manifesting."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import platform
import random
import sys
from pathlib import Path
from typing import Any, Dict, List, Sequence, Tuple

DEFAULT_OPSET = 13
SCHEMA_VERSION = 2
REPRO_POLICY_ID = "functional-equivalence-v1"


def _sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def _utc_now_iso() -> str:
    return dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat()


def _normalize_head_name(name: str) -> str:
    # Keras/TF names often look like "Head/BiasAdd:0"
    # ONNX names often look like "Head" or "Head:0"
    base = name.split(":", 1)[0]
    if "/" in base:
        base = base.split("/", 1)[0]
    return base


def _json_shape(shape: Any) -> List[int | None]:
    if hasattr(shape, "as_list"):
        return list(shape.as_list())
    return list(shape)


def _normalize_dim_value(dim: Any) -> int | None:
    if dim is None:
        return None
    if hasattr(dim, "value"):
        dim = dim.value
    if isinstance(dim, bool):
        return int(dim)
    if isinstance(dim, int):
        return dim
    if isinstance(dim, str):
        return None
    try:
        return int(dim)
    except (TypeError, ValueError):
        return None


def _normalize_shape_list(shape: Sequence[Any]) -> List[int | None]:
    return [_normalize_dim_value(dim) for dim in list(shape)]


def _shape_from_onnx_value_info(value_info: Any) -> List[int | None]:
    dims = value_info.type.tensor_type.shape.dim
    out: List[int | None] = []
    for dim in dims:
        if dim.HasField("dim_value"):
            out.append(int(dim.dim_value))
        else:
            out.append(None)
    return out


def _validate_opset_policy(opset: int, allow_override: bool) -> None:
    if opset == DEFAULT_OPSET:
        return
    if allow_override:
        return
    raise RuntimeError(
        f"Phase 1 requires opset={DEFAULT_OPSET}. Received opset={opset}. "
        "Use --allow-opset-override to bypass this guard."
    )


def _enable_determinism(seed: int) -> None:
    os.environ["PYTHONHASHSEED"] = str(seed)
    os.environ.setdefault("TF_DETERMINISTIC_OPS", "1")
    os.environ.setdefault("TF_CUDNN_DETERMINISTIC", "1")
    os.environ.setdefault("CUDA_VISIBLE_DEVICES", "")
    random.seed(seed)
    try:
        import numpy as np

        np.random.seed(seed)
    except Exception:
        pass


def _load_deps() -> Dict[str, Any]:
    try:
        import tensorflow as tf
    except Exception as exc:  # pragma: no cover
        raise RuntimeError("tensorflow is required for conversion") from exc
    try:
        import tf2onnx
    except Exception as exc:  # pragma: no cover
        raise RuntimeError("tf2onnx is required for conversion") from exc
    try:
        import onnx
    except Exception as exc:  # pragma: no cover
        raise RuntimeError("onnx is required for model validation") from exc

    try:
        import onnxruntime as ort
    except Exception:
        ort = None

    return {"tf": tf, "tf2onnx": tf2onnx, "onnx": onnx, "ort": ort}


def _build_input_signature(tf: Any, model: Any, enforce_fixed_time_axis: bool) -> List[Any]:
    sig = []
    for tensor in model.inputs:
        shape = _json_shape(tensor.shape)
        if enforce_fixed_time_axis:
            if len(shape) < 2:
                raise RuntimeError(
                    "Input does not expose a time axis at dim[1]; fixed-T enforcement failed. "
                    f"Input={tensor.name} shape={shape}"
                )
            time_dim = _normalize_dim_value(shape[1])
            if time_dim is None:
                raise RuntimeError(
                    "Input has dynamic time axis at dim[1]; fixed-T is required in Phase 1. "
                    "Use --allow-dynamic-time-axis only for explicit override workflows. "
                    f"Input={tensor.name} shape={shape}"
                )
        sig.append(
            tf.TensorSpec(
                shape=shape,
                dtype=tensor.dtype,
                name=tensor.name.split(":", 1)[0],
            )
        )
    return sig


def _get_version(module: Any) -> str:
    return str(getattr(module, "__version__", "unknown"))


def _build_head_mapping(
    keras_output_names: List[str], onnx_output_names: List[str]
) -> Tuple[List[str], List[str], List[Dict[str, Any]], bool]:
    keras_heads = [_normalize_head_name(name) for name in keras_output_names]
    onnx_heads = [_normalize_head_name(name) for name in onnx_output_names]
    length = max(len(keras_output_names), len(onnx_output_names))

    mapping: List[Dict[str, Any]] = []
    for idx in range(length):
        keras_name = keras_output_names[idx] if idx < len(keras_output_names) else ""
        onnx_name = onnx_output_names[idx] if idx < len(onnx_output_names) else ""
        keras_head = _normalize_head_name(keras_name) if keras_name else ""
        onnx_head = _normalize_head_name(onnx_name) if onnx_name else ""
        mapping.append(
            {
                "index": idx,
                "keras_output_name": keras_name,
                "keras_head": keras_head,
                "onnx_output_name": onnx_name,
                "onnx_head": onnx_head,
                "match": bool(keras_name and onnx_name and keras_head == onnx_head),
            }
        )
    head_order_match = len(keras_heads) == len(onnx_heads) and keras_heads == onnx_heads
    return keras_heads, onnx_heads, mapping, head_order_match


def _graph_io_from_model_proto(model_proto: Any) -> Tuple[List[str], List[List[int | None]], List[str]]:
    input_names: List[str] = []
    input_shapes: List[List[int | None]] = []
    for value_info in model_proto.graph.input:
        input_names.append(value_info.name)
        input_shapes.append(_shape_from_onnx_value_info(value_info))
    output_names = [value_info.name for value_info in model_proto.graph.output]
    return input_names, input_shapes, output_names


def _validate_runtime_session_signature(
    session: Any, input_signature: List[Any], keras_output_names: List[str]
) -> Tuple[List[str], List[List[int | None]], List[str]]:
    runtime_inputs = session.get_inputs()
    runtime_outputs = session.get_outputs()

    expected_input_names = [spec.name for spec in input_signature]
    expected_input_shapes = [_normalize_shape_list(_json_shape(spec.shape)) for spec in input_signature]

    actual_input_names = [item.name for item in runtime_inputs]
    actual_input_shapes = [_normalize_shape_list(item.shape) for item in runtime_inputs]
    actual_output_names = [item.name for item in runtime_outputs]

    if len(expected_input_names) != len(actual_input_names):
        raise RuntimeError(
            "ONNX Runtime input count mismatch. "
            f"Expected={len(expected_input_names)} Actual={len(actual_input_names)}"
        )
    if expected_input_names != actual_input_names:
        raise RuntimeError(
            "ONNX Runtime input name mismatch. "
            f"Expected={expected_input_names} Actual={actual_input_names}"
        )
    if expected_input_shapes != actual_input_shapes:
        raise RuntimeError(
            "ONNX Runtime input shape mismatch. "
            f"Expected={expected_input_shapes} Actual={actual_input_shapes}"
        )
    if len(keras_output_names) != len(actual_output_names):
        raise RuntimeError(
            "ONNX Runtime output count mismatch. "
            f"Expected={len(keras_output_names)} Actual={len(actual_output_names)}"
        )
    return actual_input_names, actual_input_shapes, actual_output_names


def _fixed_time_axis_contract(input_signature: List[Any], enforce_fixed_time_axis: bool) -> Dict[str, Any]:
    lengths: List[int | None] = []
    for spec in input_signature:
        shape = _normalize_shape_list(_json_shape(spec.shape))
        lengths.append(shape[1] if len(shape) > 1 else None)
    return {
        "dimension": 1,
        "enforced": enforce_fixed_time_axis,
        "lengths": lengths,
        "all_inputs_fixed": all(length is not None for length in lengths),
    }


def _build_manifest(
    *,
    args: argparse.Namespace,
    input_h5: Path,
    output_onnx: Path,
    source_sha256: str,
    onnx_sha256: str,
    tf: Any,
    tf2onnx: Any,
    onnx: Any,
    ort: Any,
    input_signature: List[Any],
    onnx_input_names: List[str],
    onnx_input_shapes: List[List[int | None]],
    keras_output_names: List[str],
    onnx_output_names: List[str],
    keras_heads: List[str],
    onnx_heads: List[str],
    head_mapping: List[Dict[str, Any]],
    head_order_match: bool,
) -> Dict[str, Any]:
    return {
        "schema_version": SCHEMA_VERSION,
        "status": "generated",
        "generated_utc": _utc_now_iso(),
        "model_id": args.model_id or output_onnx.stem,
        "source": {
            "hdf5_path": str(input_h5),
            "hdf5_sha256": source_sha256,
        },
        "onnx": {
            "onnx_path": str(output_onnx),
            "onnx_sha256": onnx_sha256,
            "opset": args.opset,
        },
        "conversion": {
            "tool": "tools/augnet/convert_to_onnx.py",
            "seed": args.seed,
            "enforce_fixed_time_axis": args.enforce_fixed_time_axis,
            "skip_runtime_check": args.skip_runtime_check,
            "allow_output_head_mismatch": args.allow_output_head_mismatch,
            "allow_opset_override": args.allow_opset_override,
        },
        "environment": {
            "python": sys.version.split()[0],
            "platform": platform.platform(),
            "tensorflow": _get_version(tf),
            "tf2onnx": _get_version(tf2onnx),
            "onnx": _get_version(onnx),
            "onnxruntime": _get_version(ort) if ort is not None else "not-installed",
        },
        "reproducibility": {
            "policy_id": REPRO_POLICY_ID,
            "policy": (
                "Functional reproducibility is required in Phase 1: identical input/output "
                "signatures and numerically equivalent inference outputs across repeated conversions."
            ),
            "byte_identical_required": False,
        },
        "signature": {
            "inputs": [
                {
                    "name": spec.name,
                    "shape": _normalize_shape_list(_json_shape(spec.shape)),
                    "dtype": str(spec.dtype.name),
                }
                for spec in input_signature
            ],
            "fixed_time_axis_contract": _fixed_time_axis_contract(
                input_signature, args.enforce_fixed_time_axis
            ),
            "onnx_input_names": onnx_input_names,
            "onnx_input_shapes": onnx_input_shapes,
            "keras_output_names": keras_output_names,
            "keras_output_heads": keras_heads,
            "onnx_output_names": onnx_output_names,
            "onnx_output_heads": onnx_heads,
            "output_head_mapping": head_mapping,
            "output_head_order_match": head_order_match,
        },
    }


def convert(args: argparse.Namespace) -> int:
    _validate_opset_policy(args.opset, args.allow_opset_override)
    deps = _load_deps()
    tf = deps["tf"]
    tf2onnx = deps["tf2onnx"]
    onnx = deps["onnx"]
    ort = deps["ort"]

    _enable_determinism(args.seed)
    tf.random.set_seed(args.seed)
    try:
        tf.keras.utils.set_random_seed(args.seed)
    except Exception:
        pass
    try:
        tf.config.threading.set_inter_op_parallelism_threads(1)
        tf.config.threading.set_intra_op_parallelism_threads(1)
    except Exception:
        pass
    try:
        tf.config.experimental.enable_op_determinism()
    except Exception:
        pass

    input_h5 = Path(args.input_h5).expanduser().resolve()
    output_onnx = Path(args.output_onnx).expanduser().resolve()

    if not input_h5.exists():
        raise RuntimeError(f"Input model not found: {input_h5}")
    if output_onnx.exists() and not args.overwrite:
        raise RuntimeError(f"Output already exists (use --overwrite): {output_onnx}")

    output_onnx.parent.mkdir(parents=True, exist_ok=True)

    model = tf.keras.models.load_model(str(input_h5), compile=False)

    keras_output_names = [t.name.split(":", 1)[0] for t in model.outputs]
    input_signature = _build_input_signature(tf, model, args.enforce_fixed_time_axis)

    # Convert using explicit input signature and pinned opset.
    model_proto, _ = tf2onnx.convert.from_keras(
        model,
        input_signature=input_signature,
        opset=args.opset,
        output_path=str(output_onnx),
    )

    onnx.checker.check_model(model_proto)

    if args.skip_runtime_check:
        onnx_input_names, onnx_input_shapes, onnx_output_names = _graph_io_from_model_proto(model_proto)
    else:
        if ort is None:
            raise RuntimeError(
                "onnxruntime is not installed; runtime check required unless --skip-runtime-check is set"
            )
        session_options = ort.SessionOptions()
        session_options.inter_op_num_threads = 1
        session_options.intra_op_num_threads = 1
        session = ort.InferenceSession(
            str(output_onnx),
            sess_options=session_options,
            providers=["CPUExecutionProvider"],
        )
        onnx_input_names, onnx_input_shapes, onnx_output_names = _validate_runtime_session_signature(
            session, input_signature, keras_output_names
        )

    keras_heads, onnx_heads, head_mapping, head_order_match = _build_head_mapping(
        keras_output_names, onnx_output_names
    )

    if not head_order_match and not args.allow_output_head_mismatch:
        raise RuntimeError(
            "Output head mismatch after conversion. "
            f"Keras={keras_heads} ONNX={onnx_heads}. "
            "Use --allow-output-head-mismatch to emit manifest with mismatch flagged."
        )

    source_sha256 = _sha256_file(input_h5)
    onnx_sha256 = _sha256_file(output_onnx)

    manifest_path = (
        Path(args.manifest).expanduser().resolve()
        if args.manifest
        else output_onnx.with_suffix(".manifest.json")
    )
    manifest_path.parent.mkdir(parents=True, exist_ok=True)

    manifest = _build_manifest(
        args=args,
        input_h5=input_h5,
        output_onnx=output_onnx,
        source_sha256=source_sha256,
        onnx_sha256=onnx_sha256,
        tf=tf,
        tf2onnx=tf2onnx,
        onnx=onnx,
        ort=ort,
        input_signature=input_signature,
        onnx_input_names=onnx_input_names,
        onnx_input_shapes=onnx_input_shapes,
        keras_output_names=keras_output_names,
        onnx_output_names=onnx_output_names,
        keras_heads=keras_heads,
        onnx_heads=onnx_heads,
        head_mapping=head_mapping,
        head_order_match=head_order_match,
    )

    with manifest_path.open("w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=2, sort_keys=True)
        f.write("\n")

    print(f"Converted: {input_h5} -> {output_onnx}")
    print(f"Manifest:  {manifest_path}")
    print(f"Heads:     {len(keras_heads)} (order match: {head_order_match})")
    return 0


def parse_args(argv: List[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input-h5", required=True, help="Input Keras .h5/.hdf5 model path")
    parser.add_argument("--output-onnx", required=True, help="Output ONNX model path")
    parser.add_argument("--manifest", default="", help="Optional explicit manifest output path")
    parser.add_argument("--model-id", default="", help="Optional model identifier for manifest")
    parser.add_argument(
        "--opset",
        type=int,
        default=DEFAULT_OPSET,
        help=f"ONNX opset version (Phase 1 fixed default: {DEFAULT_OPSET})",
    )
    parser.add_argument("--seed", type=int, default=0, help="Deterministic conversion seed")
    parser.add_argument("--overwrite", action="store_true", help="Overwrite existing ONNX output")

    parser.add_argument(
        "--allow-dynamic-time-axis",
        action="store_true",
        help="Allow dynamic dim[1] in model inputs (Phase 1 default is fixed-T only)",
    )
    parser.add_argument(
        "--allow-opset-override",
        action="store_true",
        help=f"Allow opset other than fixed Phase 1 value ({DEFAULT_OPSET})",
    )
    parser.add_argument(
        "--skip-runtime-check",
        action="store_true",
        help="Skip ONNX Runtime loading/signature check (not recommended)",
    )
    parser.add_argument(
        "--allow-output-head-mismatch",
        action="store_true",
        help="Do not fail conversion when output head names/order differ; flag mismatch in manifest",
    )

    args = parser.parse_args(argv)
    args.enforce_fixed_time_axis = not args.allow_dynamic_time_axis
    return args


def main(argv: List[str] | None = None) -> int:
    args = parse_args(argv if argv is not None else sys.argv[1:])
    return convert(args)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:  # pragma: no cover
        print(f"ERROR: {exc}", file=sys.stderr)
        raise SystemExit(1)
