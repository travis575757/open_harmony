import test from "node:test";
import assert from "node:assert/strict";
import { analysisModeUiState } from "../src/analysisMode.js";
import {
  buildModeAwareHarmonyRows,
  disagreementDiagnosticIndexForRow,
} from "../src/harmonyView.js";
import { buildAnalysisRequest } from "../src/scoreModel.js";

function baseState() {
  return {
    preset_id: "species1",
    key_tonic_pc: 0,
    mode: "major",
    analysis_backend: "rule_based",
    rule_harmonic_rhythm_chords_per_bar: 2,
    time_signature: { numerator: 4, denominator: 4 },
    voices: [
      {
        voice_index: 0,
        name: "Upper",
        notes: [
          { note_id: "v0_n0", midi: 60, is_rest: false, duration_eighths: 2, tie_start: false, tie_end: false },
          { note_id: "v0_n1", midi: 62, is_rest: false, duration_eighths: 2, tie_start: false, tie_end: false },
        ],
      },
    ],
  };
}

function responseFixture() {
  return {
    diagnostics: [],
    harmonic_slices: [
      { start_tick: 0, end_tick: 480, roman_numeral: "I", quality: "major", inversion: "root", confidence: 0.9 },
    ],
    harmonic_outputs: [
      {
        output_id: 0,
        source: "augnet_onnx",
        start_tick: 480,
        end_tick: 960,
        roman_numeral: "V",
        local_key: "C",
        tonicized_key: "C",
        chord_quality: "major",
        inversion: "root",
        chord_label: "V",
        confidence: 0.88,
        logits: { RomanNumeral31: [2.0, 0.1] },
      },
      {
        output_id: 1,
        source: "rule_based",
        start_tick: 480,
        end_tick: 960,
        roman_numeral: "ii",
      },
    ],
  };
}

const noRules = {
  enabled_rule_ids: [],
  disabled_rule_ids: [],
  severity_overrides: {},
  rule_params: {},
};

test("E2E mode switching updates request contract and harmonic UI rows", () => {
  const response = responseFixture();
  const state = baseState();

  state.analysis_backend = "rule_based";
  const reqRule = buildAnalysisRequest(state, noRules);
  assert.equal(reqRule.config.analysis_backend, "rule_based");
  assert.deepEqual(reqRule.config.harmonic_rhythm, { mode: "fixed_per_bar", chords_per_bar: 2 });
  assert.equal(analysisModeUiState(state.analysis_backend).showRuleHarmonicRhythmControls, true);
  assert.equal(buildModeAwareHarmonyRows(response, state.analysis_backend)[0].type, "rule_based");

  state.analysis_backend = "augnet_onnx";
  const reqAug = buildAnalysisRequest(state, noRules);
  assert.equal(reqAug.config.analysis_backend, "augnet_onnx");
  assert.deepEqual(reqAug.config.harmonic_rhythm, { mode: "note_onset" });
  assert.equal(analysisModeUiState(state.analysis_backend).showRuleHarmonicRhythmControls, false);
  assert.equal(buildModeAwareHarmonyRows(response, state.analysis_backend)[0].type, "augnet");
});

test("E2E note edits refresh generated request timing and harmonic rows", () => {
  const state = baseState();
  const beforeReq = buildAnalysisRequest(state, noRules);
  assert.equal(beforeReq.score.voices[0].notes[1].start_tick, 480);

  state.voices[0].notes[0].duration_eighths = 4;
  state.voices[0].notes[1].midi = 65;
  const afterReq = buildAnalysisRequest(state, noRules);
  assert.equal(afterReq.score.voices[0].notes[1].start_tick, 960);
  assert.equal(afterReq.score.voices[0].notes[1].midi, 65);

  const beforeRows = buildModeAwareHarmonyRows(
    {
      harmonic_slices: [{ start_tick: 0, end_tick: 480, roman_numeral: "I", quality: "major", inversion: "root", confidence: 0.8 }],
    },
    "rule_based",
  );
  const afterRows = buildModeAwareHarmonyRows(
    {
      harmonic_slices: [{ start_tick: 0, end_tick: 480, roman_numeral: "V", quality: "major", inversion: "first", confidence: 0.7 }],
    },
    "rule_based",
  );
  assert.equal(beforeRows[0].romanNumeral, "I");
  assert.equal(afterRows[0].romanNumeral, "V");
});

test("E2E disagreement mapping helper is inert without hybrid mode", () => {
  const response = responseFixture();
  const rows = buildModeAwareHarmonyRows(response, "augnet_onnx");
  const diagIndex = disagreementDiagnosticIndexForRow(rows, 0);
  assert.equal(diagIndex, null);
});
