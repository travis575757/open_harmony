import {
  buildAbcFromVoices,
  clampMidiForEducation,
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
import { exportMusicXml, importMusicXml } from "./musicxml.js";
import {
  keySignaturePcForMode,
  quantizeMidiToScale,
  supportedTimeSignaturesForPreset,
} from "./musicTheory.js";
import { BUILTIN_PRESET_IDS, buildRuleCatalog, getBasePresetIds, presetLabel } from "./presets.js";
import { resolveUiRuleState, toggleRuleOverride } from "./ruleConfig.js";
import { KEY_OPTIONS, buildAnalysisRequest, createDefaultVoices, keyLabelByPc, normalizeVoiceIds } from "./scoreModel.js";
import { loadCustomProfiles, loadEditorSettings, saveCustomProfiles, saveEditorSettings } from "./storage.js";
import { analyzeRequest, initAnalyzer } from "./wasmClient.js";

const ui = {
  presetSelect: document.getElementById("preset-select"),
  customBaseSelect: document.getElementById("custom-base-select"),
  voiceCount: document.getElementById("voice-count"),
  keyTonic: document.getElementById("key-tonic"),
  modeSelect: document.getElementById("mode-select"),
  timeSignatureSelect: document.getElementById("time-signature-select"),
  addMeasure: document.getElementById("add-measure"),
  removeMeasure: document.getElementById("remove-measure"),
  zoomScale: document.getElementById("zoom-scale"),
  zoomLabel: document.getElementById("zoom-label"),
  barNumberToggle: document.getElementById("bar-number-toggle"),
  romanToggle: document.getElementById("roman-toggle"),
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
  insertDuration: document.getElementById("insert-duration"),
  insertNoteBefore: document.getElementById("insert-note-before"),
  insertNoteAfter: document.getElementById("insert-note-after"),
  insertRestBefore: document.getElementById("insert-rest-before"),
  insertRestAfter: document.getElementById("insert-rest-after"),
  deleteSelectedNote: document.getElementById("delete-selected-note"),
  voiceEditors: document.getElementById("voice-editors"),
  parseErrors: document.getElementById("parse-errors"),
  diagnosticsSummary: document.getElementById("diagnostics-summary"),
  diagnosticsList: document.getElementById("diagnostics-list"),
  harmonyCard: document.getElementById("harmony-card"),
  harmonyList: document.getElementById("harmony-list"),
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
const EIGHTH_TICKS = 240;

const state = {
  preset_id: "species1",
  custom_base_preset_id: "species1",
  voice_count: 2,
  key_tonic_pc: 0,
  mode: "major",
  time_signature: { numerator: 4, denominator: 4 },
  score_zoom: 1,
  show_bar_numbers: false,
  show_roman: false,
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
  return supportedTimeSignaturesForPreset(state.preset_id);
}

function ensureTimeSignatureSupported() {
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

function refreshVoiceTextForVoice(voiceIndex) {
  if (!state.voices[voiceIndex]) return;
  const defaultDuration = speciesDefaultDurationEighths(state.preset_id);
  state.voice_raw_texts[voiceIndex] = notesToVoiceText(state.voices[voiceIndex].notes, defaultDuration);
}

function cloneNoteForClipboard(note) {
  return {
    midi: Number.isFinite(note.midi) ? note.midi : 60,
    is_rest: !!note.is_rest,
    duration_eighths: Math.max(1, Number.parseInt(note.duration_eighths, 10) || 1),
    tie_start: false,
    tie_end: false,
  };
}

function createEditableNote({ isRest, is_rest, durationEighths, duration_eighths, midi }) {
  return {
    note_id: "",
    midi: Number.isFinite(midi) ? midi : 60,
    is_rest: !!(isRest ?? is_rest),
    duration_eighths: Math.max(
      1,
      Number.parseInt(durationEighths ?? duration_eighths, 10) || 1,
    ),
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

function commitVoiceMutation(voiceIndex, newSelectedNoteIndex = null) {
  normalizeVoiceIds(state.voices);
  refreshVoiceTextForVoice(voiceIndex);
  if (Number.isInteger(newSelectedNoteIndex)) {
    selectNote(voiceIndex, newSelectedNoteIndex);
  } else {
    ensureSelectedNoteValidity();
  }
  runAnalysisAndRender();
}

function insertAtSelection(kind, position) {
  const ref = selectedNoteRef();
  if (!ref) return;
  if (isVoiceLocked(ref.voiceIndex)) return;
  const duration = Math.max(1, Number.parseInt(ui.insertDuration.value, 10) || 2);
  const newNote =
    kind === "rest"
      ? createEditableNote({ isRest: true, durationEighths: duration, midi: 60 })
      : createEditableNote({ isRest: false, durationEighths: duration, midi: selectedOrDefaultMidi() });
  const insertIndex = position === "before" ? ref.noteIndex : ref.noteIndex + 1;
  ref.voice.notes.splice(insertIndex, 0, newNote);
  commitVoiceMutation(ref.voiceIndex, insertIndex);
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
  return Math.max(
    1,
    Math.round((state.time_signature.numerator * 8) / Math.max(1, state.time_signature.denominator)),
  );
}

function voiceDurationEighths(voice) {
  return voice.notes.reduce((acc, n) => acc + Math.max(0, n.duration_eighths || 0), 0);
}

function appendRestToVoice(voice, durationEighths) {
  const d = Math.max(0, Math.round(durationEighths));
  if (d <= 0) return;
  const last = voice.notes[voice.notes.length - 1];
  if (last && last.is_rest && !last.tie_start && !last.tie_end) {
    last.duration_eighths += d;
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
  let remaining = Math.max(0, Math.round(durationEighths));
  while (remaining > 0 && voice.notes.length > 0) {
    const last = voice.notes[voice.notes.length - 1];
    const take = Math.min(remaining, Math.max(1, last.duration_eighths || 1));
    if (last.duration_eighths <= take) {
      remaining -= last.duration_eighths;
      voice.notes.pop();
    } else {
      last.duration_eighths -= take;
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
    const remove = Math.min(last.duration_eighths, current - targetDurationEighths);
    last.duration_eighths -= remove;
    current -= remove;
    if (last.duration_eighths <= 0) {
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
  let finalDuration = Math.max(0, Math.round(referenceDurationEighths));
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
      const durationTicks = Math.max(1, Math.round((note.duration_eighths || 1) * EIGHTH_TICKS));
      const startTick = cursor;
      const endTick = cursor + durationTicks;
      if (!note.is_rest && Number.isFinite(note.midi)) {
        events.push({
          note_id: note.note_id,
          start_tick: startTick,
          end_tick: endTick,
        });
      }
      cursor = endTick;
    }
  }
  return events;
}

function figuredBassForSlice(slice) {
  const inversion = String(slice?.inversion || "");
  const quality = String(slice?.quality || "");
  const isSeventh = quality.includes("7");
  if (inversion === "root") return isSeventh ? "7" : "";
  if (inversion === "first") return isSeventh ? "65" : "6";
  if (inversion === "second") return isSeventh ? "43" : "64";
  if (inversion === "third") return isSeventh ? "42" : "";
  return "";
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
    const label = rawLabel.replace(/\d+$/, "").toUpperCase();
    const figure = figuredBassForSlice(slice);
    const tick = slice.start_tick;
    let hits = startsByTick.get(tick) ?? null;
    if (!hits || hits.length === 0) {
      let fallbackTick = null;
      for (const t of startTicks) {
        if (t > tick) break;
        fallbackTick = t;
      }
      hits = fallbackTick != null ? startsByTick.get(fallbackTick) ?? null : null;
    }
    if (!hits || hits.length === 0) continue;
    const x = hits.reduce((sum, p) => sum + p.x, 0) / hits.length;
    const sourceY = Math.max(...hits.map((p) => p.y));
    anchors.push({ x, sourceY, label, figure });
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
  redrawOverlayLayers(lastResponse, { showDiagnostics: true });

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
  const url = new URL("../../../docs/planning/rules-presets.json", import.meta.url);
  return fetch(url).then((res) => {
    if (!res.ok) {
      throw new Error(`Failed to load preset schema (${res.status})`);
    }
    return res.json();
  });
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
  if (saved.time_signature && Number.isInteger(saved.time_signature.numerator)) {
    state.time_signature.numerator = saved.time_signature.numerator;
  }
  if (saved.time_signature && Number.isInteger(saved.time_signature.denominator)) {
    state.time_signature.denominator = saved.time_signature.denominator;
  }
  if (Number.isFinite(saved.score_zoom)) {
    state.score_zoom = Math.min(1.8, Math.max(0.5, Number(saved.score_zoom)));
  }
  state.show_bar_numbers = !!saved.show_bar_numbers;
  state.show_roman = !!saved.show_roman;
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
  saveEditorSettings({
    preset_id: state.preset_id,
    custom_base_preset_id: state.custom_base_preset_id,
    voice_count: state.voice_count,
    key_tonic_pc: state.key_tonic_pc,
    mode: state.mode,
    time_signature: state.time_signature,
    score_zoom: state.score_zoom,
    show_bar_numbers: state.show_bar_numbers,
    show_roman: state.show_roman,
    cantus_lock_enabled: state.cantus_lock_enabled,
    cantus_voice_index: state.cantus_voice_index,
    rule_overrides: state.rule_overrides,
    rule_filter: state.rule_filter,
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
    area.placeholder = "Example: C8 D4 z2 E2 F1";
    area.dataset.voiceIndex = String(i);
    area.disabled = locked;
    area.addEventListener("input", (event) => {
      const idx = Number.parseInt(event.target.dataset.voiceIndex, 10);
      if (isVoiceLocked(idx)) return;
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
        renderScore(lastResponse, { showDiagnostics: true, updateAudio: false });
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
      renderScore(lastResponse ?? { diagnostics: [] }, { showDiagnostics: true, updateAudio: false });
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

function renderHarmony(response) {
  if (!state.show_roman) {
    ui.harmonyCard.hidden = true;
    ui.harmonyList.innerHTML = "";
    return;
  }
  ui.harmonyCard.hidden = false;
  const slices = response.harmonic_slices ?? [];
  if (slices.length === 0) {
    ui.harmonyList.innerHTML = '<li class="harmony-item">No harmonic slices.</li>';
    return;
  }
  ui.harmonyList.innerHTML = slices
    .map((s) => {
      const label = s.roman_numeral ?? "?";
      const quality = s.quality ?? "other";
      const inversion = s.inversion ?? "unknown";
      return `<li class="harmony-item">Tick ${s.start_tick}: ${label} (${quality}, ${inversion})</li>`;
    })
    .join("");
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

async function applyAudioPlaybackUpdate(visualObj) {
  if (!ui.abcAudio) return;
  lastAudioVisualObj = visualObj;

  if (!visualObj || !window.ABCJS?.synth) {
    setAudioMessage("");
    return;
  }

  if (!supportsAbcAudio()) {
    setAudioMessage("Audio playback is unavailable in this browser.");
    return;
  }

  try {
    const controller = ensureSynthController();
    if (!controller) {
      setAudioMessage("Audio controls failed to initialize.");
      return;
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
  const availableWidth = Math.max(320, Math.floor(ui.paper.clientWidth - 20));
  const staffwidth = Math.max(260, Math.floor(availableWidth / scale));
  const preferredMeasuresPerLine = Math.max(2, Math.floor(availableWidth / 180));

  const abc = buildAbcFromVoices({
    voices: displayVoicesWithPadding(),
    presetId: state.preset_id,
    keyLabel: keyLabelByPc(keySignaturePcForMode(state.key_tonic_pc, state.mode)),
    timeSignature: state.time_signature,
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

function runAnalysisAndRender() {
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

  const response = analyzeRequest(request);
  lastResponse = response;

  renderScore(response, { showDiagnostics: true });
  renderDiagnostics(response);
  renderWarnings(response);
  renderHarmony(response);

  persistSettings();
}

function applyCantusTokens(tokens, targetVoiceIndex, opts = {}) {
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

  ui.cantusLockToggle.addEventListener("change", () => {
    state.cantus_lock_enabled = ui.cantusLockToggle.checked;
    renderCantusControls();
    renderVoiceEditors();
    renderScore(lastResponse ?? { diagnostics: [] }, { showDiagnostics: true });
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
    const m = measureUnitsEighths();
    for (const voice of state.voices) {
      appendRestToVoice(voice, m);
    }
    refreshVoiceTextsFromState();
    debounceRender();
  });

  ui.removeMeasure.addEventListener("click", () => {
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
      renderHarmony(lastResponse);
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
      const imported = importMusicXml(text, { maxVoices: 4, presetId: state.preset_id });

      state.voice_count = Math.max(1, Math.min(4, imported.voices.length));
      state.voices =
        imported.voices.length > 0
          ? imported.voices.slice(0, state.voice_count)
          : createDefaultVoices(state.voice_count, state.preset_id);
      state.preset_id = imported.preset_id ?? state.preset_id;
      state.key_tonic_pc = imported.key_tonic_pc;
      state.mode = imported.mode;
      state.time_signature = imported.time_signature;
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

  ui.insertNoteBefore.addEventListener("click", () => insertAtSelection("note", "before"));
  ui.insertNoteAfter.addEventListener("click", () => insertAtSelection("note", "after"));
  ui.insertRestBefore.addEventListener("click", () => insertAtSelection("rest", "before"));
  ui.insertRestAfter.addEventListener("click", () => insertAtSelection("rest", "after"));
  ui.deleteSelectedNote.addEventListener("click", () => deleteSelectedNote());

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
  renderCantusControls();
  rerenderSavedProfilesSelect();
  ui.ruleFilter.value = state.rule_filter;

  ui.voiceCount.value = String(state.voice_count);
  if (Number.isInteger(state.cantus_voice_index) && state.cantus_voice_index >= state.voice_count) {
    state.cantus_voice_index = null;
  }

  await initAnalyzer();
  ui.engineStatus.textContent = "Analyzer: Rust/WASM active";

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
