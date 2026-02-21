import test from "node:test";
import assert from "node:assert/strict";
import { buildAnalysisRequest } from "../src/scoreModel.js";

test("buildAnalysisRequest advances through rests but does not emit rest events", () => {
  const state = {
    preset_id: "species1",
    key_tonic_pc: 0,
    mode: "major",
    time_signature: { numerator: 4, denominator: 4 },
    voices: [
      {
        voice_index: 0,
        name: "Upper",
        notes: [
          { note_id: "v0_n0", midi: 60, is_rest: false, duration_eighths: 2, tie_start: false, tie_end: false },
          { note_id: "v0_n1", midi: 60, is_rest: true, duration_eighths: 2, tie_start: false, tie_end: false },
          { note_id: "v0_n2", midi: 62, is_rest: false, duration_eighths: 2, tie_start: false, tie_end: false },
        ],
      },
    ],
  };

  const req = buildAnalysisRequest(state, {
    enabled_rule_ids: [],
    disabled_rule_ids: [],
    severity_overrides: {},
    rule_params: {},
  });

  const notes = req.score.voices[0].notes;
  assert.equal(notes.length, 2);
  assert.equal(notes[0].note_id, "v0_n0");
  assert.equal(notes[0].start_tick, 0);
  assert.equal(notes[1].note_id, "v0_n2");
  assert.equal(notes[1].start_tick, 960);
  assert.deepEqual(req.config.harmonic_rhythm, { mode: "fixed_per_bar", chords_per_bar: 1 });
});

test("buildAnalysisRequest preserves UI voice order", () => {
  const state = {
    preset_id: "species2",
    key_tonic_pc: 0,
    mode: "major",
    time_signature: { numerator: 4, denominator: 4 },
    cantus_voice_index: 0,
    voices: [
      {
        voice_index: 0,
        name: "Upper (CF)",
        notes: [
          { note_id: "v0_n0", midi: 62, is_rest: false, duration_eighths: 8, tie_start: false, tie_end: false },
          { note_id: "v0_n1", midi: 64, is_rest: false, duration_eighths: 8, tie_start: false, tie_end: false },
        ],
      },
      {
        voice_index: 1,
        name: "Lower (CP)",
        notes: [
          { note_id: "v1_n0", midi: 50, is_rest: false, duration_eighths: 4, tie_start: false, tie_end: false },
          { note_id: "v1_n1", midi: 52, is_rest: false, duration_eighths: 4, tie_start: false, tie_end: false },
          { note_id: "v1_n2", midi: 53, is_rest: false, duration_eighths: 4, tie_start: false, tie_end: false },
          { note_id: "v1_n3", midi: 55, is_rest: false, duration_eighths: 4, tie_start: false, tie_end: false },
        ],
      },
    ],
  };

  const req = buildAnalysisRequest(state, {
    enabled_rule_ids: [],
    disabled_rule_ids: [],
    severity_overrides: {},
    rule_params: {},
  });

  assert.equal(req.score.voices.length, 2);
  assert.equal(req.score.voices[0].name, "Upper (CF)");
  assert.equal(req.score.voices[1].name, "Lower (CP)");
  assert.equal(req.score.voices[0].notes.length, 2);
  assert.equal(req.score.voices[1].notes.length, 4);
});
