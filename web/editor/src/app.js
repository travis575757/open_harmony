import {
  buildAbcFromVoices,
  clampMidiForEducation,
  durationTokenFromEighths,
  DURATION_STEP_EIGHTHS,
  MAX_DURATION_EIGHTHS,
  MIN_DURATION_EIGHTHS,
  normalizeDurationEighths,
  notesToVoiceText,
  parseVoiceText,
  speciesDefaultDurationEighths,
  validateMeterFit,
} from "./abcNotation.js";
import {
  clearOverlay,
  computeNoteCenters,
  drawDiagnosticsOverlay,
  drawRomanOverlay,
  drawSelectedOverlay,
} from "./diagnosticsOverlay.js";
import { FUX_CANTUS, getCantusById } from "./fuxCantus.js";
import { exportMusicXml } from "./musicxml.js";
import {
  keySignaturePcForMode,
  quantizeMidiToScale,
  supportedTimeSignaturesForPreset,
} from "./musicTheory.js";
import { BUILTIN_PRESET_IDS, buildRuleCatalog, getBasePresetIds, presetLabel } from "./presets.js";
import { resolveUiRuleState, toggleRuleOverride } from "./ruleConfig.js";
import { KEY_OPTIONS, buildAnalysisRequest, createDefaultVoices, keyLabelByPc, normalizeVoiceIds } from "./scoreModel.js";
import { loadCustomProfiles, loadEditorSettings, saveCustomProfiles, saveEditorSettings } from "./storage.js";
import { analyzeRequest, importMusicXmlWithWasm, initAnalyzer } from "./wasmClient.js";
import {
  DEFAULT_ANALYSIS_BACKEND,
  DEFAULT_RULE_BASED_CHORDS_PER_BAR,
  analysisErrorMessage,
  analysisModeUiState,
  buildAnalysisFailureUiModel,
  normalizeAnalysisBackend,
  normalizeRuleBasedChordsPerBar,
  persistableAnalysisSettings,
  readPersistedAnalysisSettings,
} from "./analysisMode.js";
import { buildHarmonyListMarkup, buildModeAwareHarmonyRows } from "./harmonyView.js";

const ui = {
  presetSelect: document.getElementById("preset-select"),
  customBaseSelect: document.getElementById("custom-base-select"),
  voiceCount: document.getElementById("voice-count"),
  keyTonic: document.getElementById("key-tonic"),
  modeSelect: document.getElementById("mode-select"),
  analysisMethodSelect: document.getElementById("analysis-method-select"),
  harmonicRhythmRuleControls: document.getElementById("harmonic-rhythm-rule-controls"),
  harmonicRhythmChordsPerBar: document.getElementById("harmonic-rhythm-chords-per-bar"),
  harmonicRhythmAutoNote: document.getElementById("harmonic-rhythm-auto-note"),
  augnetDebugToggle: document.getElementById("augnet-debug-toggle"),
  ruleCheckerToggle: document.getElementById("rule-checker-toggle"),
  timeSignatureSelect: document.getElementById("time-signature-select"),
  addMeasure: document.getElementById("add-measure"),
  removeMeasure: document.getElementById("remove-measure"),
  zoomScale: document.getElementById("zoom-scale"),
  zoomLabel: document.getElementById("zoom-label"),
  barNumberToggle: document.getElementById("bar-number-toggle"),
  romanToggle: document.getElementById("roman-toggle"),
  harmonicSlicesToggle: document.getElementById("harmonic-slices-toggle"),
  cantusSelect: document.getElementById("cantus-select"),
  cantusTargetVoice: document.getElementById("cantus-target-voice"),
  cantusLockToggle: document.getElementById("cantus-lock-toggle"),
  cantusLockStatus: document.getElementById("cantus-lock-status"),
  applyCantus: document.getElementById("apply-cantus"),
  customCantus: document.getElementById("custom-cantus"),
  applyCustomCantus: document.getElementById("apply-custom-cantus"),
  ruleFilter: document.getElementById("rule-filter"),
  ruleSummary: document.getElementById("rule-summary"),
  ruleList: document.getElementById("rule-list"),
  customProfileName: document.getElementById("custom-profile-name"),
  saveProfile: document.getElementById("save-profile"),
  savedProfiles: document.getElementById("saved-profiles"),
  loadProfile: document.getElementById("load-profile"),
  deleteProfile: document.getElementById("delete-profile"),
  musicXmlFile: document.getElementById("musicxml-file"),
  importMusicXml: document.getElementById("import-musicxml"),
  exportMusicXml: document.getElementById("export-musicxml"),
  engineStatus: document.getElementById("engine-status"),
  warningList: document.getElementById("warning-list"),
  paper: document.getElementById("paper"),
  abcAudio: document.getElementById("abc-audio"),
  insertNoteButtons: document.getElementById("insert-note-buttons"),
  insertRestButtons: document.getElementById("insert-rest-buttons"),
  insertNoteBefore: document.getElementById("insert-note-before"),
  insertNoteAfter: document.getElementById("insert-note-after"),
  replaceSelectedNote: document.getElementById("replace-selected-note"),
  toggleSelectedDot: document.getElementById("toggle-selected-dot"),
  toggleSelectedTie: document.getElementById("toggle-selected-tie"),
  deleteSelectedNote: document.getElementById("delete-selected-note"),
  voiceEditors: document.getElementById("voice-editors"),
  parseErrors: document.getElementById("parse-errors"),
  diagnosticsSummary: document.getElementById("diagnostics-summary"),
  diagnosticsList: document.getElementById("diagnostics-list"),
  harmonyCard: document.getElementById("harmony-card"),
  harmonyList: document.getElementById("harmony-list"),
  harmonyDump: document.getElementById("harmony-dump"),
  copyHarmonyDump: document.getElementById("copy-harmony-dump"),
};

let presetSchema = null;
let ruleCatalog = [];
let rerenderTimer = null;
let lastResponse = null;
let dragState = null;
let dragListenersBound = false;
let dragRafPending = false;
let dragLatestClientY = 0;
let lastNoteCenters = new Map();
let lastDiagnosticHitPoints = [];
let synthController = null;
let lastAudioVisualObj = null;
let audioUpdateInFlight = false;
let audioUpdateQueued = false;
let lastHarmonyRows = [];
let insertGlyphCounter = 0;
const EIGHTH_TICKS = 240;
const DURATION_EPS = 1e-6;
const ROMAN_DUPLICATE_X_EPS = 24;
const INSERT_DURATION_OPTIONS = [
  { value: 0.25, label: "32nd" },
  { value: 0.5, label: "16th" },
  { value: 1, label: "8th" },
  { value: 2, label: "Quarter" },
  { value: 4, label: "Half" },
  { value: 8, label: "Whole" },
  { value: 16, label: "Double Whole" },
];
const REST_INSERT_DURATION_OPTIONS = INSERT_DURATION_OPTIONS.filter((option) => option.value < 16);

const state = {
  preset_id: "species1",
  custom_base_preset_id: "species1",
  voice_count: 2,
  key_tonic_pc: 0,
  mode: "major",
  analysis_backend: DEFAULT_ANALYSIS_BACKEND,
  rule_harmonic_rhythm_chords_per_bar: DEFAULT_RULE_BASED_CHORDS_PER_BAR,
  show_augnet_debug: false,
  rule_checker_enabled: true,
  time_signature: { numerator: 4, denominator: 4 },
  pickup_eighths: null,
  score_zoom: 1,
  show_bar_numbers: false,
  show_roman: false,
  show_harmonic_slices: true,
  voices: createDefaultVoices(2, "species1"),
  voice_raw_texts: [],
  rule_overrides: {
    enabled_rule_ids: [],
    disabled_rule_ids: [],
    severity_overrides: {},
    rule_params: {},
  },
  rule_filter: "",
  selected_note: null,
  clipboard_note: null,
  cantus_lock_enabled: true,
  cantus_voice_index: null,
  parse_errors: [],
  selected_diagnostic_key: null,
  custom_profiles: loadCustomProfiles(),
  source_musicxml_raw: null,
  imported_timeline_locked: false,
  insert_template: {
    is_rest: false,
    duration_eighths: 2,
  },
};

function timeSignatureOptionValue(ts) {
  return `${ts.numerator}/${ts.denominator}`;
}

function parseTimeSignatureOption(raw) {
  const [a, b] = String(raw || "").split("/");
  const numerator = Number.parseInt(a, 10);
  const denominator = Number.parseInt(b, 10);
  if (!Number.isInteger(numerator) || !Number.isInteger(denominator) || numerator <= 0 || denominator <= 0) {
    return null;
  }
  return { numerator, denominator };
}

function getSupportedTimeSignatures() {
  const supported = supportedTimeSignaturesForPreset(state.preset_id);
  const hasCurrent = supported.some(
    (ts) =>
      ts.numerator === state.time_signature.numerator &&
      ts.denominator === state.time_signature.denominator,
  );
  if (state.imported_timeline_locked && !hasCurrent) {
    return [...supported, { ...state.time_signature }];
  }
  return supported;
}

function isRuleCheckerEnabled() {
  return state.rule_checker_enabled !== false;
}

function ensureTimeSignatureSupported() {
  if (state.imported_timeline_locked) return;
  const supported = getSupportedTimeSignatures();
  const isCurrentSupported = supported.some(
    (ts) =>
      ts.numerator === state.time_signature.numerator &&
      ts.denominator === state.time_signature.denominator,
  );
  if (isCurrentSupported) return;
  const fallback = supported[0] ?? { numerator: 4, denominator: 4 };
  state.time_signature = { ...fallback };
}

function selectedNoteRef() {
  if (!state.selected_note) return null;
  const { voiceIndex, noteIndex } = state.selected_note;
  const voice = state.voices[voiceIndex];
  if (!voice) return null;
  const note = voice.notes[noteIndex];
  if (!note) return null;
  return { voice, note, voiceIndex, noteIndex };
}

function nearlyEqual(a, b, eps = DURATION_EPS) {
  return Math.abs(a - b) <= eps;
}

function clearVoiceTieEnds(voice) {
  for (const note of voice.notes) {
    note.tie_end = false;
  }
}

function recomputeTieEnds(voice) {
  clearVoiceTieEnds(voice);
  for (let i = 1; i < voice.notes.length; i += 1) {
    const prev = voice.notes[i - 1];
    const cur = voice.notes[i];
    if (
      prev.tie_start &&
      !prev.is_rest &&
      !cur.is_rest &&
      Number.isFinite(prev.midi) &&
      Number.isFinite(cur.midi) &&
      prev.midi === cur.midi
    ) {
      cur.tie_end = true;
    }
  }
}

function isVoiceLocked(voiceIndex) {
  return (
    state.cantus_lock_enabled &&
    Number.isInteger(state.cantus_voice_index) &&
    state.cantus_voice_index === voiceIndex
  );
}

