from __future__ import annotations

import json
import math
import subprocess
from dataclasses import dataclass
from fractions import Fraction
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[2]


def run_cmd(cmd: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        cmd,
        cwd=str(REPO_ROOT),
        text=True,
        capture_output=True,
        check=False,
    )


def read_json(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def lcm(a: int, b: int) -> int:
    return abs(a * b) // math.gcd(a, b)


def round4(value: float) -> float:
    return round(value * 10_000.0) / 10_000.0


def to_div(value_q: float, grid: int) -> int:
    return int(round(value_q * grid))


@dataclass(frozen=True)
class NoteEvent:
    part_id: str
    voice_id: str
    spelling: str
    midi: int
    start_q: float
    end_q: float
    tie_start: bool
    tie_stop: bool
    pitch_obj: Any


def collect_note_events(score: Any) -> list[NoteEvent]:
    from music21 import chord, note, stream

    events: list[NoteEvent] = []
    parts = list(score.parts) if hasattr(score, "parts") and len(score.parts) else [score]
    for part_idx, part in enumerate(parts):
        part_id = f"P{part_idx + 1}"
        for element in part.recurse().notes:
            start_q = float(element.getOffsetInHierarchy(score))
            end_q = start_q + float(element.quarterLength or 0.0)
            voice_obj = element.getContextByClass(stream.Voice)
            if voice_obj is None:
                voice_obj = getattr(element, "voice", None)
            voice_id = str(getattr(voice_obj, "id", "") or "1")
            if isinstance(element, chord.Chord):
                for n in element.notes:
                    tie = getattr(n, "tie", None)
                    tie_type = str(tie.type) if tie is not None and tie.type else ""
                    events.append(
                        NoteEvent(
                            part_id=part_id,
                            voice_id=voice_id,
                            spelling=str(n.pitch.nameWithOctave),
                            midi=int(n.pitch.midi),
                            start_q=start_q,
                            end_q=end_q,
                            tie_start=(tie_type == "start"),
                            tie_stop=(tie_type == "stop"),
                            pitch_obj=n.pitch,
                        )
                    )
            elif isinstance(element, note.Note):
                tie = getattr(element, "tie", None)
                tie_type = str(tie.type) if tie is not None and tie.type else ""
                events.append(
                    NoteEvent(
                        part_id=part_id,
                        voice_id=voice_id,
                        spelling=str(element.pitch.nameWithOctave),
                        midi=int(element.pitch.midi),
                        start_q=start_q,
                        end_q=end_q,
                        tie_start=(tie_type == "start"),
                        tie_stop=(tie_type == "stop"),
                        pitch_obj=element.pitch,
                    )
                )
    events.sort(key=lambda n: (n.start_q, n.end_q, n.midi, n.part_id, n.voice_id, n.spelling))
    return events


def measure_spans_and_shift(score: Any) -> tuple[list[tuple[int, float, float]], int]:
    from music21 import stream

    parts = list(score.parts) if hasattr(score, "parts") and len(score.parts) else [score]
    first_part = parts[0]
    measures = list(first_part.getElementsByClass(stream.Measure))
    if not measures:
        return [(1, 0.0, float(score.highestTime or 0.0))], 0

    spans: list[tuple[int, float, float]] = []
    starts = [float(m.getOffsetInHierarchy(score)) for m in measures]
    for idx, measure in enumerate(measures):
        number = int(measure.number) if measure.number is not None else (idx + 1)
        start = starts[idx]
        if idx + 1 < len(starts):
            end = starts[idx + 1]
        else:
            measured = float(getattr(measure, "highestTime", 0.0) or 0.0)
            nominal = float(getattr(getattr(measure, "barDuration", None), "quarterLength", 0.0) or 0.0)
            fallback = nominal if nominal > 0 else 4.0
            end = max(start + max(measured, fallback), float(score.highestTime or 0.0))
        if end <= start:
            end = start + 0.25
        spans.append((number, start, end))

    shift = 0
    first_span = spans[0][2] - spans[0][1]
    first_measure = measures[0]
    nominal = float(getattr(getattr(first_measure, "barDuration", None), "quarterLength", 0.0) or 0.0)
    if nominal > 0.0 and 0.0 < first_span < nominal:
        shift = 1

    return spans, shift


def measure_number_at(spans: list[tuple[int, float, float]], at_q: float) -> int:
    for number, start, end in spans:
        if start <= at_q < end:
            return int(number)
    return int(spans[-1][0]) if spans else 1


def grid_divisions_from_events(
    note_events: list[NoteEvent],
    measure_spans: list[tuple[int, float, float]],
) -> int:
    divisions: set[int] = {1}
    for event in note_events:
        divisions.add(Fraction(event.start_q).limit_denominator(4096).denominator)
        divisions.add(Fraction(event.end_q).limit_denominator(4096).denominator)
    for _, start_q, end_q in measure_spans:
        divisions.add(Fraction(start_q).limit_denominator(4096).denominator)
        divisions.add(Fraction(end_q).limit_denominator(4096).denominator)
    grid = 1
    for value in divisions:
        grid = lcm(grid, int(value))
    return max(1, grid)


def first_mismatch(expected: Any, actual: Any, path: str = "") -> tuple[str, Any, Any] | None:
    if isinstance(expected, float) or isinstance(actual, float):
        try:
            exp_f = float(expected)
            act_f = float(actual)
        except Exception:
            return (path, expected, actual)
        if abs(exp_f - act_f) <= 1e-9:
            return None
        return (path, exp_f, act_f)

    if type(expected) is not type(actual):
        return (path, expected, actual)

    if isinstance(expected, dict):
        e_keys = sorted(expected.keys())
        a_keys = sorted(actual.keys())
        if e_keys != a_keys:
            return (f"{path}.<keys>" if path else "<keys>", e_keys, a_keys)
        for key in e_keys:
            child = f"{path}.{key}" if path else key
            mismatch = first_mismatch(expected[key], actual[key], child)
            if mismatch is not None:
                return mismatch
        return None

    if isinstance(expected, list):
        if len(expected) != len(actual):
            return (f"{path}.<len>" if path else "<len>", len(expected), len(actual))
        for idx, (e_val, a_val) in enumerate(zip(expected, actual)):
            child = f"{path}[{idx}]"
            mismatch = first_mismatch(e_val, a_val, child)
            if mismatch is not None:
                return mismatch
        return None

    if expected != actual:
        return (path, expected, actual)
    return None


def build_music21_timeline_artifact(musicxml_path: Path, fixture_id: str) -> dict[str, Any]:
    from music21 import converter, interval

    score = converter.parse(str(musicxml_path))
    chordify_probe = converter.parse(str(musicxml_path))
    chordified = chordify_probe.chordify()
    assert len(list(chordified.recurse().notes)) > 0

    note_events = collect_note_events(score)
    measure_spans, measure_shift = measure_spans_and_shift(score)
    grid = grid_divisions_from_events(note_events, measure_spans)

    boundaries = set()
    for event in note_events:
        boundaries.add(round4(event.start_q))
        boundaries.add(round4(event.end_q))
    ordered = sorted(boundaries)

    slices = []
    for left, right in zip(ordered, ordered[1:]):
        if right <= left:
            continue
        active = [n for n in note_events if n.start_q <= left < n.end_q]
        if not active:
            continue
        active.sort(key=lambda n: (n.midi, n.part_id, n.voice_id, n.spelling))
        bass = active[0]

        notes = []
        for n in active:
            if n.midi == bass.midi and n.spelling == bass.spelling:
                interval_name = "P1"
            else:
                interval_name = str(interval.Interval(noteStart=bass.pitch_obj, noteEnd=n.pitch_obj).simpleName)
            onset = abs(n.start_q - left) < 1e-9 and not n.tie_stop
            notes.append(
                {
                    "part_id": n.part_id,
                    "voice_id": n.voice_id,
                    "spelling": n.spelling,
                    "midi": n.midi,
                    "onset": onset,
                    "hold": (not onset),
                    "tie_start": n.tie_start,
                    "tie_stop": n.tie_stop,
                    "interval_from_bass": interval_name,
                }
            )

        raw_measure = measure_number_at(measure_spans, left)
        slices.append(
            {
                "index": len(slices),
                "start_div": to_div(left, grid),
                "end_div": to_div(right, grid),
                "measure_number": raw_measure - measure_shift,
                "notes": notes,
            }
        )

    return {
        "schema_version": 1,
        "source_id": fixture_id,
        "measure_number_shift": measure_shift,
        "slices": slices,
    }


def run_rust_timeline_export(fixture_id: str, musicxml_path: Path) -> dict[str, Any]:
    cmd = [
        "cargo",
        "run",
        "--quiet",
        "-p",
        "cp_music21_compat",
        "--bin",
        "export_timeline_artifact",
        "--",
        "--fixture-id",
        fixture_id,
        "--musicxml-path",
        str(musicxml_path),
        "--pretty",
    ]
    proc = run_cmd(cmd)
    if proc.returncode != 0:
        raise AssertionError(
            f"timeline export failed for {fixture_id}\n"
            f"stdout:\n{proc.stdout}\n"
            f"stderr:\n{proc.stderr}"
        )
    return json.loads(proc.stdout)


def materialize_musicxml(source_path: Path, out_path: Path) -> Path:
    from music21 import converter

    score = converter.parse(str(source_path))
    out_path.parent.mkdir(parents=True, exist_ok=True)
    score.write("musicxml", fp=str(out_path))
    return out_path
