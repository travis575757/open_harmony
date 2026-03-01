from __future__ import annotations

import os
import sys
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).resolve().parents[2]
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))

from tests.music21_compat.parity_utils import (
    build_music21_timeline_artifact,
    first_mismatch,
    materialize_musicxml,
    run_rust_timeline_export,
)


MANIFEST_PATH = REPO_ROOT / "tests" / "corpora" / "when_in_rome_ci_manifest.txt"


def _load_manifest() -> list[str]:
    lines = []
    for raw in MANIFEST_PATH.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        lines.append(line)
    return lines


def _resolve_when_in_rome_root() -> Path | None:
    env_value = os.environ.get("WHEN_IN_ROME_ROOT", "").strip()
    candidates = []
    if env_value:
        candidates.append(Path(env_value))
    candidates.append(REPO_ROOT / "tests" / "corpora" / "When-in-Rome")
    candidates.append(REPO_ROOT / ".cache" / "when-in-rome")
    for path in candidates:
        if path.exists() and path.is_dir():
            return path
    return None


def _max_pieces() -> int:
    raw = os.environ.get("WHEN_IN_ROME_MAX_PIECES", "40").strip()
    try:
        value = int(raw)
    except ValueError as exc:
        raise AssertionError(f"invalid WHEN_IN_ROME_MAX_PIECES={raw!r}") from exc
    return max(1, min(value, 40))


def _selected_entries() -> list[str]:
    return _load_manifest()[: _max_pieces()]


def _id_from_entry(index: int, rel_path: str) -> str:
    token = rel_path.replace("/", "_").replace(".", "_")
    token = "".join(ch if ch.isalnum() or ch == "_" else "_" for ch in token)
    return f"wir_{index:02d}_{token[:48]}"


def _normalize_timeline_for_parity(artifact: dict) -> dict:
    normalized = {
        "schema_version": artifact["schema_version"],
        "source_id": artifact["source_id"],
        "measure_number_shift": artifact["measure_number_shift"],
        "slices": [],
    }
    for slice_item in artifact["slices"]:
        notes = [
            {
                "spelling": note["spelling"],
                "midi": note["midi"],
                "onset": note["onset"],
                "hold": note["hold"],
                "tie_start": note["tie_start"],
                "tie_stop": note["tie_stop"],
                "interval_from_bass": note["interval_from_bass"],
            }
            for note in slice_item["notes"]
        ]
        normalized["slices"].append(
            {
                "index": slice_item["index"],
                "start_div": slice_item["start_div"],
                "end_div": slice_item["end_div"],
                "measure_number": slice_item["measure_number"],
                "notes": notes,
            }
        )
    return normalized


def test_when_in_rome_manifest_contract():
    entries = _load_manifest()
    assert len(entries) == 40
    assert len(entries) == len(set(entries))
    assert all(entry.startswith("Corpus/") for entry in entries)
    assert all(entry.endswith("/score.mxl") for entry in entries)


@pytest.mark.parametrize("entry", _selected_entries())
def test_when_in_rome_music21_vs_rust_timeline_parity(entry: str, tmp_path: Path):
    corpus_root = _resolve_when_in_rome_root()
    if corpus_root is None:
        pytest.skip(
            "When-in-Rome corpus not available. Run "
            "'git submodule update --init --recursive tests/corpora/When-in-Rome', "
            "or set WHEN_IN_ROME_ROOT."
        )

    source_path = corpus_root / entry
    assert source_path.exists(), f"manifest entry missing from corpus root: {source_path}"

    fixture_index = _selected_entries().index(entry)
    fixture_id = _id_from_entry(fixture_index, entry)
    normalized_path = materialize_musicxml(source_path, tmp_path / f"{fixture_id}.musicxml")

    expected = _normalize_timeline_for_parity(
        build_music21_timeline_artifact(normalized_path, fixture_id)
    )
    actual = _normalize_timeline_for_parity(
        run_rust_timeline_export(fixture_id, normalized_path)
    )
    mismatch = first_mismatch(expected, actual)
    assert mismatch is None, (
        f"When-in-Rome parity mismatch entry={entry} path={mismatch[0]} "
        f"expected={mismatch[1]} actual={mismatch[2]}"
    )
