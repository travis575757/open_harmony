use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type RuleId = String;
pub type NoteId = String;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScaleMode {
    Major,
    Minor,
    Dorian,
    Phrygian,
    Lydian,
    Mixolydian,
    Aeolian,
    Ionian,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeySignature {
    pub tonic_pc: u8,
    pub mode: ScaleMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimeSignature {
    pub numerator: u8,
    pub denominator: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteEvent {
    pub note_id: NoteId,
    pub voice_index: u8,
    pub midi: i16,
    pub start_tick: u32,
    pub duration_ticks: u32,
    #[serde(default)]
    pub tie_start: bool,
    #[serde(default)]
    pub tie_end: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Voice {
    pub voice_index: u8,
    pub name: String,
    pub notes: Vec<NoteEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreMeta {
    #[serde(default = "default_exercise_count")]
    pub exercise_count: u8,
    pub key_signature: KeySignature,
    pub time_signature: TimeSignature,
    #[serde(default)]
    pub ticks_per_quarter: u32,
}

fn default_exercise_count() -> u8 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedScore {
    pub meta: ScoreMeta,
    pub voices: Vec<Voice>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PresetId {
    Species1,
    Species2,
    Species3,
    Species4,
    Species5,
    GeneralVoiceLeading,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisConfig {
    pub preset_id: PresetId,
    #[serde(default)]
    pub enabled_rule_ids: Vec<RuleId>,
    #[serde(default)]
    pub disabled_rule_ids: Vec<RuleId>,
    #[serde(default)]
    pub severity_overrides: std::collections::BTreeMap<RuleId, Severity>,
    #[serde(default)]
    pub rule_params: std::collections::BTreeMap<RuleId, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisRequest {
    pub score: NormalizedScore,
    pub config: AnalysisConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteLocation {
    pub measure: u32,
    pub beat: u32,
    pub tick: u32,
    pub voice_index: u8,
    pub note_id: NoteId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisDiagnostic {
    pub rule_id: RuleId,
    pub severity: Severity,
    pub message: String,
    pub primary: NoteLocation,
    pub related: Option<NoteLocation>,
    #[serde(default)]
    pub context: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarmonicSlice {
    pub slice_id: u32,
    pub start_tick: u32,
    pub end_tick: u32,
    pub pitch_classes: Vec<u8>,
    pub root_pc: Option<u8>,
    pub quality: Option<String>,
    pub inversion: Option<String>,
    pub roman_numeral: Option<String>,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NctTag {
    pub note_id: NoteId,
    pub tag_type: String,
    pub justification: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisSummary {
    pub total_diagnostics: usize,
    pub error_count: usize,
    pub warning_count: usize,
    pub active_rule_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResponse {
    pub diagnostics: Vec<AnalysisDiagnostic>,
    pub harmonic_slices: Vec<HarmonicSlice>,
    pub nct_tags: Vec<NctTag>,
    pub summary: AnalysisSummary,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("voice count {0} exceeds supported max 4")]
    VoiceCountExceeded(usize),
    #[error("exercise_count must be 1, got {0}")]
    ExerciseCountInvalid(u8),
    #[error("invalid time signature {0}/{1}")]
    InvalidTimeSignature(u8, u8),
    #[error("note duration must be > 0 for note {0}")]
    InvalidNoteDuration(String),
}

pub fn validate_score(score: &NormalizedScore) -> Result<(), CoreError> {
    if score.meta.exercise_count != 1 {
        return Err(CoreError::ExerciseCountInvalid(score.meta.exercise_count));
    }
    if score.voices.len() > 4 {
        return Err(CoreError::VoiceCountExceeded(score.voices.len()));
    }
    let ts = &score.meta.time_signature;
    if ts.numerator == 0 || ts.denominator == 0 {
        return Err(CoreError::InvalidTimeSignature(ts.numerator, ts.denominator));
    }
    for voice in &score.voices {
        for note in &voice.notes {
            if note.duration_ticks == 0 {
                return Err(CoreError::InvalidNoteDuration(note.note_id.clone()));
            }
        }
    }
    Ok(())
}

pub fn ticks_per_measure(ts: &TimeSignature, tpq: u32) -> u32 {
    let quarter_factor = 4.0 / ts.denominator as f32;
    (ts.numerator as f32 * quarter_factor * tpq as f32) as u32
}

pub fn note_location(note: &NoteEvent, score: &NormalizedScore) -> NoteLocation {
    let tpq = if score.meta.ticks_per_quarter == 0 {
        480
    } else {
        score.meta.ticks_per_quarter
    };
    let tpm = ticks_per_measure(&score.meta.time_signature, tpq).max(1);
    let measure = note.start_tick / tpm + 1;
    let beat_ticks = (tpm / score.meta.time_signature.numerator as u32).max(1);
    let beat = (note.start_tick % tpm) / beat_ticks + 1;
    NoteLocation {
        measure,
        beat,
        tick: note.start_tick,
        voice_index: note.voice_index,
        note_id: note.note_id.clone(),
    }
}

pub fn interval_pc(a: i16, b: i16) -> u8 {
    ((a - b).rem_euclid(12)) as u8
}

pub fn is_perfect(pc: u8) -> bool {
    pc == 0 || pc == 7
}

pub fn is_consonant(pc: u8) -> bool {
    matches!(pc, 0 | 3 | 4 | 7 | 8 | 9)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_score(voices: usize, exercise_count: u8) -> NormalizedScore {
        NormalizedScore {
            meta: ScoreMeta {
                exercise_count,
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
            voices: (0..voices)
                .map(|i| Voice {
                    voice_index: i as u8,
                    name: format!("v{}", i),
                    notes: vec![NoteEvent {
                        note_id: format!("n{}", i),
                        voice_index: i as u8,
                        midi: 60 + i as i16,
                        start_tick: 0,
                        duration_ticks: 480,
                        tie_start: false,
                        tie_end: false,
                    }],
                })
                .collect(),
        }
    }

    #[test]
    fn validate_rejects_too_many_voices() {
        let s = mk_score(5, 1);
        let err = validate_score(&s).expect_err("must fail");
        assert!(matches!(err, CoreError::VoiceCountExceeded(5)));
    }

    #[test]
    fn validate_rejects_multi_exercise() {
        let s = mk_score(2, 2);
        let err = validate_score(&s).expect_err("must fail");
        assert!(matches!(err, CoreError::ExerciseCountInvalid(2)));
    }

    #[test]
    fn note_location_measure_beat_math() {
        let s = mk_score(1, 1);
        let n = NoteEvent {
            note_id: "x".to_string(),
            voice_index: 0,
            midi: 60,
            start_tick: 960,
            duration_ticks: 120,
            tie_start: false,
            tie_end: false,
        };
        let loc = note_location(&n, &s);
        assert_eq!(loc.measure, 1);
        assert_eq!(loc.beat, 3);
        assert_eq!(loc.tick, 960);
    }
}
