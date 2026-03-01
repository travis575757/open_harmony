import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const cssPath = path.resolve(__dirname, "../styles.css");
const css = fs.readFileSync(cssPath, "utf8");

test("visual regression guard: long harmony labels use wrapping-safe layout classes", () => {
  assert.match(css, /\.harmony-meta[\s\S]*overflow-wrap:\s*anywhere/);
  assert.match(css, /\.harmony-head[\s\S]*grid-template-columns:\s*minmax\(40px,\s*max-content\)\s*1fr\s*auto/);
});

test("visual regression guard: disagreement marker remains readable across zoom scales", () => {
  assert.match(css, /--oh-score-zoom:\s*1/);
  assert.match(css, /\.disagreement-pill[\s\S]*font-size:\s*clamp\(/);
  assert.match(css, /\.disagreement-pill[\s\S]*var\(--oh-score-zoom\)/);
});

