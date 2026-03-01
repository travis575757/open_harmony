#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate deterministic Phase 6 AugmentedNet decode assets."
    )
    parser.add_argument(
        "--augnet-repo",
        type=Path,
        default=Path("third_party/AugmentedNet"),
        help="Path to a local clone of https://github.com/napulen/AugmentedNet.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("crates/cp_engine/src/augnet_decode_assets.json"),
        help="Output JSON path.",
    )
    return parser.parse_args(argv)


def _git_head(repo: Path) -> str:
    try:
        proc = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=str(repo),
            check=True,
            text=True,
            capture_output=True,
        )
    except Exception:
        return "unknown"
    return proc.stdout.strip()


def _compute_numerator_pitch_classes(keys: list[str], numerators: list[str]) -> list[list[list[int]]]:
    import music21  # type: ignore

    out: list[list[list[int]]] = []
    for key in keys:
        row: list[list[int]] = []
        for rn in numerators:
            figure = rn.replace("Cad", "Cad64")
            roman = music21.roman.RomanNumeral(figure, key)
            row.append([int(pc) for pc in roman.pitchClasses])
        out.append(row)
    return out


def _compute_tonicization_scale_degrees(keys: list[str]) -> list[list[str]]:
    from AugmentedNet.keydistance import getTonicizationScaleDegree  # type: ignore

    out: list[list[str]] = []
    for local in keys:
        row: list[str] = []
        for tonicized in keys:
            row.append(str(getTonicizationScaleDegree(local, tonicized)))
        out.append(row)
    return out


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    repo = args.augnet_repo.resolve()
    if not repo.exists():
        raise FileNotFoundError(f"AugmentedNet repo not found: {repo}")

    sys.path.insert(0, str(repo))
    from AugmentedNet.chord_vocabulary import frompcset  # type: ignore
    from AugmentedNet.feature_representation import (  # type: ignore
        COMMON_ROMAN_NUMERALS,
        KEYS,
        PCSETS,
        SPELLINGS,
    )

    spellings = list(SPELLINGS)
    keys = list(KEYS)
    roman_numerals = list(COMMON_ROMAN_NUMERALS)
    pcsets = [list(pcset) for pcset in PCSETS]

    spelling_to_idx = {s: i for i, s in enumerate(spellings)}
    key_to_idx = {k: i for i, k in enumerate(keys)}
    rn_to_idx = {rn: i for i, rn in enumerate(roman_numerals)}

    qualities = sorted(
        {
            meta["quality"]
            for by_key in frompcset.values()
            for meta in by_key.values()
        }
    )
    quality_to_idx = {q: i for i, q in enumerate(qualities)}

    pcset_key_entries: list[list[dict[str, object]]] = []
    for pcset in PCSETS:
        entries: list[dict[str, object]] = []
        by_key = frompcset[pcset]
        for key in sorted(by_key):
            meta = by_key[key]
            entries.append(
                {
                    "key_index": key_to_idx[key],
                    "rn_index": rn_to_idx[meta["rn"]],
                    "quality_index": quality_to_idx[meta["quality"]],
                    "chord_spelling_indices": [
                        spelling_to_idx[s] for s in meta["chord"]
                    ],
                }
            )
        pcset_key_entries.append(entries)

    numerator_pitch_classes = _compute_numerator_pitch_classes(keys, roman_numerals)
    tonicization_scale_degrees = _compute_tonicization_scale_degrees(keys)

    asset = {
        "schema_version": 1,
        "source": {
            "repo": "https://github.com/napulen/AugmentedNet",
            "commit": _git_head(repo),
        },
        "spellings": spellings,
        "keys": keys,
        "roman_numerals": roman_numerals,
        "qualities": qualities,
        "pcsets": pcsets,
        "pcset_key_entries": pcset_key_entries,
        "numerator_pitch_classes": numerator_pitch_classes,
        "tonicization_scale_degrees": tonicization_scale_degrees,
    }

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(asset, indent=2, sort_keys=True, ensure_ascii=True) + "\n",
        encoding="utf-8",
    )
    print(args.output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
