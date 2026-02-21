let analyzer = null;
let analyzerMode = "initializing";

export async function initAnalyzer() {
  analyzer = null;
  analyzerMode = "initializing";

  try {
    const isLocalhost =
      typeof window !== "undefined" &&
      (window.location.hostname === "localhost" || window.location.hostname === "127.0.0.1");
    const bust = isLocalhost ? `?t=${Date.now()}` : "";
    const mod = await import(`../../../crates/cp_wasm/pkg/cp_wasm.js${bust}`);
    if (typeof mod.default !== "function") {
      throw new Error("WASM module init function is missing.");
    }
    await mod.default({ module_or_path: `../../../crates/cp_wasm/pkg/cp_wasm_bg.wasm${bust}` });
    if (typeof mod.analyze_json_wasm !== "function") {
      throw new Error("WASM analyze entrypoint is missing.");
    }

    analyzer = (request) => {
      const raw = mod.analyze_json_wasm(JSON.stringify(request));
      const parsed = JSON.parse(raw);
      if (parsed && parsed.error) {
        throw new Error(`WASM analyze error: ${parsed.error}`);
      }
      return parsed;
    };
    analyzerMode = "wasm";
    return analyzerMode;
  } catch (err) {
    analyzerMode = "fatal";
    const msg = err instanceof Error ? err.message : String(err);
    throw new Error(`FATAL: failed to initialize Rust/WASM analyzer: ${msg}`);
  }
}

export function getAnalyzerMode() {
  return analyzerMode;
}

export function analyzeRequest(request) {
  if (!analyzer) {
    throw new Error("FATAL: analyzer is not initialized. WASM module is required.");
  }
  return analyzer(request);
}
