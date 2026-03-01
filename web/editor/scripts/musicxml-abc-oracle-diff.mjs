#!/usr/bin/env node
import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import abcjs from "abcjs";
import { buildAbcFromVoices } from "../src/abcNotation.js";
import { keyLabelByPc } from "../src/scoreModel.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const REPO_ROOT = path.resolve(__dirname, "../../..");

const DURATION_STEP_EIGHTHS = 0.25;
const EPS = 1e-6;

function quantizeEighths(v) {
  const n = Number(v);
  if (!Number.isFinite(n)) return 0;
  return Math.round(n / DURATION_STEP_EIGHTHS) * DURATION_STEP_EIGHTHS;
}

function formatLoc(startEighths, meter) {
  const units = Math.max(1, (meter.numerator * 8) / meter.denominator);
  const measure = Math.floor(startEighths / units) + 1;
  const inMeasure = startEighths - (measure - 1) * units;
  const beatLen = 8 / meter.denominator;
  const beat = inMeasure / beatLen + 1;
  const beatStr = Number.isInteger(beat) ? String(beat) : beat.toFixed(2).replace(/\.?0+$/, "");
  return `m${measure} b${beatStr}`;
}

function parseArgs(argv) {
  const out = {
    musicxml: null,
    presetId: "general_voice_leading",
    maxVoices: 4,
    writeAbcPath: null,
    showAll: false,
  };
  for (let i = 2; i < argv.length; i += 1) {
    const arg = argv[i];
    const next = argv[i + 1];
    if (arg === "--musicxml" && next) {
      out.musicxml = next;
      i += 1;
      continue;
    }
    if (arg === "--preset-id" && next) {
      out.presetId = next;
      i += 1;
      continue;
    }
    if (arg === "--max-voices" && next) {
      out.maxVoices = Math.max(1, Math.min(4, Number.parseInt(next, 10) || 4));
      i += 1;
      continue;
    }
    if (arg === "--write-abc" && next) {
      out.writeAbcPath = next;
      i += 1;
      continue;
    }
    if (arg === "--show-all") {
      out.showAll = true;
      continue;
    }
    if (arg === "--help" || arg === "-h") {
      printHelp();
      process.exit(0);
    }
    throw new Error(`Unknown or incomplete argument: ${arg}`);
  }
  if (!out.musicxml) {
    throw new Error("Missing required --musicxml <path>.");
  }
  return out;
}

function printHelp() {
  console.log(`Usage:
  node web/editor/scripts/musicxml-abc-oracle-diff.mjs --musicxml <path> [options]

Options:
  --preset-id <id>      Preset id passed to importer (default: general_voice_leading)
  --max-voices <1..4>   Max voices imported (default: 4)
  --write-abc <path>    Write generated ABC to file for inspection
  --show-all            Print all mismatches (default prints first 20)

Prerequisite:
  wasm-pack build crates/cp_wasm --target nodejs --out-dir pkg-node
`);
}

async function resolvePath(rawPath) {
  if (!rawPath) return null;
  if (path.isAbsolute(rawPath)) return rawPath;
  const cwdCandidate = path.resolve(process.cwd(), rawPath);
  try {
    await fs.access(cwdCandidate);
    return cwdCandidate;
  } catch {
    return path.resolve(REPO_ROOT, rawPath);
  }
}

async function loadNodeWasm() {
  const pkgDir = path.join(REPO_ROOT, "crates/cp_wasm/pkg-node");
  const jsPath = path.join(pkgDir, "cp_wasm.js");
  const wasmPath = path.join(pkgDir, "cp_wasm_bg.wasm");
  try {
    await fs.access(jsPath);
    await fs.access(wasmPath);
  } catch {
    throw new Error(
      "Missing Node WASM package. Run: wasm-pack build crates/cp_wasm --target nodejs --out-dir pkg-node",
    );
  }
  const mod = await import(pathToFileURL(jsPath).href);
  if (typeof mod.default === "function") {
    await mod.default(pathToFileURL(wasmPath).href);
  }
  if (typeof mod.import_musicxml_json_wasm !== "function") {
    throw new Error("Node WASM package is missing import_musicxml_json_wasm export.");
  }
  return mod;
}

function parseAbcVoices(abcText) {
  const tune = abcjs.parseOnly(abcText)[0];
  if (!tune) return [];
  const voices = [];
  const cursors = [];

  for (const line of tune.lines || []) {
    if (!line?.staff) continue;
    for (let staffIndex = 0; staffIndex < line.staff.length; staffIndex += 1) {
      const staff = line.staff[staffIndex];
      if (!voices[staffIndex]) voices[staffIndex] = [];
      if (!Number.isFinite(cursors[staffIndex])) cursors[staffIndex] = 0;
      const elements = staff?.voices?.[0] || [];
      for (const el of elements) {
        if (el?.el_type !== "note") continue;
        const duration = quantizeEighths((Number(el.duration) || 0) * 8);
        if (duration <= EPS) continue;
        voices[staffIndex].push({
          start_eighths: quantizeEighths(cursors[staffIndex]),
          duration_eighths: duration,
          is_rest: !!el.rest,
        });
        cursors[staffIndex] = quantizeEighths(cursors[staffIndex] + duration);
      }
    }
  }
  return voices;
}

