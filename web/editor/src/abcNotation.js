const BASE_PC = {
  C: 0,
  D: 2,
  E: 4,
  F: 5,
  G: 7,
  A: 9,
  B: 11,
};

const NOTE_NAMES_SHARP = ["C", "^C", "D", "^D", "E", "F", "^F", "G", "^G", "A", "^A", "B"];
const SHARP_ORDER = ["F", "C", "G", "D", "A", "E", "B"];
const FLAT_ORDER = ["B", "E", "A", "D", "G", "C", "F"];
const DURATION_EPS = 1e-6;
const RATIONAL_DENOMS = [1, 2, 3, 4, 6, 8, 12, 16, 24, 32];

export const MIN_DURATION_EIGHTHS = 0.25;
export const MAX_DURATION_EIGHTHS = 16;
export const DURATION_STEP_EIGHTHS = 0.25;

export function speciesDefaultDurationEighths(presetId) {
  switch (presetId) {
    case "species1":
      return 8;
    case "species2":
      return 4;
    case "species3":
      return 2;
    case "species4":
      return 4;
    case "species5":
      return 2;
    default:
      return 2;
  }
}

function parseDurationText(raw, defaultDurationEighths) {
  if (!raw || raw.length === 0) {
    return normalizeDurationEighths(defaultDurationEighths, 1);
  }
  if (raw.includes("/")) {
    const [left, right] = raw.split("/");
    const numer = left.length === 0 ? 1 : Number.parseInt(left, 10);
    const denom = Number.parseInt(right, 10);
    if (!Number.isFinite(numer) || !Number.isFinite(denom) || denom === 0) {
      return null;
    }
    const value = numer / denom;
    return normalizeDurationEighths(value, null);
  }
  const n = Number.parseFloat(raw);
  if (!Number.isFinite(n)) {
    return null;
  }
  return normalizeDurationEighths(n, null);
}

function gcd(a, b) {
  let x = Math.abs(a);
  let y = Math.abs(b);
  while (y !== 0) {
    const t = x % y;
    x = y;
    y = t;
  }
  return x || 1;
}

export function normalizeDurationEighths(value, fallback = 1) {
  const parsed = Number.parseFloat(value);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    return fallback;
  }
  const clamped = Math.max(MIN_DURATION_EIGHTHS, Math.min(MAX_DURATION_EIGHTHS, parsed));
  const stepped = Math.round(clamped / DURATION_STEP_EIGHTHS) * DURATION_STEP_EIGHTHS;
  return Math.max(MIN_DURATION_EIGHTHS, Math.min(MAX_DURATION_EIGHTHS, stepped));
}

function normalizeTimelineDurationEighths(value, fallback = 1) {
  const parsed = Number.parseFloat(value);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    return fallback;
  }
  const stepped = Math.round(parsed / DURATION_STEP_EIGHTHS) * DURATION_STEP_EIGHTHS;
  return Math.max(DURATION_STEP_EIGHTHS, stepped);
}

export function durationTokenFromEighths(durationEighths) {
  const d = normalizeDurationEighths(durationEighths, 1);
  for (const denom of RATIONAL_DENOMS) {
    const numer = Math.round(d * denom);
    if (Math.abs(numer / denom - d) > DURATION_EPS) continue;
    if (denom === 1) {
      return String(numer);
    }
    const reduced = gcd(numer, denom);
    const n = numer / reduced;
    const den = denom / reduced;
    if (n === 1) return `/${den}`;
    return `${n}/${den}`;
  }
  return d.toFixed(4).replace(/\.?0+$/, "");
}

function accidentalToOffset(accidentalText) {
  if (!accidentalText) {
    return 0;
  }
  let offset = 0;
  for (const ch of accidentalText) {
    if (ch === "^") {
      offset += 1;
    }
    if (ch === "_") {
      offset -= 1;
    }
  }
  return offset;
}

