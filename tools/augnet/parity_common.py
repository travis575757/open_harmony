from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import numpy as np

SCHEMA_VERSION = 1
INPUT_NAMES = ("X_Bass19", "X_Chromagram19", "X_MeasureNoteOnset14")
OUTPUT_HEADS = (
    "Alto35",
    "Bass35",
    "HarmonicRhythm7",
    "LocalKey38",
    "PitchClassSet121",
    "RomanNumeral31",
    "Soprano35",
    "Tenor35",
    "TonicizedKey38",
)
ROUND_SCALE = 1_000_000.0
ROUND4_SCALE = 10_000.0


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


def load_manifest(manifest_path: Path) -> dict[str, Any]:
    with manifest_path.open("r", encoding="utf-8") as fh:
        data = json.load(fh)
    if int(data.get("schema_version", 0)) != SCHEMA_VERSION:
        raise ValueError(
            f"unsupported manifest schema_version={data.get('schema_version')} (expected {SCHEMA_VERSION})"
        )
    if "fixtures" not in data or not isinstance(data["fixtures"], list):
        raise ValueError("manifest must contain list field 'fixtures'")
    return data


def round6(v: float) -> float:
    return round(float(v) * ROUND_SCALE) / ROUND_SCALE


def round4(v: float) -> float:
    return round(float(v) * ROUND4_SCALE) / ROUND4_SCALE


def to_scaled4(v: float) -> int:
    return int(round(float(v) * ROUND4_SCALE))


def from_scaled4(v: int) -> float:
    return round4(v / ROUND4_SCALE)


def parse_step_and_pc(note: str) -> tuple[int, int] | None:
    if not note:
        return None
    step = note[0]
    step_idx = {"C": 0, "D": 1, "E": 2, "F": 3, "G": 4, "A": 5, "B": 6}.get(step)
    base = {"C": 0, "D": 2, "E": 4, "F": 5, "G": 7, "A": 9, "B": 11}.get(step)
    if base is None:
        return None
    alter = 0
    for c in note[1:]:
        if c == "#":
            alter += 1
        elif c == "-":
            alter -= 1
        elif c.isdigit():
            break
    return step_idx, (base + alter) % 12


def _measure_spans_and_shift(score: Any) -> tuple[list[tuple[int, float, float]], int]:
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
    if spans:
        first_span = spans[0][2] - spans[0][1]
        first_measure = measures[0]
        nominal = float(
            getattr(getattr(first_measure, "barDuration", None), "quarterLength", 0.0) or 0.0
        )
        if nominal > 0.0 and 0.0 < first_span < nominal:
            shift = 1
    return spans, shift


def _measure_number_at(spans: list[tuple[int, float, float]], at_q: float) -> int:
    for number, start, end in spans:
        if start <= at_q < end:
            return int(number)
    return int(spans[-1][0]) if spans else 1


def _collect_note_events(score: Any) -> list[NoteEvent]:
    from music21 import chord, note

    out: list[NoteEvent] = []
    parts = list(score.parts) if hasattr(score, "parts") and len(score.parts) else [score]
    for part_idx, part in enumerate(parts):
        part_id = str(getattr(part, "id", "") or f"P{part_idx + 1}")
        for element in part.recurse().notes:
            offset_q = float(element.getOffsetInHierarchy(score))
            duration_q = float(element.quarterLength or 0.0)
            end_q = offset_q + duration_q
            voice_obj = getattr(element, "voice", None)
            voice_id = str(getattr(voice_obj, "id", "") or "1")
            if isinstance(element, chord.Chord):
                for n in element.notes:
                    tie = getattr(n, "tie", None)
                    tie_type = str(tie.type) if tie is not None and tie.type else ""
                    out.append(
                        NoteEvent(
                            part_id=part_id,
                            voice_id=voice_id,
                            spelling=str(n.pitch.nameWithOctave),
                            midi=int(n.pitch.midi),
                            start_q=offset_q,
                            end_q=end_q,
                            tie_start=tie_type == "start",
                            tie_stop=tie_type == "stop",
                            pitch_obj=n.pitch,
                        )
                    )
            elif isinstance(element, note.Note):
                tie = getattr(element, "tie", None)
                tie_type = str(tie.type) if tie is not None and tie.type else ""
                out.append(
                    NoteEvent(
                        part_id=part_id,
                        voice_id=voice_id,
                        spelling=str(element.pitch.nameWithOctave),
                        midi=int(element.pitch.midi),
                        start_q=offset_q,
                        end_q=end_q,
                        tie_start=tie_type == "start",
                        tie_stop=tie_type == "stop",
                        pitch_obj=element.pitch,
                    )
                )
    out.sort(key=lambda n: (n.start_q, n.end_q, n.midi, n.part_id, n.voice_id, n.spelling))
    return out


