from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPT = REPO_ROOT / "tools" / "augnet" / "evaluate_corpus.py"


def _run(cmd: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        cmd,
        cwd=str(REPO_ROOT),
        text=True,
        capture_output=True,
        check=False,
    )


def _base_payload() -> dict:
    return {
        "deterministic_fixture_gate": {"passed": True, "report": {"status": "ok"}},
        "corpus_parity": {"mismatch_frames": 1, "total_frames": 2000},
        "musical_metrics": {
            "python": {
                "roman_numeral_exact_accuracy": 0.80,
                "local_key_accuracy": 0.81,
                "tonicized_key_accuracy": 0.79,
                "chord_quality_accuracy": 0.77,
                "inversion_accuracy": 0.75,
                "harmonic_segment_boundary_f1": 0.70,
            },
            "rust": {
                "roman_numeral_exact_accuracy": 0.8005,
                "local_key_accuracy": 0.8105,
                "tonicized_key_accuracy": 0.7905,
                "chord_quality_accuracy": 0.7705,
                "inversion_accuracy": 0.7505,
                "harmonic_segment_boundary_f1": 0.7005,
            },
        },
        "piece_rows": [{"piece_id": "example"}],
        "pieces_processed": 1,
        "pieces_with_annotation": 1,
    }


def test_corpus_pipeline_precomputed_pass_generates_reports(tmp_path: Path):
    payload = _base_payload()
    payload_path = tmp_path / "payload.json"
    payload_path.write_text(json.dumps(payload), encoding="utf-8")
    report_dir = tmp_path / "corpus_validation"

    proc = _run(
        [
            sys.executable,
            str(SCRIPT),
            "--precomputed-json",
            str(payload_path),
            "--report-dir",
            str(report_dir),
        ]
    )
    assert proc.returncode == 0, f"stdout={proc.stdout}\nstderr={proc.stderr}"
    summary = json.loads((report_dir / "summary.json").read_text(encoding="utf-8"))
    assert summary["overall_status"] == "ok"
    assert summary["corpus_parity_gate"]["passed"] is True
    assert summary["musical_evaluation_gate"]["passed"] is True
    assert (report_dir / "summary.md").exists()
    assert (report_dir / "piece_metrics.csv").exists()


def test_corpus_pipeline_precomputed_fails_on_parity_threshold(tmp_path: Path):
    payload = _base_payload()
    payload["corpus_parity"] = {"mismatch_frames": 3, "total_frames": 1000}
    payload_path = tmp_path / "payload.json"
    payload_path.write_text(json.dumps(payload), encoding="utf-8")
    report_dir = tmp_path / "corpus_validation"

    proc = _run(
        [
            sys.executable,
            str(SCRIPT),
            "--precomputed-json",
            str(payload_path),
            "--report-dir",
            str(report_dir),
        ]
    )
    assert proc.returncode == 3, f"stdout={proc.stdout}\nstderr={proc.stderr}"
    summary = json.loads((report_dir / "summary.json").read_text(encoding="utf-8"))
    assert summary["corpus_parity_gate"]["passed"] is False


def test_corpus_pipeline_precomputed_fails_on_metric_delta_threshold(tmp_path: Path):
    payload = _base_payload()
    payload["musical_metrics"]["rust"]["roman_numeral_exact_accuracy"] = 0.81
    payload_path = tmp_path / "payload.json"
    payload_path.write_text(json.dumps(payload), encoding="utf-8")
    report_dir = tmp_path / "corpus_validation"

    proc = _run(
        [
            sys.executable,
            str(SCRIPT),
            "--precomputed-json",
            str(payload_path),
            "--report-dir",
            str(report_dir),
        ]
    )
    assert proc.returncode == 4, f"stdout={proc.stdout}\nstderr={proc.stderr}"
    summary = json.loads((report_dir / "summary.json").read_text(encoding="utf-8"))
    assert summary["musical_evaluation_gate"]["passed"] is False
    assert "roman_numeral_exact_accuracy" in summary["musical_evaluation_gate"]["failing_metrics"]