export function abcTokenToMidi(token) {
  const clean = token.trim();
  const match = clean.match(/^([_=^]*)([A-Ga-g])([,']*)$/);
  if (!match) {
    return null;
  }
  const accidental = match[1] || "";
  const letter = match[2];
  const marks = match[3] || "";

  let midi = BASE_PC[letter.toUpperCase()];
  midi += letter === letter.toUpperCase() ? 60 : 72;
  midi += accidentalToOffset(accidental);
  for (const ch of marks) {
    if (ch === ",") {
      midi -= 12;
    }
    if (ch === "'") {
      midi += 12;
    }
  }
  return midi;
}

export function midiToAbcToken(midi) {
  const pitchClass = ((midi % 12) + 12) % 12;
  const octave = Math.floor(midi / 12) - 1;
  let name = NOTE_NAMES_SHARP[pitchClass];

  let baseLetter = name[name.length - 1];
  const accidental = name.length > 1 ? name.slice(0, -1) : "";

  let marks = "";
  if (octave >= 5) {
    baseLetter = baseLetter.toLowerCase();
    marks = "'".repeat(Math.max(0, octave - 5));
  } else {
    baseLetter = baseLetter.toUpperCase();
    marks = ",".repeat(Math.max(0, 4 - octave));
  }
  return `${accidental}${baseLetter}${marks}`;
}

function keySignatureAccidentals(keyLabel) {
  const label = String(keyLabel || "C");
  const sharpsByKey = {
    C: 0,
    G: 1,
    D: 2,
    A: 3,
    E: 4,
    B: 5,
    "F#": 6,
    "C#": 7,
  };
  const flatsByKey = {
    F: 1,
    Bb: 2,
    Eb: 3,
    Ab: 4,
    Db: 5,
    Gb: 6,
    Cb: 7,
  };
  const acc = {};
  for (const letter of Object.keys(BASE_PC)) {
    acc[letter] = 0;
  }
  if (sharpsByKey[label] != null) {
    for (let i = 0; i < sharpsByKey[label]; i += 1) {
      acc[SHARP_ORDER[i]] = 1;
    }
  } else if (flatsByKey[label] != null) {
    for (let i = 0; i < flatsByKey[label]; i += 1) {
      acc[FLAT_ORDER[i]] = -1;
    }
  }
  return acc;
}

function formatAbcPitchForLetter(letter, octave) {
  let outLetter = letter;
  let marks = "";
  if (octave >= 5) {
    outLetter = letter.toLowerCase();
    marks = "'".repeat(Math.max(0, octave - 5));
  } else {
    outLetter = letter.toUpperCase();
    marks = ",".repeat(Math.max(0, 4 - octave));
  }
  return `${outLetter}${marks}`;
}

function midiToAbcTokenForKey(midi, keyLabel) {
  const pc = ((midi % 12) + 12) % 12;
  const octave = Math.floor(midi / 12) - 1;
  const keyAcc = keySignatureAccidentals(keyLabel);

  for (const letter of Object.keys(BASE_PC)) {
    const defaultPc = (BASE_PC[letter] + (keyAcc[letter] || 0) + 12) % 12;
    if (defaultPc === pc) {
      return formatAbcPitchForLetter(letter, octave);
    }
  }
  return midiToAbcToken(midi);
}

export function parseVoiceText(rawText, opts = {}) {
  const defaultDurationEighths = normalizeDurationEighths(opts.defaultDurationEighths ?? 2, 2);
  const tokens = rawText
    .split(/\s+/)
    .map((t) => t.trim())
    .filter((t) => t.length > 0 && t !== "|");

  const notes = [];
  const errors = [];

  for (let ix = 0; ix < tokens.length; ix += 1) {
    const raw = tokens[ix];
    const tieStart = raw.endsWith("-");
    const tokenWithoutTie = tieStart ? raw.slice(0, -1) : raw;
    const noteMatch = tokenWithoutTie.match(/^([_=^]*[A-Ga-g][,']*|[zZxX])([0-9./]*)?$/);
    if (!noteMatch) {
      errors.push(`Token ${ix + 1}: unsupported ABC token '${raw}'`);
      continue;
    }

    const pitchToken = noteMatch[1];
    const durationText = noteMatch[2] || "";
    const isRest = /^[zZxX]$/.test(pitchToken);
    const midi = isRest ? 60 : abcTokenToMidi(pitchToken);
    const durationEighths = parseDurationText(durationText, defaultDurationEighths);

    if (midi == null) {
      errors.push(`Token ${ix + 1}: invalid pitch '${raw}'`);
      continue;
    }
    if (durationEighths == null) {
      errors.push(`Token ${ix + 1}: invalid duration '${raw}'`);
      continue;
    }

    notes.push({
      midi,
      is_rest: isRest,
      duration_eighths: durationEighths,
      tie_start: isRest ? false : tieStart,
      tie_end: false,
    });
  }

  for (let i = 1; i < notes.length; i += 1) {
    if (notes[i - 1].tie_start && notes[i - 1].midi === notes[i].midi) {
      notes[i].tie_end = true;
    }
  }

  return { notes, errors };
}

export function notesToVoiceText(notes, defaultDurationEighths) {
  return notes
    .map((n) => {
      const isRest = !!n.is_rest || !Number.isFinite(n.midi);
      const pitch = isRest ? "z" : midiToAbcToken(n.midi);
      const duration = durationTokenFromEighths(n.duration_eighths || defaultDurationEighths || 1);
      const tie = isRest ? "" : n.tie_start ? "-" : "";
      return `${pitch}${duration}${tie}`;
    })
    .join(" ");
}

function insertBarlines(tokens, durations, unitsPerMeasure, initialMeasureUnits = null) {
  if (unitsPerMeasure <= 0) {
    return tokens.join(" ");
  }
  const initialUnitsValid =
    Number.isFinite(initialMeasureUnits) &&
    initialMeasureUnits > DURATION_EPS &&
    initialMeasureUnits < unitsPerMeasure - DURATION_EPS;
  let measureUnits = initialUnitsValid ? initialMeasureUnits : unitsPerMeasure;
  let acc = 0;
  const out = [];
  for (let i = 0; i < tokens.length; i += 1) {
    let remaining = normalizeTimelineDurationEighths(durations[i] || 1, 1);
    const token = tokens[i];
    const restPrefix = token.match(/^([zZxX])[0-9./]*$/)?.[1] ?? null;
    let emittedOriginalToken = false;

    while (remaining > DURATION_EPS) {
      const remainingInMeasure = Math.max(DURATION_EPS, measureUnits - acc);
      if (remaining > remainingInMeasure + DURATION_EPS) {
        if (restPrefix) {
          out.push(`${restPrefix}${durationTokenFromEighths(remainingInMeasure)}`);
          out.push("|");
          remaining -= remainingInMeasure;
          acc = 0;
          measureUnits = unitsPerMeasure;
          continue;
        }
        if (acc > DURATION_EPS) {
          out.push("|");
          acc = 0;
          measureUnits = unitsPerMeasure;
          continue;
        }
      }

      if (restPrefix) {
        out.push(`${restPrefix}${durationTokenFromEighths(remaining)}`);
      } else if (!emittedOriginalToken) {
        out.push(token);
        emittedOriginalToken = true;
      }
      acc += remaining;
      remaining = 0;
      if (Math.abs(acc - measureUnits) <= DURATION_EPS && i < tokens.length - 1) {
        out.push("|");
        acc = 0;
        measureUnits = unitsPerMeasure;
      }
    }
  }
  return out.join(" ");
}

function quantizeTimelineEighths(value) {
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) return null;
  const q = Math.round(parsed / DURATION_STEP_EIGHTHS) * DURATION_STEP_EIGHTHS;
  return Math.max(0, q);
}

function buildVoiceRenderTimeline(voice, { presetId, keyLabel, unitsPerMeasure }) {
  const fallbackDuration = speciesDefaultDurationEighths(presetId) || 1;
  const sourceNotes =
    voice.notes && voice.notes.length > 0
      ? voice.notes
      : [
          {
            midi: 60,
            is_rest: true,
            duration_eighths: normalizeDurationEighths(unitsPerMeasure, unitsPerMeasure),
            tie_start: false,
          },
        ];

  const noteStarts = [];
  let cursor = 0;
  for (const note of sourceNotes) {
    const duration = normalizeDurationEighths(note.duration_eighths || fallbackDuration, fallbackDuration);
    const explicitStart = quantizeTimelineEighths(note.start_eighths);
    const start = explicitStart != null ? explicitStart : cursor;
    noteStarts.push({ note, start, duration });
    cursor = Math.max(cursor, start + duration);
  }

  noteStarts.sort((a, b) => a.start - b.start);

  const grouped = [];
  for (const item of noteStarts) {
    const last = grouped[grouped.length - 1];
    if (last && Math.abs(last.start - item.start) <= DURATION_EPS) {
      last.items.push(item);
      last.duration = Math.max(last.duration, item.duration);
      continue;
    }
    grouped.push({
      start: item.start,
      duration: item.duration,
      items: [item],
    });
  }

  const tokens = [];
  const durations = [];
  let timelineCursor = 0;
  for (const group of grouped) {
    const start = Math.max(group.start, timelineCursor);
    const gap = start - timelineCursor;
    if (gap > DURATION_EPS) {
      tokens.push(`z${durationTokenFromEighths(gap)}`);
      durations.push(gap);
      timelineCursor = start;
    }

    const sounding = group.items
      .map(({ note }) => note)
      .filter((note) => !note.is_rest && Number.isFinite(note.midi));
    const duration = group.duration;
    const durationToken = durationTokenFromEighths(duration);

    if (sounding.length === 0) {
      tokens.push(`z${durationToken}`);
      durations.push(duration);
      timelineCursor += duration;
      continue;
    }

    const pitchesByMidi = new Map();
    for (const note of sounding) {
      if (!pitchesByMidi.has(note.midi)) {
        pitchesByMidi.set(note.midi, midiToAbcTokenForKey(note.midi, keyLabel));
      }
    }
    const ordered = [...pitchesByMidi.entries()].sort((a, b) => a[0] - b[0]);
    const pitchToken =
      ordered.length === 1 ? ordered[0][1] : `[${ordered.map(([, token]) => token).join("")}]`;
    const tie = sounding.every((note) => note.tie_start) ? "-" : "";
    tokens.push(`${pitchToken}${durationToken}${tie}`);
    durations.push(duration);
    timelineCursor += duration;
  }

  if (tokens.length === 0) {
    tokens.push(`z${durationTokenFromEighths(unitsPerMeasure)}`);
    durations.push(unitsPerMeasure);
  }

  return { tokens, durations };
}

export function validateMeterFit(notes, timeSignature) {
  const issues = [];
  const unitsPerMeasure = Math.max(1, (timeSignature.numerator * 8) / Math.max(1, timeSignature.denominator));
  let acc = 0;
  for (let i = 0; i < notes.length; i += 1) {
    const d = normalizeDurationEighths(notes[i].duration_eighths || 1, 1);
    if (d > unitsPerMeasure + DURATION_EPS) {
      issues.push(
        `Token ${i + 1}: duration ${durationTokenFromEighths(d)} exceeds one measure (${durationTokenFromEighths(unitsPerMeasure)} eighths) in ${timeSignature.numerator}/${timeSignature.denominator}`,
      );
      continue;
    }
    if (acc > DURATION_EPS && acc + d > unitsPerMeasure + DURATION_EPS) {
      issues.push(
        `Token ${i + 1}: overfills measure in ${timeSignature.numerator}/${timeSignature.denominator}; split/tie or insert rest/bar`,
      );
      acc = 0;
    }
    acc += d;
    if (Math.abs(acc - unitsPerMeasure) <= DURATION_EPS) {
      acc = 0;
    }
  }
  return issues;
}

export function buildAbcFromVoices({
  voices,
  presetId,
  keyLabel,
  timeSignature,
  showBarNumbers = false,
  pickupEighths = null,
}) {
  const unitsPerMeasure = Math.max(1, (timeSignature.numerator * 8) / timeSignature.denominator);
  const orderedVoices = [...voices].sort((a, b) => a.voice_index - b.voice_index);
  const voiceIds = orderedVoices.map((voice) => voice.voice_index + 1);

  const lines = [
    "X:1",
    `M:${timeSignature.numerator}/${timeSignature.denominator}`,
    "L:1/8",
    `K:${keyLabel}`,
  ];
  if (showBarNumbers) {
    lines.push("%%barnumbers 1");
  }
  if (voiceIds.length > 1) {
    lines.push(`%%score ${voiceIds.map((id) => `(${id})`).join(" ")}`);
  }

  for (const voice of orderedVoices) {
    const clef = guessClef(voice.voice_index, voices.length);
    const { tokens, durations } = buildVoiceRenderTimeline(voice, { presetId, keyLabel, unitsPerMeasure });
    lines.push(`V:${voice.voice_index + 1} clef=${clef} name="${voice.name}"`);
    lines.push(insertBarlines(tokens, durations, unitsPerMeasure, pickupEighths));
  }

  return lines.join("\n");
}

function guessClef(voiceIndex, voiceCount) {
  if (voiceCount === 1) {
    return "treble";
  }
  if (voiceCount === 2) {
    return voiceIndex === 0 ? "treble" : "bass";
  }
  if (voiceCount === 3) {
    if (voiceIndex === 0) return "treble";
    if (voiceIndex === 1) return "alto";
    return "bass";
  }
  if (voiceIndex === 0) return "treble";
  if (voiceIndex === 1) return "alto";
  if (voiceIndex === 2) return "tenor";
  return "bass";
}

export function clampMidiForEducation(midi) {
  return Math.min(96, Math.max(36, midi));
}
