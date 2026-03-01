from __future__ import annotations

import functools
import json
import subprocess
import sys
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).resolve().parents[2]
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))

from tools.augnet.parity_common import first_mismatch


MANIFEST_PATH = REPO_ROOT / "tests" / "augnet_parity" / "fixtures_manifest.json"
BASELINE_DIR = REPO_ROOT / "tests" / "augnet_parity" / "music21_baseline"


def _read_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


@functools.lru_cache(maxsize=None)
def _manifest() -> dict:
    return _read_json(MANIFEST_PATH)


def _fixture_ids() -> list[str]:
    return [fixture["id"] for fixture in _manifest()["fixtures"]]


@functools.lru_cache(maxsize=None)
def _fixture_record(fixture_id: str) -> dict:
    for fixture in _manifest()["fixtures"]:
        if fixture["id"] == fixture_id:
            return fixture
    raise KeyError(f"fixture not found in manifest: {fixture_id}")


@functools.lru_cache(maxsize=None)
def _baseline_for_fixture(fixture_id: str) -> dict:
    fixture = _fixture_record(fixture_id)
    return _read_json(BASELINE_DIR / fixture["baseline_artifact"])


def _run(cmd: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        cmd,
        cwd=str(REPO_ROOT),
        text=True,
        capture_output=True,
        check=False,
    )


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
    assert proc.returncode == 0, (
        f"rust export failed for {fixture_id}\n"
        f"stdout:\n{proc.stdout}\n"
        f"stderr:\n{proc.stderr}"
    )
    return json.loads(proc.stdout)


@functools.lru_cache(maxsize=None)
def _candidate_for_fixture(fixture_id: str) -> dict:
    fixture = _fixture_record(fixture_id)
    musicxml_path = REPO_ROOT / fixture["musicxml_path"]
    return _run_rust_export(fixture_id, musicxml_path)


def _assert_stage_parity(stage_name: str, expected: dict | list, actual: dict | list, tol: float = 0.0) -> None:
    mismatch = first_mismatch(expected, actual, path="", float_tol=tol)
    assert mismatch is None, (
        f"{stage_name} mismatch: path={mismatch[0]} expected={mismatch[1]} actual={mismatch[2]}"
    )


@pytest.mark.parametrize(
    "fixture_id",
    _fixture_ids(),
)
def test_stage_a_event_and_grid_frames_match_music21_baseline(fixture_id: str):
    baseline = _baseline_for_fixture(fixture_id)
    candidate = _candidate_for_fixture(fixture_id)
    _assert_stage_parity(
        "stage_a.event_frames",
        baseline["stage_a"]["event_frames"],
        candidate["stage_a"]["event_frames"],
    )
    _assert_stage_parity(
        "stage_a.grid_frames",
        baseline["stage_a"]["grid_frames"],
        candidate["stage_a"]["grid_frames"],
    )


@pytest.mark.parametrize(
    "fixture_id",
    _fixture_ids(),
)
def test_stage_b_encoded_inputs_match_music21_baseline(fixture_id: str):
    baseline = _baseline_for_fixture(fixture_id)
    candidate = _candidate_for_fixture(fixture_id)
    _assert_stage_parity("stage_b", baseline["stage_b"], candidate["stage_b"], tol=1e-6)


def test_fixture_specific_semantics_are_preserved():
    tied = _candidate_for_fixture("tied_barlines")
    tied_events = tied["stage_a"]["event_frames"]
    assert tied_events[1]["s_is_onset"] == [False]

    pickup = _candidate_for_fixture("pickup_anacrusis")
    pickup_events = pickup["stage_a"]["event_frames"]
    assert pickup_events[0]["s_measure"] == 0

    enh = _candidate_for_fixture("enharmonic_double")
    enh_notes = enh["stage_a"]["event_frames"][0]["s_notes"]
    assert enh_notes == ["C4", "E--4", "G-4", "F##4"]
    enh_intervals = enh["stage_a"]["event_frames"][0]["s_intervals"]
    assert enh_intervals == ["d3", "d5", "AA4"]

    dense = _candidate_for_fixture("dense_poly")
    dense_events = dense["stage_a"]["event_frames"]
    assert len(dense_events[0]["s_notes"]) == 2
    assert dense_events[1]["s_is_onset"] == [False, True]

    divisions = _candidate_for_fixture("divisions_change")
    divisions_events = divisions["stage_a"]["event_frames"]
    assert divisions_events[2]["s_offset"] == 4.0
    assert divisions_events[2]["s_duration"] == 1.5
    assert divisions_events[3]["s_offset"] == 5.5

    rests = _candidate_for_fixture("rest_heavy")
    rest_events = rests["stage_a"]["event_frames"]
    assert rest_events[1]["s_notes"] is None
    assert rest_events[3]["s_notes"] is None

    long_ties = _candidate_for_fixture("long_tie_chain")
    long_tie_events = long_ties["stage_a"]["event_frames"]
    assert long_tie_events[3]["s_is_onset"] == [False]

    multipart = _candidate_for_fixture("multi_part_enharmonic")
    multipart_events = multipart["stage_a"]["event_frames"]
    assert multipart_events[0]["s_notes"] == ["G-3", "F#3"]
    assert multipart_events[0]["s_intervals"] == ["d2"]


def test_chunk_boundary_fixture_clips_to_fixed_640_steps():
    baseline = _baseline_for_fixture("chunk_boundary")
    candidate = _candidate_for_fixture("chunk_boundary")
    assert baseline["stage_b"]["max_steps"] == 640
    assert candidate["stage_b"]["max_steps"] == 640
    assert baseline["stage_b"]["active_steps"] == 640
    assert candidate["stage_b"]["active_steps"] == 640
    assert len(candidate["stage_b"]["X_Bass19"]) == 640
    assert len(candidate["stage_b"]["X_Chromagram19"]) == 640
    assert len(candidate["stage_b"]["X_MeasureNoteOnset14"]) == 640


def test_rust_export_is_deterministic_for_repeated_runs():
    fixture = _fixture_record("dense_poly")
    musicxml_path = REPO_ROOT / fixture["musicxml_path"]
    first = _run_rust_export("dense_poly", musicxml_path)
    second = _run_rust_export("dense_poly", musicxml_path)
    _assert_stage_parity("stage_a", first["stage_a"], second["stage_a"])
    _assert_stage_parity("stage_b", first["stage_b"], second["stage_b"], tol=0.0)