function selectedNoteId() {
  const ref = selectedNoteRef();
  return ref ? ref.note.note_id : null;
}

function selectNote(voiceIndex, noteIndex) {
  const voice = state.voices[voiceIndex];
  if (!voice || !voice.notes[noteIndex]) {
    state.selected_note = null;
    return;
  }
  state.selected_note = { voiceIndex, noteIndex };
}

function ensureSelectedNoteValidity() {
  const ref = selectedNoteRef();
  if (!ref) {
    state.selected_note = null;
  }
}

function invalidateSourceMusicXml() {
  state.source_musicxml_raw = null;
  state.imported_timeline_locked = false;
}

function refreshVoiceTextForVoice(voiceIndex) {
  if (!state.voices[voiceIndex]) return;
  const defaultDuration = speciesDefaultDurationEighths(state.preset_id);
  state.voice_raw_texts[voiceIndex] = notesToVoiceText(state.voices[voiceIndex].notes, defaultDuration);
}

function cloneNoteForClipboard(note) {
  return {
    midi: Number.isFinite(note.midi) ? note.midi : 60,
    is_rest: !!note.is_rest,
    duration_eighths: normalizeDurationEighths(note.duration_eighths, 1),
    tie_start: false,
    tie_end: false,
  };
}

function createEditableNote({ isRest, is_rest, durationEighths, duration_eighths, midi }) {
  return {
    note_id: "",
    midi: Number.isFinite(midi) ? midi : 60,
    is_rest: !!(isRest ?? is_rest),
    duration_eighths: normalizeDurationEighths(durationEighths ?? duration_eighths, 1),
    tie_start: false,
    tie_end: false,
  };
}

function selectedOrDefaultMidi() {
  const ref = selectedNoteRef();
  const base =
    ref && Number.isFinite(ref.note.midi) && !ref.note.is_rest
      ? ref.note.midi
      : clampMidiForEducation(60);
  return clampMidiForEducation(quantizeMidiToScale(base, state.key_tonic_pc, state.mode, 1));
}

function splitVoiceNotesToMeasures(voice) {
  const indexMap = new Map();
  if (!voice || !Array.isArray(voice.notes) || voice.notes.length === 0) {
    return indexMap;
  }
  const measure = measureUnitsEighths();
  if (!Number.isFinite(measure) || measure <= DURATION_EPS) {
    return indexMap;
  }

  const rebuilt = [];
  let cursor = 0;

  for (let oldIndex = 0; oldIndex < voice.notes.length; oldIndex += 1) {
    const note = voice.notes[oldIndex];
    indexMap.set(oldIndex, rebuilt.length);

    let remaining = normalizeDurationEighths(note.duration_eighths, 1);
    if (!Number.isFinite(remaining) || remaining <= DURATION_EPS) {
      continue;
    }

    let remainingInMeasure = measure - (cursor % measure);
    if (remainingInMeasure <= DURATION_EPS) {
      remainingInMeasure = measure;
    }

    const segments = [];
    while (remaining > DURATION_EPS) {
      const piece = Math.max(
        DURATION_STEP_EIGHTHS,
        normalizeDurationEighths(Math.min(remaining, remainingInMeasure), DURATION_STEP_EIGHTHS),
      );
      segments.push(piece);
      remaining -= piece;
      cursor += piece;
      remainingInMeasure = measure;
    }

    for (let i = 0; i < segments.length; i += 1) {
      const isLast = i === segments.length - 1;
      const canTie = !note.is_rest && Number.isFinite(note.midi);
      rebuilt.push({
        note_id: "",
        midi: Number.isFinite(note.midi) ? note.midi : 60,
        is_rest: !!note.is_rest,
        duration_eighths: segments[i],
        tie_start: canTie ? (!isLast || !!note.tie_start) : false,
        tie_end: false,
      });
    }
  }

  if (rebuilt.length > 0) {
    voice.notes = rebuilt;
  }
  return indexMap;
}

function commitVoiceMutation(voiceIndex, newSelectedNoteIndex = null) {
  invalidateSourceMusicXml();
  const voice = state.voices[voiceIndex];
  let selectedIndex = Number.isInteger(newSelectedNoteIndex) ? newSelectedNoteIndex : null;
  if (voice) {
    const indexMap = splitVoiceNotesToMeasures(voice);
    if (selectedIndex != null) {
      selectedIndex = indexMap.get(selectedIndex) ?? Math.min(selectedIndex, Math.max(voice.notes.length - 1, 0));
    }
    recomputeTieEnds(voice);
  }
  normalizeVoiceIds(state.voices);
  refreshVoiceTextForVoice(voiceIndex);
  if (Number.isInteger(selectedIndex)) {
    selectNote(voiceIndex, selectedIndex);
  } else {
    ensureSelectedNoteValidity();
  }
  runAnalysisAndRender();
}

function insertAtSelection(position) {
  const ref = selectedNoteRef();
  if (!ref) return;
  if (isVoiceLocked(ref.voiceIndex)) return;
  const template = state.insert_template ?? { is_rest: false, duration_eighths: 2 };
  const newNote = createEditableNote({
    isRest: !!template.is_rest,
    durationEighths: normalizeDurationEighths(template.duration_eighths, 2),
    midi: template.is_rest ? 60 : selectedOrDefaultMidi(),
  });
  const insertIndex = position === "before" ? ref.noteIndex : ref.noteIndex + 1;
  ref.voice.notes.splice(insertIndex, 0, newNote);
  commitVoiceMutation(ref.voiceIndex, insertIndex);
}

function replaceSelected() {
  const ref = selectedNoteRef();
  if (!ref) return;
  if (isVoiceLocked(ref.voiceIndex)) return;

  const template = state.insert_template ?? { is_rest: false, duration_eighths: 2 };
  const replacement = createEditableNote({
    isRest: !!template.is_rest,
    durationEighths: normalizeDurationEighths(template.duration_eighths, 2),
    midi:
      template.is_rest
        ? 60
        : Number.isFinite(ref.note.midi) && !ref.note.is_rest
          ? ref.note.midi
          : selectedOrDefaultMidi(),
  });
  ref.voice.notes[ref.noteIndex] = replacement;
  commitVoiceMutation(ref.voiceIndex, ref.noteIndex);
}

function toggleSelectedDot() {
  const ref = selectedNoteRef();
  if (!ref) return;
  if (isVoiceLocked(ref.voiceIndex)) return;

  const current = normalizeDurationEighths(ref.note.duration_eighths, 1);
  const dottedBase = current / 1.5;
  if (dottedBase >= MIN_DURATION_EIGHTHS - DURATION_EPS && dottedBase <= MAX_DURATION_EIGHTHS + DURATION_EPS) {
    const normalizedBase = normalizeDurationEighths(dottedBase, null);
    if (normalizedBase != null && nearlyEqual(normalizedBase * 1.5, current)) {
      ref.note.duration_eighths = normalizedBase;
      commitVoiceMutation(ref.voiceIndex, ref.noteIndex);
      return;
    }
  }

  const dotted = normalizeDurationEighths(current * 1.5, current);
  ref.note.duration_eighths = dotted;
  commitVoiceMutation(ref.voiceIndex, ref.noteIndex);
}

function toggleSelectedTieStart() {
  const ref = selectedNoteRef();
  if (!ref) return;
  if (isVoiceLocked(ref.voiceIndex)) return;
  if (ref.note.is_rest || !Number.isFinite(ref.note.midi)) return;
  ref.note.tie_start = !ref.note.tie_start;
  commitVoiceMutation(ref.voiceIndex, ref.noteIndex);
}

function deleteSelectedNote() {
  const ref = selectedNoteRef();
  if (!ref) return;
  if (isVoiceLocked(ref.voiceIndex)) return;
  ref.voice.notes.splice(ref.noteIndex, 1);
  if (ref.voice.notes.length === 0) {
    ref.voice.notes.push(
      createEditableNote({
        isRest: true,
        durationEighths: measureUnitsEighths(),
        midi: 60,
      }),
    );
  }
  const nextIndex = Math.max(0, Math.min(ref.noteIndex, ref.voice.notes.length - 1));
  commitVoiceMutation(ref.voiceIndex, nextIndex);
}

function copySelectedNote() {
  const ref = selectedNoteRef();
  if (!ref) return false;
  state.clipboard_note = cloneNoteForClipboard(ref.note);
  return true;
}

function pasteAfterSelected() {
  const ref = selectedNoteRef();
  if (!ref || !state.clipboard_note) return;
  if (isVoiceLocked(ref.voiceIndex)) return;
  const paste = createEditableNote(state.clipboard_note);
  const insertIndex = ref.noteIndex + 1;
  ref.voice.notes.splice(insertIndex, 0, paste);
  commitVoiceMutation(ref.voiceIndex, insertIndex);
}

function measureUnitsEighths() {
  return Math.max(1, (state.time_signature.numerator * 8) / Math.max(1, state.time_signature.denominator));
}

function voiceDurationEighths(voice) {
  return voice.notes.reduce((acc, n) => acc + Math.max(0, normalizeDurationEighths(n.duration_eighths, 0) || 0), 0);
}

function appendRestToVoice(voice, durationEighths) {
  const d = Math.max(0, normalizeDurationEighths(durationEighths, 0));
  if (d <= 0) return;
  const last = voice.notes[voice.notes.length - 1];
  if (last && last.is_rest && !last.tie_start && !last.tie_end) {
    last.duration_eighths = normalizeDurationEighths(last.duration_eighths + d, last.duration_eighths);
    return;
  }
  voice.notes.push({
    note_id: "",
    midi: 60,
    is_rest: true,
    duration_eighths: d,
    tie_start: false,
    tie_end: false,
  });
}

function removeDurationFromEnd(voice, durationEighths) {
  let remaining = Math.max(0, normalizeDurationEighths(durationEighths, 0));
  while (remaining > 0 && voice.notes.length > 0) {
    const last = voice.notes[voice.notes.length - 1];
    const lastDuration = normalizeDurationEighths(last.duration_eighths, 1);
    const take = Math.min(remaining, lastDuration);
    if (lastDuration <= take + DURATION_EPS) {
      remaining -= lastDuration;
      voice.notes.pop();
    } else {
      last.duration_eighths = normalizeDurationEighths(lastDuration - take, lastDuration - take);
      remaining -= take;
    }
  }
}

