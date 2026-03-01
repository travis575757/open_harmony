#!/usr/bin/env python3
from __future__ import annotations

import argparse
import csv
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[2]
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))

from tools.augnet.metrics import (  # noqa: E402
    REQUIRED_MUSICAL_METRICS,
    MusicalMetricCounters,
    components_signature,
    metric_delta_pp,
    mismatch_rate,
    parity_mismatch_count,
)
from tools.augnet.parity_common import (  # noqa: E402
    build_music21_event_frames,
    canonical_dump,
    decode_stage_d_labels,
    encode_stage_b_inputs,
    reindex_frames,
    run_onnx_stage_c,
)

EXIT_OK = 0
EXIT_DETERMINISTIC_GATE_FAIL = 2
EXIT_PARITY_GATE_FAIL = 3
EXIT_MUSICAL_GATE_FAIL = 4


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Phase 8 corpus-level validation: deterministic fixture gate, corpus parity gate, "
            "and musical evaluation gate against When-in-Rome annotations."
        )
    )
    parser.add_argument(
        "--model",
        type=Path,
        default=Path("models/augnet/AugmentedNet.onnx"),
        help="ONNX model path.",
    )
    parser.add_argument(
        "--model-manifest",
        type=Path,
        default=Path("models/augnet/model-manifest.json"),
        help="Model manifest path for Rust export runner.",
    )
    parser.add_argument(
        "--deterministic-manifest",
        type=Path,
        default=Path("tests/augnet_parity/fixtures_manifest.json"),
        help="Deterministic fixture manifest for exact-match gate.",
    )
    parser.add_argument(
        "--deterministic-baseline-dir",
        type=Path,
        default=Path("tests/augnet_parity/music21_baseline"),
        help="Pinned deterministic baseline artifact directory.",
    )
    parser.add_argument(
        "--corpus-manifest",
        type=Path,
        default=Path("tests/corpora/when_in_rome_ci_manifest.txt"),
        help="When-in-Rome manifest (one score path per line).",
    )
    parser.add_argument(
        "--when-in-rome-root",
        type=Path,
        default=None,
        help="Optional explicit When-in-Rome root path.",
    )
    parser.add_argument(
        "--max-pieces",
        type=int,
        default=None,
        help="Optional max pieces from manifest.",
    )
    parser.add_argument(
        "--piece-offset",
        type=int,
        default=0,
        help="Optional starting offset in manifest entries.",
    )
    parser.add_argument(
        "--fixed-offset",
        type=float,
        default=0.25,
        help="Fixed frame offset in quarter lengths.",
    )
    parser.add_argument(
        "--max-steps",
        type=int,
        default=640,
        help="Chunk max steps (must match model fixed-T).",
    )
    parser.add_argument(
        "--report-dir",
        type=Path,
        default=Path("target/augnet/corpus_validation"),
        help="Directory for JSON/CSV/Markdown report artifacts.",
    )
    parser.add_argument(
        "--parity-threshold",
        type=float,
        default=0.001,
        help="Corpus parity mismatch-rate threshold (0.001 == 0.1%%).",
    )
    parser.add_argument(
        "--metric-delta-pp-threshold",
        type=float,
        default=0.25,
        help="Max allowed per-metric Rust-vs-Python delta in percentage points.",
    )
    parser.add_argument(
        "--rust-export-bin",
        type=Path,
        default=Path("target/debug/augnet_corpus_export"),
        help="Rust exporter binary path.",
    )
    parser.add_argument(
        "--skip-rust-build",
        action="store_true",
        help="Skip `cargo build` for the Rust export runner.",
    )
    parser.add_argument(
        "--skip-deterministic-fixture-gate",
        action="store_true",
        help="Skip deterministic fixture exact-match gate.",
    )
    parser.add_argument(
        "--precomputed-json",
        type=Path,
        default=None,
        help="Optional precomputed payload for integration tests (skips runtime corpus execution).",
    )
    return parser.parse_args(argv)


def _run(cmd: list[str], cwd: Path = REPO_ROOT) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        cmd,
        cwd=str(cwd),
        text=True,
        capture_output=True,
        check=False,
    )


