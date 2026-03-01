import test from "node:test";
import assert from "node:assert/strict";
import {
  buildHarmonyListMarkup,
  buildModeAwareHarmonyRows,
  disagreementDiagnosticIndexForRow,
} from "../src/harmonyView.js";

function hybridResponseFixture() {
  return {
    diagnostics: [],
    harmonic_slices: [
      {
        start_tick: 0,
        end_tick: 480,
        roman_numeral: "I",
        quality: "major",
        inversion: "root",
        confidence: 0.92,
      },
    ],
    harmonic_outputs: [
      {
        output_id: 0,
        source: "augnet_onnx",
        start_tick: 480,
        end_tick: 960,
        roman_numeral: "V/ii",
        local_key: "C",
        tonicized_key: "D",
        chord_quality: "major",
        inversion: "6/5",
        chord_label: "V65/ii",
        confidence: 0.8123,
        logits: {
          RomanNumeral31: [1.0, -1.2, 0.2, 0.8],
          LocalKey38: [0.1, 1.3],
        },
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

test("label rows are mode-aware (rule_based vs augnet)", () => {
  const response = hybridResponseFixture();
  const ruleRows = buildModeAwareHarmonyRows(response, "rule_based");
  assert.equal(ruleRows.length, 1);
  assert.equal(ruleRows[0].type, "rule_based");
  assert.equal(ruleRows[0].romanNumeral, "I");

  const augRows = buildModeAwareHarmonyRows(response, "augnet_onnx");
  assert.equal(augRows.length, 1);
  assert.equal(augRows[0].type, "augnet");
  assert.equal(augRows[0].romanNumeral, "V/ii");
  assert.equal(augRows[0].localKey, "C");
  assert.equal(augRows[0].tonicizedKey, "D");
  assert.equal(augRows[0].chordQuality, "major");
  assert.equal(augRows[0].inversion, "6/5");
  assert.match(augRows[0].confidenceSummary, /RN top1/);
});

test("disagreement mapping helper returns null in non-hybrid modes", () => {
  const rows = buildModeAwareHarmonyRows(hybridResponseFixture(), "augnet_onnx");
  assert.equal(rows[0].hasDisagreement, false);
  assert.equal(rows[0].disagreementDiagnosticIndex, null);
  assert.equal(disagreementDiagnosticIndexForRow(rows, 0), null);
});

test("raw logits stay hidden by default and appear only when debug toggle is enabled", () => {
  const response = hybridResponseFixture();
  const rowsDefault = buildModeAwareHarmonyRows(response, "augnet_onnx", { showDebugLogits: false });
  assert.equal(rowsDefault[0].logitRows.length, 0);
  const htmlDefault = buildHarmonyListMarkup(rowsDefault, "augnet_onnx");
  assert.doesNotMatch(htmlDefault, /Raw logits/);

  const rowsDebug = buildModeAwareHarmonyRows(response, "augnet_onnx", { showDebugLogits: true });
  assert.equal(rowsDebug[0].logitRows.length, 2);
  const htmlDebug = buildHarmonyListMarkup(rowsDebug, "augnet_onnx");
  assert.match(htmlDebug, /Raw logits/);
  assert.match(htmlDebug, /RomanNumeral31/);
});

test("long harmonic labels keep dedicated layout classes for stable placement", () => {
  const response = hybridResponseFixture();
  response.harmonic_outputs[0].roman_numeral = "Ger65/V/V/ii/viio42";
  response.harmonic_outputs[0].chord_label = "GERMAN-AUGMENTED-SIXTH-WITH-EXTENDED-LABEL";
  const rows = buildModeAwareHarmonyRows(response, "augnet_onnx");
  const html = buildHarmonyListMarkup(rows, "augnet_onnx");
  assert.match(html, /harmony-head/);
  assert.match(html, /harmony-meta/);
  assert.match(html, /GERMAN-AUGMENTED-SIXTH-WITH-EXTENDED-LABEL/);
});
