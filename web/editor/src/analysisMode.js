export const DEFAULT_ANALYSIS_BACKEND = "rule_based";
export const DEFAULT_RULE_BASED_CHORDS_PER_BAR = 1;

const VALID_BACKENDS = new Set(["rule_based", "augnet_onnx"]);

export function normalizeAnalysisBackend(value) {
  if (typeof value !== "string") return DEFAULT_ANALYSIS_BACKEND;
  return VALID_BACKENDS.has(value) ? value : DEFAULT_ANALYSIS_BACKEND;
}

export function isAugmentedNetBackend(backend) {
  return backend === "augnet_onnx";
}

export function normalizeRuleBasedChordsPerBar(value) {
  const parsed = Number.parseInt(value, 10);
  if (!Number.isInteger(parsed)) return DEFAULT_RULE_BASED_CHORDS_PER_BAR;
  return Math.max(1, Math.min(4, parsed));
}

export function buildHarmonicRhythmConfig(analysisBackend, ruleBasedChordsPerBar) {
  if (normalizeAnalysisBackend(analysisBackend) === "rule_based") {
    return {
      mode: "fixed_per_bar",
      chords_per_bar: normalizeRuleBasedChordsPerBar(ruleBasedChordsPerBar),
    };
  }
  // AugmentedNet determines segmentation for harmony identification.
  return { mode: "note_onset" };
}

export function analysisModeUiState(analysisBackend) {
  const augnetMode = isAugmentedNetBackend(normalizeAnalysisBackend(analysisBackend));
  return {
    showRuleHarmonicRhythmControls: !augnetMode,
    showAugnetAutoRhythmNote: augnetMode,
    enableAugnetDebugToggle: augnetMode,
  };
}

export function analysisErrorMessage(error) {
  if (error instanceof Error) return error.message;
  return String(error);
}

export function isBackendUnavailableError(analysisBackend, errorMessage) {
  if (!isAugmentedNetBackend(analysisBackend)) return false;
  const msg = String(errorMessage || "");
  return msg.includes(`selected backend ${analysisBackend} is unavailable`);
}

export function buildAnalysisFailureUiModel(analysisBackend, error) {
  const backend = normalizeAnalysisBackend(analysisBackend);
  const message = analysisErrorMessage(error);
  const backendUnavailable = isBackendUnavailableError(backend, message);
  const fatal = isAugmentedNetBackend(backend);
  const statusPrefix = fatal ? "Fatal analysis error" : "Analysis error";
  const warningMessage = backendUnavailable
    ? "AugmentedNet backend is unavailable. Check model/manifest paths and rebuild artifacts."
    : message;
  return {
    backend,
    fatal,
    backendUnavailable,
    message,
    statusText: `${statusPrefix} (${backend}): ${message}`,
    warningMessage,
  };
}

export function readPersistedAnalysisSettings(savedSettings) {
  const saved = savedSettings && typeof savedSettings === "object" ? savedSettings : {};
  return {
    analysis_backend: normalizeAnalysisBackend(saved.analysis_backend),
    rule_harmonic_rhythm_chords_per_bar: normalizeRuleBasedChordsPerBar(
      saved.rule_harmonic_rhythm_chords_per_bar,
    ),
    show_augnet_debug: !!saved.show_augnet_debug,
  };
}

export function persistableAnalysisSettings(state) {
  return {
    analysis_backend: normalizeAnalysisBackend(state?.analysis_backend),
    rule_harmonic_rhythm_chords_per_bar: normalizeRuleBasedChordsPerBar(
      state?.rule_harmonic_rhythm_chords_per_bar,
    ),
    show_augnet_debug: !!state?.show_augnet_debug,
  };
}
