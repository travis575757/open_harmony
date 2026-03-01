import { normalizeDurationEighths } from "./abcNotation.js";

const MAJOR_FIFTHS_TO_PC = {
  "-7": 11,
  "-6": 6,
  "-5": 1,
  "-4": 8,
  "-3": 3,
  "-2": 10,
  "-1": 5,
  "0": 0,
  "1": 7,
  "2": 2,
  "3": 9,
  "4": 4,
  "5": 11,
  "6": 6,
  "7": 1,
};

const MINOR_FIFTHS_TO_PC = {
  "-7": 8,
  "-6": 3,
  "-5": 10,
  "-4": 5,
  "-3": 0,
  "-2": 7,
  "-1": 2,
  "0": 9,
  "1": 4,
  "2": 11,
  "3": 6,
  "4": 1,
  "5": 8,
  "6": 3,
  "7": 10,
};

const PC_TO_STEP_ALTER = [
  ["C", 0],
  ["C", 1],
  ["D", 0],
  ["D", 1],
  ["E", 0],
  ["F", 0],
  ["F", 1],
  ["G", 0],
  ["G", 1],
  ["A", 0],
  ["A", 1],
  ["B", 0],
];

function getTag(block, tagName) {
  const m = block.match(
    new RegExp(`<(?:\\w+:)?${tagName}>([\\s\\S]*?)</(?:\\w+:)?${tagName}>`, "i"),
  );
  return m ? m[1].trim() : null;
}

function getTagNumber(block, tagName, fallback = null) {
  const raw = getTag(block, tagName);
  if (raw == null) return fallback;
  const n = Number.parseInt(raw, 10);
  return Number.isFinite(n) ? n : fallback;
}

function modeFromMusicXml(rawMode) {
  if (!rawMode) return "major";
  const m = rawMode.toLowerCase();
  if (
    ["major", "minor", "dorian", "phrygian", "lydian", "mixolydian", "aeolian", "ionian"].includes(
      m,
    )
  ) {
    return m;
  }
  return "major";
}

function tonicPcFromFifths(fifths, mode) {
  if (mode === "minor" || mode === "aeolian") {
    return MINOR_FIFTHS_TO_PC[String(fifths)] ?? 0;
  }
  return MAJOR_FIFTHS_TO_PC[String(fifths)] ?? 0;
}

function pitchToMidi(step, alter, octave) {
  const base = {
    C: 0,
    D: 2,
    E: 4,
    F: 5,
    G: 7,
    A: 9,
    B: 11,
  }[step.toUpperCase()];
  if (base == null) return null;
  return (octave + 1) * 12 + base + alter;
}

function m21NameFromStepAlterOctave(step, alter, octave) {
  const acc =
    alter > 0
      ? "#".repeat(alter)
      : alter < 0
        ? "-".repeat(Math.abs(alter))
        : "";
  return `${step.toUpperCase()}${acc}${octave}`;
}

function midiToPitch(midi) {
  const pc = ((midi % 12) + 12) % 12;
  const octave = Math.floor(midi / 12) - 1;
  const [step, alter] = PC_TO_STEP_ALTER[pc];
  return { step, alter, octave };
}

function parsePartBlocks(xmlText) {
  return [...xmlText.matchAll(/<(?:\w+:)?part\b[^>]*>([\s\S]*?)<\/(?:\w+:)?part>/gi)].map(
    (m) => m[1],
  );
}

function parseMeasureBlocks(partBlock) {
  return [...partBlock.matchAll(/<(?:\w+:)?measure\b[^>]*>([\s\S]*?)<\/(?:\w+:)?measure>/gi)].map(
    (m) => m[1],
  );
}

function parseNoteBlocks(measureBlock) {
  return [...measureBlock.matchAll(/<(?:\w+:)?note\b[^>]*>([\s\S]*?)<\/(?:\w+:)?note>/gi)].map(
    (m) => m[1],
  );
}

function parseMeasureEvents(measureBlock) {
  return [...measureBlock.matchAll(/<(?:\w+:)?(note|backup|forward)\b[^>]*>([\s\S]*?)<\/(?:\w+:)?\1>/gi)].map(
    (m) => ({
      type: String(m[1]).toLowerCase(),
      block: m[2],
    }),
  );
}

const NOTE_TYPE_TO_QUARTERS = {
  breve: 8,
  whole: 4,
  half: 2,
  quarter: 1,
  eighth: 0.5,
  "16th": 0.25,
  "32nd": 0.125,
  "64th": 0.0625,
};

const NOTE_TYPE_ORDER = ["breve", "whole", "half", "quarter", "eighth", "16th", "32nd", "64th"];

