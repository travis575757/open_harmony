import test from "node:test";
import assert from "node:assert/strict";
import { analyzeRequest, getAnalyzerMode, initAnalyzer } from "../src/wasmClient.js";

function sampleRequest() {
  return {
    score: {
      meta: {
        exercise_count: 1,
        key_signature: { tonic_pc: 0, mode: "major" },
        time_signature: { numerator: 4, denominator: 4 },
        ticks_per_quarter: 480,
      },
      voices: [
        {
          voice_index: 0,
          name: "V1",
          notes: [
            {
              note_id: "n0",
              voice_index: 0,
              midi: 60,
              start_tick: 0,
              duration_ticks: 480,
              tie_start: false,
              tie_end: false,
            },
          ],
        },
      ],
    },
    config: {
      preset_id: "species1",
      enabled_rule_ids: [],
      disabled_rule_ids: [],
      severity_overrides: {},
      rule_params: {},
    },
  };
}

test("analyzeRequest throws when wasm analyzer is not initialized", () => {
  return assert.rejects(
    analyzeRequest(sampleRequest()),
    /FATAL: analyzer is not initialized/
  );
});

test("initAnalyzer rejects when wasm module cannot be loaded", async () => {
  await assert.rejects(initAnalyzer(), /FATAL: failed to initialize Rust\/WASM analyzer/);
  assert.equal(getAnalyzerMode(), "fatal");
});