def _resolve_when_in_rome_root(explicit: Path | None) -> Path:
    candidates: list[Path] = []
    if explicit is not None:
        candidates.append(explicit)
    env_value = os.environ.get("WHEN_IN_ROME_ROOT", "").strip()
    if env_value:
        candidates.append(Path(env_value))
    candidates.append(REPO_ROOT / "tests" / "corpora" / "When-in-Rome")
    candidates.append(REPO_ROOT / ".cache" / "when-in-rome")
    for path in candidates:
        if path.exists() and path.is_dir():
            return path
    raise FileNotFoundError(
        "When-in-Rome corpus root not found. Initialize submodule with "
        "'git submodule update --init --recursive tests/corpora/When-in-Rome', "
        "or use --when-in-rome-root / WHEN_IN_ROME_ROOT."
    )


def _load_manifest_entries(path: Path) -> list[str]:
    lines: list[str] = []
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        lines.append(line)
    return lines


def _piece_id(index: int, manifest_entry: str) -> str:
    token = manifest_entry.replace("/", "_").replace(".", "_")
    token = re.sub(r"[^0-9A-Za-z_]+", "_", token)
    token = re.sub(r"_+", "_", token).strip("_")
    return f"wir_{index:04d}_{token[:80]}"


def _normalize_key(value: str | None) -> str | None:
    if value is None:
        return None
    return str(value).replace("b", "-")


def _normalize_roman(value: str | None) -> str | None:
    if value is None:
        return None
    return "".join(str(value).split())


def _annotation_quality(rn: Any) -> str | None:
    figure = str(rn.figure)
    common = str(getattr(rn, "commonName", "")).lower()
    quality = str(getattr(rn, "quality", "")).lower()
    has_seventh = any(tok in figure for tok in ("7", "65", "64", "43", "42", "2", "9")) or len(rn.pitches) >= 4

    if figure.startswith(("Ger", "Fr", "It")):
        return "aug6"
    if "half-diminished seventh" in common or "ø7" in figure:
        return "hdim7"
    if "diminished seventh" in common or "o7" in figure:
        return "dim7"
    if "dominant seventh" in common or "incomplete dominant-seventh chord" in common:
        return "7"
    if "major seventh" in common:
        return "maj7"
    if "minor seventh" in common:
        return "min7"
    if "augmented seventh" in common or "augmented major tetrachord" in common:
        return "aug7"
    if "augmented sixth" in common:
        return "aug6"
    if "augmented triad" in common:
        return "aug"
    if "diminished triad" in common:
        return "dim"
    if "minor triad" in common:
        return "min"
    if "major triad" in common:
        return "maj"

    if "+" in figure:
        return "aug7" if has_seventh else "aug"
    if quality == "major":
        return "maj7" if has_seventh else "maj"
    if quality == "minor":
        return "min7" if has_seventh else "min"
    if quality == "diminished":
        return "dim7" if has_seventh else "dim"
    if quality == "augmented":
        return "aug7" if has_seventh else "aug"
    return None


def _annotation_inversion(rn: Any) -> str | None:
    try:
        inversion = str(rn.inversionName())
    except Exception:
        return None
    mapping = {
        "53": "",
        "6": "6",
        "64": "64",
        "7": "7",
        "65": "65",
        "43": "43",
        "42": "2",
        "2": "2",
    }
    return mapping.get(inversion)


def _annotation_segments(
    score_path: Path,
    analysis_path: Path,
) -> list[tuple[float, float, dict[str, str | None]]]:
    from music21 import converter

    score = converter.parse(str(score_path))
    analysis = converter.parse(str(analysis_path), format="romanText")
    rns = list(analysis.recurse().getElementsByClass("RomanNumeral"))
    if not rns:
        return []

    offsets = [float(rn.getOffsetInHierarchy(analysis)) for rn in rns]
    score_end = float(score.highestTime or analysis.highestTime or 0.0)
    score_end = max(score_end, offsets[-1])

    segments: list[tuple[float, float, dict[str, str | None]]] = []
    for idx, rn in enumerate(rns):
        start = offsets[idx]
        end = offsets[idx + 1] if idx + 1 < len(offsets) else score_end
        if end <= start:
            continue
        local_key = _normalize_key(rn.key.tonicPitchNameWithCase)
        secondary = getattr(rn, "secondaryRomanNumeralKey", None)
        tonicized_key = _normalize_key(
            secondary.tonicPitchNameWithCase if secondary is not None else local_key
        )
        label = {
            "roman_numeral": _normalize_roman(rn.figure),
            "local_key": local_key,
            "tonicized_key": tonicized_key,
            "chord_quality": _annotation_quality(rn),
            "inversion": _annotation_inversion(rn),
        }
        segments.append((start, end, label))
    return segments


