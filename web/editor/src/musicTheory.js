export const SUPPORTED_TIME_SIGNATURES = [
  { numerator: 2, denominator: 4 },
  { numerator: 3, denominator: 4 },
  { numerator: 4, denominator: 4 },
  { numerator: 2, denominator: 2 },
  { numerator: 5, denominator: 4 },
  { numerator: 6, denominator: 4 },
  { numerator: 3, denominator: 2 },
];

function modeIntervals(mode) {
  switch ((mode || "major").toLowerCase()) {
    case "minor":
    case "aeolian":
      return [0, 2, 3, 5, 7, 8, 10];
    case "dorian":
      return [0, 2, 3, 5, 7, 9, 10];
    case "phrygian":
      return [0, 1, 3, 5, 7, 8, 10];
    case "lydian":
      return [0, 2, 4, 6, 7, 9, 11];
    case "mixolydian":
      return [0, 2, 4, 5, 7, 9, 10];
    case "ionian":
    case "major":
    default:
      return [0, 2, 4, 5, 7, 9, 11];
  }
}

export function scalePitchClasses(tonicPc, mode) {
  const tonic = ((tonicPc % 12) + 12) % 12;
  return modeIntervals(mode).map((i) => (tonic + i) % 12);
}

export function keySignaturePcForMode(tonicPc, mode) {
  const tonic = ((tonicPc % 12) + 12) % 12;
  switch ((mode || "major").toLowerCase()) {
    case "minor":
    case "aeolian":
      return (tonic + 3) % 12;
    case "dorian":
      return (tonic + 10) % 12;
    case "phrygian":
      return (tonic + 8) % 12;
    case "lydian":
      return (tonic + 7) % 12;
    case "mixolydian":
      return (tonic + 5) % 12;
    case "ionian":
    case "major":
    default:
      return tonic;
  }
}

export function isSupportedTimeSignature(numerator, denominator, presetId = "") {
  const base = SUPPORTED_TIME_SIGNATURES.some(
    (ts) => ts.numerator === numerator && ts.denominator === denominator,
  );
  if (!base) return false;
  if (presetId === "species5") {
    return (
      (numerator === 4 && denominator === 4) ||
      (numerator === 2 && denominator === 2)
    );
  }
  return true;
}

export function supportedTimeSignaturesForPreset(presetId = "") {
  return SUPPORTED_TIME_SIGNATURES.filter((ts) =>
    isSupportedTimeSignature(ts.numerator, ts.denominator, presetId),
  );
}

function pitchDistance(a, b) {
  return Math.abs(a - b);
}

export function quantizeMidiToScale(midi, tonicPc, mode, direction = 0) {
  const pcs = scalePitchClasses(tonicPc, mode);
  let best = midi;
  let bestDist = Number.POSITIVE_INFINITY;

  for (let candidate = midi - 2; candidate <= midi + 2; candidate += 1) {
    const pc = ((candidate % 12) + 12) % 12;
    if (!pcs.includes(pc)) continue;
    const dist = pitchDistance(candidate, midi);
    if (dist < bestDist) {
      bestDist = dist;
      best = candidate;
      continue;
    }
    if (dist === bestDist && direction !== 0) {
      const dBest = best - midi;
      const dNew = candidate - midi;
      if (direction > 0 && dNew > dBest) {
        best = candidate;
      }
      if (direction < 0 && dNew < dBest) {
        best = candidate;
      }
    }
  }

  return best;
}
