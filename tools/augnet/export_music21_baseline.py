#!/usr/bin/env python3
from __future__ import annotations

import argparse
import platform
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))

from tools.augnet.parity_common import (
    SCHEMA_VERSION,
    build_music21_event_frames,
    canonical_dump,
    decode_stage_d_labels,
    encode_stage_b_inputs,
    load_manifest,
    reindex_frames,
    run_onnx_stage_c,
)


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Export pinned Phase 3 baseline artifacts from music21 + ONNX inference."
    )
    parser.add_argument(
        "--manifest",
        type=Path,
        default=Path("tests/augnet_parity/fixtures_manifest.json"),
        help="Fixture manifest JSON path.",
    )
    parser.add_argument(
        "--model",
        type=Path,
        default=Path("models/augnet/AugmentedNet.onnx"),
        help="ONNX model path for stage C logits/argmax export.",
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=Path("tests/augnet_parity/music21_baseline"),
        help="Directory where baseline fixture artifacts are written.",
    )
    parser.add_argument(
        "--fixture-id",
        action="append",
        default=[],
        help="Optional fixture id filter (repeatable).",
    )
    parser.add_argument(
        "--overwrite",
        action="store_true",
        help="Allow overwriting existing baseline artifact files.",
    )
    return parser.parse_args(argv)


def _baseline_for_fixture(
    fixture_id: str,
    musicxml_path: Path,
    model_path: Path,
    fixed_offset: float,
    max_steps: int,
) -> dict:
    import music21
    from music21 import converter

    score = converter.parse(str(musicxml_path))
    event_frames = build_music21_event_frames(score)
    grid_frames = reindex_frames(event_frames, fixed_offset=fixed_offset)
    stage_b = encode_stage_b_inputs(grid_frames, fixed_offset=fixed_offset, max_steps=max_steps)
    stage_c = run_onnx_stage_c(model_path, stage_b)
    stage_d = decode_stage_d_labels(stage_c)

    return {
        "schema_version": SCHEMA_VERSION,
        "fixture_id": fixture_id,
        "source_musicxml": str(musicxml_path.as_posix()),
        "stage_a": {
            "event_frames": event_frames,
            "grid_frames": grid_frames,
        },
        "stage_b": stage_b,
        "stage_c": stage_c,
        "stage_d": stage_d,
        "metadata": {
            "python": sys.version.split()[0],
            "platform": platform.platform(),
            "music21": music21.__version__,
        },
    }


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    manifest = load_manifest(args.manifest)
    fixed_offset = float(manifest.get("fixed_offset", 0.25))
    max_steps = int(manifest.get("max_steps", 640))
    fixture_filter = set(args.fixture_id or [])

    fixtures = manifest["fixtures"]
    if fixture_filter:
        fixtures = [f for f in fixtures if f["id"] in fixture_filter]

    if not fixtures:
        raise RuntimeError("no fixtures selected from manifest")

    model_path = args.model.resolve()
    if not model_path.exists():
        raise FileNotFoundError(f"model not found: {model_path}")

    args.output_dir.mkdir(parents=True, exist_ok=True)

    for fixture in fixtures:
        fixture_id = fixture["id"]
        musicxml_path = Path(fixture["musicxml_path"])
        if not musicxml_path.exists():
            raise FileNotFoundError(f"fixture musicxml not found: {musicxml_path}")

        baseline_name = fixture.get("baseline_artifact", f"{fixture_id}.json")
        out_path = args.output_dir / baseline_name
        if out_path.exists() and not args.overwrite:
            raise FileExistsError(
                f"baseline artifact exists (use --overwrite): {out_path}"
            )

        artifact = _baseline_for_fixture(
            fixture_id=fixture_id,
            musicxml_path=musicxml_path,
            model_path=model_path,
            fixed_offset=fixed_offset,
            max_steps=max_steps,
        )
        canonical_dump(artifact, out_path)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