function trimTrailingRestsToDuration(voice, targetDurationEighths) {
  let current = voiceDurationEighths(voice);
  while (current > targetDurationEighths && voice.notes.length > 0) {
    const last = voice.notes[voice.notes.length - 1];
    if (!last.is_rest) {
      break;
    }
    const lastDuration = normalizeDurationEighths(last.duration_eighths, 1);
    const remove = Math.min(lastDuration, current - targetDurationEighths);
    last.duration_eighths = normalizeDurationEighths(lastDuration - remove, lastDuration - remove);
    current -= remove;
    if (last.duration_eighths <= DURATION_EPS) {
      voice.notes.pop();
    }
  }
  return current;
}

function refreshVoiceTextsFromState() {
  const defaultDuration = speciesDefaultDurationEighths(state.preset_id);
  normalizeVoiceIds(state.voices);
  state.voice_raw_texts = state.voices.map((voice) => notesToVoiceText(voice.notes, defaultDuration));
}

function alignVoicesAfterReferenceChange(referenceDurationEighths) {
  let finalDuration = Math.max(0, normalizeDurationEighths(referenceDurationEighths, 0));
  for (const voice of state.voices) {
    const afterTrim = trimTrailingRestsToDuration(voice, finalDuration);
    if (afterTrim > finalDuration) {
      finalDuration = afterTrim;
    }
  }
  for (const voice of state.voices) {
    const d = voiceDurationEighths(voice);
    if (d < finalDuration) {
      appendRestToVoice(voice, finalDuration - d);
    }
  }
  refreshVoiceTextsFromState();
}

function displayVoicesWithPadding() {
  const clones = state.voices.map((voice) => ({
    ...voice,
    notes: voice.notes.map((n) => ({ ...n })),
  }));
  let maxDuration = clones.reduce((acc, v) => Math.max(acc, voiceDurationEighths(v)), 0);
  if (maxDuration <= 0) {
    maxDuration = measureUnitsEighths();
  }
  for (const voice of clones) {
    const d = voiceDurationEighths(voice);
    if (d < maxDuration) {
      appendRestToVoice(voice, maxDuration - d);
    }
  }
  return clones;
}

function buildTimedSoundingEvents() {
  const events = [];
  for (const voice of state.voices) {
    let cursor = 0;
    for (const note of voice.notes) {
      const durationTicks = Math.max(
        1,
        Math.round(normalizeDurationEighths(note.duration_eighths, 1) * EIGHTH_TICKS),
      );
      const explicitStartEighths = Number(note.start_eighths);
      const hasExplicitStart = Number.isFinite(explicitStartEighths) && explicitStartEighths >= 0;
      const startTick = hasExplicitStart
        ? Math.max(0, Math.round(explicitStartEighths * EIGHTH_TICKS))
        : cursor;
      const endTick = startTick + durationTicks;
      if (!note.is_rest && Number.isFinite(note.midi)) {
        events.push({
          note_id: note.note_id,
          start_tick: startTick,
          end_tick: endTick,
        });
      }
      cursor = Math.max(cursor, endTick);
    }
  }
  return events;
}

function figuredBassForSlice(slice) {
  const inversion = String(slice?.inversion || "");
  if (/^\d+$/.test(inversion)) {
    return inversion === "42" ? "2" : inversion;
  }
  const quality = String(slice?.quality || "");
  const isSeventh = quality.includes("7");
  if (inversion === "root") return isSeventh ? "7" : "";
  if (inversion === "first") return isSeventh ? "65" : "6";
  if (inversion === "second") return isSeventh ? "43" : "64";
  if (inversion === "third") return isSeventh ? "42" : "";
  return "";
}

function splitRomanLabelAndFigure(rawLabel, fallbackFigure) {
  const normalized = String(rawLabel || "").trim();
  if (!normalized) {
    return { label: "", figure: fallbackFigure || "" };
  }
  // Capture inline figures in RN strings, including tonicized forms like V65/V.
  const m = normalized.match(/^(.+?)(\d+)(\/.+)?$/);
  if (!m) {
    return { label: normalized, figure: fallbackFigure || "" };
  }
  const label = `${m[1]}${m[3] || ""}`;
  const figure = m[2] || fallbackFigure || "";
  return { label, figure };
}

function buildRomanAnchors(harmonicSlices, centers) {
  if (!state.show_roman || !Array.isArray(harmonicSlices) || harmonicSlices.length === 0) {
    return [];
  }
  const events = buildTimedSoundingEvents();
  const startsByTick = new Map();
  for (const ev of events) {
    const c = centers.get(ev.note_id);
    if (!c) continue;
    const arr = startsByTick.get(ev.start_tick) ?? [];
    arr.push({ x: c.x, y: c.y });
    startsByTick.set(ev.start_tick, arr);
  }
  const startTicks = [...startsByTick.keys()].sort((a, b) => a - b);
  const anchors = [];

  for (const slice of harmonicSlices) {
    const rawLabel = String(slice?.roman_numeral || "");
    if (!rawLabel) continue;
    const fallbackFigure = figuredBassForSlice(slice);
    const { label, figure } = splitRomanLabelAndFigure(rawLabel, fallbackFigure);
    const tick = slice.start_tick;
    let hits = startsByTick.get(tick) ?? null;
    if (!hits || hits.length === 0) {
      let fallbackTick = null;
      let fallbackDistance = Number.POSITIVE_INFINITY;
      for (const t of startTicks) {
        const d = Math.abs(t - tick);
        if (d < fallbackDistance) {
          fallbackDistance = d;
          fallbackTick = t;
        } else if (d === fallbackDistance && fallbackTick != null && t > fallbackTick) {
          // Prefer the later position in ties so the label doesn't drift left.
          fallbackTick = t;
        }
      }
      hits = fallbackTick != null ? startsByTick.get(fallbackTick) ?? null : null;
    }
    if (!hits || hits.length === 0) continue;
    const x = hits.reduce((sum, p) => sum + p.x, 0) / hits.length;
    const sourceY = Math.max(...hits.map((p) => p.y));
    const prev = anchors[anchors.length - 1];
    // Collapse near-duplicate adjacent labels to avoid unreadable repeated RN overlays.
    if (
      prev &&
      prev.label === label &&
      prev.figure === figure &&
      Math.abs(prev.x - x) <= ROMAN_DUPLICATE_X_EPS
    ) {
      continue;
    }
    anchors.push({ x, sourceY, label, figure, startTick: tick });
  }
  return anchors;
}

function diagnosticKey(diag) {
  if (!diag || !diag.primary) return "";
  const p = diag.primary;
  const r = diag.related;
  const primary = `${p.note_id ?? ""}:${p.measure ?? ""}:${p.beat ?? ""}:${p.voice_index ?? ""}`;
  const related = r
    ? `${r.note_id ?? ""}:${r.measure ?? ""}:${r.beat ?? ""}:${r.voice_index ?? ""}`
    : "";
  return `${diag.rule_id ?? ""}|${diag.severity ?? ""}|${primary}|${related}|${diag.message ?? ""}`;
}

function resolveSelectedDiagnosticIndex(response) {
  const diagnostics = response?.diagnostics ?? [];
  if (!state.selected_diagnostic_key) return -1;
  for (let i = 0; i < diagnostics.length; i += 1) {
    if (diagnosticKey(diagnostics[i]) === state.selected_diagnostic_key) {
      return i;
    }
  }
  return -1;
}

function setSelectedDiagnosticFromIndex(response, index) {
  const diagnostics = response?.diagnostics ?? [];
  if (!Number.isInteger(index) || index < 0 || index >= diagnostics.length) {
    state.selected_diagnostic_key = null;
    return false;
  }
  const key = diagnosticKey(diagnostics[index]);
  if (!key) {
    state.selected_diagnostic_key = null;
    return false;
  }
  state.selected_diagnostic_key = key;
  return true;
}

function buildDiagnosticHitPoints(diagnostics, centers) {
  const points = [];
  for (let i = 0; i < diagnostics.length; i += 1) {
    const diag = diagnostics[i];
    const primary = centers.get(diag.primary?.note_id);
    if (!primary) continue;
    points.push({ diagnosticIndex: i, x: primary.x, y: primary.y });

    const related = centers.get(diag.related?.note_id);
    if (!related) continue;
    points.push({ diagnosticIndex: i, x: related.x, y: related.y });
    points.push({
      diagnosticIndex: i,
      x: (primary.x + related.x) / 2,
      y: (primary.y + related.y) / 2,
    });
  }
  return points;
}

function redrawOverlayLayers(response, opts = {}) {
  const showDiagnostics = opts.showDiagnostics !== false;
  const svg = ui.paper.querySelector("svg");
  if (!svg) return;

  if (showDiagnostics) {
    const selectedDiagnosticIndex = resolveSelectedDiagnosticIndex(response);
    drawDiagnosticsOverlay(svg, response.diagnostics ?? [], lastNoteCenters, {
      selectedDiagnosticIndex,
    });
    lastDiagnosticHitPoints = buildDiagnosticHitPoints(response.diagnostics ?? [], lastNoteCenters);
  } else {
    clearOverlay(svg);
    lastDiagnosticHitPoints = [];
  }

  drawSelectedOverlay(svg, selectedNoteId(), lastNoteCenters);
  drawRomanOverlay(svg, buildRomanAnchors(response.harmonic_slices ?? [], lastNoteCenters));
}

function activateDiagnosticSelection(index, opts = {}) {
  const scrollList = opts.scrollList !== false;
  if (!lastResponse) return;
  if (!setSelectedDiagnosticFromIndex(lastResponse, index)) return;

  renderDiagnostics(lastResponse);
  redrawOverlayLayers(lastResponse, { showDiagnostics: isRuleCheckerEnabled() });

  if (scrollList) {
    const row = ui.diagnosticsList.querySelector(`.diag-item[data-diag-index="${index}"]`);
    row?.scrollIntoView({ block: "nearest" });
  }
}

function debounceRender() {
  if (rerenderTimer) {
    window.clearTimeout(rerenderTimer);
  }
  rerenderTimer = window.setTimeout(() => {
    rerenderTimer = null;
    runAnalysisAndRender();
  }, 120);
}

function flushPendingRender() {
  if (!rerenderTimer) return;
  window.clearTimeout(rerenderTimer);
  rerenderTimer = null;
  runAnalysisAndRender();
}

function initVoiceRawTexts() {
  const defaultDuration = speciesDefaultDurationEighths(state.preset_id);
  state.voice_raw_texts = state.voices.map((voice) => notesToVoiceText(voice.notes, defaultDuration));
}

function ensureVoiceCountConsistency() {
  if (state.voices.length === state.voice_count && state.voice_raw_texts.length === state.voice_count) {
    return;
  }
  state.voices = createDefaultVoices(state.voice_count, state.preset_id);
  initVoiceRawTexts();
}