function normalizeImportedVoice(voice) {
  return (voice.notes || [])
    .map((n) => ({
      start_eighths: quantizeEighths(n.start_eighths),
      duration_eighths: quantizeEighths(n.duration_eighths),
      is_rest: !!n.is_rest,
    }))
    .sort((a, b) => a.start_eighths - b.start_eighths || Number(a.is_rest) - Number(b.is_rest));
}

function compareVoices(imported, parsed, meter) {
  const mismatches = [];
  const importedSounding = imported.filter((n) => !n.is_rest);
  const parsedSounding = parsed.filter((n) => !n.is_rest);
  if (importedSounding.length !== parsedSounding.length) {
    mismatches.push(
      `sounding-note-count mismatch: imported=${importedSounding.length} parsed=${parsedSounding.length}`,
    );
  }
  const limit = Math.min(importedSounding.length, parsedSounding.length);
  for (let i = 0; i < limit; i += 1) {
    const a = importedSounding[i];
    const b = parsedSounding[i];
    if (Math.abs(a.start_eighths - b.start_eighths) > EPS) {
      mismatches.push(
        `start mismatch @idx ${i}: imported=${a.start_eighths} (${formatLoc(
          a.start_eighths,
          meter,
        )}) parsed=${b.start_eighths} (${formatLoc(b.start_eighths, meter)})`,
      );
    }
    if (Math.abs(a.duration_eighths - b.duration_eighths) > EPS) {
      mismatches.push(
        `duration mismatch @idx ${i}: imported=${a.duration_eighths} parsed=${b.duration_eighths}`,
      );
    }
  }

  const importedEnd = imported.reduce((acc, n) => Math.max(acc, n.start_eighths + n.duration_eighths), 0);
  const parsedEnd = parsed.reduce((acc, n) => Math.max(acc, n.start_eighths + n.duration_eighths), 0);
  if (Math.abs(importedEnd - parsedEnd) > EPS) {
    mismatches.push(`voice total span mismatch: imported=${importedEnd} parsed=${parsedEnd}`);
  }
  return mismatches;
}

async function main() {
  const opts = parseArgs(process.argv);
  const xmlPath = await resolvePath(opts.musicxml);
  const xml = await fs.readFile(xmlPath, "utf8");
  const wasm = await loadNodeWasm();

  const imported = JSON.parse(
    wasm.import_musicxml_json_wasm(
      JSON.stringify({
        xml_text: xml,
        max_voices: opts.maxVoices,
        preset_id: opts.presetId,
      }),
    ),
  );
  if (imported?.error) {
    throw new Error(`WASM import error: ${imported.error}`);
  }

  const abc = buildAbcFromVoices({
    voices: imported.voices,
    presetId: imported.preset_id,
    keyLabel: keyLabelByPc(imported.key_tonic_pc),
    timeSignature: imported.time_signature,
    pickupEighths: imported.pickup_eighths,
    showBarNumbers: false,
  });

  if (opts.writeAbcPath) {
    const outPath = path.isAbsolute(opts.writeAbcPath)
      ? opts.writeAbcPath
      : path.resolve(process.cwd(), opts.writeAbcPath);
    await fs.writeFile(outPath, abc, "utf8");
    console.log(`Wrote ABC: ${outPath}`);
  }

  const parsedVoices = parseAbcVoices(abc);
  let mismatchCount = 0;
  let shown = 0;
  const showLimit = opts.showAll ? Number.POSITIVE_INFINITY : 20;

  for (let i = 0; i < imported.voices.length; i += 1) {
    const importedVoice = normalizeImportedVoice(imported.voices[i]);
    const parsedVoice = parsedVoices[i] || [];
    const mismatches = compareVoices(importedVoice, parsedVoice, imported.time_signature);
    if (mismatches.length > 0) {
      mismatchCount += mismatches.length;
      if (shown < showLimit) {
        console.log(`\nVoice ${i + 1} mismatches:`);
        for (const m of mismatches) {
          if (shown >= showLimit) break;
          console.log(`  - ${m}`);
          shown += 1;
        }
      }
    }
  }

  if (mismatchCount > 0) {
    if (!opts.showAll && mismatchCount > shown) {
      console.log(`\n... ${mismatchCount - shown} additional mismatches hidden (use --show-all).`);
    }
    console.error(`\nFAILED: found ${mismatchCount} timing mismatches between imported timeline and parsed ABC.`);
    process.exit(1);
  }

  console.log(
    `OK: parsed ABC timing matches imported timeline for ${imported.voices.length} voices (${path.basename(
      xmlPath,
    )}).`,
  );
}

main().catch((err) => {
  console.error(err instanceof Error ? err.message : String(err));
  process.exit(1);
});
