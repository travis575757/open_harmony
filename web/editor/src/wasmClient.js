let analyzer = null;
let analyzerMode = "initializing";
let wasmMod = null;
let ortRuntimePromise = null;
let augnetSessionPromise = null;
let augnetManifestPromise = null;

const ORT_CDN_URL = "https://cdn.jsdelivr.net/npm/onnxruntime-web@1.20.1/dist/ort.min.js";
const MODEL_URL = new URL("../../../models/augnet/AugmentedNet.onnx", import.meta.url);
const MANIFEST_URL = new URL("../../../models/augnet/model-manifest.json", import.meta.url);

function parseWasmJson(raw, context) {
  const parsed = JSON.parse(raw);
  if (parsed && parsed.error) {
    const msg = String(parsed.error || "");
    if (
      msg.includes("unknown variant `relaxed`") ||
      msg.includes("unknown variant `moderate_classical`")
    ) {
      throw new Error(
        `WASM ${context} error: ${msg}. ` +
          "This usually means your WASM package is stale. Rebuild with: " +
          "`wasm-pack build crates/cp_wasm --target web --out-dir pkg`."
      );
    }
    throw new Error(`WASM ${context} error: ${msg}`);
  }
  return parsed;
}

function cloneRequest(request) {
  return JSON.parse(JSON.stringify(request));
}

function normalizeBackend(request) {
  return request?.config?.analysis_backend || "rule_based";
}

function loadOrtRuntime() {
  if (typeof window === "undefined") {
    return Promise.reject(new Error("window is unavailable for onnxruntime-web"));
  }
  if (window.ort) {
    return Promise.resolve(window.ort);
  }
  if (!ortRuntimePromise) {
    ortRuntimePromise = new Promise((resolve, reject) => {
      const existing = document.querySelector("script[data-ort-runtime='1']");
      if (existing) {
        existing.addEventListener("load", () => resolve(window.ort), { once: true });
        existing.addEventListener(
          "error",
          () => reject(new Error(`failed loading ${ORT_CDN_URL}`)),
          { once: true }
        );
        return;
      }
      const script = document.createElement("script");
      script.src = ORT_CDN_URL;
      script.async = true;
      script.dataset.ortRuntime = "1";
      script.onload = () => {
        if (!window.ort) {
          reject(new Error("onnxruntime-web loaded but window.ort is undefined"));
          return;
        }
        resolve(window.ort);
      };
      script.onerror = () => reject(new Error(`failed loading ${ORT_CDN_URL}`));
      document.head.appendChild(script);
    });
  }
  return ortRuntimePromise;
}

async function loadAugnetManifest() {
  if (!augnetManifestPromise) {
    const url = MANIFEST_URL.toString();
    augnetManifestPromise = fetch(url).then((res) => {
      if (!res.ok) {
        throw new Error(`Failed to load model manifest (${res.status}) at ${url}`);
      }
      return res.json();
    });
  }
  return augnetManifestPromise;
}

async function loadAugnetSession() {
  if (!augnetSessionPromise) {
    augnetSessionPromise = (async () => {
      const ort = await loadOrtRuntime();
      if (ort?.env?.wasm) {
        ort.env.wasm.numThreads = 1;
      }
      const url = MODEL_URL.toString();
      try {
        return await ort.InferenceSession.create(url, {
          executionProviders: ["wasm"],
        });
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        throw new Error(`Failed to create AugNet ONNX session from ${url}: ${msg}`);
      }
    })();
  }
  return augnetSessionPromise;
}

function resolveFixedTimeAxis(manifest) {
  const lengths = manifest?.signature?.fixed_time_axis_contract?.lengths;
  if (!Array.isArray(lengths) || lengths.length === 0) {
    return null;
  }
  const first = Number(lengths[0]);
  return Number.isFinite(first) && first > 0 ? first : null;
}

