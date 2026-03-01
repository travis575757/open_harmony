#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[2]
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))

from tools.augnet.parity_common import (
    SCHEMA_VERSION,
    canonical_dump,
    decode_stage_d_labels,
    first_mismatch,
    load_manifest,
    run_onnx_stage_c,
    summary,
)


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Fail-fast differential runner: cp_music21_compat candidate vs pinned music21 baseline."
    )
    parser.add_argument(
        "--manifest",
        type=Path,
        default=Path("tests/augnet_parity/fixtures_manifest.json"),
        help="Fixture manifest path.",
    )
    parser.add_argument(
        "--baseline-dir",
        type=Path,
        default=Path("tests/augnet_parity/music21_baseline"),
        help="Pinned baseline artifact directory.",
    )
    parser.add_argument(
        "--model",
        type=Path,
        default=Path("models/augnet/AugmentedNet.onnx"),
        help="ONNX model path used for candidate stage C.",
    )
    parser.add_argument(
        "--report-path",
        type=Path,
        default=Path("tests/augnet_parity/last_diff_report.json"),
        help="Machine-readable report output path.",
    )
    parser.add_argument(
        "--candidate-artifacts-dir",
        type=Path,
        default=None,
        help="Optional candidate artifact dir (fixture_id.json) to bypass Rust exporter.",
    )
    parser.add_argument(
        "--stage-b-atol",
        type=float,
        default=1e-6,
        help="Absolute tolerance for stage B float comparisons.",
    )
    parser.add_argument(
        "--logits-atol",
        type=float,
        default=1e-5,
        help="Absolute tolerance for stage C logits comparisons.",
    )
    return parser.parse_args(argv)


def _load_json(path: Path) -> dict[str, Any]:
    with path.open("r", encoding="utf-8") as fh:
        return json.load(fh)


def _run_rust_export(
    repo_root: Path,
    fixture_id: str,
    musicxml_path: Path,
    fixed_offset: float,
    max_steps: int,
) -> dict[str, Any]:
    cmd = [
        "cargo",
        "run",
        "--quiet",
        "-p",
        "cp_music21_compat",
        "--bin",
        "export_stage_artifacts",
        "--",
        "--fixture-id",
        fixture_id,
        "--musicxml-path",
        str(musicxml_path),
        "--fixed-offset",
        str(fixed_offset),
        "--max-steps",
        str(max_steps),
        "--pretty",
    ]
    proc = subprocess.run(
        cmd,
        cwd=str(repo_root),
        check=True,
        capture_output=True,
        text=True,
    )
    return json.loads(proc.stdout)


def _failure_report(
    *,
    fixture_id: str,
    stage_id: str,
    field_path: str,
    expected: Any,
    actual: Any,
    processed_fixtures: list[str],
) -> dict[str, Any]:
    return {
        "schema_version": SCHEMA_VERSION,
        "status": "mismatch",
        "fixture_id": fixture_id,
        "stage_id": stage_id,
        "field_path": field_path,
        "expected_summary": summary(expected),
        "actual_summary": summary(actual),
        "processed_fixtures": processed_fixtures,
    }


def _success_report(processed_fixtures: list[str]) -> dict[str, Any]:
    return {
        "schema_version": SCHEMA_VERSION,
        "status": "ok",
        "stages_checked": ["A", "B", "C", "D"],
        "fixtures_checked": len(processed_fixtures),
        "processed_fixtures": processed_fixtures,
    }


def _compare_or_fail(
    *,
    fixture_id: str,
    stage_id: str,
    expected: Any,
    actual: Any,
    processed_fixtures: list[str],
    report_path: Path,
    float_tol: float,
) -> bool:
    mismatch = first_mismatch(expected, actual, path="", float_tol=float_tol)
    if mismatch is None:
        return False
    field_path, exp, act = mismatch
    report = _failure_report(
        fixture_id=fixture_id,
        stage_id=stage_id,
        field_path=field_path,
        expected=exp,
        actual=act,
        processed_fixtures=processed_fixtures,
    )
    report_path.parent.mkdir(parents=True, exist_ok=True)
    canonical_dump(report, report_path)
    return True


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    repo_root = REPO_ROOT
    manifest = load_manifest(args.manifest)
    fixed_offset = float(manifest.get("fixed_offset", 0.25))
    max_steps = int(manifest.get("max_steps", 640))
    model_path = args.model.resolve()
    if not model_path.exists():
        raise FileNotFoundError(f"model not found: {model_path}")

    processed: list[str] = []
    for fixture in manifest["fixtures"]:
        fixture_id = fixture["id"]
        musicxml_path = Path(fixture["musicxml_path"])
        baseline_name = fixture.get("baseline_artifact", f"{fixture_id}.json")
        baseline_path = args.baseline_dir / baseline_name
        if not baseline_path.exists():
            raise FileNotFoundError(f"baseline artifact missing: {baseline_path}")
        expected = _load_json(baseline_path)

        if args.candidate_artifacts_dir is not None:
            candidate_path = args.candidate_artifacts_dir / f"{fixture_id}.json"
            if not candidate_path.exists():
                raise FileNotFoundError(f"candidate artifact missing: {candidate_path}")
            candidate = _load_json(candidate_path)
        else:
            candidate = _run_rust_export(
                repo_root=repo_root,
                fixture_id=fixture_id,
                musicxml_path=musicxml_path,
                fixed_offset=fixed_offset,
                max_steps=max_steps,
            )

        processed.append(fixture_id)

        if _compare_or_fail(
            fixture_id=fixture_id,
            stage_id="A",
            expected=expected["stage_a"]["event_frames"],
            actual=candidate["stage_a"]["event_frames"],
            processed_fixtures=processed,
            report_path=args.report_path,
            float_tol=0.0,
        ):
            return 1

        if _compare_or_fail(
            fixture_id=fixture_id,
            stage_id="B",
            expected=expected["stage_b"],
            actual=candidate["stage_b"],
            processed_fixtures=processed,
            report_path=args.report_path,
            float_tol=args.stage_b_atol,
        ):
            return 1

        candidate_stage_c = run_onnx_stage_c(model_path=model_path, stage_b=candidate["stage_b"])
        if _compare_or_fail(
            fixture_id=fixture_id,
            stage_id="C",
            expected=expected["stage_c"],
            actual=candidate_stage_c,
            processed_fixtures=processed,
            report_path=args.report_path,
            float_tol=args.logits_atol,
        ):
            return 1

        candidate_stage_d = decode_stage_d_labels(candidate_stage_c)
        if _compare_or_fail(
            fixture_id=fixture_id,
            stage_id="D",
            expected=expected["stage_d"],
            actual=candidate_stage_d,
            processed_fixtures=processed,
            report_path=args.report_path,
            float_tol=0.0,
        ):
            return 1

    report = _success_report(processed)
    args.report_path.parent.mkdir(parents=True, exist_ok=True)
    canonical_dump(report, args.report_path)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
