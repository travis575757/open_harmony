from __future__ import annotations

import json
from pathlib import Path

from tools.augnet.parity_common import decode_stage_d_labels


REPO_ROOT = Path(__file__).resolve().parents[2]
BASELINE_DIR = REPO_ROOT / "tests" / "augnet_parity" / "music21_baseline"


def _load(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def test_stage_d_schema_preserves_head_logits_and_confidence_vectors():
    fixture = _load(BASELINE_DIR / "tied_barlines.json")
    stage_d = fixture["stage_d"]
    assert "heads" in stage_d
    assert "labels" in stage_d
    for head_name, head in stage_d["heads"].items():
        assert head["raw_logits"], f"{head_name}: raw logits missing"
        assert len(head["raw_logits"]) == stage_d["effective_steps"], f"{head_name}: row count"
        assert len(head["argmax"]) == stage_d["effective_steps"], f"{head_name}: argmax rows"
        assert len(head["confidence_top1"]) == stage_d["effective_steps"], f"{head_name}: top1 rows"
        assert len(head["confidence_margin"]) == stage_d["effective_steps"], f"{head_name}: margin rows"


def test_confidence_derivation_and_non_decision_behavior():
    stage_c = {
        "schema_version": 1,
        "effective_steps": 1,
        "heads": {},
    }

    # Build all required heads with deterministic logits.
    widths = {
        "Alto35": 35,
        "Bass35": 35,
        "HarmonicRhythm7": 7,
        "LocalKey38": 38,
        "PitchClassSet121": 121,
        "RomanNumeral31": 31,
        "Soprano35": 35,
        "Tenor35": 35,
        "TonicizedKey38": 38,
    }
    for head, width in widths.items():
        logits = [-9.0] * width
        logits[0] = 9.0
        argmax = 0
        if head == "LocalKey38":
            logits[0] = 10.0
            logits[1] = 9.0
            argmax = 1
        stage_c["heads"][head] = {
            "shape": [1, width],
            "logits": [logits],
            "argmax": [argmax],
        }

    stage_d = decode_stage_d_labels(stage_c)
    lk = stage_d["heads"]["LocalKey38"]
    assert lk["argmax"][0] == 1
    assert lk["confidence_top1"][0] < 0.5
    assert lk["confidence_margin"][0] < 0.0


def test_postprocess_baseline_labels_include_required_decoded_fields():
    fixture = _load(BASELINE_DIR / "pickup_anacrusis.json")
    label = fixture["stage_d"]["labels"][0]
    for field in [
        "roman_numeral_resolved",
        "local_key",
        "tonicized_key_resolved",
        "chord_quality",
        "inversion_figure",
        "component_labels",
        "component_confidence",
    ]:
        assert field in label, f"missing Stage D field: {field}"