function loadPresetSchema() {
  const candidates = [
    new URL("../rules-presets.json", import.meta.url),
    new URL("../../../docs/planning/rules-presets.json", import.meta.url),
    new URL("/docs/planning/rules-presets.json", window.location.origin),
  ];
  const failures = [];

  async function loadNext(index) {
    if (index >= candidates.length) {
      const details = failures.length > 0 ? ` Tried: ${failures.join("; ")}` : "";
      throw new Error(`Failed to load preset schema (404).${details}`);
    }
    const url = candidates[index];
    try {
      const res = await fetch(url);
      if (!res.ok) {
        failures.push(`${url.pathname} -> ${res.status}`);
        return loadNext(index + 1);
      }
      return res.json();
    } catch (err) {
      failures.push(`${url.pathname} -> ${err?.message ?? String(err)}`);
      return loadNext(index + 1);
    }
  }

  return loadNext(0);
}

function loadSettingsIntoState() {
  const saved = loadEditorSettings();
  if (!saved) return;

  if (BUILTIN_PRESET_IDS.includes(saved.preset_id)) {
    state.preset_id = saved.preset_id;
  }
  if (typeof saved.custom_base_preset_id === "string") {
    state.custom_base_preset_id = saved.custom_base_preset_id;
  }
  if (Number.isInteger(saved.voice_count) && saved.voice_count >= 1 && saved.voice_count <= 4) {
    state.voice_count = saved.voice_count;
  }
  if (Number.isInteger(saved.key_tonic_pc) && saved.key_tonic_pc >= 0 && saved.key_tonic_pc <= 11) {
    state.key_tonic_pc = saved.key_tonic_pc;
  }
  if (typeof saved.mode === "string") {
    state.mode = saved.mode;
  }
  const analysisSettings = readPersistedAnalysisSettings(saved);
  state.analysis_backend = analysisSettings.analysis_backend;
  state.rule_harmonic_rhythm_chords_per_bar =
    analysisSettings.rule_harmonic_rhythm_chords_per_bar;
  state.show_augnet_debug = analysisSettings.show_augnet_debug;
  if (typeof saved.rule_checker_enabled === "boolean") {
    state.rule_checker_enabled = saved.rule_checker_enabled;
  }
  if (saved.time_signature && Number.isInteger(saved.time_signature.numerator)) {
    state.time_signature.numerator = saved.time_signature.numerator;
  }
  if (saved.time_signature && Number.isInteger(saved.time_signature.denominator)) {
    state.time_signature.denominator = saved.time_signature.denominator;
  }
  if (Number.isFinite(saved.pickup_eighths)) {
    state.pickup_eighths = normalizeDurationEighths(saved.pickup_eighths, null);
  }
  if (Number.isFinite(saved.score_zoom)) {
    state.score_zoom = Math.min(1.8, Math.max(0.5, Number(saved.score_zoom)));
  }
  state.show_bar_numbers = !!saved.show_bar_numbers;
  state.show_roman = !!saved.show_roman;
  state.show_harmonic_slices = saved.show_harmonic_slices !== false;
  state.cantus_lock_enabled = saved.cantus_lock_enabled !== false;
  if (Number.isInteger(saved.cantus_voice_index)) {
    state.cantus_voice_index = saved.cantus_voice_index;
  }
  if (saved.rule_overrides && typeof saved.rule_overrides === "object") {
    state.rule_overrides = {
      enabled_rule_ids: [...(saved.rule_overrides.enabled_rule_ids ?? [])],
      disabled_rule_ids: [...(saved.rule_overrides.disabled_rule_ids ?? [])],
      severity_overrides: { ...(saved.rule_overrides.severity_overrides ?? {}) },
      rule_params: { ...(saved.rule_overrides.rule_params ?? {}) },
    };
  }
  if (typeof saved.rule_filter === "string") {
    state.rule_filter = saved.rule_filter;
  }
  if (saved.insert_template && typeof saved.insert_template === "object") {
    state.insert_template = {
      is_rest: !!saved.insert_template.is_rest,
      duration_eighths: normalizeDurationEighths(saved.insert_template.duration_eighths, 2),
    };
  }
  if (Array.isArray(saved.voice_raw_texts) && saved.voice_raw_texts.length > 0) {
    state.voice_raw_texts = saved.voice_raw_texts.slice(0, 4);
    state.voice_count = state.voice_raw_texts.length;
  }
  if (!Array.isArray(state.voice_raw_texts) || state.voice_raw_texts.length === 0) {
    ensureVoiceCountConsistency();
  }
  ensureTimeSignatureSupported();
}

function persistSettings() {
  const analysisSettings = persistableAnalysisSettings(state);
  saveEditorSettings({
    preset_id: state.preset_id,
    custom_base_preset_id: state.custom_base_preset_id,
    voice_count: state.voice_count,
    key_tonic_pc: state.key_tonic_pc,
    mode: state.mode,
    analysis_backend: analysisSettings.analysis_backend,
    rule_harmonic_rhythm_chords_per_bar: analysisSettings.rule_harmonic_rhythm_chords_per_bar,
    show_augnet_debug: analysisSettings.show_augnet_debug,
    rule_checker_enabled: state.rule_checker_enabled,
    time_signature: state.time_signature,
    pickup_eighths: state.pickup_eighths,
    score_zoom: state.score_zoom,
    show_bar_numbers: state.show_bar_numbers,
    show_roman: state.show_roman,
    show_harmonic_slices: state.show_harmonic_slices,
    cantus_lock_enabled: state.cantus_lock_enabled,
    cantus_voice_index: state.cantus_voice_index,
    rule_overrides: state.rule_overrides,
    rule_filter: state.rule_filter,
    insert_template: state.insert_template,
    voice_raw_texts: state.voice_raw_texts,
  });
}

function renderPresetControls() {
  ui.presetSelect.innerHTML = BUILTIN_PRESET_IDS.map((id) => `<option value="${id}">${presetLabel(id)}</option>`).join("");
  ui.presetSelect.value = state.preset_id;

  const baseIds = getBasePresetIds(presetSchema);
  ui.customBaseSelect.innerHTML = baseIds
    .map((id) => `<option value="${id}">${presetLabel(id)}</option>`)
    .join("");
  ui.customBaseSelect.value = state.custom_base_preset_id;
  ui.customBaseSelect.disabled = state.preset_id !== "custom";
}

function renderKeyControls() {
  ui.keyTonic.innerHTML = KEY_OPTIONS.map(
    (opt) => `<option value="${opt.tonic_pc}">${opt.label}</option>`,
  ).join("");
  ui.keyTonic.value = String(state.key_tonic_pc);
  ui.modeSelect.value = state.mode;
  ensureTimeSignatureSupported();
  const supportedTs = getSupportedTimeSignatures();
  ui.timeSignatureSelect.innerHTML = supportedTs
    .map((ts) => `<option value="${timeSignatureOptionValue(ts)}">${ts.numerator}/${ts.denominator}</option>`)
    .join("");
  ui.timeSignatureSelect.value = timeSignatureOptionValue(state.time_signature);
  ui.zoomScale.value = String(Math.round(state.score_zoom * 100));
  ui.zoomLabel.textContent = `${Math.round(state.score_zoom * 100)}%`;
  ui.barNumberToggle.checked = state.show_bar_numbers;
  ui.romanToggle.checked = state.show_roman;
  ui.harmonicSlicesToggle.checked = state.show_harmonic_slices;
}

function renderAnalysisControls() {
  state.analysis_backend = normalizeAnalysisBackend(state.analysis_backend);
  state.rule_harmonic_rhythm_chords_per_bar = normalizeRuleBasedChordsPerBar(
    state.rule_harmonic_rhythm_chords_per_bar,
  );
  const uiState = analysisModeUiState(state.analysis_backend);
  ui.analysisMethodSelect.value = state.analysis_backend;
  ui.harmonicRhythmChordsPerBar.value = String(state.rule_harmonic_rhythm_chords_per_bar);
  ui.harmonicRhythmRuleControls.hidden = !uiState.showRuleHarmonicRhythmControls;
  ui.harmonicRhythmAutoNote.hidden = !uiState.showAugnetAutoRhythmNote;
  ui.augnetDebugToggle.disabled = !uiState.enableAugnetDebugToggle;
  if (!uiState.enableAugnetDebugToggle) {
    state.show_augnet_debug = false;
  }
  ui.augnetDebugToggle.checked = state.show_augnet_debug;
  ui.ruleCheckerToggle.checked = state.rule_checker_enabled !== false;
}

function buildInsertGlyphAbc(isRest, durationEighths) {
  const token = durationTokenFromEighths(durationEighths);
  const body = `${isRest ? "z" : "C"}${token}`;
  return `X:1\nM:4/4\nL:1/8\nK:C\n${body}`;
}