def build_music21_event_frames(score: Any) -> list[dict[str, Any]]:
    from music21 import interval

    notes = _collect_note_events(score)
    spans, measure_shift = _measure_spans_and_shift(score)
    if not spans:
        return []

    boundaries = set()
    for _, start, end in spans:
        boundaries.add(round4(start))
        boundaries.add(round4(end))
    for n in notes:
        boundaries.add(round4(n.start_q))
        boundaries.add(round4(n.end_q))
    ordered = sorted(boundaries)

    rows: list[dict[str, Any]] = []
    for left, right in zip(ordered, ordered[1:]):
        if right <= left:
            continue
        active = [n for n in notes if n.start_q <= left < n.end_q]
        measure = _measure_number_at(spans, left) - measure_shift
        row: dict[str, Any] = {
            "s_offset": round4(left),
            "s_duration": round4(right - left),
            "s_measure": int(measure),
            "s_notes": None,
            "s_intervals": None,
            "s_is_onset": None,
        }
        if active:
            active.sort(key=lambda n: (n.midi, n.part_id, n.voice_id, n.spelling))
            bass = active[0]
            row["s_notes"] = [n.spelling for n in active]
            row["s_intervals"] = [
                str(interval.Interval(noteStart=bass.pitch_obj, noteEnd=n.pitch_obj).simpleName)
                for n in active[1:]
            ]
            row["s_is_onset"] = [bool(n.start_q == left and not n.tie_stop) for n in active]
        rows.append(row)

    if rows:
        score_last = round4(spans[-1][2])
        current_last = round4(rows[-1]["s_offset"] + rows[-1]["s_duration"])
        delta = round4(score_last - current_last)
        if delta != 0.0:
            rows[-1]["s_duration"] = round4(rows[-1]["s_duration"] + delta)

    deduped: list[dict[str, Any]] = []
    seen = set()
    for row in rows:
        key = to_scaled4(row["s_offset"])
        if key in seen:
            continue
        seen.add(key)
        deduped.append(row)
    return deduped


def _is_missing(value: Any, sentinel: Any) -> bool:
    if isinstance(sentinel, float) and np.isnan(sentinel):
        try:
            return bool(np.isnan(float(value)))
        except Exception:
            return False
    return value == sentinel


def _fill_forward(rows: list[dict[str, Any]], key: str, missing: Any) -> None:
    carry: Any = None
    for row in rows:
        if not _is_missing(row[key], missing):
            carry = row[key]
        elif carry is not None:
            row[key] = carry


def _fill_backward(rows: list[dict[str, Any]], key: str, missing: Any) -> None:
    carry: Any = None
    for row in reversed(rows):
        if not _is_missing(row[key], missing):
            carry = row[key]
        elif carry is not None:
            row[key] = carry


