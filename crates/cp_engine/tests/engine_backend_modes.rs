use cp_core::{
    AnalysisBackend, AnalysisConfig, AnalysisRequest, AugmentedNetBackendConfig, HarmonicRhythm,
    KeySignature, NormalizedScore, NoteEvent, PresetId, ScaleMode, ScoreMeta, TimeSignature, Voice,
};
use cp_engine::{analyze, EngineError};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

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
                        note_id: "s0".to_string(),
                        voice_index: 0,
                        midi: 60,
                        start_tick: 0,
                        duration_ticks: 480,
                        tie_start: false,
                        tie_end: false,
                    },
                    NoteEvent {
                        note_id: "s1".to_string(),
                        voice_index: 0,
                        midi: 67,
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
                        note_id: "b0".to_string(),
                        voice_index: 1,
                        midi: 53,
                        start_tick: 0,
                        duration_ticks: 480,
                        tie_start: false,
                        tie_end: false,
                    },
                    NoteEvent {
                        note_id: "b1".to_string(),
                        voice_index: 1,
                        midi: 60,
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

fn open_fifth_score() -> NormalizedScore {
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
                name: "Upper".to_string(),
                notes: vec![NoteEvent {
                    note_id: "u0".to_string(),
                    voice_index: 0,
                    midi: 67,
                    start_tick: 0,
                    duration_ticks: 960,
                    tie_start: false,
                    tie_end: false,
                }],
            },
            Voice {
                voice_index: 1,
                name: "Lower".to_string(),
                notes: vec![NoteEvent {
                    note_id: "l0".to_string(),
                    voice_index: 1,
                    midi: 60,
                    start_tick: 0,
                    duration_ticks: 960,
                    tie_start: false,
                    tie_end: false,
                }],
            },
        ],
    }
}

fn config(backend: AnalysisBackend) -> AnalysisConfig {
    AnalysisConfig {
        preset_id: PresetId::Species1,
        enabled_rule_ids: vec![],
        disabled_rule_ids: vec![],
        severity_overrides: BTreeMap::new(),
        rule_params: BTreeMap::new(),
        harmonic_rhythm: HarmonicRhythm::NoteOnset,
        analysis_backend: backend,
        augnet_backend: AugmentedNetBackendConfig::default(),
    }
}