def _project_truth_frames(
    offsets: list[float],
    segments: list[tuple[float, float, dict[str, str | None]]],
) -> list[dict[str, str | None]]:
    if not offsets:
        return []
    if not segments:
        return [
            {
                "roman_numeral": None,
                "local_key": None,
                "tonicized_key": None,
                "chord_quality": None,
                "inversion": None,
            }
            for _ in offsets
        ]

    out: list[dict[str, str | None]] = []
    seg_idx = 0
    for offset in offsets:
        while seg_idx + 1 < len(segments) and offset >= segments[seg_idx][1]:
            seg_idx += 1
        start, end, label = segments[seg_idx]
        if start <= offset < end:
            out.append(dict(label))
        else:
            out.append(
                {
                    "roman_numeral": None,
                    "local_key": None,
                    "tonicized_key": None,
                    "chord_quality": None,
                    "inversion": None,
                }
            )
    return out


def _materialize_musicxml(source_path: Path, out_path: Path) -> Path:
    from music21 import converter

    score = converter.parse(str(source_path))
    out_path.parent.mkdir(parents=True, exist_ok=True)
    score.write("musicxml", fp=str(out_path))
    return out_path


def _python_frames_for_score(
    musicxml_path: Path,
    model_path: Path,
    fixed_offset: float,
    max_steps: int,
) -> list[dict[str, Any]]:
    from music21 import converter

    score = converter.parse(str(musicxml_path))
    event_frames = build_music21_event_frames(score)
    grid_frames = reindex_frames(event_frames, fixed_offset=fixed_offset)
    if not grid_frames:
        return []

    frames: list[dict[str, Any]] = []
    for chunk_start in range(0, len(grid_frames), max_steps):
        chunk_frames = grid_frames[chunk_start : chunk_start + max_steps]
        stage_b = encode_stage_b_inputs(chunk_frames, fixed_offset=fixed_offset, max_steps=max_steps)
        stage_c = run_onnx_stage_c(model_path=model_path, stage_b=stage_b)
        stage_d = decode_stage_d_labels(stage_c)
        active = len(chunk_frames)
        for local_idx in range(active):
            label = stage_d["labels"][local_idx]
            frame = {
                "global_step": chunk_start + local_idx,
                "offset_q": float(chunk_frames[local_idx]["s_offset"]),
                "roman_numeral": _normalize_roman(label["roman_numeral_formatted"]),
                "local_key": _normalize_key(label["local_key"]),
                "tonicized_key": _normalize_key(label["tonicized_key_resolved"]),
                "chord_quality": str(label["chord_quality"]),
                "inversion": str(label["inversion_figure"]),
                "components": label["components"],
            }
            frame["components_signature"] = components_signature(frame)
            frames.append(frame)
    return frames


