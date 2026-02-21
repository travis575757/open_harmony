use cp_core::{AnalysisRequest, AnalysisResponse};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum IoError {
    #[error("invalid request json: {0}")]
    InvalidRequest(#[from] serde_json::Error),
}

pub fn parse_request_json(input: &str) -> Result<AnalysisRequest, IoError> {
    Ok(serde_json::from_str(input)?)
}

pub fn to_response_json(resp: &AnalysisResponse) -> Result<String, IoError> {
    Ok(serde_json::to_string(resp)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cp_core::{
        AnalysisConfig, AnalysisRequest, AnalysisResponse, AnalysisSummary, HarmonicRhythm,
        HarmonicSlice, KeySignature, NctTag, NormalizedScore, PresetId, ScaleMode, ScoreMeta,
        TimeSignature, Voice,
    };
    use std::collections::BTreeMap;

    #[test]
    fn parses_request_json() {
        let req = AnalysisRequest {
            score: NormalizedScore {
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
                voices: vec![Voice {
                    voice_index: 0,
                    name: "v0".to_string(),
                    notes: vec![],
                }],
            },
            config: AnalysisConfig {
                preset_id: PresetId::Species1,
                enabled_rule_ids: vec![],
                disabled_rule_ids: vec![],
                severity_overrides: BTreeMap::new(),
                rule_params: BTreeMap::new(),
                harmonic_rhythm: HarmonicRhythm::NoteOnset,
            },
        };
        let st = serde_json::to_string(&req).expect("serialize");
        let reparsed = parse_request_json(&st).expect("parse");
        assert_eq!(reparsed.score.meta.exercise_count, 1);
    }

    #[test]
    fn missing_harmonic_rhythm_defaults_to_note_onset() {
        let raw = r#"{
          "score": {
            "meta": {
              "exercise_count": 1,
              "key_signature": {"tonic_pc": 0, "mode": "major"},
              "time_signature": {"numerator": 4, "denominator": 4},
              "ticks_per_quarter": 480
            },
            "voices": [{"voice_index": 0, "name": "v0", "notes": []}]
          },
          "config": {
            "preset_id": "species1",
            "enabled_rule_ids": [],
            "disabled_rule_ids": [],
            "severity_overrides": {},
            "rule_params": {}
          }
        }"#;
        let reparsed = parse_request_json(raw).expect("parse legacy request");
        assert_eq!(reparsed.config.harmonic_rhythm, HarmonicRhythm::NoteOnset);
    }

    #[test]
    fn serializes_response_json() {
        let resp = AnalysisResponse {
            diagnostics: vec![],
            harmonic_slices: vec![HarmonicSlice {
                slice_id: 0,
                start_tick: 0,
                end_tick: 10,
                pitch_classes: vec![0, 4, 7],
                root_pc: Some(0),
                quality: Some("major".to_string()),
                inversion: Some("root".to_string()),
                roman_numeral: Some("I".to_string()),
                confidence: 0.9,
                inferred_root: Some(false),
                missing_tones: vec![],
                chord_form: Some("complete_triad".to_string()),
            }],
            nct_tags: vec![NctTag {
                note_id: "n1".to_string(),
                tag_type: "passing".to_string(),
                justification: "x".to_string(),
            }],
            summary: AnalysisSummary {
                total_diagnostics: 0,
                error_count: 0,
                warning_count: 0,
                active_rule_count: 1,
            },
            warnings: vec![],
        };
        let st = to_response_json(&resp).expect("json");
        assert!(st.contains("\"harmonic_slices\""));
    }
}