def reindex_frames(initial_frames: list[dict[str, Any]], fixed_offset: float) -> list[dict[str, Any]]:
    if not initial_frames:
        return []
    min_offset_i = to_scaled4(initial_frames[0]["s_offset"])
    max_offset_i = to_scaled4(initial_frames[-1]["s_offset"] + initial_frames[-1]["s_duration"])
    step_i = max(1, to_scaled4(fixed_offset))

    new_index: list[int] = []
    cur = min_offset_i
    while cur < max_offset_i:
        new_index.append(cur)
        cur += step_i

    all_index = sorted(set(new_index + [to_scaled4(row["s_offset"]) for row in initial_frames]))
    by_offset: dict[int, dict[str, Any]] = {}
    for row in initial_frames:
        by_offset.setdefault(to_scaled4(row["s_offset"]), dict(row))

    rows: list[dict[str, Any]] = []
    for idx in all_index:
        if idx in by_offset:
            rows.append(dict(by_offset[idx]))
        else:
            rows.append(
                {
                    "s_offset": from_scaled4(idx),
                    "s_duration": float("nan"),
                    "s_measure": -(2**31),
                    "s_notes": None,
                    "s_intervals": None,
                    "s_is_onset": None,
                }
            )

    _fill_forward(rows, "s_notes", None)
    _fill_backward(rows, "s_notes", None)

    for row in rows:
        if row["s_is_onset"] is None:
            count = len(row["s_notes"] or [])
            row["s_is_onset"] = [False] * count

    _fill_forward(rows, "s_duration", float("nan"))
    _fill_backward(rows, "s_duration", float("nan"))
    _fill_forward(rows, "s_measure", -(2**31))
    _fill_backward(rows, "s_measure", -(2**31))
    _fill_forward(rows, "s_intervals", None)
    _fill_backward(rows, "s_intervals", None)

    new_index_set = set(new_index)
    out = [row for row in rows if to_scaled4(row["s_offset"]) in new_index_set]
    out.sort(key=lambda row: to_scaled4(row["s_offset"]))
    return out


def canonical_dump(data: Any, path: Path) -> None:
    rendered = json.dumps(data, indent=2, sort_keys=True, ensure_ascii=True)
    path.write_text(f"{rendered}\n", encoding="utf-8")


def encode_stage_b_inputs(grid_frames: list[dict[str, Any]], fixed_offset: float, max_steps: int) -> dict[str, Any]:
    steps = max(1, int(max_steps))
    active_steps = min(len(grid_frames), steps)
    x_bass19 = [[-1.0] * 19 for _ in range(steps)]
    x_chromagram19 = [[-1.0] * 19 for _ in range(steps)]
    x_measure_note_onset14 = [[-1.0] * 14 for _ in range(steps)]

    onset_pattern = []
    for x in range(64):
        bits = list(reversed(f"{x:06b}0"))
        pattern = [float(int(bit)) for bit in bits]
        onset_pattern.append(pattern)
    onset_pattern[0][0] = 1.0

    prev_measure = -2**31
    measure_idx = 0
    note_idx = 0

    for t in range(active_steps):
        frame = grid_frames[t]
        notes = list(frame.get("s_notes") or [])
        onsets = [bool(v) for v in (frame.get("s_is_onset") or [])]
        measure = int(frame.get("s_measure", 0))

        x_bass19[t] = [0.0] * 19
        x_chromagram19[t] = [0.0] * 19
        x_measure_note_onset14[t] = [0.0] * 14

        if notes:
            parsed = parse_step_and_pc(notes[0])
            if parsed is not None:
                step_idx, pc_idx = parsed
                x_bass19[t][step_idx] = 1.0
                x_bass19[t][7 + pc_idx] = 1.0

        for spelling in notes:
            parsed = parse_step_and_pc(spelling)
            if parsed is None:
                continue
            step_idx, pc_idx = parsed
            x_chromagram19[t][step_idx] = 1.0
            x_chromagram19[t][7 + pc_idx] = 1.0

        if measure != prev_measure:
            measure_idx = 0
            prev_measure = measure
        x_measure_note_onset14[t][:7] = onset_pattern[measure_idx]
        measure_idx = min(measure_idx + 1, len(onset_pattern) - 1)

        if any(onsets):
            note_idx = 0
        x_measure_note_onset14[t][7:] = onset_pattern[note_idx]
        note_idx = min(note_idx + 1, len(onset_pattern) - 1)

    return {
        "schema_version": SCHEMA_VERSION,
        "fixed_offset": fixed_offset,
        "max_steps": steps,
        "active_steps": active_steps,
        "X_Bass19": x_bass19,
        "X_Chromagram19": x_chromagram19,
        "X_MeasureNoteOnset14": x_measure_note_onset14,
    }