def _rust_frames_for_score(
    rust_bin: Path,
    fixture_id: str,
    musicxml_path: Path,
    model_path: Path,
    manifest_path: Path,
    fixed_offset: float,
    max_steps: int,
) -> list[dict[str, Any]]:
    shared_args = [
        "--fixture-id",
        fixture_id,
        "--musicxml-path",
        str(musicxml_path),
        "--model-path",
        str(model_path),
        "--manifest-path",
        str(manifest_path),
        "--fixed-offset",
        str(fixed_offset),
        "--max-steps",
        str(max_steps),
    ]
    cmd = [str(rust_bin), *shared_args]
    proc = _run(cmd)
    if proc.returncode != 0 and "libonnxruntime" in (proc.stderr or ""):
        cmd = [
            "cargo",
            "run",
            "--quiet",
            "-p",
            "cp_engine",
            "--bin",
            "augnet_corpus_export",
            "--",
            *shared_args,
        ]
        proc = _run(cmd)
    if proc.returncode != 0:
        raise RuntimeError(
            "rust export failed\n"
            f"cmd={' '.join(cmd)}\nstdout={proc.stdout}\nstderr={proc.stderr}"
        )
    parsed = json.loads(proc.stdout)
    out: list[dict[str, Any]] = []
    for row in parsed.get("frames", []):
        frame = {
            "global_step": int(row["global_step"]),
            "offset_q": float(row["offset_q"]),
            "roman_numeral": _normalize_roman(row.get("roman_numeral")),
            "local_key": _normalize_key(row.get("local_key")),
            "tonicized_key": _normalize_key(row.get("tonicized_key")),
            "chord_quality": row.get("chord_quality"),
            "inversion": row.get("inversion"),
            "components": row.get("components") or {},
        }
        frame["components_signature"] = components_signature(frame)
        out.append(frame)
    return out


def _analysis_path_for_piece(piece_dir: Path) -> Path | None:
    for name in ("analysis.txt", "analysis_A.txt", "analysis_B.txt"):
        path = piece_dir / name
        if path.exists():
            return path
    return None


def _coerce_metric_report(raw: dict[str, Any]) -> dict[str, dict[str, float | int]]:
    report: dict[str, dict[str, float | int]] = {}
    for metric in REQUIRED_MUSICAL_METRICS:
        if metric not in raw:
            raise KeyError(f"missing metric in report: {metric}")
        value = raw[metric]
        if isinstance(value, dict):
            item = dict(value)
            if "value" not in item:
                raise KeyError(f"metric {metric} missing 'value'")
            report[metric] = item  # type: ignore[assignment]
        else:
            report[metric] = {"value": float(value)}
    return report


def _run_deterministic_fixture_gate(
    args: argparse.Namespace,
    report_path: Path,
) -> tuple[bool, dict[str, Any]]:
    if args.skip_deterministic_fixture_gate:
        return True, {"status": "skipped"}

    cmd = [
        sys.executable,
        str(REPO_ROOT / "tools" / "augnet" / "diff_against_music21.py"),
        "--manifest",
        str(args.deterministic_manifest),
        "--baseline-dir",
        str(args.deterministic_baseline_dir),
        "--model",
        str(args.model),
        "--report-path",
        str(report_path),
    ]
    proc = _run(cmd)
    if report_path.exists():
        report = json.loads(report_path.read_text(encoding="utf-8"))
    else:
        report = {}
    report["runner_return_code"] = proc.returncode
    if proc.returncode != 0:
        report["runner_stdout"] = proc.stdout
        report["runner_stderr"] = proc.stderr
    passed = proc.returncode == 0 and report.get("status") == "ok"
    return passed, report


def _build_rust_export_binary(rust_bin: Path, skip_build: bool) -> None:
    if skip_build:
        if not rust_bin.exists():
            raise FileNotFoundError(f"Rust export binary not found: {rust_bin}")
        return
    cmd = ["cargo", "build", "--quiet", "-p", "cp_engine", "--bin", "augnet_corpus_export"]
    proc = _run(cmd)
    if proc.returncode != 0:
        raise RuntimeError(
            "failed to build rust export binary\n"
            f"stdout={proc.stdout}\n"
            f"stderr={proc.stderr}"
        )
    if not rust_bin.exists():
        raise FileNotFoundError(f"Rust export binary missing after build: {rust_bin}")


def _write_piece_csv(rows: list[dict[str, Any]], path: Path) -> None:
    if not rows:
        path.write_text("piece_id\n", encoding="utf-8")
        return
    fieldnames: list[str] = []
    for row in rows:
        for key in row.keys():
            if key not in fieldnames:
                fieldnames.append(key)
    with path.open("w", encoding="utf-8", newline="") as fh:
        writer = csv.DictWriter(fh, fieldnames=fieldnames)
        writer.writeheader()
        for row in rows:
            writer.writerow(row)


