import { normalizeDurationEighths, speciesDefaultDurationEighths } from "./abcNotation.js";
import {
  buildHarmonicRhythmConfig,
  normalizeAnalysisBackend,
} from "./analysisMode.js";

const TPQ = 480;

export const KEY_OPTIONS = [
  { label: "C", tonic_pc: 0 },
  { label: "C#/Db", tonic_pc: 1 },
  { label: "D", tonic_pc: 2 },
  { label: "D#/Eb", tonic_pc: 3 },
  { label: "E", tonic_pc: 4 },
  { label: "F", tonic_pc: 5 },
  { label: "F#/Gb", tonic_pc: 6 },
  { label: "G", tonic_pc: 7 },
  { label: "G#/Ab", tonic_pc: 8 },
  { label: "A", tonic_pc: 9 },
  { label: "A#/Bb", tonic_pc: 10 },
  { label: "B", tonic_pc: 11 },
];

function defaultVoiceName(index, count) {
  if (count === 1) return "Voice 1";
  if (count === 2) return index === 0 ? "Upper" : "Lower";
  if (count === 3) {
    return ["Upper", "Middle", "Lower"][index] ?? `Voice ${index + 1}`;
  }
  return ["Soprano", "Alto", "Tenor", "Bass"][index] ?? `Voice ${index + 1}`;
}

export function createDefaultVoices(voiceCount, presetId) {
  const duration = speciesDefaultDurationEighths(presetId);
  const defaults = [
    [72, 74, 76, 77, 79, 77, 76, 74],
    [60, 62, 64, 65, 67, 65, 64, 62],
    [55, 57, 59, 60, 62, 60, 59, 57],
    [48, 50, 52, 53, 55, 53, 52, 50],
  ];
  return Array.from({ length: voiceCount }, (_, voiceIndex) => {
    const pitches = defaults[voiceIndex] ?? defaults[defaults.length - 1];
    return {
      voice_index: voiceIndex,
      name: defaultVoiceName(voiceIndex, voiceCount),
      notes: pitches.map((midi, noteIndex) => ({
        note_id: `v${voiceIndex}_n${noteIndex}`,
        midi,
        is_rest: false,
        duration_eighths: duration,
        tie_start: false,
        tie_end: false,
      })),
    };
  });
}

export function normalizeVoiceIds(voices) {
  for (const voice of voices) {
    for (let i = 0; i < voice.notes.length; i += 1) {
      voice.notes[i].note_id = `v${voice.voice_index}_n${i}`;
    }
  }
}

function voicesToEngine(voices) {
  const augnetNoteSpellings = {};
  const engineVoices = voices.map((voice) => {
    let cursorTick = 0;
    const notes = [];
    for (const note of voice.notes) {
      const normalizedDuration = normalizeDurationEighths(note.duration_eighths, 1);
      const durationTicks = Math.max(1, Math.round(normalizedDuration * (TPQ / 2)));
      const explicitStartEighths = Number(note.start_eighths);
      const hasExplicitStart = Number.isFinite(explicitStartEighths) && explicitStartEighths >= 0;
      const startTick = hasExplicitStart
        ? Math.max(0, Math.round(explicitStartEighths * (TPQ / 2)))
        : cursorTick;
      if (note.is_rest || !Number.isFinite(note.midi)) {
        cursorTick = Math.max(cursorTick, startTick + durationTicks);
        continue;
      }
      const out = {
        note_id: note.note_id,
        voice_index: voice.voice_index,
        midi: note.midi,
        start_tick: startTick,
        duration_ticks: durationTicks,
        tie_start: !!note.tie_start,
        tie_end: !!note.tie_end,
      };
      notes.push(out);
      const spellingMidi = Number(note.spelling_midi);
      const spellingM21 = typeof note.spelling_m21 === "string" ? note.spelling_m21.trim() : "";
      if (
        spellingM21 &&
        Number.isFinite(spellingMidi) &&
        spellingMidi === note.midi
      ) {
        augnetNoteSpellings[note.note_id] = spellingM21;
      }
      cursorTick = Math.max(cursorTick, startTick + durationTicks);
    }
    notes.sort((a, b) => a.start_tick - b.start_tick || a.note_id.localeCompare(b.note_id));

    return {
      voice_index: voice.voice_index,
      name: voice.name,
      notes,
    };
  });

  return {
    voices: engineVoices,
    augnetNoteSpellings,
  };
}

export function buildAnalysisRequest(state, resolvedRuleSet) {
  const analysisBackend = normalizeAnalysisBackend(state.analysis_backend);
  const voicePayload = voicesToEngine(state.voices);
  const request = {
    score: {
      meta: {
        exercise_count: 1,
        key_signature: {
          tonic_pc: state.key_tonic_pc,
          mode: state.mode,
        },
        time_signature: {
          numerator: state.time_signature.numerator,
          denominator: state.time_signature.denominator,
        },
        ticks_per_quarter: TPQ,
      },
      voices: voicePayload.voices,
    },
    config: {
      preset_id: state.preset_id,
      enabled_rule_ids: resolvedRuleSet.enabled_rule_ids,
      disabled_rule_ids: resolvedRuleSet.disabled_rule_ids,
      severity_overrides: resolvedRuleSet.severity_overrides,
      rule_params: resolvedRuleSet.rule_params,
      rule_checker_enabled: state.rule_checker_enabled !== false,
      analysis_backend: analysisBackend,
      harmonic_rhythm: buildHarmonicRhythmConfig(
        analysisBackend,
        state.rule_harmonic_rhythm_chords_per_bar,
      ),
    },
  };
  if (Object.keys(voicePayload.augnetNoteSpellings).length > 0) {
    request.augnet_note_spellings = voicePayload.augnetNoteSpellings;
  }
  if (typeof state.source_musicxml_raw === "string" && state.source_musicxml_raw.trim()) {
    request.augnet_source_musicxml = state.source_musicxml_raw;
  }
  return request;
}

export function noteIndexMap(voices) {
  const map = new Map();
  for (const voice of voices) {
    for (let i = 0; i < voice.notes.length; i += 1) {
      map.set(voice.notes[i].note_id, {
        voice_index: voice.voice_index,
        note_index: i,
      });
    }
  }
  return map;
}

export function keyLabelByPc(tonicPc) {
  const canonicalMajor = {
    0: "C",
    1: "Db",
    2: "D",
    3: "Eb",
    4: "E",
    5: "F",
    6: "F#",
    7: "G",
    8: "Ab",
    9: "A",
    10: "Bb",
    11: "B",
  };
  const pc = ((tonicPc % 12) + 12) % 12;
  return canonicalMajor[pc] ?? "C";
}
