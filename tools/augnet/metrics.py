from __future__ import annotations

from dataclasses import dataclass
from typing import Iterable, Sequence

REQUIRED_MUSICAL_METRICS = (
    "roman_numeral_exact_accuracy",
    "local_key_accuracy",
    "tonicized_key_accuracy",
    "chord_quality_accuracy",
    "inversion_accuracy",
    "harmonic_segment_boundary_f1",
)

PARITY_KEYS = (
    "roman_numeral",
    "local_key",
    "tonicized_key",
    "chord_quality",
    "inversion",
    "components_signature",
)


@dataclass
class AccuracyCounter:
    correct: int = 0
    total: int = 0

    def add(self, expected: str | None, actual: str | None) -> None:
        if expected is None:
            return
        self.total += 1
        if expected == actual:
            self.correct += 1

    @property
    def value(self) -> float:
        if self.total == 0:
            return 0.0
        return self.correct / self.total


@dataclass
class BoundaryCounter:
    tp: int = 0
    fp: int = 0
    fn: int = 0

    def add(self, truth_tokens: Sequence[str | None], pred_tokens: Sequence[str | None]) -> None:
        truth_boundaries = boundary_indices(truth_tokens)
        pred_boundaries = boundary_indices(pred_tokens)
        self.tp += len(truth_boundaries & pred_boundaries)
        self.fp += len(pred_boundaries - truth_boundaries)
        self.fn += len(truth_boundaries - pred_boundaries)

    @property
    def precision(self) -> float:
        denom = self.tp + self.fp
        if denom == 0:
            return 1.0 if self.fn == 0 else 0.0
        return self.tp / denom

    @property
    def recall(self) -> float:
        denom = self.tp + self.fn
        if denom == 0:
            return 1.0 if self.fp == 0 else 0.0
        return self.tp / denom

    @property
    def f1(self) -> float:
        p = self.precision
        r = self.recall
        if p == 0.0 and r == 0.0:
            return 0.0
        return 2.0 * p * r / (p + r)


@dataclass
class MusicalMetricCounters:
    roman_numeral_exact_accuracy: AccuracyCounter
    local_key_accuracy: AccuracyCounter
    tonicized_key_accuracy: AccuracyCounter
    chord_quality_accuracy: AccuracyCounter
    inversion_accuracy: AccuracyCounter
    harmonic_segment_boundary_f1: BoundaryCounter

    @classmethod
    def empty(cls) -> "MusicalMetricCounters":
        return cls(
            roman_numeral_exact_accuracy=AccuracyCounter(),
            local_key_accuracy=AccuracyCounter(),
            tonicized_key_accuracy=AccuracyCounter(),
            chord_quality_accuracy=AccuracyCounter(),
            inversion_accuracy=AccuracyCounter(),
            harmonic_segment_boundary_f1=BoundaryCounter(),
        )

    def add_frames(self, truth_frames: Sequence[dict], pred_frames: Sequence[dict]) -> None:
        pair_count = min(len(truth_frames), len(pred_frames))
        truth_tokens: list[str | None] = []
        pred_tokens: list[str | None] = []

        for idx in range(pair_count):
            truth = truth_frames[idx]
            pred = pred_frames[idx]
            self.roman_numeral_exact_accuracy.add(truth.get("roman_numeral"), pred.get("roman_numeral"))
            self.local_key_accuracy.add(truth.get("local_key"), pred.get("local_key"))
            self.tonicized_key_accuracy.add(truth.get("tonicized_key"), pred.get("tonicized_key"))
            self.chord_quality_accuracy.add(truth.get("chord_quality"), pred.get("chord_quality"))
            self.inversion_accuracy.add(truth.get("inversion"), pred.get("inversion"))
            truth_tokens.append(boundary_token(truth))
            pred_tokens.append(boundary_token(pred))

        self.harmonic_segment_boundary_f1.add(truth_tokens, pred_tokens)

    def to_report_dict(self) -> dict[str, dict[str, float | int]]:
        return {
            "roman_numeral_exact_accuracy": {
                "correct": self.roman_numeral_exact_accuracy.correct,
                "total": self.roman_numeral_exact_accuracy.total,
                "value": self.roman_numeral_exact_accuracy.value,
            },
            "local_key_accuracy": {
                "correct": self.local_key_accuracy.correct,
                "total": self.local_key_accuracy.total,
                "value": self.local_key_accuracy.value,
            },
            "tonicized_key_accuracy": {
                "correct": self.tonicized_key_accuracy.correct,
                "total": self.tonicized_key_accuracy.total,
                "value": self.tonicized_key_accuracy.value,
            },
            "chord_quality_accuracy": {
                "correct": self.chord_quality_accuracy.correct,
                "total": self.chord_quality_accuracy.total,
                "value": self.chord_quality_accuracy.value,
            },
            "inversion_accuracy": {
                "correct": self.inversion_accuracy.correct,
                "total": self.inversion_accuracy.total,
                "value": self.inversion_accuracy.value,
            },
            "harmonic_segment_boundary_f1": {
                "tp": self.harmonic_segment_boundary_f1.tp,
                "fp": self.harmonic_segment_boundary_f1.fp,
                "fn": self.harmonic_segment_boundary_f1.fn,
                "value": self.harmonic_segment_boundary_f1.f1,
            },
        }


def boundary_token(frame: dict) -> str | None:
    roman = frame.get("roman_numeral")
    local_key = frame.get("local_key")
    tonicized = frame.get("tonicized_key")
    if roman is None or local_key is None or tonicized is None:
        return None
    return f"{local_key}|{tonicized}|{roman}"


def boundary_indices(tokens: Sequence[str | None]) -> set[int]:
    out: set[int] = set()
    if not tokens:
        return out
    prev = tokens[0]
    for idx in range(1, len(tokens)):
        current = tokens[idx]
        if prev is not None and current is not None and current != prev:
            out.add(idx)
        prev = current
    return out


def components_signature(frame: dict) -> str:
    components = frame.get("components") or {}
    pairs = [f"{k}:{components[k]}" for k in sorted(components)]
    return "|".join(pairs)


def parity_mismatch_count(
    expected_frames: Sequence[dict],
    actual_frames: Sequence[dict],
    keys: Iterable[str] = PARITY_KEYS,
) -> tuple[int, int]:
    keys = tuple(keys)
    pair_count = min(len(expected_frames), len(actual_frames))
    total = max(len(expected_frames), len(actual_frames))
    mismatches = 0

    for idx in range(pair_count):
        left = expected_frames[idx]
        right = actual_frames[idx]
        row_mismatch = False
        for key in keys:
            lval = left.get(key)
            rval = right.get(key)
            if lval != rval:
                row_mismatch = True
                break
        if row_mismatch:
            mismatches += 1

    mismatches += total - pair_count
    return mismatches, total


def mismatch_rate(mismatches: int, total: int) -> float:
    if total <= 0:
        return 0.0
    return mismatches / total


def metric_value(report: dict[str, dict[str, float | int]], name: str) -> float:
    metric = report[name]
    value = metric.get("value")
    return float(value if value is not None else 0.0)


def metric_delta_pp(
    left_report: dict[str, dict[str, float | int]],
    right_report: dict[str, dict[str, float | int]],
    metric_name: str,
) -> float:
    return abs(metric_value(left_report, metric_name) - metric_value(right_report, metric_name)) * 100.0
