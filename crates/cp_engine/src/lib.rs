pub mod augnet_onnx;
pub mod augnet_postprocess;
pub mod augnet_preprocess;

#[cfg(feature = "augnet_onnx_backend")]
use cp_core::{note_location, ticks_per_measure, NormalizedScore, NoteEvent, NoteLocation};
use cp_core::{
    validate_score, AnalysisBackend, AnalysisDiagnostic, AnalysisRequest, AnalysisResponse,
    AnalysisSummary, CoreError, HarmonicOutput, HarmonicOutputSource, HarmonicSlice, NctTag,
    PresetId, RuleId, Severity,
};
use cp_harmony::analyze_harmony;
#[cfg(feature = "augnet_onnx_backend")]
use cp_music21_compat::PitchSpelling;
#[cfg(feature = "augnet_onnx_backend")]
use cp_music21_compat::{
    augnet_reindex_frames, encode_stage_b_inputs, simple_interval_name, AugnetScoreFrame,
};
use cp_rules::{rule_registry, validate_rule_params, RuleContext, RuleParamIssue};
use indexmap::{IndexMap, IndexSet};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
#[cfg(feature = "augnet_onnx_backend")]
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Deserialize)]
struct PresetSchema {
    groups: IndexMap<String, Vec<RuleId>>,
    presets: Vec<PresetDef>,
}

#[derive(Debug, Deserialize)]
struct PresetDef {
    preset_id: String,
    #[serde(default)]
    include_groups: Vec<String>,
    #[serde(default)]
    include_rules: Vec<RuleId>,
    #[serde(default)]
    exclude_groups: Vec<String>,
    #[serde(default)]
    exclude_rules: Vec<RuleId>,
    #[serde(default)]
    severity_overrides: BTreeMap<RuleId, Severity>,
    #[serde(default)]
    rule_param_defaults: BTreeMap<RuleId, Value>,
}

#[derive(Debug, Clone)]
pub struct ResolvedPreset {
    pub active_rules: IndexSet<RuleId>,
    pub severity_overrides: BTreeMap<RuleId, Severity>,
    pub rule_params: BTreeMap<RuleId, Value>,
}

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("failed to parse preset schema: {0}")]
    PresetSchema(#[from] serde_json::Error),
    #[error("preset {0} not found")]
    PresetNotFound(String),
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error(
        "invalid rule parameters:\n{details}",
        details = format_param_issues(issues)
    )]
    InvalidRuleParams { issues: Vec<RuleParamIssue> },
    #[error("selected backend {backend} is unavailable: {details}")]
    BackendUnavailable {
        backend: &'static str,
        details: String,
    },
    #[error("augmentednet inference failed: {0}")]
    AugmentedNetInference(String),
}

pub type EngineResult<T> = std::result::Result<T, EngineError>;

fn format_param_issues(issues: &[RuleParamIssue]) -> String {
    let mut out = String::new();
    for i in issues {
        let actual = i
            .actual
            .as_ref()
            .map(|s| format!(", actual={}", s))
            .unwrap_or_default();
        out.push_str(&format!(
            "- rule={} field={} reason={} expected={}{}\n",
            i.rule_id, i.field_path, i.reason, i.expected, actual
        ));
    }
    out.trim_end().to_string()
}

fn preset_name(id: &PresetId) -> &'static str {
    match id {
        PresetId::Species1 => "species1",
        PresetId::Species2 => "species2",
        PresetId::Species3 => "species3",
        PresetId::Species4 => "species4",
        PresetId::Species5 => "species5",
        PresetId::GeneralVoiceLeading => "general_voice_leading",
        PresetId::ModerateClassical => "moderate_classical",
        PresetId::Relaxed => "relaxed",
        PresetId::Custom => "custom",
    }
}

fn embedded_preset_json() -> &'static str {
    include_str!("../../../docs/planning/rules-presets.json")
}

pub fn resolve_preset(req: &AnalysisRequest) -> EngineResult<ResolvedPreset> {
    let schema: PresetSchema = serde_json::from_str(embedded_preset_json())?;
    let pid = preset_name(&req.config.preset_id);
    let p = schema
        .presets
        .iter()
        .find(|p| p.preset_id == pid)
        .ok_or_else(|| EngineError::PresetNotFound(pid.to_string()))?;

    let mut active = IndexSet::new();
    for g in &p.include_groups {
        if let Some(ids) = schema.groups.get(g) {
            for id in ids {
                active.insert(id.clone());
            }
        }
    }
    for id in &p.include_rules {
        active.insert(id.clone());
    }

    for g in &p.exclude_groups {
        if let Some(ids) = schema.groups.get(g) {
            for id in ids {
                active.shift_remove(id);
            }
        }
    }
    for id in &p.exclude_rules {
        active.shift_remove(id);
    }

    for id in &req.config.enabled_rule_ids {
        active.insert(id.clone());
    }
    for id in &req.config.disabled_rule_ids {
        active.shift_remove(id);
    }

    let mut sev = p.severity_overrides.clone();
    for (k, v) in &req.config.severity_overrides {
        sev.insert(k.clone(), *v);
    }
    let mut merged_params = p.rule_param_defaults.clone();
    for (k, v) in &req.config.rule_params {
        merged_params.insert(k.clone(), v.clone());
    }
    Ok(ResolvedPreset {
        active_rules: active,
        severity_overrides: sev,
        rule_params: merged_params,
    })
}