function dotMultiplier(dotCount) {
  const d = Math.max(0, Math.min(2, dotCount || 0));
  return 2 - 1 / Math.pow(2, d);
}

function parseDotCount(noteBlock) {
  return [...noteBlock.matchAll(/<(?:\w+:)?dot\b[^>]*\/?>/gi)].length;
}

function durationEighthsFromType(noteType, dotCount) {
  if (!noteType) return null;
  const quarters = NOTE_TYPE_TO_QUARTERS[noteType.toLowerCase()];
  if (!quarters) return null;
  return normalizeDurationEighths(quarters * 2 * dotMultiplier(dotCount), null);
}

function durationQuartersFromType(noteType, dotCount) {
  if (!noteType) return null;
  const quarters = NOTE_TYPE_TO_QUARTERS[noteType.toLowerCase()];
  if (!quarters) return null;
  return quarters * dotMultiplier(dotCount);
}

function durationDivisionsFromType(noteType, dotCount, divisions) {
  if (!noteType) return null;
  const quarters = NOTE_TYPE_TO_QUARTERS[noteType.toLowerCase()];
  if (!quarters) return null;
  return Math.max(1, Math.round(quarters * divisions * dotMultiplier(dotCount)));
}

function noteTypeFromDurationDivisions(durationDivisions, divisions) {
  for (const type of NOTE_TYPE_ORDER) {
    const quarters = NOTE_TYPE_TO_QUARTERS[type];
    const base = quarters * divisions;
    for (let dots = 0; dots <= 2; dots += 1) {
      const expected = Math.round(base * dotMultiplier(dots));
      if (expected === durationDivisions) {
        return { type, dots };
      }
    }
  }
  return null;
}

