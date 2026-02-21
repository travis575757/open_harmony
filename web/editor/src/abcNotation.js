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
    return defaultDurationEighths;
  }
  if (raw.includes("/")) {
    const [left, right] = raw.split("/");
    const numer = left.length === 0 ? 1 : Number.parseInt(left, 10);
    const denom = Number.parseInt(right, 10);
    if (!Number.isFinite(numer) || !Number.isFinite(denom) || denom === 0) {
      return null;
    }
    const value = numer / denom;
    return Math.max(1, Math.round(value));
  }
  const n = Number.parseInt(raw, 10);
  if (!Number.isFinite(n)) {
    return null;
  }
  return Math.max(1, n);
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
  const defaultDurationEighths = opts.defaultDurationEighths ?? 2;
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
    const noteMatch = tokenWithoutTie.match(/^([_=^]*[A-Ga-g][,']*|[zZxX])(\d*(?:\/\d+)?)?$/);
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
      const duration = String(n.duration_eighths || defaultDurationEighths || 1);
      const tie = isRest ? "" : n.tie_start ? "-" : "";
      return `${pitch}${duration}${tie}`;
    })
    .join(" ");
}

function insertBarlines(tokens, durations, unitsPerMeasure) {
  if (unitsPerMeasure <= 0) {
    return tokens.join(" ");
  }
  let acc = 0;
  const out = [];
  for (let i = 0; i < tokens.length; i += 1) {
    const d = Math.max(1, durations[i] || 1);
    if (acc > 0 && acc + d > unitsPerMeasure) {
      out.push("|");
      acc = 0;
    }
    out.push(tokens[i]);
    acc += d;
    if (acc === unitsPerMeasure && i < tokens.length - 1) {
      out.push("|");
      acc = 0;
    }
  }
  return out.join(" ");
}

export function validateMeterFit(notes, timeSignature) {
  const issues = [];
  const unitsPerMeasure = Math.max(
    1,
    Math.round((timeSignature.numerator * 8) / Math.max(1, timeSignature.denominator)),
  );
  let acc = 0;
  for (let i = 0; i < notes.length; i += 1) {
    const d = Math.max(1, notes[i].duration_eighths || 1);
    if (d > unitsPerMeasure) {
      issues.push(
        `Token ${i + 1}: duration ${d} exceeds one measure (${unitsPerMeasure} eighths) in ${timeSignature.numerator}/${timeSignature.denominator}`,
      );
      continue;
    }
    if (acc > 0 && acc + d > unitsPerMeasure) {
      issues.push(
        `Token ${i + 1}: overfills measure in ${timeSignature.numerator}/${timeSignature.denominator}; split/tie or insert rest/bar`,
      );
      acc = 0;
    }
    acc += d;
    if (acc === unitsPerMeasure) {
      acc = 0;
    }
  }
  return issues;
}

export function buildAbcFromVoices({ voices, presetId, keyLabel, timeSignature, showBarNumbers = false }) {
  const unitsPerMeasure = Math.round((timeSignature.numerator * 8) / timeSignature.denominator);

  const lines = [
    "X:1",
    `M:${timeSignature.numerator}/${timeSignature.denominator}`,
    "L:1/8",
    `K:${keyLabel}`,
  ];
  if (showBarNumbers) {
    lines.push("%%barnumbers 1");
  }

  for (const voice of voices) {
    const sourceNotes =
      voice.notes && voice.notes.length > 0
        ? voice.notes
        : [
            {
              midi: 60,
              is_rest: true,
              duration_eighths: Math.max(1, unitsPerMeasure),
              tie_start: false,
            },
          ];
    const clef = guessClef(voice.voice_index, voices.length);
    const tokens = sourceNotes.map((note) => {
      const isRest = !!note.is_rest || !Number.isFinite(note.midi);
      const pitch = isRest ? "z" : midiToAbcTokenForKey(note.midi, keyLabel);
      const dur = String(note.duration_eighths || speciesDefaultDurationEighths(presetId) || 1);
      const tie = isRest ? "" : note.tie_start ? "-" : "";
      return `${pitch}${dur}${tie}`;
    });
    const durations = sourceNotes.map((n) => n.duration_eighths);
    lines.push(`V:${voice.voice_index + 1} clef=${clef} name="${voice.name}"`);
    lines.push(insertBarlines(tokens, durations, unitsPerMeasure));
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