function flattenMatrix(matrix) {
  const rows = Array.isArray(matrix) ? matrix.length : 0;
  const cols = rows > 0 && Array.isArray(matrix[0]) ? matrix[0].length : 0;
  const out = new Float32Array(rows * cols);
  let ptr = 0;
  for (let r = 0; r < rows; r += 1) {
    const row = matrix[r];
    for (let c = 0; c < cols; c += 1) {
      out[ptr] = Number(row[c] ?? 0);
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

function m21NameToPc(name) {
  if (!name || typeof name !== "string") return null;
  const m = name.match(/^([A-Ga-g])([#-]*)(-?\d+)?$/);
  if (!m) return null;
  const base = { C: 0, D: 2, E: 4, F: 5, G: 7, A: 9, B: 11 }[m[1].toUpperCase()];
  let acc = 0;
  for (const ch of m[2] || "") {
    if (ch === "#") acc += 1;
    if (ch === "-") acc -= 1;
  }
  return ((base + acc) % 12 + 12) % 12;
}

function runRuleBasedSync(request) {
  if (!analyzer) {
    throw new Error("FATAL: analyzer is not initialized. WASM module is required.");
  }
  return analyzer(request);
}

function buildAugnetInputFeeds(ort, chunk) {
  const tensors = chunk.tensors;
  return {
    X_Bass19: new ort.Tensor("float32", flattenMatrix(tensors.X_Bass19), [1, tensors.max_steps, 19]),
    X_Chromagram19: new ort.Tensor(
      "float32",
      flattenMatrix(tensors.X_Chromagram19),
      [1, tensors.max_steps, 19]
    ),
    X_MeasureNoteOnset14: new ort.Tensor(
      "float32",
      flattenMatrix(tensors.X_MeasureNoteOnset14),
      [1, tensors.max_steps, 14]
    ),
  };
}

function buildStageCArtifact(manifest, outputs, activeSteps) {
  const heads = {};
  const ordered = manifest?.signature?.onnx_output_heads || [];
  for (const headName of ordered) {
    const out = outputs[headName];
    if (!out) {
      throw new Error(`Missing output head '${headName}' from onnxruntime-web session`);
    }
    const dims = out.dims || [];
    if (dims.length !== 3 || dims[0] !== 1) {
      throw new Error(`Unexpected output shape for '${headName}': [${dims.join(",")}]`);
    }
    const classes = Number(dims[2]);
    const logits = [];
    const argmaxVec = [];
    const data = out.data;
    for (let step = 0; step < activeSteps; step += 1) {
      const start = step * classes;
      const end = start + classes;
      const row = Array.from(data.slice(start, end), (x) => Number(x));
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

async function runAugnetPipeline(request) {
  if (!wasmMod || typeof wasmMod.prepare_augnet_chunks_json_wasm !== "function") {
    throw new Error("WASM AugNet preprocessing export is missing.");
  }
  if (!wasmMod || typeof wasmMod.decode_augnet_stage_d_json_wasm !== "function") {
    throw new Error("WASM AugNet postprocessing export is missing.");
  }

  const manifest = await loadAugnetManifest();
  const fixedAxis = resolveFixedTimeAxis(manifest);
  const requestForPrep = cloneRequest(request);
  requestForPrep.config = requestForPrep.config || {};
  requestForPrep.config.augnet_backend = requestForPrep.config.augnet_backend || {};
  if (fixedAxis !== null) {
    requestForPrep.config.augnet_backend.max_steps = fixedAxis;
  }
  const prepWithAxis = parseWasmJson(
    wasmMod.prepare_augnet_chunks_json_wasm(JSON.stringify(requestForPrep)),
    "prepare_augnet_chunks_json"
  );
  const session = await loadAugnetSession();
  const ort = await loadOrtRuntime();

  const harmonic_outputs = [];
  const harmonic_slices = [];

  for (const chunk of prepWithAxis.chunks || []) {
    if (
      fixedAxis !== null &&
      Number(chunk?.tensors?.max_steps || 0) !== fixedAxis
    ) {
      throw new Error(
        `AugNet preprocess max_steps ${chunk?.tensors?.max_steps} does not match model fixed axis ${fixedAxis}`
      );
    }
    const feeds = buildAugnetInputFeeds(ort, chunk);
    const outputs = await session.run(feeds);
    const stageC = buildStageCArtifact(manifest, outputs, chunk.tensors.active_steps);
    const stageD = parseWasmJson(
      wasmMod.decode_augnet_stage_d_json_wasm(JSON.stringify(stageC)),
      "decode_augnet_stage_d_json"
    );

    for (const label of stageD.labels || []) {
      // Match AugmentedNet solveChordSegmentation: emit only harmonic-change frames.
      if (Number(label?.harmonic_rhythm) !== 0) {
        continue;
      }
      const globalStep = chunk.global_start_step + label.time_index;
      const startTick = globalStep * prepWithAxis.step_ticks;
      const endTick = startTick + prepWithAxis.step_ticks;
      const logits = {};
      for (const [headName, headData] of Object.entries(stageD.heads || {})) {
        if (Array.isArray(headData.raw_logits) && headData.raw_logits[label.time_index]) {
          logits[headName] = headData.raw_logits[label.time_index];
        }
      }
      const confidence = label?.component_confidence?.RomanNumeral31?.confidence_top1 ?? null;
      harmonic_outputs.push({
        output_id: harmonic_outputs.length,
        start_tick: startTick,
        end_tick: endTick,
        source: "augnet_onnx",
        roman_numeral: label.roman_numeral_formatted,
        local_key: label.local_key,
        tonicized_key: label.tonicized_key_resolved,
        chord_quality: label.chord_quality,
        inversion: label.inversion_figure,
        chord_label: label.chord_label_formatted,
        confidence,
        logits,
      });
      harmonic_slices.push({
        slice_id: harmonic_slices.length,
        start_tick: startTick,
        end_tick: endTick,
        pitch_classes: label.pitch_class_set_resolved,
        root_pc: m21NameToPc(label.chord_root),
        quality: label.chord_quality,
        inversion: label.inversion_figure,
        roman_numeral: label.roman_numeral_formatted,
        confidence: Number(confidence || 0),
        inferred_root: null,
        missing_tones: [],
        chord_form: label.chord_label_formatted,
      });
    }
  }

  harmonic_outputs.sort((a, b) => (a.start_tick - b.start_tick) || (a.output_id - b.output_id));
  harmonic_slices.sort((a, b) => (a.start_tick - b.start_tick) || (a.slice_id - b.slice_id));

  return { harmonic_outputs, harmonic_slices, warnings: [] };
}

function buildRuleRequest(request) {
  const out = cloneRequest(request);
  out.config = out.config || {};
  out.config.analysis_backend = "rule_based";
  return out;
}

function ruleCheckerEnabled(request) {
  return request?.config?.rule_checker_enabled !== false;
}

export async function initAnalyzer() {
  analyzer = null;
  analyzerMode = "initializing";
  wasmMod = null;

  try {
    const isLocalhost =
      typeof window !== "undefined" &&
      (window.location.hostname === "localhost" || window.location.hostname === "127.0.0.1");
    const bust = isLocalhost ? `${Date.now()}` : "";
    const moduleUrl = new URL("../../../crates/cp_wasm/pkg/cp_wasm.js", import.meta.url);
    const wasmUrl = new URL("../../../crates/cp_wasm/pkg/cp_wasm_bg.wasm", import.meta.url);
    if (bust) {
      moduleUrl.searchParams.set("t", bust);
      wasmUrl.searchParams.set("t", bust);
    }
    wasmMod = await import(moduleUrl.toString());
    if (typeof wasmMod.default !== "function") {
      throw new Error("WASM module init function is missing.");
    }
    await wasmMod.default({ module_or_path: wasmUrl.toString() });
    if (typeof wasmMod.analyze_json_wasm !== "function") {
      throw new Error("WASM analyze entrypoint is missing.");
    }

    analyzer = (request) => parseWasmJson(wasmMod.analyze_json_wasm(JSON.stringify(request)), "analyze");
    analyzerMode = "wasm";
    return analyzerMode;
  } catch (err) {
    analyzerMode = "fatal";
    const msg = err instanceof Error ? err.message : String(err);
    throw new Error(
      `FATAL: failed to initialize Rust/WASM analyzer: ${msg}. ` +
        "Expected server root: repository root with crates/cp_wasm/pkg built via wasm-pack."
    );
  }
}

export function getAnalyzerMode() {
  return analyzerMode;
}

export async function importMusicXmlWithWasm(xmlText, opts = {}) {
  if (!wasmMod || typeof wasmMod.import_musicxml_json_wasm !== "function") {
    throw new Error("WASM MusicXML import entrypoint is missing.");
  }
  const payload = {
    xml_text: String(xmlText || ""),
    max_voices: Number.isFinite(opts.maxVoices) ? Math.max(1, Math.min(4, Number(opts.maxVoices))) : 4,
    preset_id: typeof opts.presetId === "string" && opts.presetId.trim() ? opts.presetId.trim() : "species1",
  };
  return parseWasmJson(
    wasmMod.import_musicxml_json_wasm(JSON.stringify(payload)),
    "import_musicxml_json"
  );
}

export async function analyzeRequest(request) {
  const backend = normalizeBackend(request);
  const includeRuleDiagnostics = ruleCheckerEnabled(request);
  const rule = includeRuleDiagnostics ? runRuleBasedSync(buildRuleRequest(request)) : null;
  if (backend === "rule_based") {
    if (rule) return rule;
    const raw = runRuleBasedSync(buildRuleRequest(request));
    return {
      ...raw,
      diagnostics: [],
      nct_tags: [],
      summary: {
        total_diagnostics: 0,
        error_count: 0,
        warning_count: 0,
        active_rule_count: 0,
      },
      warnings: [],
    };
  }

  if (backend === "augnet_onnx") {
    const augnet = await runAugnetPipeline(request);
    return {
      diagnostics: includeRuleDiagnostics ? rule?.diagnostics || [] : [],
      harmonic_slices: augnet.harmonic_slices,
      harmonic_outputs: augnet.harmonic_outputs,
      nct_tags: includeRuleDiagnostics ? rule?.nct_tags || [] : [],
      summary: includeRuleDiagnostics
        ? rule?.summary || {
            total_diagnostics: 0,
            error_count: 0,
            warning_count: 0,
            active_rule_count: 0,
          }
        : {
            total_diagnostics: 0,
            error_count: 0,
            warning_count: 0,
            active_rule_count: 0,
          },
      warnings: includeRuleDiagnostics
        ? [...(rule?.warnings || []), ...(augnet.warnings || [])]
        : [...(augnet.warnings || [])],
    };
  }

  throw new Error(`Unsupported analysis backend: ${backend}`);
}