function renderInsertButton(container, { isRest, durationEighths, label }) {
  const isActive =
    !!state.insert_template &&
    state.insert_template.is_rest === isRest &&
    nearlyEqual(
      normalizeDurationEighths(state.insert_template.duration_eighths, 1),
      normalizeDurationEighths(durationEighths, 1),
    );
  const button = document.createElement("button");
  button.type = "button";
  button.className = `insert-choice${isActive ? " active" : ""}`;
  button.dataset.rest = isRest ? "1" : "0";
  button.dataset.duration = String(durationEighths);
  button.title = `${isRest ? "Rest" : "Note"} ${label}`;

  const glyph = document.createElement("div");
  glyph.className = "insert-glyph";
  glyph.id = `insert-glyph-${insertGlyphCounter++}`;
  button.appendChild(glyph);

  const text = document.createElement("div");
  text.className = "insert-choice-label";
  text.textContent = label;
  button.appendChild(text);
  container.appendChild(button);

  if (window.ABCJS?.renderAbc) {
    try {
      window.ABCJS.renderAbc(glyph.id, buildInsertGlyphAbc(isRest, durationEighths), {
        add_classes: true,
        responsive: "resize",
        scale: 1.8,
        staffwidth: 170,
        wrap: { preferredMeasuresPerLine: 1 },
      });
      const svg = glyph.querySelector("svg");
      if (svg) {
        svg.style.pointerEvents = "none";
        const staffJunk = [
          ".abcjs-staff",
          ".abcjs-clef",
          ".abcjs-key-signature",
          ".abcjs-time-signature",
          ".abcjs-bar",
          ".abcjs-ledger",
        ].flatMap((selector) => [...svg.querySelectorAll(selector)]);
        for (const el of staffJunk) {
          el.style.display = "none";
        }
        const PRIMITIVE_SELECTOR = "path, ellipse, circle, rect, line, polyline, polygon";
        const SYMBOL_ROOT_SELECTORS = [
          ".abcjs-notehead",
          ".abcjs-rest",
          ".abcjs-stem",
          ".abcjs-flag",
          ".abcjs-beam-elem",
          ".abcjs-accidentals",
          '[class*=" abcjs-d"]',
          '[class^="abcjs-d"]',
          '[class*=" abcjs-n"]',
          '[class^="abcjs-n"]',
        ];

        const symbolRootSet = new Set();
        for (const selector of SYMBOL_ROOT_SELECTORS) {
          for (const el of svg.querySelectorAll(selector)) {
            if (el instanceof SVGGraphicsElement) {
              symbolRootSet.add(el);
            }
          }
        }

        function isVisiblePrimitive(el) {
          const fill = String(el.getAttribute("fill") || "").toLowerCase();
          const stroke = String(el.getAttribute("stroke") || "").toLowerCase();
          const opacity = Number.parseFloat(el.getAttribute("opacity") || "1");
          const fillOpacity = Number.parseFloat(el.getAttribute("fill-opacity") || "1");
          const strokeOpacity = Number.parseFloat(el.getAttribute("stroke-opacity") || "1");
          const style = String(el.getAttribute("style") || "").toLowerCase();
          if (style.includes("display:none") || style.includes("visibility:hidden")) return false;
          if (Number.isFinite(opacity) && opacity <= 0) return false;
          if (
            (fill === "none" || (Number.isFinite(fillOpacity) && fillOpacity <= 0)) &&
            (stroke === "none" || (Number.isFinite(strokeOpacity) && strokeOpacity <= 0))
          ) {
            return false;
          }
          return true;
        }

        let symbolPrimitives = [...symbolRootSet].flatMap((root) => {
          if (root.matches(PRIMITIVE_SELECTOR)) return [root];
          return [...root.querySelectorAll(PRIMITIVE_SELECTOR)];
        });
        symbolPrimitives = symbolPrimitives.filter(
          (el) => el instanceof SVGGraphicsElement && isVisiblePrimitive(el),
        );

        if (symbolPrimitives.length === 0) {
          symbolPrimitives = [...svg.querySelectorAll(PRIMITIVE_SELECTOR)].filter(
            (el) => el instanceof SVGGraphicsElement && isVisiblePrimitive(el),
          );
        }

        let minX = Number.POSITIVE_INFINITY;
        let minY = Number.POSITIVE_INFINITY;
        let maxX = Number.NEGATIVE_INFINITY;
        let maxY = Number.NEGATIVE_INFINITY;
        for (const el of symbolPrimitives) {
          if (!(el instanceof SVGGraphicsElement) || typeof el.getBBox !== "function") continue;
          const b = el.getBBox();
          if (!Number.isFinite(b.width) || !Number.isFinite(b.height)) continue;
          if (b.width <= 0 || b.height <= 0) continue;
          minX = Math.min(minX, b.x);
          minY = Math.min(minY, b.y);
          maxX = Math.max(maxX, b.x + b.width);
          maxY = Math.max(maxY, b.y + b.height);
        }

        if (Number.isFinite(minX) && Number.isFinite(minY) && Number.isFinite(maxX) && Number.isFinite(maxY)) {
          const width = Math.max(1, maxX - minX);
          const height = Math.max(1, maxY - minY);
          const side = Math.max(width, height);
          const cx = minX + width / 2;
          const cy = minY + height / 2;
          const pad = Math.max(2, side * 0.16);
          const framedSide = side + pad * 2;
          svg.setAttribute(
            "viewBox",
            `${cx - framedSide / 2} ${cy - framedSide / 2} ${framedSide} ${framedSide}`,
          );
          svg.setAttribute("preserveAspectRatio", "xMidYMid meet");
        }
      }
    } catch (_err) {
      glyph.textContent = isRest ? "z" : "C";
    }
  } else {
    glyph.textContent = isRest ? "z" : "C";
  }
}

function renderInsertControls() {
  if (!ui.insertNoteButtons || !ui.insertRestButtons) return;
  ui.insertNoteButtons.innerHTML = "";
  ui.insertRestButtons.innerHTML = "";
  for (const option of INSERT_DURATION_OPTIONS) {
    renderInsertButton(ui.insertNoteButtons, {
      isRest: false,
      durationEighths: option.value,
      label: option.label,
    });
  }
  for (const option of REST_INSERT_DURATION_OPTIONS) {
    renderInsertButton(ui.insertRestButtons, {
      isRest: true,
      durationEighths: option.value,
      label: option.label,
    });
  }
}

function renderCantusControls() {
  ui.cantusSelect.innerHTML = FUX_CANTUS.map((entry) => `<option value="${entry.id}">${entry.label}</option>`).join("");

  ui.cantusTargetVoice.innerHTML = state.voices
    .map((voice) => `<option value="${voice.voice_index}">${voice.name}</option>`)
    .join("");
  ui.cantusLockToggle.checked = state.cantus_lock_enabled;

  if (state.cantus_lock_enabled && Number.isInteger(state.cantus_voice_index)) {
    const voice = state.voices[state.cantus_voice_index];
    ui.cantusLockStatus.textContent = voice
      ? `Locked: ${voice.name}`
      : "Locked cantus voice is out of range.";
  } else {
    ui.cantusLockStatus.textContent = "Cantus voice is unlocked.";
  }
}

function parseAllVoices() {
  if (state.imported_timeline_locked) {
    normalizeVoiceIds(state.voices);
    ensureSelectedNoteValidity();
    state.parse_errors = [];
    return;
  }
  const defaultDuration = speciesDefaultDurationEighths(state.preset_id);
  const parseErrors = [];

  const parsedVoices = state.voice_raw_texts.map((rawText, voiceIndex) => {
    const out = parseVoiceText(rawText, { defaultDurationEighths: defaultDuration });
    for (const err of out.errors) {
      parseErrors.push(`Voice ${voiceIndex + 1}: ${err}`);
    }
    for (const issue of validateMeterFit(out.notes, state.time_signature)) {
      parseErrors.push(`Voice ${voiceIndex + 1}: ${issue}`);
    }

    return {
      voice_index: voiceIndex,
      name: state.voices[voiceIndex]?.name ?? `Voice ${voiceIndex + 1}`,
      notes: out.notes,
    };
  });

  state.voices = parsedVoices;
  normalizeVoiceIds(state.voices);
  ensureSelectedNoteValidity();
  state.parse_errors = parseErrors;
}

function renderVoiceEditors() {
  const container = document.createElement("div");
  for (let i = 0; i < state.voice_count; i += 1) {
    const row = document.createElement("div");
    row.className = "voice-editor-row";

    const label = document.createElement("label");
    const locked = isVoiceLocked(i);
    label.textContent = `${state.voices[i]?.name ?? `Voice ${i + 1}`}${locked ? " (Cantus locked)" : ""}`;

    const area = document.createElement("textarea");
    area.rows = 2;
    area.value = state.voice_raw_texts[i] ?? "";
    area.placeholder = "Example: C8 D2 z1 E/2 F/4";
    area.dataset.voiceIndex = String(i);
    area.disabled = locked;
    area.addEventListener("input", (event) => {
      const idx = Number.parseInt(event.target.dataset.voiceIndex, 10);
      if (isVoiceLocked(idx)) return;
      invalidateSourceMusicXml();
      state.voice_raw_texts[idx] = event.target.value;
      parseAllVoices();
      debounceRender();
    });

    row.appendChild(label);
    row.appendChild(area);
    container.appendChild(row);
  }
  ui.voiceEditors.replaceChildren(container);
  renderParseErrors();
}

function renderParseErrors() {
  if (state.parse_errors.length === 0) {
    ui.parseErrors.textContent = "";
  } else {
    ui.parseErrors.textContent = state.parse_errors.join(" | ");
  }
}

function renderRules() {
  const resolved = resolveUiRuleState(presetSchema, state);
  const baseSet = new Set(resolved.base_active_rule_ids);
  const activeSet = new Set(resolved.active_rule_ids);
  const query = (state.rule_filter ?? "").trim().toLowerCase();
  const visibleRows = query
    ? ruleCatalog.filter((row) => row.rule_id.toLowerCase().includes(query) || row.group.toLowerCase().includes(query))
    : ruleCatalog;

  ui.ruleSummary.textContent = `${resolved.active_rule_ids.length}/${ruleCatalog.length} active rules | showing ${visibleRows.length}`;

  const fragment = document.createDocumentFragment();
  for (const row of visibleRows) {
    const isActive = activeSet.has(row.rule_id);
    const isBase = baseSet.has(row.rule_id);
    const severityOverride = state.rule_overrides.severity_overrides[row.rule_id] ?? "";

    const wrap = document.createElement("div");
    wrap.className = "rule-row";

    const main = document.createElement("div");
    main.className = "rule-row-main";

    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    checkbox.checked = isActive;
    checkbox.addEventListener("change", () => {
      toggleRuleOverride(state, row.rule_id, checkbox.checked, isBase);
      renderRules();
      debounceRender();
    });

    const id = document.createElement("div");
    id.className = "rule-id";
    id.textContent = row.rule_id;

    main.appendChild(checkbox);
    main.appendChild(id);

    const meta = document.createElement("div");
    meta.className = "rule-row-meta";

    const group = document.createElement("div");
    group.className = "rule-group";
    group.textContent = `${row.group}${isBase ? " | base" : " | override"}`;

    const severity = document.createElement("select");
    severity.className = "rule-severity";
    severity.setAttribute("aria-label", `Severity override for ${row.rule_id}`);
    severity.innerHTML =
      '<option value="">default</option><option value="error">error</option><option value="warning">warning</option><option value="info">info</option>';
    severity.value = severityOverride;
    severity.addEventListener("change", () => {
      const value = severity.value;
      if (!value) {
        delete state.rule_overrides.severity_overrides[row.rule_id];
      } else {
        state.rule_overrides.severity_overrides[row.rule_id] = value;
      }
      debounceRender();
    });

    meta.appendChild(group);
    meta.appendChild(severity);

    wrap.appendChild(main);
    wrap.appendChild(meta);
    fragment.appendChild(wrap);
  }

  ui.ruleList.replaceChildren(fragment);
}