def run_onnx_stage_c(
    model_path: Path,
    stage_b: dict[str, Any],
    logits_round_digits: int = 6,
) -> dict[str, Any]:
    import onnxruntime as ort

    opts = ort.SessionOptions()
    opts.intra_op_num_threads = 1
    opts.inter_op_num_threads = 1
    session = ort.InferenceSession(
        str(model_path),
        sess_options=opts,
        providers=["CPUExecutionProvider"],
    )

    feed = {
        "X_Bass19": np.asarray([stage_b["X_Bass19"]], dtype=np.float32),
        "X_Chromagram19": np.asarray([stage_b["X_Chromagram19"]], dtype=np.float32),
        "X_MeasureNoteOnset14": np.asarray([stage_b["X_MeasureNoteOnset14"]], dtype=np.float32),
    }

    output_names = [out.name for out in session.get_outputs()]
    outputs = session.run(output_names, feed)
    effective_steps = max(1, min(int(stage_b["active_steps"]), int(stage_b["max_steps"])))

    heads: dict[str, Any] = {}
    for head_name, logits in zip(output_names, outputs):
        if logits.ndim != 3 or logits.shape[0] != 1:
            raise RuntimeError(f"unexpected output shape for {head_name}: {list(logits.shape)}")
        clipped = logits[0, :effective_steps, :]
        rounded_logits = np.round(clipped.astype(np.float64), logits_round_digits).tolist()
        argmax = np.argmax(clipped, axis=-1).astype(np.int64).tolist()
        heads[head_name] = {
            "shape": [effective_steps, int(clipped.shape[-1])],
            "logits": rounded_logits,
            "argmax": argmax,
        }

    return {
        "schema_version": SCHEMA_VERSION,
        "effective_steps": effective_steps,
        "heads": {k: heads[k] for k in sorted(heads)},
    }


def _decode_assets() -> dict[str, Any]:
    if not hasattr(_decode_assets, "_cache"):
        path = Path(__file__).resolve().parents[2] / "crates" / "cp_engine" / "src" / "augnet_decode_assets.json"
        _decode_assets._cache = json.loads(path.read_text(encoding="utf-8"))
    return _decode_assets._cache


def _spelling_to_pc(spelling: str) -> int:
    base = {"C": 0, "D": 2, "E": 4, "F": 5, "G": 7, "A": 9, "B": 11}[spelling[0]]
    alter = spelling.count("#") - spelling.count("-")
    return (base + alter) % 12


def _cosine_similarity(v1: np.ndarray, v2: np.ndarray) -> float:
    n1 = float(np.linalg.norm(v1))
    n2 = float(np.linalg.norm(v2))
    if n1 == 0.0 or n2 == 0.0:
        return -1.0
    return float(np.dot(v1, v2) / (n1 * n2))


