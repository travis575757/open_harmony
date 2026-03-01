import test from "node:test";
import assert from "node:assert/strict";
import {
  analysisModeUiState,
  buildAnalysisFailureUiModel,
  buildHarmonicRhythmConfig,
  persistableAnalysisSettings,
  readPersistedAnalysisSettings,
} from "../src/analysisMode.js";

test("analysis mode UI state toggles rule rhythm control and debug toggle", () => {
  assert.deepEqual(analysisModeUiState("rule_based"), {
    showRuleHarmonicRhythmControls: true,
    showAugnetAutoRhythmNote: false,
    enableAugnetDebugToggle: false,
  });
  assert.deepEqual(analysisModeUiState("augnet_onnx"), {
    showRuleHarmonicRhythmControls: false,
    showAugnetAutoRhythmNote: true,
    enableAugnetDebugToggle: true,
  });
});

test("analysis settings persistence roundtrip keeps selected backend", () => {
  const fromSaved = readPersistedAnalysisSettings({
    analysis_backend: "augnet_onnx",
    rule_harmonic_rhythm_chords_per_bar: 3,
    show_augnet_debug: true,
  });
  assert.deepEqual(fromSaved, {
    analysis_backend: "augnet_onnx",
    rule_harmonic_rhythm_chords_per_bar: 3,
    show_augnet_debug: true,
  });

  const persisted = persistableAnalysisSettings(fromSaved);
  assert.deepEqual(persisted, fromSaved);
});

test("harmonic rhythm is rule-configurable only for rule_based mode", () => {
  assert.deepEqual(buildHarmonicRhythmConfig("rule_based", 4), {
    mode: "fixed_per_bar",
    chords_per_bar: 4,
  });
  assert.deepEqual(buildHarmonicRhythmConfig("augnet_onnx", 4), { mode: "note_onset" });
});

test("AugNet backend unavailable builds fatal UI state", () => {
  const failure = buildAnalysisFailureUiModel(
    "augnet_onnx",
    new Error("selected backend augnet_onnx is unavailable: missing model"),
  );
  assert.equal(failure.fatal, true);
  assert.equal(failure.backendUnavailable, true);
  assert.match(failure.statusText, /^Fatal analysis error \(augnet_onnx\):/);
});