function attachDragHandlers(svg) {
  const getActiveSvg = () => ui.paper.querySelector("svg");

  const clientToSvgPoint = (clientX, clientY) => {
    const active = getActiveSvg();
    if (!active) return null;
    const ctm = active.getScreenCTM();
    if (!ctm) return null;
    const p = active.createSVGPoint();
    p.x = clientX;
    p.y = clientY;
    return p.matrixTransform(ctm.inverse());
  };

  const pxToSvgUnits = (px) => {
    const active = getActiveSvg();
    if (!active) return px;
    const ctm = active.getScreenCTM();
    if (!ctm) return px;
    const scaleX = Math.hypot(ctm.a, ctm.b) || 1;
    return px / scaleX;
  };

  const findNearestNote = (clientX, clientY) => {
    const p = clientToSvgPoint(clientX, clientY);
    if (!p || lastNoteCenters.size === 0) return null;

    const hitRadius = pxToSvgUnits(18);
    const hitRadiusSq = hitRadius * hitRadius;

    let best = null;
    for (const voice of state.voices) {
      for (let i = 0; i < voice.notes.length; i += 1) {
        const note = voice.notes[i];
        const c = lastNoteCenters.get(note.note_id);
        if (!c) continue;
        const dx = c.x - p.x;
        const dy = c.y - p.y;
        const d2 = dx * dx + dy * dy;
        if (d2 <= hitRadiusSq && (!best || d2 < best.d2)) {
          best = { d2, voiceIndex: voice.voice_index, noteIndex: i };
        }
      }
    }
    return best;
  };

  const findNearestDiagnostic = (clientX, clientY) => {
    const p = clientToSvgPoint(clientX, clientY);
    if (!p || lastDiagnosticHitPoints.length === 0) return null;

    const hitRadius = pxToSvgUnits(16);
    const hitRadiusSq = hitRadius * hitRadius;

    let best = null;
    for (const point of lastDiagnosticHitPoints) {
      const dx = point.x - p.x;
      const dy = point.y - p.y;
      const d2 = dx * dx + dy * dy;
      if (d2 <= hitRadiusSq && (!best || d2 < best.d2)) {
        best = { d2, diagnosticIndex: point.diagnosticIndex };
      }
    }
    return best;
  };

  const svgToClientPoint = (svgX, svgY) => {
    const active = getActiveSvg();
    if (!active) return null;
    const ctm = active.getScreenCTM();
    if (!ctm) return null;
    const p = active.createSVGPoint();
    p.x = svgX;
    p.y = svgY;
    return p.matrixTransform(ctm);
  };

  const applyDragFrame = () => {
    dragRafPending = false;
    if (!dragState) return;

    const note = state.voices[dragState.voiceIndex]?.notes[dragState.noteIndex];
    if (!note) return;
    if (isVoiceLocked(dragState.voiceIndex)) return;

    const centerSvg = lastNoteCenters.get(note.note_id);
    if (!centerSvg) return;
    const centerClient = svgToClientPoint(centerSvg.x, centerSvg.y);
    if (!centerClient) return;

    const targetCenterY = dragLatestClientY - dragState.pointerOffsetY;
    const deltaY = centerClient.y - targetCenterY;
    const pixelsPerSemitone = Math.max(1.6, 2.3 * (state.score_zoom || 1));
    let semis = Math.round(deltaY / pixelsPerSemitone);
    if (semis === 0) return;

    // Avoid giant jumps on slow frames.
    semis = Math.max(-3, Math.min(3, semis));
    const direction = semis > 0 ? 1 : -1;
    const candidateMidi = clampMidiForEducation(note.midi + semis);
    const nextMidi = clampMidiForEducation(
      quantizeMidiToScale(candidateMidi, state.key_tonic_pc, state.mode, direction),
    );
    if (nextMidi === note.midi) return;

    note.midi = nextMidi;
    dragState.lastMidi = nextMidi;
    dragState.changed = true;

    renderScore(lastResponse ?? { diagnostics: [] }, { showDiagnostics: false, updateAudio: false });
  };

  if (!dragListenersBound) {
    window.addEventListener("pointermove", (event) => {
      if (!dragState) return;
      dragLatestClientY = event.clientY;
      if (!dragRafPending) {
        dragRafPending = true;
        window.requestAnimationFrame(applyDragFrame);
      }
    });
    const finishDrag = () => {
      if (!dragState) return;
      if (dragState.changed) {
        const defaultDuration = speciesDefaultDurationEighths(state.preset_id);
        state.voice_raw_texts[dragState.voiceIndex] = notesToVoiceText(
          state.voices[dragState.voiceIndex].notes,
          defaultDuration,
        );
        runAnalysisAndRender();
      } else if (lastResponse) {
        renderScore(lastResponse, { showDiagnostics: isRuleCheckerEnabled(), updateAudio: false });
      }
      dragState = null;
    };
    window.addEventListener("pointerup", finishDrag);
    window.addEventListener("pointercancel", finishDrag);
    dragListenersBound = true;
  }

  const startDrag = (event) => {
    const hit = findNearestNote(event.clientX, event.clientY);
    if (!hit) return;
    const voiceIndex = hit.voiceIndex;
    const noteIndex = hit.noteIndex;
    const note = state.voices[voiceIndex]?.notes[noteIndex];
    if (!note) return;
    const centerSvg = lastNoteCenters.get(note.note_id);
    if (!centerSvg) return;
    const centerClient = svgToClientPoint(centerSvg.x, centerSvg.y);
    if (!centerClient) return;

    selectNote(voiceIndex, noteIndex);
    if (isVoiceLocked(voiceIndex)) {
      renderScore(lastResponse ?? { diagnostics: [] }, { showDiagnostics: isRuleCheckerEnabled(), updateAudio: false });
      return;
    }
    dragState = {
      voiceIndex,
      noteIndex,
      startY: event.clientY,
      startCenterY: centerClient.y,
      pointerOffsetY: event.clientY - centerClient.y,
      lastMidi: note.midi,
      changed: false,
    };
    renderScore(lastResponse ?? { diagnostics: [] }, { showDiagnostics: false, updateAudio: false });
    dragLatestClientY = event.clientY;
  };

  svg.addEventListener("pointerdown", startDrag);
  svg.addEventListener("click", (event) => {
    const hit = findNearestDiagnostic(event.clientX, event.clientY);
    if (!hit) return;
    activateDiagnosticSelection(hit.diagnosticIndex, { scrollList: true });
  });
}

function renderDiagnostics(response) {
  if (!isRuleCheckerEnabled()) {
    ui.diagnosticsSummary.textContent = "Rule checker disabled.";
    state.selected_diagnostic_key = null;
    ui.diagnosticsList.innerHTML = '<li class="diag-item">Rule diagnostics are disabled.</li>';
    return;
  }
  const items = response.diagnostics ?? [];
  ui.diagnosticsSummary.textContent = `${response.summary.error_count} errors, ${response.summary.warning_count} warnings, ${response.summary.active_rule_count} active rules`;

  if (items.length === 0) {
    state.selected_diagnostic_key = null;
    ui.diagnosticsList.innerHTML = '<li class="diag-item">No diagnostics.</li>';
    return;
  }

  const selectedIndex = resolveSelectedDiagnosticIndex(response);
  if (selectedIndex < 0 && state.selected_diagnostic_key) {
    state.selected_diagnostic_key = null;
  }

  ui.diagnosticsList.innerHTML = items
    .map((d, idx) => {
      const related = d.related
        ? ` | Related: m${d.related.measure} b${d.related.beat} v${d.related.voice_index + 1}`
        : "";
      const activeClass = idx === selectedIndex ? " active" : "";
      return `<li class="diag-item ${d.severity}${activeClass}" data-diag-index="${idx}" tabindex="0"><strong>${d.rule_id}</strong><br>${d.message}<br>At m${d.primary.measure} b${d.primary.beat} v${d.primary.voice_index + 1}${related}</li>`;
    })
    .join("");
}

function renderWarnings(response) {
  const warnings = response.warnings ?? [];
  if (warnings.length === 0) {
    ui.warningList.innerHTML = "";
    return;
  }
  ui.warningList.innerHTML = warnings.map((w) => `<li class="warning-item">${w}</li>`).join("");
}

function sanitizeDumpField(value) {
  const text = value == null ? "" : String(value);
  return text.replace(/\t/g, " ").replace(/\r?\n/g, " ").trim();
}

function formatHarmonyDump(response) {
  const backend = normalizeAnalysisBackend(state.analysis_backend);
  const lines = [`# backend\t${backend}`];

  if (backend === "augnet_onnx") {
    const outputs = (response.harmonic_outputs ?? [])
      .filter((entry) => entry?.source === "augnet_onnx")
      .slice()
      .sort((a, b) => (a.start_tick - b.start_tick) || (a.output_id - b.output_id));
    lines.push("index\tstart_tick\tend_tick\troman_numeral\tlocal_key\ttonicized_key\tquality\tinversion\tchord_label\tconfidence");
    for (let i = 0; i < outputs.length; i += 1) {
      const row = outputs[i];
      lines.push(
        [
          i,
          row?.start_tick ?? "",
          row?.end_tick ?? "",
          sanitizeDumpField(row?.roman_numeral),
          sanitizeDumpField(row?.local_key),
          sanitizeDumpField(row?.tonicized_key),
          sanitizeDumpField(row?.chord_quality),
          sanitizeDumpField(row?.inversion),
          sanitizeDumpField(row?.chord_label),
          Number.isFinite(row?.confidence) ? Number(row.confidence).toFixed(6) : "",
        ].join("\t"),
      );
    }
    return lines.join("\n");
  }

  const slices = (response.harmonic_slices ?? [])
    .slice()
    .sort((a, b) => (a.start_tick - b.start_tick) || (a.slice_id - b.slice_id));
  lines.push("index\tstart_tick\tend_tick\troman_numeral\tquality\tinversion\tconfidence");
  for (let i = 0; i < slices.length; i += 1) {
    const row = slices[i];
    lines.push(
      [
        i,
        row?.start_tick ?? "",
        row?.end_tick ?? "",
        sanitizeDumpField(row?.roman_numeral),
        sanitizeDumpField(row?.quality),
        sanitizeDumpField(row?.inversion),
        Number.isFinite(row?.confidence) ? Number(row.confidence).toFixed(6) : "",
      ].join("\t"),
    );
  }
  return lines.join("\n");
}

function renderHarmonyDump(response) {
  if (!ui.harmonyDump) return;
  if (!state.show_harmonic_slices || !response) {
    ui.harmonyDump.value = "";
    return;
  }
  ui.harmonyDump.value = formatHarmonyDump(response);
}

function renderHarmony(response) {
  if (!state.show_harmonic_slices) {
    ui.harmonyCard.hidden = true;
    ui.harmonyList.innerHTML = "";
    renderHarmonyDump(null);
    lastHarmonyRows = [];
    return;
  }
  ui.harmonyCard.hidden = false;
  lastHarmonyRows = buildModeAwareHarmonyRows(response, state.analysis_backend, {
    showDebugLogits: state.show_augnet_debug,
  });
  ui.harmonyList.innerHTML = buildHarmonyListMarkup(lastHarmonyRows, state.analysis_backend);
  renderHarmonyDump(response);
}

function setAudioMessage(message) {
  if (!ui.abcAudio) return;
  ui.abcAudio.innerHTML = message ? `<p class="subtle">${message}</p>` : "";
}

function supportsAbcAudio() {
  const synth = window.ABCJS?.synth;
  if (!synth || typeof synth.supportsAudio !== "function") return false;
  try {
    return !!synth.supportsAudio();
  } catch (_err) {
    return false;
  }
}

