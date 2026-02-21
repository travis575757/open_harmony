import test from "node:test";
import assert from "node:assert/strict";
import {
  abcTokenToMidi,
  buildAbcFromVoices,
  midiToAbcToken,
  notesToVoiceText,
  parseVoiceText,
  speciesDefaultDurationEighths,
  validateMeterFit,
} from "../src/abcNotation.js";

test("species defaults are stable", () => {
  assert.equal(speciesDefaultDurationEighths("species1"), 8);
  assert.equal(speciesDefaultDurationEighths("species2"), 4);
  assert.equal(speciesDefaultDurationEighths("species3"), 2);
  assert.equal(speciesDefaultDurationEighths("general_voice_leading"), 2);
});

test("ABC pitch conversion maps basic octaves", () => {
  assert.equal(abcTokenToMidi("C"), 60);
  assert.equal(abcTokenToMidi("c"), 72);
  assert.equal(abcTokenToMidi("C,"), 48);
  assert.equal(abcTokenToMidi("^F"), 66);

  assert.equal(midiToAbcToken(60), "C");
  assert.equal(midiToAbcToken(72), "c");
  assert.equal(midiToAbcToken(48), "C,");
});

test("parse voice text handles durations and ties", () => {
  const parsed = parseVoiceText("C8 D4 E2 F- F", { defaultDurationEighths: 2 });
  assert.equal(parsed.errors.length, 0);
  assert.equal(parsed.notes.length, 5);
  assert.equal(parsed.notes[0].duration_eighths, 8);
  assert.equal(parsed.notes[1].duration_eighths, 4);
  assert.equal(parsed.notes[2].duration_eighths, 2);
  assert.equal(parsed.notes[3].tie_start, true);
  assert.equal(parsed.notes[4].tie_end, true);
});

test("parse and serialize rests", () => {
  const parsed = parseVoiceText("C4 z2 D2 z4", { defaultDurationEighths: 2 });
  assert.equal(parsed.errors.length, 0);
  assert.equal(parsed.notes.length, 4);
  assert.equal(parsed.notes[1].is_rest, true);
  assert.equal(parsed.notes[3].is_rest, true);
  const txt = notesToVoiceText(parsed.notes, 2);
  assert.match(txt, /^C4 z2 D2 z4$/);
});

test("notes to text and ABC build produce staff blocks", () => {
  const voice0 = {
    voice_index: 0,
    name: "Soprano",
    notes: [
      { midi: 72, duration_eighths: 2, tie_start: false },
      { midi: 74, duration_eighths: 2, tie_start: false },
    ],
  };
  const voice1 = {
    voice_index: 1,
    name: "Bass",
    notes: [
      { midi: 48, duration_eighths: 2, tie_start: false },
      { midi: 50, duration_eighths: 2, tie_start: false },
    ],
  };

  const txt = notesToVoiceText(voice0.notes, 2);
  assert.match(txt, /^c2 d2$/);

  const abc = buildAbcFromVoices({
    voices: [voice0, voice1],
    presetId: "general_voice_leading",
    keyLabel: "C",
    timeSignature: { numerator: 4, denominator: 4 },
    showBarNumbers: false,
  });

  assert.match(abc, /V:1 clef=treble/);
  assert.match(abc, /V:2 clef=bass/);
  assert.match(abc, /K:C/);
});

test("meter fit reports overfill and renderer inserts bar before overflow", () => {
  const voice = {
    voice_index: 0,
    name: "V1",
    notes: [
      { midi: 60, is_rest: false, duration_eighths: 6, tie_start: false },
      { midi: 62, is_rest: false, duration_eighths: 6, tie_start: false },
    ],
  };
  const issues = validateMeterFit(voice.notes, { numerator: 4, denominator: 4 });
  assert.equal(issues.length, 1);
  const abc = buildAbcFromVoices({
    voices: [voice],
    presetId: "general_voice_leading",
    keyLabel: "C",
    timeSignature: { numerator: 4, denominator: 4 },
    showBarNumbers: false,
  });
  assert.match(abc, /C6 \| D6/);
});

test("bar number directive toggles", () => {
  const voice = {
    voice_index: 0,
    name: "V1",
    notes: [{ midi: 60, is_rest: false, duration_eighths: 2, tie_start: false }],
  };
  const withBars = buildAbcFromVoices({
    voices: [voice],
    presetId: "species1",
    keyLabel: "C",
    timeSignature: { numerator: 4, denominator: 4 },
    showBarNumbers: true,
  });
  const withoutBars = buildAbcFromVoices({
    voices: [voice],
    presetId: "species1",
    keyLabel: "C",
    timeSignature: { numerator: 4, denominator: 4 },
    showBarNumbers: false,
  });
  assert.match(withBars, /%%barnumbers 1/);
  assert.doesNotMatch(withoutBars, /%%barnumbers 1/);
});
