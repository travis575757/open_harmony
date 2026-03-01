from __future__ import annotations

import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))

from tools.augnet.metrics import (  # noqa: E402
    MusicalMetricCounters,
    boundary_indices,
    mismatch_rate,
    parity_mismatch_count,
)


def test_boundary_indices_detect_changes_and_ignore_unknown_spans():
    tokens = ["I", "I", None, "V", "V", "vi", None, "vi", "I"]
    # Boundaries are only counted where both adjacent tokens are known.
    assert boundary_indices(tokens) == {5, 8}


def test_musical_metric_counters_accuracy_and_boundary_f1():
    truth = [
        {
            "roman_numeral": "I",
            "local_key": "C",
            "tonicized_key": "C",
            "chord_quality": "maj",
            "inversion": "",
        },
        {
            "roman_numeral": "I",
            "local_key": "C",
            "tonicized_key": "C",
            "chord_quality": "maj",
            "inversion": "6",
        },
        {
            "roman_numeral": "V",
            "local_key": "C",
            "tonicized_key": "C",
            "chord_quality": "7",
            "inversion": "7",
        },
    ]
    pred = [
        {
            "roman_numeral": "I",
            "local_key": "C",
            "tonicized_key": "C",
            "chord_quality": "maj",
            "inversion": "",
        },
        {
            "roman_numeral": "V",
            "local_key": "C",
            "tonicized_key": "C",
            "chord_quality": "7",
            "inversion": "65",
        },
        {
            "roman_numeral": "V",
            "local_key": "C",
            "tonicized_key": "C",
            "chord_quality": "7",
            "inversion": "7",
        },
    ]
    counters = MusicalMetricCounters.empty()
    counters.add_frames(truth, pred)
    report = counters.to_report_dict()

    assert report["roman_numeral_exact_accuracy"]["correct"] == 2
    assert report["roman_numeral_exact_accuracy"]["total"] == 3
    assert report["inversion_accuracy"]["correct"] == 2
    assert report["inversion_accuracy"]["total"] == 3
    assert report["harmonic_segment_boundary_f1"]["tp"] == 0
    assert report["harmonic_segment_boundary_f1"]["fp"] == 1
    assert report["harmonic_segment_boundary_f1"]["fn"] == 1
    assert report["harmonic_segment_boundary_f1"]["value"] == 0.0


def test_parity_mismatch_count_includes_length_delta():
    expected = [
        {"roman_numeral": "I", "local_key": "C"},
        {"roman_numeral": "V", "local_key": "C"},
        {"roman_numeral": "I", "local_key": "C"},
    ]
    actual = [
        {"roman_numeral": "I", "local_key": "C"},
        {"roman_numeral": "vi", "local_key": "C"},
    ]
    mismatches, total = parity_mismatch_count(expected, actual, keys=("roman_numeral", "local_key"))
    assert mismatches == 2
    assert total == 3
    assert mismatch_rate(mismatches, total) == 2 / 3