function withTimeout(promise, ms, label) {
  let timeoutId = null;
  const timeoutPromise = new Promise((_, reject) => {
    timeoutId = window.setTimeout(() => {
      reject(new Error(`${label} timed out after ${ms}ms`));
    }, ms);
  });
  return Promise.race([promise, timeoutPromise]).finally(() => {
    if (timeoutId != null) {
      window.clearTimeout(timeoutId);
    }
  });
}

function ensureSynthController() {
  if (!ui.abcAudio) return null;
  if (synthController) return synthController;
  const SynthController = window.ABCJS?.synth?.SynthController;
  if (typeof SynthController !== "function") return null;
  const controller = new SynthController();
  controller.load("#abc-audio", null, {
    displayRestart: true,
    displayPlay: true,
    displayProgress: true,
    displayWarp: true,
    displayLoop: false,
  });
  synthController = controller;
  return synthController;
}

function resetSynthController() {
  if (!ui.abcAudio) return;
  if (synthController && typeof synthController.pause === "function") {
    try {
      synthController.pause();
    } catch (_err) {
      // ignore stale controller errors during refresh
    }
  }
  synthController = null;
  ui.abcAudio.innerHTML = "";
}

async function applyAudioPlaybackUpdate(visualObj) {
  if (!ui.abcAudio) return;
  lastAudioVisualObj = visualObj;

  if (!visualObj || !window.ABCJS?.synth) {
    resetSynthController();
    setAudioMessage("");
    return;
  }

  if (!supportsAbcAudio()) {
    setAudioMessage("Audio playback is unavailable in this browser.");
    return;
  }

  try {
    resetSynthController();
    const controller = ensureSynthController();
    if (!controller) {
      setAudioMessage("Audio controls failed to initialize.");
      return;
    }
    if (typeof controller.pause === "function") {
      controller.pause();
    }
    await withTimeout(controller.setTune(visualObj, false, {}), 4000, "Audio update");
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    setAudioMessage(`Audio setup failed: ${msg}`);
    console.error(err);
  }
}

function updateAudioPlayback(visualObj) {
  lastAudioVisualObj = visualObj;
  if (audioUpdateInFlight) {
    audioUpdateQueued = true;
    return;
  }

  audioUpdateInFlight = true;
  void (async () => {
    do {
      audioUpdateQueued = false;
      await applyAudioPlaybackUpdate(lastAudioVisualObj);
    } while (audioUpdateQueued);
    audioUpdateInFlight = false;
  })();
}

function renderScore(response, opts = {}) {
  const showDiagnostics = opts.showDiagnostics !== false;
  const updateAudio = opts.updateAudio !== false;

  if (!window.ABCJS) {
    ui.paper.textContent = "ABCJS library failed to load.";
    setAudioMessage("ABCJS library failed to load.");
    return;
  }

  const scale = Math.max(0.5, Math.min(1.8, state.score_zoom || 1));
  document.documentElement.style.setProperty("--oh-score-zoom", String(scale));
  const availableWidth = Math.max(320, Math.floor(ui.paper.clientWidth - 20));
  const staffwidth = Math.max(260, Math.floor(availableWidth / scale));
  const preferredMeasuresPerLine = Math.max(2, Math.floor(availableWidth / 180));

  const abc = buildAbcFromVoices({
    voices: displayVoicesWithPadding(),
    presetId: state.preset_id,
    keyLabel: keyLabelByPc(keySignaturePcForMode(state.key_tonic_pc, state.mode)),
    timeSignature: state.time_signature,
    pickupEighths: state.pickup_eighths,
    showBarNumbers: state.show_bar_numbers,
  });

  const rendered = window.ABCJS.renderAbc("paper", abc, {
    responsive: "resize",
    add_classes: true,
    scale,
    staffwidth,
    wrap: {
      preferredMeasuresPerLine,
      minSpacing: 1.2,
      maxSpacing: 2.8,
      lastLineLimit: 0.4,
    },
    oneSvgPerLine: false,
  });
  const visualObj = Array.isArray(rendered) ? rendered[0] : null;
  if (updateAudio) {
    updateAudioPlayback(visualObj);
  }

  const svg = ui.paper.querySelector("svg");
  if (!svg) return;

  const centers = computeNoteCenters(svg, state.voices);
  lastNoteCenters = centers;
  redrawOverlayLayers(response, { showDiagnostics });
  attachDragHandlers(svg);
}

function setAnalyzerReadyStatus() {
  ui.engineStatus.textContent = `Analyzer: Rust/WASM active (${state.analysis_backend})`;
}

function renderAnalysisFailure(error, requestedBackend) {
  const failure = buildAnalysisFailureUiModel(requestedBackend, error);
  const msg = analysisErrorMessage(error);
  ui.engineStatus.textContent = failure.statusText;
  ui.warningList.innerHTML = `<li class="warning-item fatal-warning">${failure.warningMessage}</li>`;
  ui.diagnosticsSummary.textContent = "Analysis failed.";
  ui.diagnosticsList.innerHTML = `<li class="diag-item error">${msg}</li>`;
  if (state.show_harmonic_slices) {
    ui.harmonyCard.hidden = false;
    ui.harmonyList.innerHTML = `<li class="harmony-item">${failure.warningMessage}</li>`;
    renderHarmonyDump(null);
  } else {
    ui.harmonyCard.hidden = true;
    ui.harmonyList.innerHTML = "";
    renderHarmonyDump(null);
  }
  lastResponse = null;
  lastHarmonyRows = [];
  renderScore({ diagnostics: [], harmonic_slices: [] }, { showDiagnostics: false });
}

async function runAnalysisAndRender() {
  parseAllVoices();
  const activeEl = document.activeElement;
  const typingInVoiceEditor =
    activeEl instanceof HTMLTextAreaElement && activeEl.closest("#voice-editors") !== null;
  if (typingInVoiceEditor) {
    renderParseErrors();
  } else {
    renderVoiceEditors();
  }
  renderRules();

  const resolvedRules = resolveUiRuleState(presetSchema, state);
  const request = buildAnalysisRequest(state, resolvedRules);
  let response;
  try {
    response = await analyzeRequest(request);
  } catch (err) {
    renderAnalysisFailure(err, request?.config?.analysis_backend ?? state.analysis_backend);
    persistSettings();
    return;
  }
  lastResponse = response;
  setAnalyzerReadyStatus();

  renderScore(response, { showDiagnostics: isRuleCheckerEnabled() });
  renderDiagnostics(response);
  renderWarnings(response);
  renderHarmony(response);

  persistSettings();
}

function applyCantusTokens(tokens, targetVoiceIndex, opts = {}) {
  invalidateSourceMusicXml();
  const defaultDuration = opts.defaultDurationEighths ?? 8;
  const parsed = parseVoiceText(tokens, { defaultDurationEighths: defaultDuration });
  if (parsed.notes.length === 0) {
    return;
  }
  state.voices[targetVoiceIndex].notes = parsed.notes;
  const targetDuration = voiceDurationEighths(state.voices[targetVoiceIndex]);
  alignVoicesAfterReferenceChange(targetDuration);
}

function rerenderSavedProfilesSelect() {
  ui.savedProfiles.innerHTML = state.custom_profiles
    .map((profile, idx) => `<option value="${idx}">${profile.name}</option>`)
    .join("");
}

