#!/usr/bin/env python3
"""Generate staff-level MusicXML timing fixtures using music21.

This fixture is used by web editor tests as an oracle for pickup detection
and onset timing against a known third-party parser.
"""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path

from music21 import converter, stream


def staff_num_from_part(part: stream.Part, fallback_index: int) -> int:
  pid = str(getattr(part, "id", "") or "")
  m = re.search(r"Staff(\d+)$", pid)
  if m:
    return int(m.group(1))
  return fallback_index + 1


def pickup_eighths(score: stream.Score) -> float | None:
  if not score.parts:
    return None
  first_part = score.parts[0]
  measures = list(first_part.getElementsByClass(stream.Measure))
  if not measures:
    return None
  m0 = measures[0]
  is_pickup = getattr(m0, "number", None) == 0 or "implicit" in str(m0)
  span = float(m0.highestTime or 0.0)
  if not is_pickup or span <= 0:
    return None
  return span * 2.0


def build_fixture(score_path: Path) -> dict:
  score = converter.parse(str(score_path))
  out = {
    "source_path": str(score_path.as_posix()),
    "pickup_eighths": pickup_eighths(score),
    "staffs": [],
  }

  for idx, part in enumerate(score.parts):
    staff_num = staff_num_from_part(part, idx)
    starts = set()
    for n in part.recurse().notes:
      if n.duration.isGrace:
        continue
      starts.add(round(float(n.getOffsetInHierarchy(part)) * 2.0, 6))
    out["staffs"].append(
      {
        "staff_num": staff_num,
        "sounding_start_eighths": sorted(starts),
      }
    )
  out["staffs"].sort(key=lambda x: x["staff_num"])
  return out


def main() -> None:
  parser = argparse.ArgumentParser(description="Generate music21 timing fixture for web MusicXML import tests.")
  parser.add_argument("--score", required=True, type=Path, help="Input MusicXML/MXL score path.")
  parser.add_argument("--output", required=True, type=Path, help="Output JSON fixture path.")
  args = parser.parse_args()

  fixture = build_fixture(args.score)
  args.output.parent.mkdir(parents=True, exist_ok=True)
  args.output.write_text(json.dumps(fixture, indent=2), encoding="utf-8")
  print(f"Wrote {args.output}")


if __name__ == "__main__":
  main()
