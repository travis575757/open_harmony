//! Compatibility layer for the Phase 2 AugmentedNet preprocessing/postprocessing path.
//!
//! This crate ports the full `music21` behavior surface used by AugmentedNet's parser,
//! representation, and tonicization helpers. It is not a full generic `music21` clone.
//!
//! Implemented-vs-out-of-scope compatibility boundaries are documented in
//! `crates/cp_music21_compat/README.md` under "Compatibility matrix".

mod augnet;
mod encoding;
mod error;
mod interval;
mod key;
mod musicxml;
mod pitch;
mod timeline;

pub use augnet::{augnet_initial_frames, augnet_reindex_frames, AugnetScoreFrame};
pub use encoding::{encode_stage_b_inputs, StageBInputs};
pub use error::CompatError;
pub use interval::{
    interval_class_info, interval_label, parse_interval_spec, simple_interval_name,
    IntervalClassInfo, IntervalSpec,
};
pub use key::{
    tonic_relative_pc, tonicization_denominator, tonicization_scale_degree, transpose_key_m21,
    transpose_pcset, transpose_pitch_m21, weber_euclidean, KeyContext, KeyMode,
};
pub use musicxml::{parse_musicxml, MeasureSpan, ParsedNote, ParsedScore};
pub use pitch::{PitchSpelling, Step};
pub use timeline::{build_timeline, SliceNote, TimelineArtifact, TimelineSlice};

/// Stable API boundary for ONNX-adjacent layers.
///
/// Future native replacements should implement this trait so adapter/UI code can remain unchanged.
pub trait Music21CompatApi {
    fn timeline_from_musicxml(
        &self,
        source_id: &str,
        musicxml: &str,
    ) -> Result<TimelineArtifact, CompatError>;
    fn augnet_frames_from_musicxml(
        &self,
        musicxml: &str,
        fixed_offset: f64,
        event_based: bool,
    ) -> Result<Vec<AugnetScoreFrame>, CompatError>;
    fn tonicization_denominator(&self, pitch: &PitchSpelling, key: &KeyContext) -> String;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Music21Compat;

impl Music21CompatApi for Music21Compat {
    fn timeline_from_musicxml(
        &self,
        source_id: &str,
        musicxml: &str,
    ) -> Result<TimelineArtifact, CompatError> {
        let parsed = parse_musicxml(musicxml)?;
        Ok(build_timeline(&parsed, source_id))
    }

    fn augnet_frames_from_musicxml(
        &self,
        musicxml: &str,
        fixed_offset: f64,
        event_based: bool,
    ) -> Result<Vec<AugnetScoreFrame>, CompatError> {
        let parsed = parse_musicxml(musicxml)?;
        let initial = augnet_initial_frames(&parsed);
        if event_based {
            Ok(initial)
        } else {
            Ok(augnet_reindex_frames(&initial, fixed_offset))
        }
    }

    fn tonicization_denominator(&self, pitch: &PitchSpelling, key: &KeyContext) -> String {
        tonicization_denominator(pitch, key)
    }
}

/// Deterministic JSON serialization for parity artifacts.
///
/// Field order is fixed by typed structs, and all collections are pre-sorted before serialization.
pub fn serialize_timeline_artifact(artifact: &TimelineArtifact) -> Result<String, CompatError> {
    serde_json::to_string_pretty(artifact).map_err(|e| CompatError::Serialization(e.to_string()))
}

pub fn serialize_augnet_frames(frames: &[AugnetScoreFrame]) -> Result<String, CompatError> {
    serde_json::to_string_pretty(frames).map_err(|e| CompatError::Serialization(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pitch::Step;

    #[test]
    fn api_trait_works_with_default_impl() {
        let api = Music21Compat;
        let key = KeyContext {
            tonic: PitchSpelling {
                step: Step::C,
                alter: 0,
                octave: 4,
            },
            mode: KeyMode::Major,
        };
        let d = PitchSpelling {
            step: Step::D,
            alter: 0,
            octave: 4,
        };
        assert_eq!(api.tonicization_denominator(&d, &key), "II");
    }

    #[test]
    fn deterministic_serialization_is_stable() {
        let artifact = TimelineArtifact {
            schema_version: 1,
            source_id: "x".to_string(),
            measure_number_shift: 0,
            slices: vec![],
        };
        let a = serialize_timeline_artifact(&artifact).expect("serialize");
        let b = serialize_timeline_artifact(&artifact).expect("serialize");
        assert_eq!(a, b);
    }

    #[test]
    fn augnet_frame_serialization_is_stable() {
        let frames = vec![AugnetScoreFrame {
            s_offset: 0.0,
            s_duration: 0.25,
            s_measure: 1,
            s_notes: Some(vec!["C4".to_string()]),
            s_intervals: Some(vec![]),
            s_is_onset: Some(vec![true]),
        }];
        let a = serialize_augnet_frames(&frames).expect("serialize");
        let b = serialize_augnet_frames(&frames).expect("serialize");
        assert_eq!(a, b);
    }
}
