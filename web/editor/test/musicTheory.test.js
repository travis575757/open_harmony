import test from "node:test";
import assert from "node:assert/strict";
import {
  keySignaturePcForMode,
  isSupportedTimeSignature,
  quantizeMidiToScale,
  supportedTimeSignaturesForPreset,
} from "../src/musicTheory.js";

test("species5 time signatures are restricted", () => {
  const list = supportedTimeSignaturesForPreset("species5");
  const labels = list.map((ts) => `${ts.numerator}/${ts.denominator}`);
  assert.deepEqual(labels, ["4/4", "2/2"]);
  assert.equal(isSupportedTimeSignature(4, 4, "species5"), true);
  assert.equal(isSupportedTimeSignature(3, 4, "species5"), false);
});

test("non-species5 presets use full supported list", () => {
  const list = supportedTimeSignaturesForPreset("species1");
  assert.ok(list.some((ts) => ts.numerator === 3 && ts.denominator === 4));
  assert.ok(list.some((ts) => ts.numerator === 6 && ts.denominator === 4));
});

test("scale quantizer respects direction on ties", () => {
  assert.equal(quantizeMidiToScale(61, 0, "major", 1), 62);
  assert.equal(quantizeMidiToScale(61, 0, "major", -1), 60);
  assert.equal(quantizeMidiToScale(64, 0, "major", 1), 64);
});

test("mode key-signature pc follows relative major mapping", () => {
  assert.equal(keySignaturePcForMode(9, "minor"), 0); // A minor -> C major signature
  assert.equal(keySignaturePcForMode(2, "dorian"), 0); // D dorian -> C major signature
  assert.equal(keySignaturePcForMode(7, "mixolydian"), 0); // G mixolydian -> C major signature
});
