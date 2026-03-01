import { isAugmentedNetBackend, normalizeAnalysisBackend } from "./analysisMode.js";

function escapeHtml(value) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function asPercent(value) {
  if (!Number.isFinite(value)) return "n/a";
  return `${(value * 100).toFixed(1)}%`;
}

function softmaxTop1AndMargin(logits) {
  if (!Array.isArray(logits) || logits.length === 0) return null;
  const finite = logits.filter((v) => Number.isFinite(v));
  if (finite.length === 0) return null;
  const maxLogit = Math.max(...finite);
  const exp = finite.map((v) => Math.exp(v - maxLogit));
  const denom = exp.reduce((sum, v) => sum + v, 0);
  if (!Number.isFinite(denom) || denom <= 0) return null;
  const probs = exp.map((v) => v / denom).sort((a, b) => b - a);
  const top1 = probs[0] ?? 0;
  const top2 = probs[1] ?? 0;
  return { top1, margin: top1 - top2 };
}

function romanHeadLogits(logitsMap) {
  if (!logitsMap || typeof logitsMap !== "object") return null;
  if (Array.isArray(logitsMap.RomanNumeral31)) return logitsMap.RomanNumeral31;
  const romanEntry = Object.entries(logitsMap).find(([k, v]) => k.includes("Roman") && Array.isArray(v));
  return romanEntry ? romanEntry[1] : null;
}

function normalizeLogitRows(logitsMap) {
  if (!logitsMap || typeof logitsMap !== "object") return [];
  return Object.entries(logitsMap)
    .filter(([, values]) => Array.isArray(values))
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([head, values]) => ({
      head,
      values: values.filter((v) => Number.isFinite(v)),
    }));
}

function formatConfidenceSummary(output) {
  const parts = [];
  if (Number.isFinite(output?.confidence)) {
    parts.push(`RN top1 ${asPercent(output.confidence)}`);
  }
  const romanLogits = romanHeadLogits(output?.logits);
  const summary = softmaxTop1AndMargin(romanLogits);
  if (summary) {
    parts.push(`margin ${asPercent(summary.margin)}`);
  }
  return parts.join(" | ") || "n/a";
}

function formatTonicization(localKey, tonicizedKey) {
  const local = String(localKey || "?");
  const tonicized = String(tonicizedKey || "");
  if (!tonicized || tonicized === local) {
    return `Local ${local}`;
  }
  return `Local ${local} -> tonicized ${tonicized}`;
}

function augnetOutputs(response) {
  const outputs = Array.isArray(response?.harmonic_outputs) ? response.harmonic_outputs : [];
  return outputs.filter((o) => o?.source === "augnet_onnx");
}

export function buildModeAwareHarmonyRows(response, analysisBackend, opts = {}) {
  const backend = normalizeAnalysisBackend(analysisBackend);
  const showDebugLogits = !!opts.showDebugLogits;

  if (!isAugmentedNetBackend(backend)) {
    const slices = Array.isArray(response?.harmonic_slices) ? response.harmonic_slices : [];
    return slices.map((slice, idx) => ({
      rowKey: `rule-${slice?.start_tick ?? idx}-${idx}`,
      type: "rule_based",
      startTick: slice?.start_tick ?? 0,
      endTick: slice?.end_tick ?? 0,
      romanNumeral: String(slice?.roman_numeral || "?"),
      chordQuality: String(slice?.quality || "other"),
      inversion: String(slice?.inversion || "unknown"),
      confidenceSummary: Number.isFinite(slice?.confidence)
        ? `confidence ${asPercent(slice.confidence)}`
        : "confidence n/a",
      hasDisagreement: false,
      disagreementDiagnosticIndex: null,
      logitRows: [],
    }));
  }

  const outputs = augnetOutputs(response);
  return outputs.map((output, idx) => {
    const startTick = output?.start_tick ?? 0;
    const logitRows = normalizeLogitRows(output?.logits);
    return {
      rowKey: `aug-${startTick}-${idx}`,
      type: "augnet",
      startTick,
      endTick: output?.end_tick ?? 0,
      romanNumeral: String(output?.roman_numeral || "?"),
      localKey: String(output?.local_key || "?"),
      tonicizedKey: output?.tonicized_key ? String(output.tonicized_key) : null,
      tonicizationText: formatTonicization(output?.local_key, output?.tonicized_key),
      chordQuality: String(output?.chord_quality || "other"),
      inversion: String(output?.inversion || "unknown"),
      chordLabel: output?.chord_label ? String(output.chord_label) : null,
      confidenceSummary: formatConfidenceSummary(output),
      hasDisagreement: false,
      disagreementDiagnosticIndex: null,
      logitRows: showDebugLogits ? logitRows : [],
    };
  });
}

function formatLogitPreview(values) {
  if (!Array.isArray(values) || values.length === 0) return "[]";
  const trimmed = values.slice(0, 6).map((v) => Number(v).toFixed(3));
  const suffix = values.length > 6 ? ", ..." : "";
  return `[${trimmed.join(", ")}${suffix}]`;
}

function augnetRowMarkup(row) {
  const logitsBlock = row.logitRows.length
    ? `<details class="harmony-logits"><summary>Raw logits (${row.logitRows.length} heads)</summary>${row.logitRows
      .map(
        (entry) =>
          `<div class="harmony-logit-row"><span class="harmony-logit-head">${escapeHtml(entry.head)}</span><code>${escapeHtml(formatLogitPreview(entry.values))}</code></div>`,
      )
      .join("")}</details>`
    : "";

  return `<li class="harmony-item harmony-item-augnet" data-start-tick="${row.startTick}">
    <div class="harmony-head">
      <span class="harmony-rn">${escapeHtml(row.romanNumeral)}</span>
      <span class="harmony-tick">Tick ${row.startTick}</span>
    </div>
    <div class="harmony-meta">${escapeHtml(row.tonicizationText)} | Quality ${escapeHtml(row.chordQuality)} | Inversion ${escapeHtml(row.inversion)}</div>
    <div class="harmony-meta">Chord ${escapeHtml(row.chordLabel || row.romanNumeral)}</div>
    <div class="harmony-confidence">Confidence ${escapeHtml(row.confidenceSummary)}</div>
    ${logitsBlock}
  </li>`;
}

function ruleRowMarkup(row) {
  return `<li class="harmony-item harmony-item-rule" data-start-tick="${row.startTick}">
    <div class="harmony-head">
      <span class="harmony-rn">${escapeHtml(row.romanNumeral)}</span>
      <span class="harmony-tick">Tick ${row.startTick}</span>
    </div>
    <div class="harmony-meta">Quality ${escapeHtml(row.chordQuality)} | Inversion ${escapeHtml(row.inversion)}</div>
    <div class="harmony-confidence">${escapeHtml(row.confidenceSummary)}</div>
  </li>`;
}

export function buildHarmonyListMarkup(rows, analysisBackend) {
  const backend = normalizeAnalysisBackend(analysisBackend);
  if (!Array.isArray(rows) || rows.length === 0) {
    return '<li class="harmony-item">No harmonic slices.</li>';
  }
  if (!isAugmentedNetBackend(backend)) {
    return rows.map((row) => ruleRowMarkup(row)).join("");
  }
  return rows.map((row) => augnetRowMarkup(row)).join("");
}

export function disagreementDiagnosticIndexForRow(rows, rowIndex) {
  return null;
}