#[derive(Debug)]
struct RulePipelineResult {
    diagnostics: Vec<AnalysisDiagnostic>,
    harmonic_slices: Vec<HarmonicSlice>,
    harmonic_outputs: Vec<HarmonicOutput>,
    nct_tags: Vec<NctTag>,
    warnings: Vec<String>,
}

#[cfg(feature = "augnet_onnx_backend")]
#[derive(Debug)]
struct AugnetPipelineResult {
    harmonic_slices: Vec<HarmonicSlice>,
    harmonic_outputs: Vec<HarmonicOutput>,
    warnings: Vec<String>,
}

pub fn analyze(req: &AnalysisRequest) -> EngineResult<AnalysisResponse> {
    validate_score(&req.score)?;
    match req.config.analysis_backend {
        AnalysisBackend::RuleBased => analyze_rule_based_mode(req),
        AnalysisBackend::AugnetOnnx => analyze_augnet_mode(req),
        AnalysisBackend::Hybrid => analyze_hybrid_mode(req),
    }
}

fn analyze_rule_based_mode(req: &AnalysisRequest) -> EngineResult<AnalysisResponse> {
    let resolved = resolve_preset(req)?;
    validate_resolved_rule_params(&resolved, req.score.voices.len())?;
    let mut rule = analyze_rule_pipeline(req, &resolved);
    sort_diagnostics(&mut rule.diagnostics);
    Ok(AnalysisResponse {
        summary: build_summary(&rule.diagnostics, resolved.active_rules.len()),
        diagnostics: rule.diagnostics,
        harmonic_slices: rule.harmonic_slices,
        harmonic_outputs: rule.harmonic_outputs,
        nct_tags: rule.nct_tags,
        warnings: rule.warnings,
    })
}

#[cfg(feature = "augnet_onnx_backend")]
fn analyze_augnet_mode(req: &AnalysisRequest) -> EngineResult<AnalysisResponse> {
    let (_rule_slices, nct_tags, mut warnings) = analyze_harmony(req);
    let aug = analyze_augnet_pipeline(req, AnalysisBackend::AugnetOnnx)?;
    warnings.extend(aug.warnings);
    Ok(AnalysisResponse {
        diagnostics: Vec::new(),
        harmonic_slices: aug.harmonic_slices,
        harmonic_outputs: aug.harmonic_outputs,
        nct_tags,
        summary: build_summary(&[], 0),
        warnings,
    })
}

#[cfg(not(feature = "augnet_onnx_backend"))]
fn analyze_augnet_mode(_req: &AnalysisRequest) -> EngineResult<AnalysisResponse> {
    Err(EngineError::BackendUnavailable {
        backend: backend_name(AnalysisBackend::AugnetOnnx),
        details: "cp_engine built without augnet_onnx_backend feature".to_string(),
    })
}

#[cfg(feature = "augnet_onnx_backend")]
fn analyze_hybrid_mode(req: &AnalysisRequest) -> EngineResult<AnalysisResponse> {
    let resolved = resolve_preset(req)?;
    validate_resolved_rule_params(&resolved, req.score.voices.len())?;
    let mut rule = analyze_rule_pipeline(req, &resolved);
    let aug = analyze_augnet_pipeline(req, AnalysisBackend::Hybrid)?;

    let mut diagnostics = rule.diagnostics;
    diagnostics.extend(build_hybrid_disagreement_diagnostics(
        &req.score,
        &rule.harmonic_outputs,
        &aug.harmonic_outputs,
    ));
    sort_diagnostics(&mut diagnostics);

    let mut harmonic_outputs = rule.harmonic_outputs;
    harmonic_outputs.extend(aug.harmonic_outputs);

    rule.warnings.extend(aug.warnings);
    Ok(AnalysisResponse {
        summary: build_summary(&diagnostics, resolved.active_rules.len()),
        diagnostics,
        harmonic_slices: aug.harmonic_slices,
        harmonic_outputs,
        nct_tags: rule.nct_tags,
        warnings: rule.warnings,
    })
}

#[cfg(not(feature = "augnet_onnx_backend"))]
fn analyze_hybrid_mode(_req: &AnalysisRequest) -> EngineResult<AnalysisResponse> {
    Err(EngineError::BackendUnavailable {
        backend: backend_name(AnalysisBackend::Hybrid),
        details: "cp_engine built without augnet_onnx_backend feature".to_string(),
    })
}

fn analyze_rule_pipeline(req: &AnalysisRequest, resolved: &ResolvedPreset) -> RulePipelineResult {
    let registry = rule_registry();
    let rule_ctx = RuleContext {
        score: &req.score,
        preset_id: &req.config.preset_id,
        rule_params: &resolved.rule_params,
    };
    let mut diagnostics = Vec::new();
    let mut warnings = Vec::new();

    for rid in &resolved.active_rules {
        let Some(rule) = registry.get(rid) else {
            warnings.push(format!("active rule {} has no implementation yet", rid));
            continue;
        };
        diagnostics.extend(rule.evaluate(&rule_ctx));
    }

    for d in &mut diagnostics {
        if let Some(s) = resolved.severity_overrides.get(&d.rule_id) {
            d.severity = *s;
        }
    }

    let (harmonic_slices, nct_tags, harm_warnings) = analyze_harmony(req);
    warnings.extend(harm_warnings);
    let harmonic_outputs = rule_slices_to_harmonic_outputs(&harmonic_slices);

    RulePipelineResult {
        diagnostics,
        harmonic_slices,
        harmonic_outputs,
        nct_tags,
        warnings,
    }
}