function bindUiEvents() {
  ui.presetSelect.addEventListener("change", () => {
    state.preset_id = ui.presetSelect.value;
    if (state.preset_id !== "custom") {
      state.custom_base_preset_id = state.preset_id;
    }
    ensureTimeSignatureSupported();
    renderKeyControls();
    ui.customBaseSelect.disabled = state.preset_id !== "custom";
    parseAllVoices();
    runAnalysisAndRender();
  });

  ui.customBaseSelect.addEventListener("change", () => {
    state.custom_base_preset_id = ui.customBaseSelect.value;
    debounceRender();
  });

  ui.voiceCount.addEventListener("change", () => {
    state.voice_count = Number.parseInt(ui.voiceCount.value, 10);
    state.voices = createDefaultVoices(state.voice_count, state.preset_id);
    invalidateSourceMusicXml();
    if (Number.isInteger(state.cantus_voice_index) && state.cantus_voice_index >= state.voice_count) {
      state.cantus_voice_index = null;
    }
    initVoiceRawTexts();
    state.selected_note = null;
    renderCantusControls();
    debounceRender();
  });

  ui.keyTonic.addEventListener("change", () => {
    state.key_tonic_pc = Number.parseInt(ui.keyTonic.value, 10);
    debounceRender();
  });

  ui.modeSelect.addEventListener("change", () => {
    state.mode = ui.modeSelect.value;
    debounceRender();
  });

  ui.analysisMethodSelect.addEventListener("change", () => {
    state.analysis_backend = normalizeAnalysisBackend(ui.analysisMethodSelect.value);
    renderAnalysisControls();
    debounceRender();
  });

  ui.ruleCheckerToggle.addEventListener("change", () => {
    state.rule_checker_enabled = !!ui.ruleCheckerToggle.checked;
    debounceRender();
  });

  ui.harmonicRhythmChordsPerBar.addEventListener("change", () => {
    state.rule_harmonic_rhythm_chords_per_bar = normalizeRuleBasedChordsPerBar(
      ui.harmonicRhythmChordsPerBar.value,
    );
    if (state.analysis_backend === "rule_based") {
      debounceRender();
    } else {
      persistSettings();
    }
  });

  ui.augnetDebugToggle.addEventListener("change", () => {
    if (ui.augnetDebugToggle.disabled) {
      state.show_augnet_debug = false;
      ui.augnetDebugToggle.checked = false;
      return;
    }
    state.show_augnet_debug = ui.augnetDebugToggle.checked;
    if (lastResponse) {
      renderHarmony(lastResponse);
    }
    persistSettings();
  });

  ui.cantusLockToggle.addEventListener("change", () => {
    state.cantus_lock_enabled = ui.cantusLockToggle.checked;
    renderCantusControls();
    renderVoiceEditors();
    renderScore(lastResponse ?? { diagnostics: [] }, { showDiagnostics: isRuleCheckerEnabled() });
    persistSettings();
  });

  ui.timeSignatureSelect.addEventListener("change", () => {
    const parsed = parseTimeSignatureOption(ui.timeSignatureSelect.value);
    if (!parsed) return;
    state.time_signature = parsed;
    debounceRender();
  });

  ui.zoomScale.addEventListener("input", () => {
    const pct = Math.max(50, Math.min(180, Number.parseInt(ui.zoomScale.value, 10) || 100));
    state.score_zoom = pct / 100;
    ui.zoomLabel.textContent = `${pct}%`;
    debounceRender();
  });

  ui.barNumberToggle.addEventListener("change", () => {
    state.show_bar_numbers = ui.barNumberToggle.checked;
    debounceRender();
  });

  ui.addMeasure.addEventListener("click", () => {
    invalidateSourceMusicXml();
    const m = measureUnitsEighths();
    for (const voice of state.voices) {
      appendRestToVoice(voice, m);
    }
    refreshVoiceTextsFromState();
    debounceRender();
  });

  ui.removeMeasure.addEventListener("click", () => {
    invalidateSourceMusicXml();
    const m = measureUnitsEighths();
    for (const voice of state.voices) {
      removeDurationFromEnd(voice, m);
    }
    refreshVoiceTextsFromState();
    debounceRender();
  });

  ui.romanToggle.addEventListener("change", () => {
    state.show_roman = ui.romanToggle.checked;
    if (lastResponse) {
      redrawOverlayLayers(lastResponse, { showDiagnostics: isRuleCheckerEnabled() });
    }
    persistSettings();
  });

  ui.harmonicSlicesToggle.addEventListener("change", () => {
    state.show_harmonic_slices = ui.harmonicSlicesToggle.checked;
    if (lastResponse) {
      renderHarmony(lastResponse);
    } else {
      renderHarmonyDump(null);
    }
    persistSettings();
  });

  ui.ruleFilter.addEventListener("input", () => {
    state.rule_filter = ui.ruleFilter.value ?? "";
    renderRules();
  });

  ui.applyCantus.addEventListener("click", () => {
    const cantus = getCantusById(ui.cantusSelect.value);
    if (!cantus) return;
    const target = Number.parseInt(ui.cantusTargetVoice.value, 10);
    applyCantusTokens(cantus.tokens, target, { defaultDurationEighths: 8 });
    state.cantus_voice_index = target;
    if (cantus.mode) {
      state.mode = cantus.mode;
      ui.modeSelect.value = cantus.mode;
    }
    renderCantusControls();
    debounceRender();
  });

  ui.applyCustomCantus.addEventListener("click", () => {
    const target = Number.parseInt(ui.cantusTargetVoice.value, 10);
    applyCantusTokens(ui.customCantus.value, target, { defaultDurationEighths: 8 });
    state.cantus_voice_index = target;
    renderCantusControls();
    debounceRender();
  });

  ui.saveProfile.addEventListener("click", () => {
    const name = ui.customProfileName.value.trim();
    if (!name) return;
    state.custom_profiles.push({
      name,
      base_preset_id: state.preset_id === "custom" ? state.custom_base_preset_id : state.preset_id,
      enabled_rule_ids: [...state.rule_overrides.enabled_rule_ids],
      disabled_rule_ids: [...state.rule_overrides.disabled_rule_ids],
      severity_overrides: { ...state.rule_overrides.severity_overrides },
      rule_params: { ...state.rule_overrides.rule_params },
    });
    saveCustomProfiles(state.custom_profiles);
    ui.customProfileName.value = "";
    rerenderSavedProfilesSelect();
  });

  ui.loadProfile.addEventListener("click", () => {
    const idx = Number.parseInt(ui.savedProfiles.value, 10);
    const profile = state.custom_profiles[idx];
    if (!profile) return;
    state.preset_id = "custom";
    state.custom_base_preset_id = profile.base_preset_id;
    state.rule_overrides = {
      enabled_rule_ids: [...(profile.enabled_rule_ids ?? [])],
      disabled_rule_ids: [...(profile.disabled_rule_ids ?? [])],
      severity_overrides: { ...(profile.severity_overrides ?? {}) },
      rule_params: { ...(profile.rule_params ?? {}) },
    };
    renderPresetControls();
    debounceRender();
  });

  ui.deleteProfile.addEventListener("click", () => {
    const idx = Number.parseInt(ui.savedProfiles.value, 10);
    if (!Number.isInteger(idx) || idx < 0 || idx >= state.custom_profiles.length) return;
    state.custom_profiles.splice(idx, 1);
    saveCustomProfiles(state.custom_profiles);
    rerenderSavedProfilesSelect();
  });

  ui.importMusicXml.addEventListener("click", async () => {
    try {
      const file = ui.musicXmlFile.files?.[0];
      if (!file) {
        ui.parseErrors.textContent = "Select a MusicXML file first.";
        return;
      }
      const text = await file.text();
      const imported = await importMusicXmlWithWasm(text, { maxVoices: 4, presetId: state.preset_id });

      state.voice_count = Math.max(1, Math.min(4, imported.voices.length));
      state.voices =
        imported.voices.length > 0
          ? imported.voices.slice(0, state.voice_count)
          : createDefaultVoices(state.voice_count, state.preset_id);
      state.preset_id = imported.preset_id ?? state.preset_id;
      state.key_tonic_pc = imported.key_tonic_pc;
      state.mode = imported.mode;
      state.time_signature = imported.time_signature;
      state.pickup_eighths = Number.isFinite(imported.pickup_eighths) ? imported.pickup_eighths : null;
      state.source_musicxml_raw = text;
      state.imported_timeline_locked = true;
      if (Number.isInteger(state.cantus_voice_index) && state.cantus_voice_index >= state.voice_count) {
        state.cantus_voice_index = null;
      }
      ensureTimeSignatureSupported();
      normalizeVoiceIds(state.voices);
      initVoiceRawTexts();

      renderPresetControls();
      renderKeyControls();
      renderCantusControls();
      ui.voiceCount.value = String(state.voice_count);
      rerenderSavedProfilesSelect();
      ui.parseErrors.textContent = "";
      runAnalysisAndRender();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      ui.parseErrors.textContent = `MusicXML import failed: ${msg}`;
      console.error(err);
    }
  });

  ui.exportMusicXml.addEventListener("click", () => {
    const xml = exportMusicXml(state);
    const blob = new Blob([xml], { type: "application/xml" });
    const link = document.createElement("a");
    link.href = URL.createObjectURL(blob);
    link.download = "open-harmony-export.musicxml";
    link.click();
    URL.revokeObjectURL(link.href);
  });

  ui.abcAudio?.addEventListener("pointerdown", () => {
    flushPendingRender();
  });

  ui.insertNoteButtons?.addEventListener("click", (event) => {
    const button = event.target.closest("button[data-duration][data-rest]");
    if (!button) return;
    const duration = normalizeDurationEighths(button.dataset.duration, 2);
    const isRest = button.dataset.rest === "1";
    state.insert_template = {
      is_rest: isRest,
      duration_eighths: duration,
    };
    renderInsertControls();
    persistSettings();
  });

  ui.insertRestButtons?.addEventListener("click", (event) => {
    const button = event.target.closest("button[data-duration][data-rest]");
    if (!button) return;
    const duration = normalizeDurationEighths(button.dataset.duration, 2);
    const isRest = button.dataset.rest === "1";
    state.insert_template = {
      is_rest: isRest,
      duration_eighths: duration,
    };
    renderInsertControls();
    persistSettings();
  });

  ui.insertNoteBefore.addEventListener("click", () => insertAtSelection("before"));
  ui.insertNoteAfter.addEventListener("click", () => insertAtSelection("after"));
  ui.replaceSelectedNote.addEventListener("click", () => replaceSelected());
  ui.toggleSelectedDot.addEventListener("click", () => toggleSelectedDot());
  ui.toggleSelectedTie.addEventListener("click", () => toggleSelectedTieStart());
  ui.deleteSelectedNote.addEventListener("click", () => deleteSelectedNote());
  ui.copyHarmonyDump?.addEventListener("click", async () => {
    const text = ui.harmonyDump?.value ?? "";
    if (!text) return;
    try {
      if (navigator?.clipboard?.writeText) {
        await navigator.clipboard.writeText(text);
      } else {
        ui.harmonyDump.focus();
        ui.harmonyDump.select();
        document.execCommand("copy");
      }
      ui.copyHarmonyDump.textContent = "Copied";
      window.setTimeout(() => {
        if (ui.copyHarmonyDump) ui.copyHarmonyDump.textContent = "Copy Harmony Output";
      }, 1100);
    } catch (err) {
      console.error("Failed to copy harmony dump", err);
    }
  });

  ui.diagnosticsList.addEventListener("click", (event) => {
    const row = event.target.closest(".diag-item[data-diag-index]");
    if (!row) return;
    const idx = Number.parseInt(row.dataset.diagIndex, 10);
    if (!Number.isInteger(idx)) return;
    activateDiagnosticSelection(idx, { scrollList: false });
  });

  ui.diagnosticsList.addEventListener("keydown", (event) => {
    if (event.key !== "Enter" && event.key !== " ") return;
    const row = event.target.closest(".diag-item[data-diag-index]");
    if (!row) return;
    const idx = Number.parseInt(row.dataset.diagIndex, 10);
    if (!Number.isInteger(idx)) return;
    event.preventDefault();
    activateDiagnosticSelection(idx, { scrollList: false });
  });

  window.addEventListener("keydown", (event) => {
    if (!(event.ctrlKey || event.metaKey)) return;
    if (event.altKey) return;
    const active = document.activeElement;
    if (
      active instanceof HTMLInputElement ||
      active instanceof HTMLTextAreaElement ||
      active instanceof HTMLSelectElement ||
      (active && active.isContentEditable)
    ) {
      return;
    }

    const key = event.key.toLowerCase();
    if (key === "c") {
      if (copySelectedNote()) {
        event.preventDefault();
      }
      return;
    }
    if (key === "x") {
      if (copySelectedNote()) {
        deleteSelectedNote();
        event.preventDefault();
      }
      return;
    }
    if (key === "v") {
      if (state.clipboard_note) {
        pasteAfterSelected();
        event.preventDefault();
      }
    }
  });
}

async function boot() {
  presetSchema = await loadPresetSchema();
  ruleCatalog = buildRuleCatalog(presetSchema);

  loadSettingsIntoState();
  ensureVoiceCountConsistency();
  if (!state.voice_raw_texts || state.voice_raw_texts.length === 0) {
    initVoiceRawTexts();
  }

  renderPresetControls();
  renderKeyControls();
  renderAnalysisControls();
  renderInsertControls();
  renderCantusControls();
  rerenderSavedProfilesSelect();
  ui.ruleFilter.value = state.rule_filter;

  ui.voiceCount.value = String(state.voice_count);
  if (Number.isInteger(state.cantus_voice_index) && state.cantus_voice_index >= state.voice_count) {
    state.cantus_voice_index = null;
  }

  await initAnalyzer();
  setAnalyzerReadyStatus();

  bindUiEvents();

  parseAllVoices();
  renderVoiceEditors();
  renderRules();

  runAnalysisAndRender();
}

boot().catch((err) => {
  ui.engineStatus.textContent = `Fatal initialization error: ${err.message}`;
  console.error("FATAL: web analyzer initialization failed.", err);
});