def _build_markdown(summary: dict[str, Any]) -> str:
    lines: list[str] = []
    lines.append("# Phase 8 Corpus Validation Summary")
    lines.append("")
    lines.append(f"- overall_status: `{summary['overall_status']}`")
    lines.append(
        f"- deterministic_fixture_gate: `{summary['deterministic_fixture_gate']['passed']}`"
    )
    parity = summary["corpus_parity_gate"]
    lines.append(
        f"- corpus_parity_rate: `{parity['mismatch_rate']:.6f}` (threshold `{parity['threshold']:.6f}`)"
    )
    lines.append(f"- pieces_processed: `{summary['pieces_processed']}`")
    lines.append(f"- pieces_with_annotation: `{summary['pieces_with_annotation']}`")
    lines.append("")
    lines.append("## Musical Metrics")
    for metric in REQUIRED_MUSICAL_METRICS:
        item = summary["musical_evaluation_gate"]["metrics"][metric]
        lines.append(
            f"- {metric}: python={item['python_value'] * 100.0:.4f}% "
            f"rust={item['rust_value'] * 100.0:.4f}% "
            f"delta_pp={item['delta_pp']:.4f} "
            f"(threshold={summary['musical_evaluation_gate']['threshold_pp']:.4f}) "
            f"passed={item['passed']}"
        )
    lines.append("")
    if summary["warnings"]:
        lines.append("## Warnings")
        for warning in summary["warnings"]:
            lines.append(f"- {warning}")
    return "\n".join(lines) + "\n"


def _finalize_summary_and_exit_code(
    *,
    report_dir: Path,
    deterministic_passed: bool,
    deterministic_report: dict[str, Any],
    parity_mismatches: int,
    parity_total: int,
    python_metric_report: dict[str, dict[str, float | int]],
    rust_metric_report: dict[str, dict[str, float | int]],
    piece_rows: list[dict[str, Any]],
    pieces_processed: int,
    pieces_with_annotation: int,
    warnings: list[str],
    parity_threshold: float,
    metric_delta_pp_threshold: float,
) -> tuple[int, dict[str, Any]]:
    parity_value = mismatch_rate(parity_mismatches, parity_total)
    parity_passed = parity_value <= parity_threshold

    metric_rows: dict[str, Any] = {}
    failing_metrics: list[str] = []
    for metric in REQUIRED_MUSICAL_METRICS:
        py_value = float(python_metric_report[metric]["value"])
        rust_value = float(rust_metric_report[metric]["value"])
        delta = metric_delta_pp(python_metric_report, rust_metric_report, metric)
        metric_passed = delta <= metric_delta_pp_threshold
        if not metric_passed:
            failing_metrics.append(metric)
        metric_rows[metric] = {
            "python_value": py_value,
            "rust_value": rust_value,
            "delta_pp": delta,
            "passed": metric_passed,
        }

    musical_passed = len(failing_metrics) == 0
    if deterministic_passed and parity_passed and musical_passed:
        exit_code = EXIT_OK
        overall = "ok"
    elif not deterministic_passed:
        exit_code = EXIT_DETERMINISTIC_GATE_FAIL
        overall = "failed"
    elif not parity_passed:
        exit_code = EXIT_PARITY_GATE_FAIL
        overall = "failed"
    else:
        exit_code = EXIT_MUSICAL_GATE_FAIL
        overall = "failed"

    summary = {
        "overall_status": overall,
        "exit_code": exit_code,
        "deterministic_fixture_gate": {
            "passed": deterministic_passed,
            "report": deterministic_report,
        },
        "corpus_parity_gate": {
            "passed": parity_passed,
            "mismatch_frames": parity_mismatches,
            "total_frames": parity_total,
            "mismatch_rate": parity_value,
            "threshold": parity_threshold,
        },
        "musical_evaluation_gate": {
            "passed": musical_passed,
            "threshold_pp": metric_delta_pp_threshold,
            "metrics": metric_rows,
            "failing_metrics": failing_metrics,
            "python_metrics": python_metric_report,
            "rust_metrics": rust_metric_report,
        },
        "pieces_processed": pieces_processed,
        "pieces_with_annotation": pieces_with_annotation,
        "pieces_without_annotation": pieces_processed - pieces_with_annotation,
        "warnings": warnings,
        "artifacts": {
            "summary_json": str((report_dir / "summary.json").as_posix()),
            "piece_csv": str((report_dir / "piece_metrics.csv").as_posix()),
            "summary_md": str((report_dir / "summary.md").as_posix()),
        },
    }

    canonical_dump(summary, report_dir / "summary.json")
    _write_piece_csv(piece_rows, report_dir / "piece_metrics.csv")
    (report_dir / "summary.md").write_text(_build_markdown(summary), encoding="utf-8")
    return exit_code, summary