fn validate_resolved_rule_params(
    resolved: &ResolvedPreset,
    voice_count: usize,
) -> EngineResult<()> {
    if let Err(issues) = validate_rule_params(
        resolved.active_rules.iter(),
        &resolved.rule_params,
        voice_count,
    ) {
        return Err(EngineError::InvalidRuleParams { issues });
    }
    Ok(())
}

#[cfg(feature = "augnet_onnx_backend")]
fn analyze_augnet_pipeline(
    req: &AnalysisRequest,
    selected_backend: AnalysisBackend,
) -> EngineResult<AugnetPipelineResult> {
    let config = build_augnet_backend_config(req);
    let backend = augnet_onnx::AugmentedNetOnnxBackend::new(config).map_err(|err| {
        EngineError::BackendUnavailable {
            backend: backend_name(selected_backend),
            details: err.to_string(),
        }
    })?;

    let fixed_offset = req.config.augnet_backend.fixed_offset;
    if !fixed_offset.is_finite() || fixed_offset <= 0.0 {
        return Err(EngineError::AugmentedNetInference(format!(
            "augnet_backend.fixed_offset must be finite and > 0, got {fixed_offset}"
        )));
    }
    let configured_steps = req.config.augnet_backend.max_steps.max(1);
    if configured_steps != backend.fixed_t() {
        return Err(EngineError::AugmentedNetInference(format!(
            "augnet_backend.max_steps={} does not match model fixed_t={}",
            configured_steps,
            backend.fixed_t()
        )));
    }

    let frames = build_augnet_frames_from_score(&req.score, fixed_offset);
    let step_ticks = score_step_ticks(&req.score, fixed_offset);
    let mut harmonic_slices = Vec::new();
    let mut harmonic_outputs = Vec::new();

    for (chunk_index, chunk_frames) in frames.chunks(configured_steps).enumerate() {
        let chunk_start_step = chunk_index * configured_steps;
        let stage_b = encode_stage_b_inputs(chunk_frames, fixed_offset, configured_steps);
        let tensors = crate::augnet_preprocess::stage_b_inputs_to_onnx_tensors(stage_b);
        let inference = backend
            .infer(&tensors)
            .map_err(|e| EngineError::AugmentedNetInference(e.to_string()))?;
        let stage_d = inference
            .to_stage_d_artifact()
            .map_err(|e| EngineError::AugmentedNetInference(e.to_string()))?;

        for label in &stage_d.labels {
            // Match AugmentedNet inference.solveChordSegmentation: keep harmonic changes only.
            if label.harmonic_rhythm != 0 {
                continue;
            }
            let global_step = chunk_start_step + label.time_index;
            let start_tick = (global_step as u32).saturating_mul(step_ticks);
            let end_tick = start_tick.saturating_add(step_ticks.max(1));
            let mut logits = BTreeMap::new();
            for (head_name, head_data) in &stage_d.heads {
                if let Some(row) = head_data.raw_logits.get(label.time_index) {
                    logits.insert(head_name.clone(), row.clone());
                }
            }
            let confidence = label
                .component_confidence
                .get("RomanNumeral31")
                .map(|c| c.confidence_top1);
            let output_id = harmonic_outputs.len() as u32;
            harmonic_outputs.push(HarmonicOutput {
                output_id,
                start_tick,
                end_tick,
                source: HarmonicOutputSource::AugnetOnnx,
                roman_numeral: Some(label.roman_numeral_formatted.clone()),
                local_key: Some(label.local_key.clone()),
                tonicized_key: Some(label.tonicized_key_resolved.clone()),
                chord_quality: Some(label.chord_quality.clone()),
                inversion: Some(label.inversion_figure.clone()),
                chord_label: Some(label.chord_label_formatted.clone()),
                confidence,
                logits,
            });
            harmonic_slices.push(HarmonicSlice {
                slice_id: harmonic_slices.len() as u32,
                start_tick,
                end_tick,
                pitch_classes: label.pitch_class_set_resolved.clone(),
                root_pc: m21_name_to_pc(&label.chord_root),
                quality: Some(label.chord_quality.clone()),
                inversion: Some(label.inversion_figure.clone()),
                roman_numeral: Some(label.roman_numeral_formatted.clone()),
                confidence: confidence.unwrap_or(0.0) as f32,
                inferred_root: None,
                missing_tones: Vec::new(),
                chord_form: Some(label.chord_label_formatted.clone()),
            });
        }
    }

    Ok(AugnetPipelineResult {
        harmonic_slices,
        harmonic_outputs,
        warnings: Vec::new(),
    })
}

#[cfg(feature = "augnet_onnx_backend")]
fn build_augnet_backend_config(req: &AnalysisRequest) -> augnet_onnx::AugmentedNetOnnxConfig {
    let mut config = augnet_onnx::AugmentedNetOnnxConfig::default();
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    config.model_path = repo_root.join("models/augnet/AugmentedNet.onnx");
    config.manifest_path = repo_root.join("models/augnet/model-manifest.json");

    if let Some(path) = &req.config.augnet_backend.model_path {
        config.model_path = PathBuf::from(path);
    }
    if let Some(path) = &req.config.augnet_backend.manifest_path {
        config.manifest_path = PathBuf::from(path);
    }
    config
}

