from __future__ import annotations

import json
import importlib.util
import subprocess
import sys
from pathlib import Path

import pytest


REPO_ROOT = Path(__file__).resolve().parents[2]
MANIFEST_PATH = REPO_ROOT / "tests" / "augnet_parity" / "fixtures_manifest.json"
BASELINE_DIR = REPO_ROOT / "tests" / "augnet_parity" / "music21_baseline"
MODEL_PATH = REPO_ROOT / "models" / "augnet" / "AugmentedNet.onnx"
DIFF_SCRIPT = REPO_ROOT / "tools" / "augnet" / "diff_against_music21.py"
EXPORT_SCRIPT = REPO_ROOT / "tools" / "augnet" / "export_music21_baseline.py"


def _read_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def _run(cmd: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        cmd,
        cwd=str(REPO_ROOT),
        text=True,
        capture_output=True,
        check=False,
    )


def _require_modules(*names: str) -> None:
    missing = [name for name in names if importlib.util.find_spec(name) is None]
    if missing:
        pytest.skip(f"requires optional dependencies: {', '.join(missing)}")


def _run_rust_export(fixture_id: str, musicxml_path: Path) -> dict:
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
        "0.25",
        "--max-steps",
        "640",
        "--pretty",
    ]
    proc = _run(cmd)
    if proc.returncode != 0:
        raise AssertionError(f"rust export failed\nstdout={proc.stdout}\nstderr={proc.stderr}")
    return json.loads(proc.stdout)


def test_diff_runner_is_deterministic_across_repeated_runs(tmp_path: Path):
    _require_modules("onnxruntime")

    report_a = tmp_path / "report_a.json"
    report_b = tmp_path / "report_b.json"
    cmd_base = [
        sys.executable,
        str(DIFF_SCRIPT),
        "--manifest",
        str(MANIFEST_PATH),
        "--baseline-dir",
        str(BASELINE_DIR),
        "--model",
        str(MODEL_PATH),
    ]

    run_a = _run(cmd_base + ["--report-path", str(report_a)])
    assert run_a.returncode == 0, f"diff run A failed\nstdout={run_a.stdout}\nstderr={run_a.stderr}"
    run_b = _run(cmd_base + ["--report-path", str(report_b)])
    assert run_b.returncode == 0, f"diff run B failed\nstdout={run_b.stdout}\nstderr={run_b.stderr}"

    assert report_a.read_text(encoding="utf-8") == report_b.read_text(encoding="utf-8")
    report = _read_json(report_a)
    assert report["status"] == "ok"
    assert report["stages_checked"] == ["A", "B", "C", "D"]
    assert report["fixtures_checked"] >= 5


def test_diff_runner_fail_fast_and_machine_readable_report(tmp_path: Path):
    manifest = _read_json(MANIFEST_PATH)
    first_fixture = manifest["fixtures"][0]
    fixture_id = first_fixture["id"]
    fixture_path = REPO_ROOT / first_fixture["musicxml_path"]

    candidate_dir = tmp_path / "candidate"
    candidate_dir.mkdir(parents=True, exist_ok=True)
    candidate = _run_rust_export(fixture_id, fixture_path)
    candidate["stage_a"]["event_frames"][0]["s_measure"] += 999
    (candidate_dir / f"{fixture_id}.json").write_text(
        json.dumps(candidate, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )

    report_path = tmp_path / "failfast_report.json"
    cmd = [
        sys.executable,
        str(DIFF_SCRIPT),
        "--manifest",
        str(MANIFEST_PATH),
        "--baseline-dir",
        str(BASELINE_DIR),
        "--model",
        str(MODEL_PATH),
        "--candidate-artifacts-dir",
        str(candidate_dir),
        "--report-path",
        str(report_path),
    ]
    run = _run(cmd)
    assert run.returncode == 1, f"expected mismatch exit code\nstdout={run.stdout}\nstderr={run.stderr}"
    report = _read_json(report_path)
    assert report["status"] == "mismatch"
    assert report["fixture_id"] == fixture_id
    assert report["stage_id"] == "A"
    assert report["field_path"]
    assert report["processed_fixtures"] == [fixture_id]
    assert "expected_summary" in report
    assert "actual_summary" in report


def test_manifest_edge_cases_export_and_diff_commands(tmp_path: Path):
    _require_modules("music21", "onnxruntime")

    manifest = _read_json(MANIFEST_PATH)
    tags = {tag for item in manifest["fixtures"] for tag in item.get("tags", [])}
    for required in {
        "ties",
        "anacrusis",
        "enharmonic",
        "dense_poly",
        "chunk_boundary",
        "divisions_change",
        "rest_heavy",
        "long_tie_chain",
        "multi_part_enharmonic",
    }:
        assert required in tags, f"missing required fixture tag: {required}"

    export_a = tmp_path / "baseline_a"
    export_b = tmp_path / "baseline_b"
    cmd_base = [
        sys.executable,
        str(EXPORT_SCRIPT),
        "--manifest",
        str(MANIFEST_PATH),
        "--model",
        str(MODEL_PATH),
        "--overwrite",
    ]
    run_a = _run(cmd_base + ["--output-dir", str(export_a)])
    assert run_a.returncode == 0, f"export A failed\nstdout={run_a.stdout}\nstderr={run_a.stderr}"
    run_b = _run(cmd_base + ["--output-dir", str(export_b)])
    assert run_b.returncode == 0, f"export B failed\nstdout={run_b.stdout}\nstderr={run_b.stderr}"

    for fixture in manifest["fixtures"]:
        baseline_file = fixture["baseline_artifact"]
        content_a = (export_a / baseline_file).read_text(encoding="utf-8")
        content_b = (export_b / baseline_file).read_text(encoding="utf-8")
        assert content_a == content_b, f"non-deterministic export for {baseline_file}"

    diff_report = tmp_path / "diff_report.json"
    diff = _run(
        [
            sys.executable,
            str(DIFF_SCRIPT),
            "--manifest",
            str(MANIFEST_PATH),
            "--baseline-dir",
            str(BASELINE_DIR),
            "--model",
            str(MODEL_PATH),
            "--report-path",
            str(diff_report),
        ]
    )
    assert diff.returncode == 0, f"diff failed\nstdout={diff.stdout}\nstderr={diff.stderr}"
    report = _read_json(diff_report)
    assert report["status"] == "ok"
