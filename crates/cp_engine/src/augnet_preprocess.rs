#[cfg(feature = "augnet_onnx_backend")]
use crate::augnet_onnx::AugmentedNetInferenceOutput;
#[cfg(feature = "augnet_onnx_backend")]
use crate::augnet_onnx::AugmentedNetOnnxBackend;
use crate::augnet_onnx::{AugmentedNetInputTensors, AugmentedNetOnnxError, AugmentedNetOnnxResult};
use cp_music21_compat::{
    augnet_initial_frames, augnet_reindex_frames, encode_stage_b_inputs, parse_musicxml,
    AugnetScoreFrame, StageBInputs,
};
use serde::{Deserialize, Serialize};

pub const PREPROCESS_SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_FIXED_OFFSET: f64 = 0.125;
pub const DEFAULT_MAX_STEPS: usize = 640;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AugmentedNetPreprocessMode {
    #[default]
    Parity,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AugmentedNetPreprocessConfig {
    pub fixed_offset: f64,
    pub max_steps: usize,
    pub mode: AugmentedNetPreprocessMode,
}

impl Default for AugmentedNetPreprocessConfig {
    fn default() -> Self {
        Self {
            fixed_offset: DEFAULT_FIXED_OFFSET,
            max_steps: DEFAULT_MAX_STEPS,
            mode: AugmentedNetPreprocessMode::Parity,
        }
    }
}

impl AugmentedNetPreprocessConfig {
    fn validate(&self) -> AugmentedNetOnnxResult<()> {
        if !self.fixed_offset.is_finite() || self.fixed_offset <= 0.0 {
            return Err(AugmentedNetOnnxError::Preprocessing(format!(
                "fixed_offset must be finite and > 0, got {}",
                self.fixed_offset
            )));
        }
        if self.max_steps == 0 {
            return Err(AugmentedNetOnnxError::Preprocessing(
                "max_steps must be > 0".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AugmentedNetPreprocessChunk {
    pub chunk_index: usize,
    pub global_start_step: usize,
    pub global_end_step_exclusive: usize,
    pub tensors: AugmentedNetInputTensors,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AugmentedNetPreprocessArtifact {
    pub schema_version: u32,
    pub fixed_offset: f64,
    pub event_frames: Vec<AugnetScoreFrame>,
    pub grid_frames: Vec<AugnetScoreFrame>,
    pub chunks: Vec<AugmentedNetPreprocessChunk>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StageBTensorParityStats {
    pub tensor_count: usize,
    pub row_count: usize,
    pub value_count: usize,
    pub binary_value_count: usize,
    pub continuous_value_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StageBTensorParityMismatch {
    pub path: String,
    pub reason: String,
    pub expected: String,
    pub actual: String,
}

impl std::fmt::Display for StageBTensorParityMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "path={} reason={} expected={} actual={}",
            self.path, self.reason, self.expected, self.actual
        )
    }
}

impl std::error::Error for StageBTensorParityMismatch {}

pub fn stage_b_inputs_to_onnx_tensors(stage_b: StageBInputs) -> AugmentedNetInputTensors {
    AugmentedNetInputTensors {
        schema_version: stage_b.schema_version,
        fixed_offset: stage_b.fixed_offset,
        max_steps: stage_b.max_steps,
        active_steps: stage_b.active_steps,
        x_bass19: stage_b.x_bass19,
        x_chromagram19: stage_b.x_chromagram19,
        x_measure_note_onset14: stage_b.x_measure_note_onset14,
    }
}

pub fn preprocess_musicxml_to_chunks(
    musicxml: &str,
    config: &AugmentedNetPreprocessConfig,
) -> AugmentedNetOnnxResult<AugmentedNetPreprocessArtifact> {
    config.validate()?;
    let parsed = parse_musicxml(musicxml)
        .map_err(|e| AugmentedNetOnnxError::Preprocessing(format!("musicxml parse failed: {e}")))?;
    let event_frames = augnet_initial_frames(&parsed);
    let grid_frames = match config.mode {
        AugmentedNetPreprocessMode::Parity => {
            augnet_reindex_frames(&event_frames, config.fixed_offset)
        }
    };

    let chunks = encode_grid_chunks(&grid_frames, config.fixed_offset, config.max_steps);
    Ok(AugmentedNetPreprocessArtifact {
        schema_version: PREPROCESS_SCHEMA_VERSION,
        fixed_offset: config.fixed_offset,
        event_frames,
        grid_frames,
        chunks,
    })
}

#[cfg(feature = "augnet_onnx_backend")]
pub fn infer_musicxml_chunks(
    backend: &AugmentedNetOnnxBackend,
    musicxml: &str,
    config: &AugmentedNetPreprocessConfig,
) -> AugmentedNetOnnxResult<Vec<AugmentedNetInferenceOutput>> {
    let artifact = preprocess_musicxml_to_chunks(musicxml, config)?;
    backend.infer_preprocessed_chunks(&artifact.chunks)
}

pub fn compare_stage_b_tensors(
    expected: &AugmentedNetInputTensors,
    actual: &AugmentedNetInputTensors,
    float_atol: f32,
) -> Result<StageBTensorParityStats, StageBTensorParityMismatch> {
    if expected.schema_version != actual.schema_version {
        return Err(StageBTensorParityMismatch {
            path: "stage_b.schema_version".to_string(),
            reason: "schema version mismatch".to_string(),
            expected: expected.schema_version.to_string(),
            actual: actual.schema_version.to_string(),
        });
    }
    if expected.max_steps != actual.max_steps {
        return Err(StageBTensorParityMismatch {
            path: "stage_b.max_steps".to_string(),
            reason: "max_steps mismatch".to_string(),
            expected: expected.max_steps.to_string(),
            actual: actual.max_steps.to_string(),
        });
    }
    if expected.active_steps != actual.active_steps {
        return Err(StageBTensorParityMismatch {
            path: "stage_b.active_steps".to_string(),
            reason: "active_steps mismatch".to_string(),
            expected: expected.active_steps.to_string(),
            actual: actual.active_steps.to_string(),
        });
    }
    if expected.fixed_offset != actual.fixed_offset {
        return Err(StageBTensorParityMismatch {
            path: "stage_b.fixed_offset".to_string(),
            reason: "fixed_offset mismatch".to_string(),
            expected: format!("{:.8}", expected.fixed_offset),
            actual: format!("{:.8}", actual.fixed_offset),
        });
    }

    let mut stats = StageBTensorParityStats {
        tensor_count: 0,
        row_count: 0,
        value_count: 0,
        binary_value_count: 0,
        continuous_value_count: 0,
    };
    compare_matrix(
        "X_Bass19",
        &expected.x_bass19,
        &actual.x_bass19,
        float_atol,
        &mut stats,
    )?;
    compare_matrix(
        "X_Chromagram19",
        &expected.x_chromagram19,
        &actual.x_chromagram19,
        float_atol,
        &mut stats,
    )?;
    compare_matrix(
        "X_MeasureNoteOnset14",
        &expected.x_measure_note_onset14,
        &actual.x_measure_note_onset14,
        float_atol,
        &mut stats,
    )?;

    Ok(stats)
}

fn encode_grid_chunks(
    grid_frames: &[AugnetScoreFrame],
    fixed_offset: f64,
    max_steps: usize,
) -> Vec<AugmentedNetPreprocessChunk> {
    if grid_frames.is_empty() {
        let empty =
            stage_b_inputs_to_onnx_tensors(encode_stage_b_inputs(&[], fixed_offset, max_steps));
        return vec![AugmentedNetPreprocessChunk {
            chunk_index: 0,
            global_start_step: 0,
            global_end_step_exclusive: 0,
            tensors: empty,
        }];
    }

    let mut chunks = Vec::new();
    for (chunk_index, start) in (0..grid_frames.len()).step_by(max_steps).enumerate() {
        let end = (start + max_steps).min(grid_frames.len());
        let slice = &grid_frames[start..end];
        let stage_b = encode_stage_b_inputs(slice, fixed_offset, max_steps);
        chunks.push(AugmentedNetPreprocessChunk {
            chunk_index,
            global_start_step: start,
            global_end_step_exclusive: end,
            tensors: stage_b_inputs_to_onnx_tensors(stage_b),
        });
    }
    chunks
}

fn compare_matrix(
    tensor_name: &str,
    expected: &[Vec<f32>],
    actual: &[Vec<f32>],
    float_atol: f32,
    stats: &mut StageBTensorParityStats,
) -> Result<(), StageBTensorParityMismatch> {
    stats.tensor_count += 1;
    if expected.len() != actual.len() {
        return Err(StageBTensorParityMismatch {
            path: format!("stage_b.{tensor_name}.<rows>"),
            reason: "row count mismatch".to_string(),
            expected: expected.len().to_string(),
            actual: actual.len().to_string(),
        });
    }
    stats.row_count += expected.len();

    for (row_idx, (e_row, a_row)) in expected.iter().zip(actual).enumerate() {
        if e_row.len() != a_row.len() {
            return Err(StageBTensorParityMismatch {
                path: format!("stage_b.{tensor_name}[{row_idx}].<cols>"),
                reason: "column count mismatch".to_string(),
                expected: e_row.len().to_string(),
                actual: a_row.len().to_string(),
            });
        }
        for (col_idx, (ev, av)) in e_row.iter().zip(a_row).enumerate() {
            stats.value_count += 1;
            let path = format!("stage_b.{tensor_name}[{row_idx}][{col_idx}]");
            if is_binary_channel(tensor_name, col_idx) {
                stats.binary_value_count += 1;
                if !is_binary_value(*ev) {
                    return Err(StageBTensorParityMismatch {
                        path,
                        reason: "expected baseline value is non-binary for a binary channel"
                            .to_string(),
                        expected: format!("{ev:.6}"),
                        actual: format!("{av:.6}"),
                    });
                }
                if !is_binary_value(*av) {
                    return Err(StageBTensorParityMismatch {
                        path,
                        reason: "candidate value is non-binary for a binary channel".to_string(),
                        expected: format!("{ev:.6}"),
                        actual: format!("{av:.6}"),
                    });
                }
                if ev != av {
                    return Err(StageBTensorParityMismatch {
                        path,
                        reason: "binary channel mismatch (exact equality required)".to_string(),
                        expected: format!("{ev:.6}"),
                        actual: format!("{av:.6}"),
                    });
                }
            } else {
                stats.continuous_value_count += 1;
                if (ev - av).abs() > float_atol {
                    return Err(StageBTensorParityMismatch {
                        path,
                        reason: format!("continuous channel mismatch (|diff|>{float_atol})"),
                        expected: format!("{ev:.6}"),
                        actual: format!("{av:.6}"),
                    });
                }
            }
        }
    }

    Ok(())
}

fn is_binary_channel(tensor_name: &str, col_idx: usize) -> bool {
    match tensor_name {
        "X_Bass19" => col_idx < 19,
        "X_Chromagram19" => col_idx < 19,
        "X_MeasureNoteOnset14" => true,
        _ => false,
    }
}

fn is_binary_value(v: f32) -> bool {
    v == -1.0 || v == 0.0 || v == 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_channel_detection_matches_contract() {
        assert!(is_binary_channel("X_Bass19", 0));
        assert!(is_binary_channel("X_Bass19", 14));
        assert!(is_binary_channel("X_Bass19", 15));
        assert!(is_binary_channel("X_Bass19", 18));

        assert!(is_binary_channel("X_Chromagram19", 0));
        assert!(is_binary_channel("X_Chromagram19", 11));
        assert!(is_binary_channel("X_Chromagram19", 14));
        assert!(is_binary_channel("X_Chromagram19", 16));
        assert!(is_binary_channel("X_Chromagram19", 12));

        assert!(is_binary_channel("X_MeasureNoteOnset14", 13));
        assert!(is_binary_value(-1.0));
    }

    #[test]
    fn tensor_compare_reports_actionable_path() {
        let expected = AugmentedNetInputTensors {
            schema_version: 1,
            fixed_offset: 0.25,
            max_steps: 1,
            active_steps: 1,
            x_bass19: vec![vec![1.0; 19]],
            x_chromagram19: vec![vec![0.0; 19]],
            x_measure_note_onset14: vec![vec![0.0; 14]],
        };
        let mut actual = expected.clone();
        actual.x_bass19[0][0] = 0.0;
        let mismatch = compare_stage_b_tensors(&expected, &actual, 1e-6).expect_err("mismatch");
        assert_eq!(mismatch.path, "stage_b.X_Bass19[0][0]");
        assert!(mismatch.reason.contains("binary channel mismatch"));
    }
}