#[cfg(feature = "augnet_onnx_backend")]
fn score_step_ticks(score: &NormalizedScore, fixed_offset: f64) -> u32 {
    let tpq = if score.meta.ticks_per_quarter == 0 {
        480
    } else {
        score.meta.ticks_per_quarter
    };
    ((tpq as f64) * fixed_offset).round().max(1.0) as u32
}

#[cfg(feature = "augnet_onnx_backend")]
fn build_augnet_frames_from_score(
    score: &NormalizedScore,
    fixed_offset: f64,
) -> Vec<AugnetScoreFrame> {
    let end_tick = score
        .voices
        .iter()
        .flat_map(|v| v.notes.iter())
        .map(|n| n.start_tick.saturating_add(n.duration_ticks))
        .max();
    let tpq = if score.meta.ticks_per_quarter == 0 {
        480
    } else {
        score.meta.ticks_per_quarter
    };
    let tpm = ticks_per_measure(&score.meta.time_signature, tpq).max(1);

    let mut boundaries: Vec<u32> = score
        .voices
        .iter()
        .flat_map(|v| v.notes.iter())
        .flat_map(|n| [n.start_tick, n.start_tick.saturating_add(n.duration_ticks)])
        .collect();
    if let Some(last_tick) = end_tick {
        let mut m = 0u32;
        while m <= last_tick {
            boundaries.push(m);
            m = m.saturating_add(tpm);
            if m == u32::MAX {
                break;
            }
        }
        boundaries.push(last_tick);
    }
    boundaries.sort_unstable();
    boundaries.dedup();

    if boundaries.len() < 2 {
        return Vec::new();
    }

    let mut initial = Vec::with_capacity(boundaries.len().saturating_sub(1));
    for window in boundaries.windows(2) {
        let tick = window[0];
        let next = window[1];
        if next <= tick {
            continue;
        }
        let mut active: Vec<&NoteEvent> = score
            .voices
            .iter()
            .flat_map(|voice| voice.notes.iter())
            .filter(|note| {
                note.start_tick <= tick
                    && tick < note.start_tick.saturating_add(note.duration_ticks)
            })
            .collect();
        active.sort_by(|a, b| {
            (a.midi, a.voice_index, a.note_id.as_str()).cmp(&(
                b.midi,
                b.voice_index,
                b.note_id.as_str(),
            ))
        });
        let measure = (tick / tpm) as i32 + 1;
        let offset = (tick as f64) / (tpq as f64);
        let duration = (next.saturating_sub(tick) as f64) / (tpq as f64);
        if active.is_empty() {
            initial.push(AugnetScoreFrame {
                s_offset: offset,
                s_duration: duration,
                s_measure: measure,
                s_notes: None,
                s_intervals: None,
                s_is_onset: None,
            });
            continue;
        }

        let notes: Vec<String> = active
            .iter()
            .map(|note| midi_to_m21_name(note.midi))
            .collect();
        let intervals = intervals_from_m21_note_names(&notes);
        let onsets: Vec<bool> = active
            .iter()
            .map(|note| note.start_tick == tick && !note.tie_end)
            .collect();
        initial.push(AugnetScoreFrame {
            s_offset: offset,
            s_duration: duration,
            s_measure: measure,
            s_notes: Some(notes),
            s_intervals: Some(intervals),
            s_is_onset: Some(onsets),
        });
    }

    augnet_reindex_frames(&initial, fixed_offset)
}

#[cfg(feature = "augnet_onnx_backend")]
fn midi_to_m21_name(midi: i16) -> String {
    const M21_PC_NAMES: [&str; 12] = [
        "C", "C#", "D", "E-", "E", "F", "F#", "G", "A-", "A", "B-", "B",
    ];
    let pc = midi.rem_euclid(12) as usize;
    let octave = midi.div_euclid(12) - 1;
    format!("{}{}", M21_PC_NAMES[pc], octave)
}

#[cfg(feature = "augnet_onnx_backend")]
fn intervals_from_m21_note_names(notes: &[String]) -> Vec<String> {
    if notes.len() <= 1 {
        return Vec::new();
    }
    let Ok((bass, _)) = PitchSpelling::parse_m21_pitch_name(&notes[0]) else {
        return vec!["P1".to_string(); notes.len().saturating_sub(1)];
    };
    notes
        .iter()
        .skip(1)
        .map(|name| {
            PitchSpelling::parse_m21_pitch_name(name)
                .map(|(pitch, _)| simple_interval_name(&bass, &pitch))
                .unwrap_or_else(|_| "P1".to_string())
        })
        .collect()
}

#[cfg(feature = "augnet_onnx_backend")]
fn m21_name_to_pc(name: &str) -> Option<u8> {
    PitchSpelling::parse_m21_pitch_name(name)
        .ok()
        .map(|(pitch, _)| pitch.pitch_class())
}