def _weber_euclidean(k1: str, k2: str) -> float:
    diagonal = [
        "B--",
        "c-",
        "F-",
        "g-",
        "C-",
        "d-",
        "G-",
        "a-",
        "D-",
        "e-",
        "A-",
        "b-",
        "E-",
        "f",
        "B-",
        "c",
        "F",
        "g",
        "C",
        "d",
        "G",
        "a",
        "D",
        "e",
        "A",
        "b",
        "E",
        "f#",
        "B",
        "c#",
        "F#",
        "g#",
        "C#",
        "d#",
        "G#",
        "a#",
        "D#",
        "e#",
        "A#",
        "b#",
    ]
    i1 = diagonal.index(k1)
    i2 = diagonal.index(k2)
    flatter, sharper = sorted((i1, i2))
    best = float("inf")
    for i in range(len(diagonal) // 2):
        new_x = flatter + 2 * i
        new_y = flatter + 3 * i
        dx = sharper - new_x
        dy = sharper - new_y
        d = float((dx * dx + dy * dy) ** 0.5)
        if d < best:
            best = d
    return best


def _force_tonicization(
    local_idx: int, candidate_indices: list[int], tonicization_scale_degrees: list[list[str]], keys: list[str]
) -> int:
    local_key = keys[local_idx]
    best_idx = candidate_indices[0]
    best_distance = float("inf")
    for candidate_idx in candidate_indices:
        candidate_key = keys[candidate_idx]
        distance = _weber_euclidean(local_key, candidate_key)
        degree = tonicization_scale_degrees[local_idx][candidate_idx]
        if degree not in {"i", "III"}:
            distance *= 1.05
        if degree not in {"i", "I", "III", "iv", "IV", "v", "V"}:
            distance *= 1.05
        if distance < best_distance:
            best_idx = candidate_idx
            best_distance = distance
    return best_idx


def _confidence_for_decision(logits: list[float], chosen_idx: int) -> tuple[float, float]:
    import math

    row32 = [float(np.float32(v)) for v in logits]
    max_logit = max(row32)
    exp_values = [math.exp(v - max_logit) for v in row32]
    denom = sum(exp_values)
    chosen = exp_values[chosen_idx] / denom
    second = 0.0
    if len(exp_values) > 1:
        second = max(v for i, v in enumerate(exp_values) if i != chosen_idx) / denom
    return round6(chosen), round6(chosen - second)


def _head_label_to_string(head: str, idx: int, assets: dict[str, Any]) -> str:
    if head in {"Alto35", "Bass35", "Soprano35", "Tenor35"}:
        return str(assets["spellings"][idx])
    if head in {"LocalKey38", "TonicizedKey38"}:
        return str(assets["keys"][idx])
    if head == "RomanNumeral31":
        return str(assets["roman_numerals"][idx])
    if head == "PitchClassSet121":
        pcs = assets["pcsets"][idx]
        return "[" + ",".join(str(int(v)) for v in pcs) + "]"
    if head == "HarmonicRhythm7":
        return str(int(idx))
    raise RuntimeError(f"unknown head label mapping: {head}")


def _resolve_rn(
    *,
    bass: str,
    tenor: str,
    alto: str,
    soprano: str,
    predicted_pcset: list[int],
    local_idx: int,
    rn_idx: int,
    tonicized_idx: int,
    assets: dict[str, Any],
) -> dict[str, Any]:
    keys = assets["keys"]
    roman_numerals = assets["roman_numerals"]
    pcsets = assets["pcsets"]
    pcset_key_entries = assets["pcset_key_entries"]
    tonicization_scale_degrees = assets["tonicization_scale_degrees"]

    vec = np.zeros(12, dtype=np.float64)
    for spelling in (bass, tenor, alto, soprano):
        vec[_spelling_to_pc(spelling)] += 1.0
    for pc in predicted_pcset:
        vec[int(pc)] += 1.0
    for pc in assets["numerator_pitch_classes"][tonicized_idx][rn_idx]:
        vec[int(pc)] += 1.0

    best_pcset_idx = 0
    best_similarity = -2.0
    for idx, pcs in enumerate(pcsets):
        v2 = np.zeros(12, dtype=np.float64)
        for pc in pcs:
            v2[int(pc)] = 1.0
        sim = _cosine_similarity(vec, v2)
        if sim > best_similarity:
            best_similarity = sim
            best_pcset_idx = idx

    entries = pcset_key_entries[best_pcset_idx]
    by_key = {int(entry["key_index"]): entry for entry in entries}
    resolved_tonicized_idx = tonicized_idx
    if resolved_tonicized_idx not in by_key:
        resolved_tonicized_idx = _force_tonicization(
            local_idx=local_idx,
            candidate_indices=sorted(by_key.keys()),
            tonicization_scale_degrees=tonicization_scale_degrees,
            keys=keys,
        )
    entry = by_key[resolved_tonicized_idx]

    chord = [assets["spellings"][int(i)] for i in entry["chord_spelling_indices"]]
    quality = assets["qualities"][int(entry["quality_index"])]
    rn_figure = roman_numerals[int(entry["rn_index"])]

    inversion = chord.index(bass) if bass in chord else 0
    if len(pcsets[best_pcset_idx]) == 4:
        inv_fig = {0: "7", 1: "65", 2: "43", 3: "2"}[inversion]
    else:
        inv_fig = {0: "", 1: "6", 2: "64"}[inversion]

    if inv_fig in {"65", "43", "2"}:
        rn_figure = rn_figure.replace("7", inv_fig)
    elif inv_fig in {"6", "64"}:
        rn_figure = f"{rn_figure}{inv_fig}"

    predicted_numerator = roman_numerals[rn_idx]
    resolved_rn = "Cad64" if (predicted_numerator == "Cad" and inversion == 2) else rn_figure
    tonicization = None
    if resolved_tonicized_idx != local_idx:
        tonicization = tonicization_scale_degrees[local_idx][resolved_tonicized_idx]
        resolved_rn = f"{resolved_rn}/{tonicization}"

    chord_label_raw = f"{chord[0]}{quality}"
    if inversion != 0:
        chord_label_raw = f"{chord_label_raw}/{chord[inversion]}"
    chord_label_formatted = chord_label_raw
    if chord_label_formatted.endswith("maj"):
        chord_label_formatted = chord_label_formatted[:-3]
    chord_label_formatted = chord_label_formatted.replace("-", "b")
    formatted_rn = "I" if resolved_rn == "I/I" else resolved_rn

    return {
        "roman_numeral_resolved": resolved_rn,
        "roman_numeral_formatted": formatted_rn,
        "tonicized_key_resolved": keys[resolved_tonicized_idx],
        "tonicization": tonicization,
        "pitch_class_set_resolved": [int(v) for v in pcsets[best_pcset_idx]],
        "chord_pitch_names": chord,
        "chord_root": chord[0],
        "chord_quality": quality,
        "chord_bass": chord[inversion],
        "inversion_index": inversion,
        "inversion_figure": inv_fig,
        "chord_label_raw": chord_label_raw,
        "chord_label_formatted": chord_label_formatted,
        "is_cadential_64": bool(str(resolved_rn).startswith("Cad64")),
    }


def decode_stage_d_labels(stage_c: dict[str, Any]) -> dict[str, Any]:
    assets = _decode_assets()
    heads = stage_c["heads"]
    steps = int(stage_c["effective_steps"])
    decoded_heads: dict[str, Any] = {}
    labels: list[dict[str, Any]] = []
    required_heads = [
        "Alto35",
        "Bass35",
        "HarmonicRhythm7",
        "LocalKey38",
        "PitchClassSet121",
        "RomanNumeral31",
        "Soprano35",
        "Tenor35",
        "TonicizedKey38",
    ]

    for head in required_heads:
        if head not in heads:
            raise RuntimeError(f"missing required head in stage_c: {head}")

    for head_name in sorted(heads):
        head = heads[head_name]
        decoded_labels: list[str] = []
        conf_top1: list[float] = []
        conf_margin: list[float] = []
        for t in range(steps):
            idx = int(head["argmax"][t])
            decoded_labels.append(_head_label_to_string(head_name, idx, assets))
            top1, margin = _confidence_for_decision(head["logits"][t], idx)
            conf_top1.append(top1)
            conf_margin.append(margin)
        decoded_heads[head_name] = {
            "shape": head["shape"],
            "raw_logits": head["logits"],
            "argmax": head["argmax"],
            "decoded_labels": decoded_labels,
            "confidence_top1": conf_top1,
            "confidence_margin": conf_margin,
        }

    for t in range(steps):
        components = {head: int(decoded_heads[head]["argmax"][t]) for head in sorted(decoded_heads)}
        component_labels = {
            head: str(decoded_heads[head]["decoded_labels"][t]) for head in sorted(decoded_heads)
        }
        component_confidence = {
            head: {
                "confidence_top1": decoded_heads[head]["confidence_top1"][t],
                "confidence_margin": decoded_heads[head]["confidence_margin"][t],
            }
            for head in sorted(decoded_heads)
        }

        local_idx = components["LocalKey38"]
        tonicized_idx = components["TonicizedKey38"]
        rn_idx = components["RomanNumeral31"]
        pcs_idx = components["PitchClassSet121"]
        hr_idx = components["HarmonicRhythm7"]

        resolved = _resolve_rn(
            bass=assets["spellings"][components["Bass35"]],
            tenor=assets["spellings"][components["Tenor35"]],
            alto=assets["spellings"][components["Alto35"]],
            soprano=assets["spellings"][components["Soprano35"]],
            predicted_pcset=[int(v) for v in assets["pcsets"][pcs_idx]],
            local_idx=local_idx,
            rn_idx=rn_idx,
            tonicized_idx=tonicized_idx,
            assets=assets,
        )
        labels.append(
            {
                "time_index": t,
                "components": components,
                "component_labels": component_labels,
                "component_confidence": component_confidence,
                "local_key": assets["keys"][local_idx],
                "tonicized_key_predicted": assets["keys"][tonicized_idx],
                "tonicized_key_resolved": resolved["tonicized_key_resolved"],
                "tonicization": resolved["tonicization"],
                "roman_numeral_predicted": assets["roman_numerals"][rn_idx],
                "roman_numeral_resolved": resolved["roman_numeral_resolved"],
                "roman_numeral_formatted": resolved["roman_numeral_formatted"],
                "pitch_class_set_predicted": [int(v) for v in assets["pcsets"][pcs_idx]],
                "pitch_class_set_resolved": resolved["pitch_class_set_resolved"],
                "chord_pitch_names": resolved["chord_pitch_names"],
                "chord_root": resolved["chord_root"],
                "chord_quality": resolved["chord_quality"],
                "chord_bass": resolved["chord_bass"],
                "inversion_index": resolved["inversion_index"],
                "inversion_figure": resolved["inversion_figure"],
                "chord_label_raw": resolved["chord_label_raw"],
                "chord_label_formatted": resolved["chord_label_formatted"],
                "harmonic_rhythm": int(hr_idx),
                "is_cadential_64": resolved["is_cadential_64"],
            }
        )

    return {
        "schema_version": int(assets.get("schema_version", SCHEMA_VERSION)),
        "effective_steps": steps,
        "heads": {head: decoded_heads[head] for head in sorted(decoded_heads)},
        "labels": labels,
    }


def summary(value: Any) -> Any:
    if isinstance(value, list):
        if not value:
            return {"type": "list", "len": 0}
        return {"type": "list", "len": len(value), "first": summary(value[0])}
    if isinstance(value, dict):
        keys = sorted(value.keys())
        return {"type": "dict", "keys": keys[:8], "key_count": len(keys)}
    if isinstance(value, float):
        return round6(value)
    return value


def first_mismatch(expected: Any, actual: Any, path: str, float_tol: float = 0.0) -> tuple[str, Any, Any] | None:
    if isinstance(expected, float) or isinstance(actual, float):
        try:
            exp_f = float(expected)
            act_f = float(actual)
        except Exception:
            return (path, expected, actual)
        if abs(exp_f - act_f) <= float_tol:
            return None
        return (path, exp_f, act_f)

    if type(expected) is not type(actual):
        return (path, expected, actual)

    if isinstance(expected, dict):
        exp_keys = sorted(expected.keys())
        act_keys = sorted(actual.keys())
        if exp_keys != act_keys:
            return (f"{path}.<keys>" if path else "<keys>", exp_keys, act_keys)
        for key in exp_keys:
            next_path = f"{path}.{key}" if path else key
            mismatch = first_mismatch(expected[key], actual[key], next_path, float_tol=float_tol)
            if mismatch is not None:
                return mismatch
        return None

    if isinstance(expected, list):
        if len(expected) != len(actual):
            return (f"{path}.<len>" if path else "<len>", len(expected), len(actual))
        for idx, (left, right) in enumerate(zip(expected, actual)):
            next_path = f"{path}[{idx}]"
            mismatch = first_mismatch(left, right, next_path, float_tol=float_tol)
            if mismatch is not None:
                return mismatch
        return None

    if expected != actual:
        return (path, expected, actual)
    return None