fn request(score: NormalizedScore, backend: AnalysisBackend) -> AnalysisRequest {
    AnalysisRequest {
        score,
        config: config(backend),
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

#[test]
fn backend_modes_rule_based_mode_end_to_end() {
    let req = request(base_score(), AnalysisBackend::RuleBased);
    let res = analyze(&req).expect("rule_based analysis");
    assert!(!res.harmonic_slices.is_empty());
    assert!(!res.harmonic_outputs.is_empty());
    assert!(res
        .harmonic_outputs
        .iter()
        .all(|output| output.source == cp_core::HarmonicOutputSource::RuleBased));
    assert!(res
        .harmonic_outputs
        .iter()
        .all(|output| output.logits.is_empty()));
}

#[test]
fn backend_modes_augnet_onnx_mode_end_to_end() {
    let req = request(base_score(), AnalysisBackend::AugnetOnnx);
    let res = analyze(&req).expect("augnet_onnx analysis");
    assert!(!res.harmonic_slices.is_empty());
    assert!(!res.harmonic_outputs.is_empty());
    assert!(res.diagnostics.is_empty());
    assert!(res
        .harmonic_outputs
        .iter()
        .all(|output| output.source == cp_core::HarmonicOutputSource::AugnetOnnx));
}

#[test]
fn backend_modes_hybrid_mode_end_to_end() {
    let mut req = request(base_score(), AnalysisBackend::Hybrid);
    req.config
        .enabled_rule_ids
        .push("gen.motion.parallel_perfects_forbidden".to_string());

    let hybrid = analyze(&req).expect("hybrid analysis");
    assert!(!hybrid.harmonic_outputs.is_empty());
    assert!(hybrid
        .harmonic_outputs
        .iter()
        .any(|output| output.source == cp_core::HarmonicOutputSource::RuleBased));
    assert!(hybrid
        .harmonic_outputs
        .iter()
        .any(|output| output.source == cp_core::HarmonicOutputSource::AugnetOnnx));

    let mut rule_req = req.clone();
    rule_req.config.analysis_backend = AnalysisBackend::RuleBased;
    let rule_only = analyze(&rule_req).expect("rule_based analysis");
    let rule_ids_hybrid: BTreeSet<String> = hybrid
        .diagnostics
        .iter()
        .filter(|d| d.rule_id != "hybrid.harmony.disagreement")
        .map(|d| d.rule_id.clone())
        .collect();
    let rule_ids_rule_mode: BTreeSet<String> = rule_only
        .diagnostics
        .iter()
        .map(|d| d.rule_id.clone())
        .collect();
    assert_eq!(rule_ids_hybrid, rule_ids_rule_mode);
}

#[test]
fn backend_modes_hybrid_emits_disagreement_diagnostic_with_context_and_preserves_both_sides() {
    let req = request(open_fifth_score(), AnalysisBackend::Hybrid);
    let res = analyze(&req).expect("hybrid analysis");

    let disagreement = res
        .diagnostics
        .iter()
        .find(|d| d.rule_id == "hybrid.harmony.disagreement")
        .expect("expected hybrid disagreement diagnostic");
    assert_eq!(disagreement.severity, cp_core::Severity::Info);
    assert!(disagreement.primary.note_id.len() > 0);
    assert!(disagreement
        .context
        .contains_key("rule_based.interpretation"));
    assert!(disagreement
        .context
        .contains_key("augnet_onnx.interpretation"));
    assert_eq!(
        disagreement
            .context
            .get("rule_based.source")
            .map(|s| s.as_str()),
        Some("rule_based")
    );
    assert_eq!(
        disagreement
            .context
            .get("augnet_onnx.source")
            .map(|s| s.as_str()),
        Some("augnet_onnx")
    );

    let has_rule = res
        .harmonic_outputs
        .iter()
        .any(|output| output.source == cp_core::HarmonicOutputSource::RuleBased);
    let has_augnet = res
        .harmonic_outputs
        .iter()
        .any(|output| output.source == cp_core::HarmonicOutputSource::AugnetOnnx);
    assert!(
        has_rule && has_augnet,
        "both interpretations must be preserved"
    );
}

#[test]
fn backend_modes_output_contract_includes_source_attribution_and_logits() {
    let augnet = analyze(&request(base_score(), AnalysisBackend::AugnetOnnx)).expect("augnet");
    let first_augnet = augnet.harmonic_outputs.first().expect("augnet output");
    assert_eq!(
        first_augnet.source,
        cp_core::HarmonicOutputSource::AugnetOnnx
    );
    assert!(
        first_augnet.logits.contains_key("RomanNumeral31"),
        "roman numeral logits missing"
    );

    let hybrid = analyze(&request(base_score(), AnalysisBackend::Hybrid)).expect("hybrid");
    assert!(hybrid.harmonic_outputs.iter().any(|output| output.source
        == cp_core::HarmonicOutputSource::AugnetOnnx
        && !output.logits.is_empty()));
    assert!(hybrid.harmonic_outputs.iter().any(|output| output.source
        == cp_core::HarmonicOutputSource::RuleBased
        && output.logits.is_empty()));
}

#[test]
fn backend_modes_augnet_mode_backend_unavailable_is_fatal() {
    let mut req = request(base_score(), AnalysisBackend::AugnetOnnx);
    req.config.augnet_backend.model_path = Some(
        repo_root()
            .join("models/augnet/DOES_NOT_EXIST.onnx")
            .to_string_lossy()
            .to_string(),
    );
    let err = analyze(&req).expect_err("missing model must fail");
    assert!(matches!(
        err,
        EngineError::BackendUnavailable {
            backend: "augnet_onnx",
            ..
        }
    ));
}

#[test]
fn backend_modes_hybrid_mode_backend_unavailable_is_fatal() {
    let mut req = request(base_score(), AnalysisBackend::Hybrid);
    req.config.augnet_backend.model_path = Some(
        repo_root()
            .join("models/augnet/DOES_NOT_EXIST.onnx")
            .to_string_lossy()
            .to_string(),
    );
    let err = analyze(&req).expect_err("missing model must fail");
    assert!(matches!(
        err,
        EngineError::BackendUnavailable {
            backend: "hybrid",
            ..
        }
    ));
}