fn rule_slices_to_harmonic_outputs(slices: &[HarmonicSlice]) -> Vec<HarmonicOutput> {
    slices
        .iter()
        .enumerate()
        .map(|(idx, slice)| HarmonicOutput {
            output_id: idx as u32,
            start_tick: slice.start_tick,
            end_tick: slice.end_tick,
            source: HarmonicOutputSource::RuleBased,
            roman_numeral: slice.roman_numeral.clone(),
            local_key: None,
            tonicized_key: None,
            chord_quality: slice.quality.clone(),
            inversion: slice.inversion.clone(),
            chord_label: slice.chord_form.clone(),
            confidence: Some(slice.confidence as f64),
            logits: BTreeMap::new(),
        })
        .collect()
}

#[cfg(feature = "augnet_onnx_backend")]
fn build_hybrid_disagreement_diagnostics(
    score: &NormalizedScore,
    rule_outputs: &[HarmonicOutput],
    augnet_outputs: &[HarmonicOutput],
) -> Vec<AnalysisDiagnostic> {
    let mut out = Vec::new();
    for aug in augnet_outputs {
        let Some(rule) = find_rule_output_for_tick(rule_outputs, aug.start_tick) else {
            continue;
        };
        let rule_interpretation = interpretation_label(rule);
        let aug_interpretation = interpretation_label(aug);
        if rule_interpretation == aug_interpretation {
            continue;
        }
        let mut context = BTreeMap::new();
        context.insert("rule_based.source".to_string(), "rule_based".to_string());
        context.insert("augnet_onnx.source".to_string(), "augnet_onnx".to_string());
        context.insert(
            "rule_based.interpretation".to_string(),
            rule_interpretation.clone(),
        );
        context.insert(
            "augnet_onnx.interpretation".to_string(),
            aug_interpretation.clone(),
        );
        context.insert(
            "rule_based.start_tick".to_string(),
            rule.start_tick.to_string(),
        );
        context.insert(
            "augnet_onnx.start_tick".to_string(),
            aug.start_tick.to_string(),
        );
        let primary = note_location_at_tick(score, aug.start_tick);
        let related = note_location_at_tick(score, rule.start_tick);
        out.push(AnalysisDiagnostic {
            rule_id: "hybrid.harmony.disagreement".to_string(),
            severity: Severity::Info,
            message: format!(
                "hybrid harmony disagreement: rule_based={} augnet_onnx={}",
                rule_interpretation, aug_interpretation
            ),
            primary,
            related: Some(related),
            context,
        });
    }
    out
}

#[cfg(feature = "augnet_onnx_backend")]
fn find_rule_output_for_tick<'a>(
    rule_outputs: &'a [HarmonicOutput],
    tick: u32,
) -> Option<&'a HarmonicOutput> {
    if let Some(found) = rule_outputs
        .iter()
        .find(|output| output.start_tick <= tick && tick < output.end_tick)
    {
        return Some(found);
    }
    rule_outputs
        .iter()
        .min_by_key(|output| output.start_tick.abs_diff(tick))
}

#[cfg(feature = "augnet_onnx_backend")]
fn interpretation_label(output: &HarmonicOutput) -> String {
    if let Some(roman) = &output.roman_numeral {
        return roman.clone();
    }
    if let Some(label) = &output.chord_label {
        return label.clone();
    }
    if let Some(quality) = &output.chord_quality {
        return quality.clone();
    }
    "unknown".to_string()
}

#[cfg(feature = "augnet_onnx_backend")]
fn note_location_at_tick(score: &NormalizedScore, tick: u32) -> NoteLocation {
    let note = score
        .voices
        .iter()
        .flat_map(|voice| voice.notes.iter())
        .find(|note| {
            note.start_tick <= tick && tick < note.start_tick.saturating_add(note.duration_ticks)
        })
        .or_else(|| {
            score
                .voices
                .iter()
                .flat_map(|voice| voice.notes.iter())
                .find(|note| note.start_tick >= tick)
        })
        .or_else(|| {
            score
                .voices
                .iter()
                .flat_map(|voice| voice.notes.iter())
                .next()
        });
    if let Some(note) = note {
        return note_location(note, score);
    }
    let tpq = if score.meta.ticks_per_quarter == 0 {
        480
    } else {
        score.meta.ticks_per_quarter
    };
    let tpm = ticks_per_measure(&score.meta.time_signature, tpq).max(1);
    let beat_ticks = (tpm / score.meta.time_signature.numerator as u32).max(1);
    NoteLocation {
        measure: tick / tpm + 1,
        beat: (tick % tpm) / beat_ticks + 1,
        tick,
        voice_index: 0,
        note_id: format!("tick_{tick}"),
    }
}

fn sort_diagnostics(diagnostics: &mut [AnalysisDiagnostic]) {
    diagnostics.sort_by(|a, b| {
        (
            a.primary.tick,
            a.primary.voice_index,
            a.primary.note_id.as_str(),
            a.rule_id.as_str(),
        )
            .cmp(&(
                b.primary.tick,
                b.primary.voice_index,
                b.primary.note_id.as_str(),
                b.rule_id.as_str(),
            ))
    });
}

fn build_summary(diagnostics: &[AnalysisDiagnostic], active_rule_count: usize) -> AnalysisSummary {
    let error_count = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count();
    let warning_count = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Warning)
        .count();
    AnalysisSummary {
        total_diagnostics: error_count + warning_count,
        error_count,
        warning_count,
        active_rule_count,
    }
}

