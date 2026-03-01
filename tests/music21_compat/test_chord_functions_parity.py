from __future__ import annotations

import functools
import sys
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).resolve().parents[2]
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))

from tests.music21_compat.parity_utils import (
    build_music21_timeline_artifact,
    first_mismatch,
    read_json,
    run_rust_timeline_export,
)


MANIFEST_PATH = REPO_ROOT / "tests" / "music21_compat" / "fixtures_manifest.json"


@functools.lru_cache(maxsize=None)
def _manifest() -> dict:
    data = read_json(MANIFEST_PATH)
    assert int(data.get("schema_version", 0)) == 1
    return data


@functools.lru_cache(maxsize=None)
def _fixture_record(fixture_id: str) -> dict:
    for fixture in _manifest()["fixtures"]:
        if fixture["id"] == fixture_id:
            return fixture
    raise KeyError(f"unknown fixture id: {fixture_id}")


@functools.lru_cache(maxsize=None)
def _baseline_timeline(fixture_id: str) -> dict:
    fixture = _fixture_record(fixture_id)
    musicxml_path = REPO_ROOT / fixture["musicxml_path"]
    return build_music21_timeline_artifact(musicxml_path, fixture_id)


@functools.lru_cache(maxsize=None)
def _candidate_timeline(fixture_id: str) -> dict:
    fixture = _fixture_record(fixture_id)
    musicxml_path = REPO_ROOT / fixture["musicxml_path"]
    return run_rust_timeline_export(fixture_id, musicxml_path)


@pytest.mark.parametrize(
    "fixture_id",
    ["tied_barlines", "pickup_anacrusis", "enharmonic_double", "dense_poly", "chunk_boundary"],
)
def test_chordify_timeline_parity_against_rust_port(fixture_id: str):
    expected = _baseline_timeline(fixture_id)
    actual = _candidate_timeline(fixture_id)
    mismatch = first_mismatch(expected, actual)
    assert mismatch is None, (
        f"timeline mismatch fixture={fixture_id} path={mismatch[0]} "
        f"expected={mismatch[1]} actual={mismatch[2]}"
    )


def test_edge_case_semantics_for_chord_functions():
    tied = _candidate_timeline("tied_barlines")
    assert tied["slices"][1]["notes"][0]["onset"] is False
    assert tied["slices"][1]["notes"][0]["hold"] is True
    assert tied["slices"][1]["notes"][0]["tie_stop"] is True

    pickup = _candidate_timeline("pickup_anacrusis")
    assert pickup["measure_number_shift"] == 1
    assert pickup["slices"][0]["measure_number"] == 0

    enharm = _candidate_timeline("enharmonic_double")
    intervals = [n["interval_from_bass"] for n in enharm["slices"][0]["notes"]]
    assert intervals == ["P1", "d3", "d5", "AA4"]

    dense = _candidate_timeline("dense_poly")
    assert dense["slices"][0]["notes"][1]["interval_from_bass"] == "P4"
    assert dense["slices"][1]["notes"][0]["hold"] is True


def test_chunk_boundary_produces_stable_slice_count():
    baseline = _baseline_timeline("chunk_boundary")
    candidate = _candidate_timeline("chunk_boundary")
    assert len(baseline["slices"]) == 41
    assert len(candidate["slices"]) == 41
    assert baseline["slices"][-1]["measure_number"] == 41
    assert candidate["slices"][-1]["measure_number"] == 41


def test_rust_timeline_export_is_deterministic():
    fixture = _fixture_record("dense_poly")
    musicxml_path = REPO_ROOT / fixture["musicxml_path"]
    first = run_rust_timeline_export("dense_poly", musicxml_path)
    second = run_rust_timeline_export("dense_poly", musicxml_path)
    mismatch = first_mismatch(first, second)
    assert mismatch is None, (
        f"non-deterministic export path={mismatch[0]} expected={mismatch[1]} actual={mismatch[2]}"
    )
