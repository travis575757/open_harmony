use serde::{Deserialize, Serialize};

use crate::augnet::AugnetScoreFrame;
use crate::pitch::{PitchSpelling, Step};

const PADDING_VALUE: f32 = -1.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StageBInputs {
    pub schema_version: u32,
    pub fixed_offset: f64,
    pub max_steps: usize,
    pub active_steps: usize,
    #[serde(rename = "X_Bass19")]
    pub x_bass19: Vec<Vec<f32>>,
    #[serde(rename = "X_Chromagram19")]
    pub x_chromagram19: Vec<Vec<f32>>,
    #[serde(rename = "X_MeasureNoteOnset14")]
    pub x_measure_note_onset14: Vec<Vec<f32>>,
}

fn build_onset_pattern() -> Vec<[f32; 7]> {
    let mut out = Vec::with_capacity(64);
    for x in 0..64usize {
        let bits = format!("{x:06b}0");
        let mut arr = [0.0f32; 7];
        for (idx, ch) in bits.chars().rev().enumerate() {
            arr[idx] = if ch == '1' { 1.0 } else { 0.0 };
        }
        out.push(arr);
    }
    out[0][0] = 1.0;
    out
}

fn parse_step_and_pc(note: &str) -> Option<(usize, usize)> {
    let (pitch, _) = PitchSpelling::parse_m21_pitch_name(note).ok()?;
    let step_idx = match pitch.step {
        Step::C => 0usize,
        Step::D => 1usize,
        Step::E => 2usize,
        Step::F => 3usize,
        Step::G => 4usize,
        Step::A => 5usize,
        Step::B => 6usize,
    };
    Some((step_idx, pitch.pitch_class() as usize))
}