fn backend_name(backend: AnalysisBackend) -> &'static str {
    match backend {
        AnalysisBackend::RuleBased => "rule_based",
        AnalysisBackend::AugnetOnnx => "augnet_onnx",
        AnalysisBackend::Hybrid => "hybrid",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cp_core::{
        AnalysisBackend, AnalysisConfig, AnalysisRequest, AugmentedNetBackendConfig,
        HarmonicRhythm, KeySignature, NormalizedScore, NoteEvent, PresetId, ScaleMode, ScoreMeta,
        TimeSignature, Voice,
    };

    fn base_score() -> NormalizedScore {
        NormalizedScore {
            meta: ScoreMeta {
                exercise_count: 1,
                key_signature: KeySignature {
                    tonic_pc: 0,
                    mode: ScaleMode::Major,
                },
                time_signature: TimeSignature {
                    numerator: 4,
                    denominator: 4,
                },
                ticks_per_quarter: 480,
            },
            voices: vec![
                Voice {
                    voice_index: 0,
                    name: "Sop".to_string(),
                    notes: vec![
                        NoteEvent {
                            note_id: "n1".to_string(),
                            voice_index: 0,
                            midi: 60,
                            start_tick: 0,
                            duration_ticks: 480,
                            tie_start: false,
                            tie_end: false,
                        },
                        NoteEvent {
                            note_id: "n2".to_string(),
                            voice_index: 0,
                            midi: 62,
                            start_tick: 480,
                            duration_ticks: 480,
                            tie_start: false,
                            tie_end: false,
                        },
                    ],
                },
                Voice {
                    voice_index: 1,
                    name: "Bass".to_string(),
                    notes: vec![
                        NoteEvent {
                            note_id: "b1".to_string(),
                            voice_index: 1,
                            midi: 53,
                            start_tick: 0,
                            duration_ticks: 480,
                            tie_start: false,
                            tie_end: false,
                        },
                        NoteEvent {
                            note_id: "b2".to_string(),
                            voice_index: 1,
                            midi: 55,
                            start_tick: 480,
                            duration_ticks: 480,
                            tie_start: false,
                            tie_end: false,
                        },
                    ],
                },
            ],
        }
    }

    #[test]
    fn resolves_species1_preset() {
        let req = AnalysisRequest {
            score: base_score(),
            config: AnalysisConfig {
                preset_id: PresetId::Species1,
                enabled_rule_ids: vec![],
                disabled_rule_ids: vec![],
                severity_overrides: BTreeMap::new(),
                rule_params: BTreeMap::new(),
                harmonic_rhythm: HarmonicRhythm::NoteOnset,
                analysis_backend: AnalysisBackend::RuleBased,
                augnet_backend: AugmentedNetBackendConfig::default(),
            },
        };
        let p = resolve_preset(&req).expect("resolve");
        assert!(p.active_rules.contains("sp1.rhythm.one_to_one_only"));
        assert!(!p.active_rules.contains("sp2.rhythm.two_to_one_only"));
    }

    #[test]
    fn detects_parallel_perfects() {
        let mut s = base_score();
        s.voices[0].notes[1].midi = 67;
        s.voices[1].notes[1].midi = 60;
        let req = AnalysisRequest {
            score: s,
            config: AnalysisConfig {
                preset_id: PresetId::Species1,
                enabled_rule_ids: vec!["gen.motion.parallel_perfects_forbidden".to_string()],
                disabled_rule_ids: vec![],
                severity_overrides: BTreeMap::new(),
                rule_params: BTreeMap::new(),
                harmonic_rhythm: HarmonicRhythm::NoteOnset,
                analysis_backend: AnalysisBackend::RuleBased,
                augnet_backend: AugmentedNetBackendConfig::default(),
            },
        };
        let res = analyze(&req).expect("analyze");
        assert!(res
            .diagnostics
            .iter()
            .any(|d| d.rule_id == "gen.motion.parallel_perfects_forbidden"));
    }

    #[test]
    fn produces_harmony_annotations() {
        let req = AnalysisRequest {
            score: base_score(),
            config: AnalysisConfig {
                preset_id: PresetId::Species1,
                enabled_rule_ids: vec![],
                disabled_rule_ids: vec![],
                severity_overrides: BTreeMap::new(),
                rule_params: BTreeMap::new(),
                harmonic_rhythm: HarmonicRhythm::NoteOnset,
                analysis_backend: AnalysisBackend::RuleBased,
                augnet_backend: AugmentedNetBackendConfig::default(),
            },
        };
        let res = analyze(&req).expect("analyze");
        assert!(!res.harmonic_slices.is_empty());
    }

    #[test]
    fn general_preset_includes_tonal_rule_and_excludes_species_specific() {
        let req = AnalysisRequest {
            score: base_score(),
            config: AnalysisConfig {
                preset_id: PresetId::GeneralVoiceLeading,
                enabled_rule_ids: vec![],
                disabled_rule_ids: vec![],
                severity_overrides: BTreeMap::new(),
                rule_params: BTreeMap::new(),
                harmonic_rhythm: HarmonicRhythm::NoteOnset,
                analysis_backend: AnalysisBackend::RuleBased,
                augnet_backend: AugmentedNetBackendConfig::default(),
            },
        };
        let p = resolve_preset(&req).expect("resolve");
        assert!(p
            .active_rules
            .contains("gen.voice.leading_tone_not_doubled"));
        assert!(!p.active_rules.contains("sp1.rhythm.one_to_one_only"));
    }

    #[test]
    fn no_missing_rule_warnings_for_general_preset() {
        let req = AnalysisRequest {
            score: base_score(),
            config: AnalysisConfig {
                preset_id: PresetId::GeneralVoiceLeading,
                enabled_rule_ids: vec![],
                disabled_rule_ids: vec![],
                severity_overrides: BTreeMap::new(),
                rule_params: BTreeMap::new(),
                harmonic_rhythm: HarmonicRhythm::NoteOnset,
                analysis_backend: AnalysisBackend::RuleBased,
                augnet_backend: AugmentedNetBackendConfig::default(),
            },
        };
        let res = analyze(&req).expect("analyze");
        assert!(!res
            .warnings
            .iter()
            .any(|w| w.contains("has no implementation yet")));
    }

    #[test]
    fn preset_defaults_expose_rule_params() {
        let req = AnalysisRequest {
            score: base_score(),
            config: AnalysisConfig {
                preset_id: PresetId::GeneralVoiceLeading,
                enabled_rule_ids: vec![],
                disabled_rule_ids: vec![],
                severity_overrides: BTreeMap::new(),
                rule_params: BTreeMap::new(),
                harmonic_rhythm: HarmonicRhythm::NoteOnset,
                analysis_backend: AnalysisBackend::RuleBased,
                augnet_backend: AugmentedNetBackendConfig::default(),
            },
        };
        let p = resolve_preset(&req).expect("resolve");
        assert!(p
            .rule_params
            .contains_key("gen.motion.contrary_and_oblique_preferred"));
    }

    #[test]
    fn moderate_and_relaxed_presets_resolve() {
        for preset_id in [PresetId::ModerateClassical, PresetId::Relaxed] {
            let req = AnalysisRequest {
                score: base_score(),
                config: AnalysisConfig {
                    preset_id: preset_id.clone(),
                    enabled_rule_ids: vec![],
                    disabled_rule_ids: vec![],
                    severity_overrides: BTreeMap::new(),
                    rule_params: BTreeMap::new(),
                    harmonic_rhythm: HarmonicRhythm::NoteOnset,
                    analysis_backend: AnalysisBackend::RuleBased,
                    augnet_backend: AugmentedNetBackendConfig::default(),
                },
            };
            let p = resolve_preset(&req).expect("resolve");
            assert!(p.active_rules.contains("gen.motion.parallel_perfects_forbidden"));
            assert!(p
                .rule_params
                .contains_key("gen.spacing.upper_adjacent_max_octave"));
        }
    }

    #[test]
    fn invalid_rule_params_fail_fast() {
        let mut params = BTreeMap::new();
        params.insert(
            "gen.motion.contrary_and_oblique_preferred".to_string(),
            serde_json::json!({
                "pair_mode": "all_pairs",
                "similar_motion_ratio_max": 1.5
            }),
        );
        let req = AnalysisRequest {
            score: base_score(),
            config: AnalysisConfig {
                preset_id: PresetId::GeneralVoiceLeading,
                enabled_rule_ids: vec![],
                disabled_rule_ids: vec![],
                severity_overrides: BTreeMap::new(),
                rule_params: params,
                harmonic_rhythm: HarmonicRhythm::NoteOnset,
                analysis_backend: AnalysisBackend::RuleBased,
                augnet_backend: AugmentedNetBackendConfig::default(),
            },
        };
        let err = analyze(&req).expect_err("must fail");
        assert!(matches!(err, EngineError::InvalidRuleParams { .. }));
    }

    #[cfg(feature = "augnet_onnx_backend")]
    #[test]
    fn augnet_frame_builder_computes_intervals_and_onsets() {
        let score = NormalizedScore {
            meta: ScoreMeta {
                exercise_count: 1,
                key_signature: KeySignature {
                    tonic_pc: 0,
                    mode: ScaleMode::Major,
                },
                time_signature: TimeSignature {
                    numerator: 4,
                    denominator: 4,
                },
                ticks_per_quarter: 480,
            },
            voices: vec![
                Voice {
                    voice_index: 0,
                    name: "Sop".to_string(),
                    notes: vec![NoteEvent {
                        note_id: "s0".to_string(),
                        voice_index: 0,
                        midi: 64, // E4
                        start_tick: 0,
                        duration_ticks: 960,
                        tie_start: false,
                        tie_end: false,
                    }],
                },
                Voice {
                    voice_index: 1,
                    name: "Alto".to_string(),
                    notes: vec![
                        NoteEvent {
                            note_id: "a0".to_string(),
                            voice_index: 1,
                            midi: 67, // G4
                            start_tick: 0,
                            duration_ticks: 480,
                            tie_start: false,
                            tie_end: false,
                        },
                        NoteEvent {
                            note_id: "a1".to_string(),
                            voice_index: 1,
                            midi: 69, // A4
                            start_tick: 480,
                            duration_ticks: 480,
                            tie_start: false,
                            tie_end: false,
                        },
                    ],
                },
                Voice {
                    voice_index: 2,
                    name: "Bass".to_string(),
                    notes: vec![NoteEvent {
                        note_id: "b0".to_string(),
                        voice_index: 2,
                        midi: 60, // C4
                        start_tick: 0,
                        duration_ticks: 960,
                        tie_start: false,
                        tie_end: false,
                    }],
                },
            ],
        };

        let fixed_offset = 0.25;
        let frames = build_augnet_frames_from_score(&score, fixed_offset);
        assert_eq!(score_step_ticks(&score, fixed_offset), 120);
        assert_eq!(frames.len(), 8);

        let f0 = &frames[0];
        assert_eq!(
            f0.s_notes.as_ref().expect("notes"),
            &vec!["C4".to_string(), "E4".to_string(), "G4".to_string()]
        );
        assert_eq!(
            f0.s_intervals.as_ref().expect("intervals"),
            &vec!["M3".to_string(), "P5".to_string()]
        );
        assert_eq!(
            f0.s_is_onset.as_ref().expect("onsets"),
            &vec![true, true, true]
        );

        let f1 = &frames[1];
        assert_eq!(
            f1.s_notes.as_ref().expect("notes"),
            &vec!["C4".to_string(), "E4".to_string(), "G4".to_string()]
        );
        assert_eq!(
            f1.s_intervals.as_ref().expect("intervals"),
            &vec!["M3".to_string(), "P5".to_string()]
        );
        assert_eq!(
            f1.s_is_onset.as_ref().expect("onsets"),
            &vec![false, false, false]
        );

        let f4 = &frames[4];
        assert_eq!(
            f4.s_notes.as_ref().expect("notes"),
            &vec!["C4".to_string(), "E4".to_string(), "A4".to_string()]
        );
        assert_eq!(
            f4.s_intervals.as_ref().expect("intervals"),
            &vec!["M3".to_string(), "M6".to_string()]
        );
        assert_eq!(
            f4.s_is_onset.as_ref().expect("onsets"),
            &vec![false, false, true]
        );
    }

    #[cfg(feature = "augnet_onnx_backend")]
    #[test]
    fn augnet_frame_builder_respects_measure_indexing_and_step_offsets() {
        let score = NormalizedScore {
            meta: ScoreMeta {
                exercise_count: 1,
                key_signature: KeySignature {
                    tonic_pc: 0,
                    mode: ScaleMode::Major,
                },
                time_signature: TimeSignature {
                    numerator: 3,
                    denominator: 4,
                },
                ticks_per_quarter: 480,
            },
            voices: vec![Voice {
                voice_index: 0,
                name: "Mono".to_string(),
                notes: vec![NoteEvent {
                    note_id: "n0".to_string(),
                    voice_index: 0,
                    midi: 60,
                    start_tick: 0,
                    duration_ticks: 3000,
                    tie_start: false,
                    tie_end: false,
                }],
            }],
        };

        let fixed_offset = 0.25;
        let frames = build_augnet_frames_from_score(&score, fixed_offset);
        assert_eq!(score_step_ticks(&score, fixed_offset), 120);
        assert!(frames.len() > 12);
        assert_eq!(frames[0].s_measure, 1);
        assert_eq!(frames[12].s_measure, 2);
        assert!((frames[0].s_offset - 0.0).abs() < f64::EPSILON);
        assert!((frames[12].s_offset - 3.0).abs() < f64::EPSILON);
    }

    #[cfg(feature = "augnet_onnx_backend")]
    #[test]
    fn augnet_frame_builder_tie_end_suppresses_onset() {
        let score = NormalizedScore {
            meta: ScoreMeta {
                exercise_count: 1,
                key_signature: KeySignature {
                    tonic_pc: 0,
                    mode: ScaleMode::Major,
                },
                time_signature: TimeSignature {
                    numerator: 4,
                    denominator: 4,
                },
                ticks_per_quarter: 480,
            },
            voices: vec![
                Voice {
                    voice_index: 0,
                    name: "Upper".to_string(),
                    notes: vec![
                        NoteEvent {
                            note_id: "u0".to_string(),
                            voice_index: 0,
                            midi: 72,
                            start_tick: 0,
                            duration_ticks: 480,
                            tie_start: true,
                            tie_end: false,
                        },
                        NoteEvent {
                            note_id: "u1".to_string(),
                            voice_index: 0,
                            midi: 72,
                            start_tick: 480,
                            duration_ticks: 480,
                            tie_start: false,
                            tie_end: true,
                        },
                    ],
                },
                Voice {
                    voice_index: 1,
                    name: "Bass".to_string(),
                    notes: vec![
                        NoteEvent {
                            note_id: "b0".to_string(),
                            voice_index: 1,
                            midi: 60,
                            start_tick: 0,
                            duration_ticks: 480,
                            tie_start: false,
                            tie_end: false,
                        },
                        NoteEvent {
                            note_id: "b1".to_string(),
                            voice_index: 1,
                            midi: 62,
                            start_tick: 480,
                            duration_ticks: 480,
                            tie_start: false,
                            tie_end: false,
                        },
                    ],
                },
            ],
        };

        let frames = build_augnet_frames_from_score(&score, 0.25);
        let f4 = &frames[4];
        assert_eq!(
            f4.s_notes.as_ref().expect("notes"),
            &vec!["D4".to_string(), "C5".to_string()]
        );
        assert_eq!(
            f4.s_intervals.as_ref().expect("intervals"),
            &vec!["m7".to_string()]
        );
        assert_eq!(f4.s_is_onset.as_ref().expect("onsets"), &vec![true, false]);
    }
}
