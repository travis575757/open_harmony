#!/usr/bin/env node
import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import * as ort from "onnxruntime-web";
import { importMusicXml } from "../src/musicxml.js";
import { buildAnalysisRequest } from "../src/scoreModel.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const REPO_ROOT = path.resolve(__dirname, "../../..");

async function resolveInputPath(rawPath) {
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

function parseArgs(argv) {
  const out = {
    musicxml: null,
    request: null,
    output: null,
    maxRows: 256,
    presetId: "general_voice_leading",
    voiceCount: 4,
    keyPc: null,
    mode: null,
  };
  for (let i = 2; i < argv.length; i += 1) {
    const arg = argv[i];
    const next = argv[i + 1];
    if (arg === "--musicxml" && next) {
      out.musicxml = next;
      i += 1;
      continue;
    }
    if (arg === "--request" && next) {
      out.request = next;
      i += 1;
      continue;
    }
    if (arg === "--output" && next) {
      out.output = next;
      i += 1;
      continue;
    }
    if (arg === "--max-rows" && next) {
      out.maxRows = Number.parseInt(next, 10);
      i += 1;
      continue;
    }
    if (arg === "--preset-id" && next) {
      out.presetId = next;
      i += 1;
      continue;
    }
    if (arg === "--voice-count" && next) {
      out.voiceCount = Number.parseInt(next, 10);
      i += 1;
      continue;
    }
    if (arg === "--key-pc" && next) {
      out.keyPc = Number.parseInt(next, 10);
      i += 1;
      continue;
    }
    if (arg === "--mode" && next) {
      out.mode = next;
      i += 1;
      continue;
    }
    if (arg === "--help" || arg === "-h") {
      printHelp();
      process.exit(0);
    }
    throw new Error(`Unknown or incomplete argument: ${arg}`);
  }
  if (!out.musicxml && !out.request) {
    throw new Error("Provide either --musicxml <path> or --request <path>.");
  }
  return out;
}

function printHelp() {
  console.log(`Usage:
  node web/editor/scripts/augnet-debug-dump.mjs --musicxml <path> [options]
  node web/editor/scripts/augnet-debug-dump.mjs --request <analysis-request.json> [options]

Options:
  --output <path>        Write TSV dump to file (also prints to stdout).
  --max-rows <n>         Limit output rows (default: 256).
  --preset-id <id>       Preset id when building request from MusicXML (default: general_voice_leading).
  --voice-count <n>      Voices to import from MusicXML (default: 4, max 4).
  --key-pc <0..11>       Override tonic pitch class in request.
  --mode <major|minor|dorian|phrygian|lydian|mixolydian|aeolian|ionian>  Override mode.

Prerequisite:
  wasm-pack build crates/cp_wasm --target nodejs --out-dir pkg-node
`);
}

function resolveFixedTimeAxis(manifest) {
  const lengths = manifest?.signature?.fixed_time_axis_contract?.lengths;
  if (!Array.isArray(lengths) || lengths.length === 0) return null;
  const v = Number(lengths[0]);
  return Number.isFinite(v) && v > 0 ? v : null;
}

function flattenMatrix(matrix) {
  const rows = Array.isArray(matrix) ? matrix.length : 0;
  const cols = rows > 0 && Array.isArray(matrix[0]) ? matrix[0].length : 0;
  const out = new Float32Array(rows * cols);
  let ptr = 0;
  for (let r = 0; r < rows; r += 1) {
    for (let c = 0; c < cols; c += 1) {
      out[ptr] = Number(matrix[r][c] ?? 0);
      ptr += 1;
    }
  }
  return out;
}

function argmax(row) {
  let maxIdx = 0;
  let maxVal = row[0] ?? Number.NEGATIVE_INFINITY;
  for (let i = 1; i < row.length; i += 1) {
    if (row[i] > maxVal) {
      maxVal = row[i];
      maxIdx = i;
    }
  }
  return maxIdx;
}

function buildStageCArtifact(manifest, outputs, activeSteps) {
  const heads = {};
  for (const headName of manifest?.signature?.onnx_output_heads || []) {
    const out = outputs[headName];
    if (!out) {
      throw new Error(`Missing ONNX output head '${headName}'`);
    }
    const classes = Number(out.dims?.[2] ?? 0);
    const logits = [];
    const argmaxVec = [];
    for (let step = 0; step < activeSteps; step += 1) {
      const start = step * classes;
      const end = start + classes;
      const row = Array.from(out.data.slice(start, end), (x) => Number(x));
      logits.push(row);
      argmaxVec.push(argmax(row));
    }
    heads[headName] = {
      shape: [activeSteps, classes],
      logits,
      argmax: argmaxVec,
    };
  }
  return {
    schema_version: 1,
    effective_steps: activeSteps,
    heads,
  };
}

function formatDump(rows) {
  const lines = ["# backend\taugnet_onnx"];
  lines.push(
    [
      "index",
      "start_tick",
      "end_tick",
      "roman_numeral",
      "local_key",
      "tonicized_key",
      "quality",
      "inversion",
      "chord_label",
      "confidence",
    ].join("\t"),
  );
  rows.forEach((row, idx) => {
    lines.push(
      [
        idx,
        row.start_tick,
        row.end_tick,
        row.roman_numeral ?? "",
        row.local_key ?? "",
        row.tonicized_key ?? "",
        row.chord_quality ?? "",
        row.inversion ?? "",
        row.chord_label ?? "",
        row.confidence ?? "",
      ].join("\t"),
    );
  });
  return `${lines.join("\n")}\n`;
}

async function loadRequest(opts) {
  if (opts.request) {
    const requestPath = await resolveInputPath(opts.request);
    const raw = await fs.readFile(requestPath, "utf8");
    return JSON.parse(raw);
  }
  const xmlPath = await resolveInputPath(opts.musicxml);
  const xml = await fs.readFile(xmlPath, "utf8");
  const imported = importMusicXml(xml, {
    maxVoices: Math.max(1, Math.min(4, opts.voiceCount || 4)),
    presetId: opts.presetId,
  });
  const state = {
    preset_id: opts.presetId || imported.preset_id,
    key_tonic_pc:
      Number.isInteger(opts.keyPc) && opts.keyPc >= 0 ? opts.keyPc : imported.key_tonic_pc,
    mode: opts.mode || imported.mode,
    time_signature: imported.time_signature,
    voices: imported.voices,
    source_musicxml_raw: xml,
    analysis_backend: "augnet_onnx",
    rule_harmonic_rhythm_chords_per_bar: 1,
  };
  const resolvedRules = {
    enabled_rule_ids: [],
    disabled_rule_ids: [],
    severity_overrides: {},
    rule_params: {},
  };
  return buildAnalysisRequest(state, resolvedRules);
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
  return mod;
}

async function main() {
  const opts = parseArgs(process.argv);
  const request = await loadRequest(opts);
  const wasmMod = await loadNodeWasm();
  const manifest = JSON.parse(
    await fs.readFile(path.join(REPO_ROOT, "models/augnet/model-manifest.json"), "utf8"),
  );
  const fixedAxis = resolveFixedTimeAxis(manifest);

  const requestForPrep = JSON.parse(JSON.stringify(request));
  requestForPrep.config = requestForPrep.config || {};
  requestForPrep.config.augnet_backend = requestForPrep.config.augnet_backend || {};
  if (fixedAxis !== null) {
    requestForPrep.config.augnet_backend.max_steps = fixedAxis;
  }

  const prep = JSON.parse(
    wasmMod.prepare_augnet_chunks_json_wasm(JSON.stringify(requestForPrep)),
  );
  if (prep?.error) {
    throw new Error(`WASM prepare error: ${prep.error}`);
  }

  const modelPath = path.join(REPO_ROOT, "models/augnet/AugmentedNet.onnx");
  const modelBuffer = await fs.readFile(modelPath);
  ort.env.wasm.numThreads = 1;
  const session = await ort.InferenceSession.create(modelBuffer, {
    executionProviders: ["wasm"],
  });

  const harmonicOutputs = [];
  for (const chunk of prep.chunks || []) {
    const tensors = chunk.tensors;
    const feeds = {
      X_Bass19: new ort.Tensor(
        "float32",
        flattenMatrix(tensors.X_Bass19),
        [1, tensors.max_steps, 19],
      ),
      X_Chromagram19: new ort.Tensor(
        "float32",
        flattenMatrix(tensors.X_Chromagram19),
        [1, tensors.max_steps, 19],
      ),
      X_MeasureNoteOnset14: new ort.Tensor(
        "float32",
        flattenMatrix(tensors.X_MeasureNoteOnset14),
        [1, tensors.max_steps, 14],
      ),
    };
    const outputs = await session.run(feeds);
    const stageC = buildStageCArtifact(manifest, outputs, tensors.active_steps);
    const stageD = JSON.parse(
      wasmMod.decode_augnet_stage_d_json_wasm(JSON.stringify(stageC)),
    );
    if (stageD?.error) {
      throw new Error(`WASM decode error: ${stageD.error}`);
    }
    for (const label of stageD.labels || []) {
      if (Number(label?.harmonic_rhythm) !== 0) {
        continue;
      }
      const globalStep = chunk.global_start_step + label.time_index;
      const startTick = globalStep * prep.step_ticks;
      const endTick = startTick + prep.step_ticks;
      harmonicOutputs.push({
        start_tick: startTick,
        end_tick: endTick,
        roman_numeral: label.roman_numeral_formatted,
        local_key: label.local_key,
        tonicized_key: label.tonicized_key_resolved,
        chord_quality: label.chord_quality,
        inversion: label.inversion_figure,
        chord_label: label.chord_label_formatted,
        confidence: label?.component_confidence?.RomanNumeral31?.confidence_top1 ?? "",
      });
    }
  }

  harmonicOutputs.sort((a, b) => a.start_tick - b.start_tick);
  const limited = harmonicOutputs.slice(0, Math.max(1, opts.maxRows || 256));
  const dump = formatDump(limited);
  process.stdout.write(dump);
  if (opts.output) {
    await fs.writeFile(path.resolve(REPO_ROOT, opts.output), dump, "utf8");
  }
}

main().catch((err) => {
  const msg = err instanceof Error ? err.stack || err.message : String(err);
  process.stderr.write(`${msg}\n`);
  process.exit(1);
});