pub fn encode_stage_b_inputs(
    grid_frames: &[AugnetScoreFrame],
    fixed_offset: f64,
    max_steps: usize,
) -> StageBInputs {
    let steps = max_steps.max(1);
    let active_steps = grid_frames.len().min(steps);

    // AugmentedNet padToSequenceLength(..., value=-1) semantics.
    let mut x_bass19 = vec![vec![PADDING_VALUE; 19]; steps];
    let mut x_chromagram19 = vec![vec![PADDING_VALUE; 19]; steps];
    let mut x_measure_note_onset14 = vec![vec![PADDING_VALUE; 14]; steps];

    let onset_pattern = build_onset_pattern();
    let mut prev_measure = i32::MIN;
    let mut measure_idx = 0usize;
    let mut note_idx = 0usize;

    for t in 0..active_steps {
        let frame = &grid_frames[t];
        let notes = frame.s_notes.as_deref().unwrap_or(&[]);
        let onsets = frame.s_is_onset.as_deref().unwrap_or(&[]);

        x_bass19[t].fill(0.0);
        x_chromagram19[t].fill(0.0);
        x_measure_note_onset14[t].fill(0.0);

        // Bass19 = Bass7(letter one-hot) + Bass12(pc one-hot).
        if let Some(first) = notes.first() {
            if let Some((step_idx, pc_idx)) = parse_step_and_pc(first) {
                x_bass19[t][step_idx] = 1.0;
                x_bass19[t][7 + pc_idx] = 1.0;
            }
        }

        // Chromagram19 = Chromagram7(letter many-hot) + Chromagram12(pc many-hot).
        for note in notes {
            if let Some((step_idx, pc_idx)) = parse_step_and_pc(note) {
                x_chromagram19[t][step_idx] = 1.0;
                x_chromagram19[t][7 + pc_idx] = 1.0;
            }
        }

        // MeasureOnset7.
        if frame.s_measure != prev_measure {
            measure_idx = 0;
            prev_measure = frame.s_measure;
        }
        for i in 0..7 {
            x_measure_note_onset14[t][i] = onset_pattern[measure_idx][i];
        }
        measure_idx = (measure_idx + 1).min(onset_pattern.len() - 1);

        // NoteOnset7.
        if onsets.iter().any(|v| *v) {
            note_idx = 0;
        }
        for i in 0..7 {
            x_measure_note_onset14[t][7 + i] = onset_pattern[note_idx][i];
        }
        note_idx = (note_idx + 1).min(onset_pattern.len() - 1);
    }

    StageBInputs {
        schema_version: 1,
        fixed_offset,
        max_steps: steps,
        active_steps,
        x_bass19,
        x_chromagram19,
        x_measure_note_onset14,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_step_and_pc_handles_double_accidentals() {
        assert_eq!(parse_step_and_pc("C4"), Some((0, 0)));
        assert_eq!(parse_step_and_pc("E--4"), Some((2, 2)));
        assert_eq!(parse_step_and_pc("F##4"), Some((3, 7)));
        assert_eq!(parse_step_and_pc("G-4"), Some((4, 6)));
    }

    #[test]
    fn stage_b_uses_augnet_padding_and_feature_layout() {
        let frames = vec![
            AugnetScoreFrame {
                s_offset: 0.0,
                s_duration: 0.25,
                s_measure: 1,
                s_notes: Some(vec!["C4".to_string(), "E4".to_string(), "G4".to_string()]),
                s_intervals: Some(vec!["M3".to_string(), "P5".to_string()]),
                s_is_onset: Some(vec![true, true, true]),
            },
            AugnetScoreFrame {
                s_offset: 0.25,
                s_duration: 0.25,
                s_measure: 1,
                s_notes: Some(vec!["C4".to_string(), "E4".to_string(), "G4".to_string()]),
                s_intervals: Some(vec!["M3".to_string(), "P5".to_string()]),
                s_is_onset: Some(vec![false, false, false]),
            },
        ];

        let encoded = encode_stage_b_inputs(&frames, 0.125, 4);
        assert_eq!(encoded.active_steps, 2);

        // Bass7(C) + Bass12(C)
        assert_eq!(encoded.x_bass19[0][0], 1.0);
        assert_eq!(encoded.x_bass19[0][7], 1.0);
        assert_eq!(encoded.x_bass19[0].iter().sum::<f32>(), 2.0);

        // Chromagram7(C,E,G) + Chromagram12(0,4,7)
        assert_eq!(encoded.x_chromagram19[0][0], 1.0);
        assert_eq!(encoded.x_chromagram19[0][2], 1.0);
        assert_eq!(encoded.x_chromagram19[0][4], 1.0);
        assert_eq!(encoded.x_chromagram19[0][7], 1.0);
        assert_eq!(encoded.x_chromagram19[0][11], 1.0);
        assert_eq!(encoded.x_chromagram19[0][14], 1.0);

        // MeasureNoteOnset14 first frame starts both measure and note-onset counters at index 0.
        assert_eq!(encoded.x_measure_note_onset14[0][0], 1.0);
        assert_eq!(encoded.x_measure_note_onset14[0][7], 1.0);

        // Padded frames remain sentinel -1.
        assert!(encoded.x_bass19[2].iter().all(|v| *v == PADDING_VALUE));
        assert!(encoded.x_chromagram19[3]
            .iter()
            .all(|v| *v == PADDING_VALUE));
        assert!(encoded.x_measure_note_onset14[2]
            .iter()
            .all(|v| *v == PADDING_VALUE));
    }

    #[test]
    fn encoding_is_deterministic() {
        let frame = AugnetScoreFrame {
            s_offset: 0.0,
            s_duration: 0.25,
            s_measure: 1,
            s_notes: Some(vec!["C4".to_string(), "E4".to_string(), "G4".to_string()]),
            s_intervals: Some(vec!["M3".to_string(), "P5".to_string()]),
            s_is_onset: Some(vec![true, true, true]),
        };
        let a = encode_stage_b_inputs(&[frame.clone()], 0.125, 4);
        let b = encode_stage_b_inputs(&[frame], 0.125, 4);
        assert_eq!(a, b);
    }
}