export function importMusicXml(xmlText, opts = {}) {
  const maxVoices = opts.maxVoices ?? 4;
  const fallbackPresetId = opts.presetId ?? "species1";
  const EIGHTH_EPS = 1e-6;

  const normalized = String(xmlText || "");
  if (!normalized.includes("<score-partwise") && !normalized.includes("<score-timewise")) {
    if (normalized.startsWith("PK")) {
      throw new Error("Compressed .mxl files are not supported yet. Use uncompressed .musicxml/.xml.");
    }
    throw new Error("Invalid MusicXML: missing score-partwise/score-timewise root.");
  }

  const beats = getTagNumber(normalized, "beats", 4);
  const beatType = getTagNumber(normalized, "beat-type", 4);
  const mode = modeFromMusicXml(getTag(normalized, "mode"));
  const fifths = getTagNumber(normalized, "fifths", 0);
  const tonicPc = tonicPcFromFifths(fifths, mode);

  const partBlocks = parsePartBlocks(normalized);
  if (partBlocks.length === 0) {
    throw new Error("No <part> blocks found in MusicXML.");
  }
  const byVoiceKey = new Map();
  const fallbackMeasureQuarters = Math.max(0.25, (beats * 4) / Math.max(1, beatType));
  let firstMeasureSpanQuarters = 0;

  for (let p = 0; p < partBlocks.length; p += 1) {
    const partBlock = partBlocks[p];
    const measures = parseMeasureBlocks(partBlock);
    let defaultDivisions = 1;
    let partCursorQuarters = 0;
    const lastStartByVoice = new Map();

    for (let measureIndex = 0; measureIndex < measures.length; measureIndex += 1) {
      const measureBlock = measures[measureIndex];
      const d = getTagNumber(measureBlock, "divisions", null);
      if (d != null && d > 0) {
        defaultDivisions = d;
      }
      const events = parseMeasureEvents(measureBlock);
      let measureCursorQuarters = 0;
      let measureMaxEndQuarters = 0;
      for (const event of events) {
        if (event.type === "backup" || event.type === "forward") {
          const durationDivRaw = getTagNumber(event.block, "duration", null);
          const durationQuarters =
            (durationDivRaw != null && durationDivRaw > 0
              ? durationDivRaw / defaultDivisions
              : durationQuartersFromType(getTag(event.block, "type"), 0)) ?? 0;
          if (event.type === "backup") {
            measureCursorQuarters = Math.max(0, measureCursorQuarters - durationQuarters);
          } else {
            measureCursorQuarters += durationQuarters;
            measureMaxEndQuarters = Math.max(measureMaxEndQuarters, measureCursorQuarters);
          }
          continue;
        }

        const noteBlock = event.block;
        const isGrace = /<(?:\w+:)?grace\b/i.test(noteBlock);
        if (isGrace) {
          // Grace notes are non-metrical; skip them in editor import timeline.
          continue;
        }
        const voiceTag = getTagNumber(noteBlock, "voice", 1);
        const staffTag = getTagNumber(noteBlock, "staff", 1);
        const voiceKey = `${p}-${staffTag}-${voiceTag}`;
        const isChordTone = /<chord\s*\/>/i.test(noteBlock);
        const isRest = /<rest\b/i.test(noteBlock);
        const type = getTag(noteBlock, "type");
        const dots = parseDotCount(noteBlock);
        const durationDivRaw = getTagNumber(noteBlock, "duration", null);

        const durationQuarters =
          (durationDivRaw != null && durationDivRaw > 0
            ? durationDivRaw / defaultDivisions
            : durationQuartersFromType(type, dots)) ?? 1;
        const durationEighths =
          durationEighthsFromType(type, dots) ??
          normalizeDurationEighths(durationQuarters * 2, null) ??
          2;

        const startQuarters = isChordTone
          ? (lastStartByVoice.get(voiceKey) ?? partCursorQuarters + measureCursorQuarters)
          : partCursorQuarters + measureCursorQuarters;
        const endQuarters = startQuarters + durationQuarters;
        measureMaxEndQuarters = Math.max(measureMaxEndQuarters, endQuarters - partCursorQuarters);

        const arr = byVoiceKey.get(voiceKey) ?? [];
        if (isRest) {
          arr.push({
            start_quarters: startQuarters,
            midi: 60,
            is_rest: true,
            duration_eighths: durationEighths,
            tie_start: false,
            tie_end: false,
            voice_num: voiceTag,
            staff_num: staffTag,
          });
          byVoiceKey.set(voiceKey, arr);
        } else {
          const step = getTag(noteBlock, "step") ?? "C";
          const alter = getTagNumber(noteBlock, "alter", 0);
          const octave = getTagNumber(noteBlock, "octave", 4);
          const midi = pitchToMidi(step, alter, octave);
          if (midi != null) {
            arr.push({
              start_quarters: startQuarters,
              midi,
              is_rest: false,
              spelling_m21: m21NameFromStepAlterOctave(step, alter, octave),
              duration_eighths: durationEighths,
              tie_start: /<tie\b[^>]*type=["']start["'][^>]*\/?>/i.test(noteBlock),
              tie_end: /<tie\b[^>]*type=["']stop["'][^>]*\/?>/i.test(noteBlock),
              voice_num: voiceTag,
              staff_num: staffTag,
            });
            byVoiceKey.set(voiceKey, arr);
          }
        }

        lastStartByVoice.set(voiceKey, startQuarters);
        if (!isChordTone) {
          measureCursorQuarters += durationQuarters;
          measureMaxEndQuarters = Math.max(measureMaxEndQuarters, measureCursorQuarters);
        }
      }
      const span = measureMaxEndQuarters > 0 ? measureMaxEndQuarters : fallbackMeasureQuarters;
      if (measureIndex === 0) {
        firstMeasureSpanQuarters = Math.max(firstMeasureSpanQuarters, span);
      }
      partCursorQuarters += span;
    }
  }

  const rankedVoices = [...byVoiceKey.entries()].map(([key, notes]) => {
    const sounding = notes.filter((n) => !n.is_rest);
    const durationSum = sounding.reduce(
      (acc, n) => acc + Math.max(0, normalizeDurationEighths(n.duration_eighths, 0)),
      0,
    );
    const firstStart = notes.reduce((acc, n) => Math.min(acc, n.start_quarters), Number.POSITIVE_INFINITY);
    const [partRaw, staffRaw, voiceRaw] = key.split("-");
    return {
      key,
      notes,
      part_num: Number.parseInt(partRaw, 10) || 0,
      staff_num: Number.parseInt(staffRaw, 10) || 0,
      voice_num: Number.parseInt(voiceRaw, 10) || 0,
      sounding_count: sounding.length,
      duration_sum: durationSum,
      first_start: Number.isFinite(firstStart) ? firstStart : Number.POSITIVE_INFINITY,
    };
  });
  rankedVoices.sort(
    (a, b) =>
      b.sounding_count - a.sounding_count ||
      b.duration_sum - a.duration_sum ||
      a.first_start - b.first_start ||
      a.part_num - b.part_num ||
      a.staff_num - b.staff_num ||
      a.voice_num - b.voice_num,
  );

  const selected = rankedVoices.slice(0, Math.max(1, maxVoices));
  selected.sort(
    (a, b) =>
      a.part_num - b.part_num || a.staff_num - b.staff_num || a.voice_num - b.voice_num || a.key.localeCompare(b.key),
  );

  if (selected.length === 0) {
    throw new Error("No notes were parsed from the MusicXML file.");
  }
  const voices = selected.map((entry, index) => {
    const notes = entry.notes.sort(
      (a, b) => a.start_quarters - b.start_quarters || a.midi - b.midi || Number(a.is_rest) - Number(b.is_rest),
    );
    return {
      voice_index: index,
      name: `Voice ${index + 1}`,
      source_staff_num: entry.staff_num,
      source_voice_num: entry.voice_num,
      notes: notes.map((n, noteIndex) => ({
        note_id: `v${index}_n${noteIndex}`,
        midi: n.midi,
        is_rest: !!n.is_rest,
        start_eighths: n.start_quarters * 2,
        spelling_m21: n.spelling_m21 ?? null,
        spelling_midi: n.spelling_m21 ? n.midi : null,
        duration_eighths: n.duration_eighths,
        tie_start: n.tie_start,
        tie_end: n.tie_end,
      })),
    };
  });
  const fullMeasureEighths = (beats * 8) / Math.max(1, beatType);
  const pickupCandidate =
    firstMeasureSpanQuarters > 0
      ? normalizeDurationEighths(firstMeasureSpanQuarters * 2, null)
      : null;
  const pickupEighths =
    Number.isFinite(pickupCandidate) &&
    pickupCandidate > EIGHTH_EPS &&
    pickupCandidate < fullMeasureEighths - EIGHTH_EPS
      ? pickupCandidate
      : null;

  return {
    preset_id: fallbackPresetId,
    key_tonic_pc: tonicPc,
    mode,
    time_signature: {
      numerator: beats,
      denominator: beatType,
    },
    pickup_eighths: pickupEighths,
    voices,
  };
}

function escapeXml(s) {
  return s
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&apos;");
}

export function exportMusicXml(state) {
  const divisions = 8;
  const beats = state.time_signature.numerator;
  const beatType = state.time_signature.denominator;
  const measureUnits = Math.max(1, Math.round((beats * divisions * 4) / beatType));

  const partList = state.voices
    .map(
      (v) =>
        `<score-part id="P${v.voice_index + 1}"><part-name>${escapeXml(v.name)}</part-name></score-part>`,
    )
    .join("");

  const parts = state.voices
    .map((voice) => {
      const measures = [];
      let currentMeasure = [];
      let usedUnits = 0;
      let measureNo = 1;

      const pushMeasure = () => {
        const attrs =
          measureNo === 1
            ? `<attributes><divisions>${divisions}</divisions><key><fifths>0</fifths><mode>${escapeXml(
                state.mode,
              )}</mode></key><time><beats>${beats}</beats><beat-type>${beatType}</beat-type></time><clef><sign>G</sign><line>2</line></clef></attributes>`
            : "";
        measures.push(`<measure number="${measureNo}">${attrs}${currentMeasure.join("")}</measure>`);
        currentMeasure = [];
        usedUnits = 0;
        measureNo += 1;
      };

      for (const note of voice.notes) {
        const durationUnits = Math.max(
          1,
          Math.round((normalizeDurationEighths(note.duration_eighths, 1) * divisions) / 2),
        );
        if (usedUnits + durationUnits > measureUnits && currentMeasure.length > 0) {
          pushMeasure();
        }
        const notation = noteTypeFromDurationDivisions(durationUnits, divisions);
        const typeTag = notation
          ? `<type>${notation.type}</type>${"<dot/>".repeat(notation.dots)}`
          : "";

        if (note.is_rest || !Number.isFinite(note.midi)) {
          currentMeasure.push(
            `<note><rest/><duration>${durationUnits}</duration><voice>1</voice>${typeTag}</note>`,
          );
        } else {
          const pitch = midiToPitch(note.midi);
          const alterTag = pitch.alter !== 0 ? `<alter>${pitch.alter}</alter>` : "";
          const tieStartTag = note.tie_start ? `<tie type="start"/>` : "";
          const tieEndTag = note.tie_end ? `<tie type="stop"/>` : "";
          currentMeasure.push(
            `<note>${tieStartTag}${tieEndTag}<pitch><step>${pitch.step}</step>${alterTag}<octave>${pitch.octave}</octave></pitch><duration>${durationUnits}</duration><voice>1</voice>${typeTag}</note>`,
          );
        }
        usedUnits += durationUnits;
      }

      if (currentMeasure.length > 0 || measures.length === 0) {
        pushMeasure();
      }

      return `<part id="P${voice.voice_index + 1}">${measures.join("")}</part>`;
    })
    .join("");

  return `<?xml version="1.0" encoding="UTF-8"?>\n<score-partwise version="3.1"><part-list>${partList}</part-list>${parts}</score-partwise>`;
}