def _run_precomputed_mode(args: argparse.Namespace) -> int:
    payload = json.loads(args.precomputed_json.read_text(encoding="utf-8"))
    report_dir = args.report_dir
    report_dir.mkdir(parents=True, exist_ok=True)

    deterministic = payload.get("deterministic_fixture_gate", {})
    deterministic_passed = bool(deterministic.get("passed", True))
    deterministic_report = dict(deterministic.get("report", {}))

    parity = payload.get("corpus_parity", {})
    mismatches = int(parity.get("mismatch_frames", 0))
    total = int(parity.get("total_frames", 0))

    musical = payload.get("musical_metrics", {})
    python_metric_report = _coerce_metric_report(musical.get("python", {}))
    rust_metric_report = _coerce_metric_report(musical.get("rust", {}))

    piece_rows = list(payload.get("piece_rows", []))
    pieces_processed = int(payload.get("pieces_processed", len(piece_rows)))
    pieces_with_annotation = int(payload.get("pieces_with_annotation", pieces_processed))
    warnings = list(payload.get("warnings", []))

    exit_code, _summary = _finalize_summary_and_exit_code(
        report_dir=report_dir,
        deterministic_passed=deterministic_passed,
        deterministic_report=deterministic_report,
        parity_mismatches=mismatches,
        parity_total=total,
        python_metric_report=python_metric_report,
        rust_metric_report=rust_metric_report,
        piece_rows=piece_rows,
        pieces_processed=pieces_processed,
        pieces_with_annotation=pieces_with_annotation,
        warnings=warnings,
        parity_threshold=args.parity_threshold,
        metric_delta_pp_threshold=args.metric_delta_pp_threshold,
    )
    return exit_code


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    report_dir = args.report_dir
    report_dir.mkdir(parents=True, exist_ok=True)

    if args.precomputed_json is not None:
        return _run_precomputed_mode(args)

    model_path = args.model.resolve()
    if not model_path.exists():
        raise FileNotFoundError(f"model not found: {model_path}")
    model_manifest_path = args.model_manifest.resolve()
    if not model_manifest_path.exists():
        raise FileNotFoundError(f"model manifest not found: {model_manifest_path}")

    deterministic_report_path = report_dir / "deterministic_fixture_diff_report.json"
    deterministic_passed, deterministic_report = _run_deterministic_fixture_gate(
        args, deterministic_report_path
    )

    when_in_rome_root = _resolve_when_in_rome_root(args.when_in_rome_root)
    entries = _load_manifest_entries(args.corpus_manifest)
    if args.piece_offset < 0:
        raise ValueError("--piece-offset must be >= 0")
    entries = entries[args.piece_offset :]
    if args.max_pieces is not None:
        entries = entries[: max(0, args.max_pieces)]
    if not entries:
        raise RuntimeError("no corpus entries selected")

    rust_bin = args.rust_export_bin
    if not rust_bin.is_absolute():
        rust_bin = REPO_ROOT / rust_bin
    _build_rust_export_binary(rust_bin, skip_build=args.skip_rust_build)

    python_counters = MusicalMetricCounters.empty()
    rust_counters = MusicalMetricCounters.empty()
    piece_rows: list[dict[str, Any]] = []
    warnings: list[str] = []

    parity_mismatches = 0
    parity_total = 0
    pieces_with_annotation = 0

    with tempfile.TemporaryDirectory(prefix="corpus_eval_") as tmp_dir:
        tmp_root = Path(tmp_dir)
        for rel_index, manifest_entry in enumerate(entries):
            source_score = when_in_rome_root / manifest_entry
            if not source_score.exists():
                raise FileNotFoundError(f"manifest entry not found: {source_score}")
            piece_id = _piece_id(args.piece_offset + rel_index, manifest_entry)
            normalized_musicxml = _materialize_musicxml(
                source_score, tmp_root / f"{piece_id}.musicxml"
            )
            python_frames = _python_frames_for_score(
                musicxml_path=normalized_musicxml,
                model_path=model_path,
                fixed_offset=args.fixed_offset,
                max_steps=args.max_steps,
            )
            rust_frames = _rust_frames_for_score(
                rust_bin=rust_bin,
                fixture_id=piece_id,
                musicxml_path=normalized_musicxml,
                model_path=model_path,
                manifest_path=model_manifest_path,
                fixed_offset=args.fixed_offset,
                max_steps=args.max_steps,
            )
            piece_mismatches, piece_total = parity_mismatch_count(python_frames, rust_frames)
            parity_mismatches += piece_mismatches
            parity_total += piece_total

            piece_dir = source_score.parent
            analysis_path = _analysis_path_for_piece(piece_dir)
            row: dict[str, Any] = {
                "piece_id": piece_id,
                "manifest_entry": manifest_entry,
                "frame_count_python": len(python_frames),
                "frame_count_rust": len(rust_frames),
                "parity_mismatch_frames": piece_mismatches,
                "parity_total_frames": piece_total,
                "parity_mismatch_rate": mismatch_rate(piece_mismatches, piece_total),
                "has_annotation": bool(analysis_path),
                "analysis_path": str(analysis_path.as_posix()) if analysis_path else "",
            }

            if analysis_path is not None:
                segments = _annotation_segments(
                    score_path=normalized_musicxml,
                    analysis_path=analysis_path,
                )
                if not segments:
                    warnings.append(
                        f"{piece_id}: annotation parse produced no roman numerals ({analysis_path})"
                    )
                else:
                    pieces_with_annotation += 1
                    truth_for_python = _project_truth_frames(
                        [float(frame["offset_q"]) for frame in python_frames], segments
                    )
                    truth_for_rust = _project_truth_frames(
                        [float(frame["offset_q"]) for frame in rust_frames], segments
                    )
                    piece_py = MusicalMetricCounters.empty()
                    piece_rust = MusicalMetricCounters.empty()
                    piece_py.add_frames(truth_for_python, python_frames)
                    piece_rust.add_frames(truth_for_rust, rust_frames)
                    python_counters.add_frames(truth_for_python, python_frames)
                    rust_counters.add_frames(truth_for_rust, rust_frames)
                    piece_py_report = piece_py.to_report_dict()
                    piece_rust_report = piece_rust.to_report_dict()
                    for metric in REQUIRED_MUSICAL_METRICS:
                        row[f"python_{metric}"] = piece_py_report[metric]["value"]
                        row[f"rust_{metric}"] = piece_rust_report[metric]["value"]
            else:
                warnings.append(f"{piece_id}: no human analysis file found in {piece_dir}")

            piece_rows.append(row)

    python_metric_report = python_counters.to_report_dict()
    rust_metric_report = rust_counters.to_report_dict()
    for metric in REQUIRED_MUSICAL_METRICS:
        py_total = python_metric_report[metric].get("total")
        if isinstance(py_total, int) and py_total == 0 and metric != "harmonic_segment_boundary_f1":
            raise RuntimeError(f"metric has zero evaluated truth frames: {metric}")

    exit_code, _summary = _finalize_summary_and_exit_code(
        report_dir=report_dir,
        deterministic_passed=deterministic_passed,
        deterministic_report=deterministic_report,
        parity_mismatches=parity_mismatches,
        parity_total=parity_total,
        python_metric_report=python_metric_report,
        rust_metric_report=rust_metric_report,
        piece_rows=piece_rows,
        pieces_processed=len(entries),
        pieces_with_annotation=pieces_with_annotation,
        warnings=warnings,
        parity_threshold=args.parity_threshold,
        metric_delta_pp_threshold=args.metric_delta_pp_threshold,
    )
    return exit_code


if __name__ == "__main__":
    raise SystemExit(main())
