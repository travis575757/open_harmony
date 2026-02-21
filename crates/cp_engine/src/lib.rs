use cp_core::{
    validate_score, AnalysisDiagnostic, AnalysisRequest, AnalysisResponse, AnalysisSummary, CoreError,
    PresetId, RuleId, Severity,
};
use cp_harmony::analyze_harmony;
use cp_rules::{rule_registry, validate_rule_params, RuleContext, RuleParamIssue};
use indexmap::{IndexMap, IndexSet};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
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

pub fn analyze(req: &AnalysisRequest) -> EngineResult<AnalysisResponse> {
    validate_score(&req.score)?;
    let resolved = resolve_preset(req)?;
    let registry = rule_registry();
    if let Err(issues) = validate_rule_params(
        resolved.active_rules.iter(),
        &resolved.rule_params,
        req.score.voices.len(),
    ) {
        return Err(EngineError::InvalidRuleParams { issues });
    }

    let rule_ctx = RuleContext {
        score: &req.score,
        preset_id: &req.config.preset_id,
        rule_params: &resolved.rule_params,
    };
    let mut diagnostics: Vec<AnalysisDiagnostic> = Vec::new();
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

    let error_count = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count();
    let warning_count = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Warning)
        .count();

    Ok(AnalysisResponse {
        diagnostics,
        harmonic_slices,
        nct_tags,
        summary: AnalysisSummary {
            total_diagnostics: error_count + warning_count,
            error_count,
            warning_count,
            active_rule_count: resolved.active_rules.len(),
        },
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cp_core::{
        AnalysisConfig, AnalysisRequest, KeySignature, NormalizedScore, NoteEvent, PresetId,
        ScaleMode, ScoreMeta, TimeSignature, Voice,
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
            },
        };
        let p = resolve_preset(&req).expect("resolve");
        assert!(p
            .rule_params
            .contains_key("gen.motion.contrary_and_oblique_preferred"));
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
            },
        };
        let err = analyze(&req).expect_err("must fail");
        assert!(matches!(err, EngineError::InvalidRuleParams { .. }));
    }
}
