#[cfg(feature = "augnet_onnx_backend")]
use ort::{
    execution_providers::{CPUExecutionProviderOptions, ExecutionProvider},
    tensor::TensorElementDataType,
    value::Value,
    Environment, Session, SessionBuilder,
};
use serde::{Deserialize, Serialize};
#[cfg(feature = "augnet_onnx_backend")]
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
#[cfg(feature = "augnet_onnx_backend")]
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};
#[cfg(feature = "augnet_onnx_backend")]
use std::sync::Arc;
use thiserror::Error;

pub const AUGNET_INPUT_ORDER: [&str; 3] = ["X_Bass19", "X_Chromagram19", "X_MeasureNoteOnset14"];
pub const AUGNET_HEAD_ORDER: [&str; 9] = [
    "Alto35",
    "Bass35",
    "HarmonicRhythm7",
    "LocalKey38",
    "PitchClassSet121",
    "RomanNumeral31",
    "Soprano35",
    "Tenor35",
    "TonicizedKey38",
];
pub const PARITY_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone)]
pub struct AugmentedNetOnnxConfig {
    pub model_path: PathBuf,
    pub manifest_path: PathBuf,
    pub expected_model_id: String,
    pub expected_opset: i64,
    pub intra_threads: usize,
    pub inter_threads: usize,
}

impl Default for AugmentedNetOnnxConfig {
    fn default() -> Self {
        Self {
            model_path: PathBuf::from("models/augnet/AugmentedNet.onnx"),
            manifest_path: PathBuf::from("models/augnet/model-manifest.json"),
            expected_model_id: "augmentednet-v1".to_string(),
            expected_opset: 13,
            intra_threads: 1,
            inter_threads: 1,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct AugmentedNetModelManifest {
    pub model_id: String,
    pub onnx: ManifestOnnxSection,
    pub signature: ManifestSignatureSection,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ManifestOnnxSection {
    pub onnx_sha256: String,
    pub opset: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ManifestSignatureSection {
    pub onnx_input_names: Vec<String>,
    pub onnx_input_shapes: Vec<Vec<Option<i64>>>,
    pub onnx_output_heads: Vec<String>,
    pub onnx_output_names: Vec<String>,
    pub output_head_order_match: bool,
    pub fixed_time_axis_contract: ManifestFixedTimeAxisContract,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ManifestFixedTimeAxisContract {
    pub all_inputs_fixed: bool,
    pub dimension: usize,
    pub enforced: bool,
    pub lengths: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AugmentedNetInputTensors {
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

impl AugmentedNetInputTensors {
    pub fn validate_contract(&self, fixed_t: usize) -> AugmentedNetOnnxResult<()> {
        if self.max_steps != fixed_t {
            return Err(AugmentedNetOnnxError::InputContract(format!(
                "fixed-T contract violation: max_steps={}, expected={fixed_t}",
                self.max_steps
            )));
        }
        if self.active_steps > self.max_steps {
            return Err(AugmentedNetOnnxError::InputContract(format!(
                "active_steps {} exceeds max_steps {}",
                self.active_steps, self.max_steps
            )));
        }
        validate_matrix("X_Bass19", &self.x_bass19, self.max_steps, 19)?;
        validate_matrix("X_Chromagram19", &self.x_chromagram19, self.max_steps, 19)?;
        validate_matrix(
            "X_MeasureNoteOnset14",
            &self.x_measure_note_onset14,
            self.max_steps,
            14,
        )?;
        Ok(())
    }

    fn effective_steps(&self) -> usize {
        self.active_steps.max(1).min(self.max_steps)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StageCHeadArtifact {
    pub shape: [usize; 2],
    pub logits: Vec<Vec<f32>>,
    pub argmax: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StageCArtifact {
    pub schema_version: u32,
    pub effective_steps: usize,
    pub heads: BTreeMap<String, StageCHeadArtifact>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AugmentedNetHeadOutput {
    pub shape: [usize; 2],
    pub raw_logits: Vec<Vec<f32>>,
    pub argmax: Vec<usize>,
}

impl AugmentedNetHeadOutput {
    fn as_stage_c_head(&self) -> StageCHeadArtifact {
        StageCHeadArtifact {
            shape: self.shape,
            logits: self
                .raw_logits
                .iter()
                .map(|row| row.iter().map(|v| round6(*v)).collect())
                .collect(),
            argmax: self.argmax.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AugmentedNetTypedOutputs {
    pub alto35: AugmentedNetHeadOutput,
    pub bass35: AugmentedNetHeadOutput,
    pub harmonic_rhythm7: AugmentedNetHeadOutput,
    pub local_key38: AugmentedNetHeadOutput,
    pub pitch_class_set121: AugmentedNetHeadOutput,
    pub roman_numeral31: AugmentedNetHeadOutput,
    pub soprano35: AugmentedNetHeadOutput,
    pub tenor35: AugmentedNetHeadOutput,
    pub tonicized_key38: AugmentedNetHeadOutput,
}

impl AugmentedNetTypedOutputs {
    fn from_head_map(
        heads: &BTreeMap<String, AugmentedNetHeadOutput>,
    ) -> AugmentedNetOnnxResult<Self> {
        Ok(Self {
            alto35: required_head(heads, "Alto35")?,
            bass35: required_head(heads, "Bass35")?,
            harmonic_rhythm7: required_head(heads, "HarmonicRhythm7")?,
            local_key38: required_head(heads, "LocalKey38")?,
            pitch_class_set121: required_head(heads, "PitchClassSet121")?,
            roman_numeral31: required_head(heads, "RomanNumeral31")?,
            soprano35: required_head(heads, "Soprano35")?,
            tenor35: required_head(heads, "Tenor35")?,
            tonicized_key38: required_head(heads, "TonicizedKey38")?,
        })
    }

    pub fn as_head_map(&self) -> BTreeMap<String, AugmentedNetHeadOutput> {
        BTreeMap::from([
            ("Alto35".to_string(), self.alto35.clone()),
            ("Bass35".to_string(), self.bass35.clone()),
            ("HarmonicRhythm7".to_string(), self.harmonic_rhythm7.clone()),
            ("LocalKey38".to_string(), self.local_key38.clone()),
            (
                "PitchClassSet121".to_string(),
                self.pitch_class_set121.clone(),
            ),
            ("RomanNumeral31".to_string(), self.roman_numeral31.clone()),
            ("Soprano35".to_string(), self.soprano35.clone()),
            ("Tenor35".to_string(), self.tenor35.clone()),
            ("TonicizedKey38".to_string(), self.tonicized_key38.clone()),
        ])
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AugmentedNetInferenceOutput {
    pub effective_steps: usize,
    pub output_order: Vec<String>,
    pub typed_outputs: AugmentedNetTypedOutputs,
}

impl AugmentedNetInferenceOutput {
    pub fn to_stage_c_artifact(&self) -> StageCArtifact {
        let mut heads = BTreeMap::new();
        for (head, data) in self.typed_outputs.as_head_map() {
            heads.insert(head, data.as_stage_c_head());
        }
        StageCArtifact {
            schema_version: PARITY_SCHEMA_VERSION,
            effective_steps: self.effective_steps,
            heads,
        }
    }

    pub fn to_stage_d_artifact(
        &self,
    ) -> AugmentedNetOnnxResult<crate::augnet_postprocess::StageDArtifact> {
        crate::augnet_postprocess::decode_stage_d_from_inference(self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnnxBoundaryParityFixture {
    pub fixture_id: String,
    pub stage_b: AugmentedNetInputTensors,
    pub stage_c: StageCArtifact,
}

#[derive(Debug, Clone)]
pub struct OnnxBoundaryParityOptions {
    pub logits_atol: f32,
    pub diff_artifact_dir: Option<PathBuf>,
}

impl Default for OnnxBoundaryParityOptions {
    fn default() -> Self {
        Self {
            logits_atol: 1e-5,
            diff_artifact_dir: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnnxBoundaryParityReport {
    pub fixtures_checked: usize,
    pub logits_atol: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeadDiffArtifact {
    pub fixture_id: String,
    pub head: String,
    pub expected_shape: [usize; 2],
    pub actual_shape: [usize; 2],
    pub argmax_equal: bool,
    pub max_abs_diff: f32,
    pub first_exceedance: Option<[usize; 2]>,
}

#[derive(Debug, Error)]
pub enum AugmentedNetOnnxError {
    #[error("failed to read file {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse JSON {path}: {source}")]
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("manifest validation failed: {0}")]
    ManifestValidation(String),
    #[error("session bootstrap failed: {0}")]
    SessionBootstrap(String),
    #[error("input contract validation failed: {0}")]
    InputContract(String),
    #[error("output contract validation failed: {0}")]
    OutputContract(String),
    #[error("preprocessing failed: {0}")]
    Preprocessing(String),
    #[error("inference failed: {0}")]
    Inference(String),
    #[error("onnx boundary parity mismatch: {0}")]
    ParityMismatch(String),
}

pub type AugmentedNetOnnxResult<T> = Result<T, AugmentedNetOnnxError>;

#[cfg(feature = "augnet_onnx_backend")]
pub struct AugmentedNetOnnxBackend {
    _environment: Arc<Environment>,
    manifest: AugmentedNetModelManifest,
    session: Session,
    fixed_t: usize,
}

#[cfg(feature = "augnet_onnx_backend")]
impl AugmentedNetOnnxBackend {
    pub fn new(config: AugmentedNetOnnxConfig) -> AugmentedNetOnnxResult<Self> {
        let manifest = load_manifest(&config.manifest_path)?;
        validate_manifest_fields(&manifest, &config)?;
        let model_sha256 = sha256_hex(&config.model_path)?;
        if model_sha256 != manifest.onnx.onnx_sha256 {
            return Err(AugmentedNetOnnxError::ManifestValidation(format!(
                "onnx sha256 mismatch for {}: manifest={}, actual={}",
                config.model_path.display(),
                manifest.onnx.onnx_sha256,
                model_sha256
            )));
        }

        let fixed_t = *manifest
            .signature
            .fixed_time_axis_contract
            .lengths
            .first()
            .ok_or_else(|| {
                AugmentedNetOnnxError::ManifestValidation(
                    "fixed_time_axis_contract.lengths is empty".to_string(),
                )
            })?;

        let environment = Environment::builder()
            .with_name("augnet-onnx")
            .build()
            .map_err(|e| {
                AugmentedNetOnnxError::SessionBootstrap(format!(
                    "unable to initialize ORT environment: {e}"
                ))
            })?
            .into_arc();
        let session = SessionBuilder::new(&environment)
            .map_err(|e| {
                AugmentedNetOnnxError::SessionBootstrap(format!(
                    "unable to initialize ORT session builder: {e}"
                ))
            })?
            .with_execution_providers([ExecutionProvider::CPU(
                CPUExecutionProviderOptions::default(),
            )])
            .map_err(|e| {
                AugmentedNetOnnxError::SessionBootstrap(format!(
                    "unable to register CPU execution provider: {e}"
                ))
            })?
            .with_intra_threads(config.intra_threads as i16)
            .map_err(|e| {
                AugmentedNetOnnxError::SessionBootstrap(format!(
                    "failed setting intra_threads={}: {e}",
                    config.intra_threads
                ))
            })?
            .with_inter_threads(config.inter_threads as i16)
            .map_err(|e| {
                AugmentedNetOnnxError::SessionBootstrap(format!(
                    "failed setting inter_threads={}: {e}",
                    config.inter_threads
                ))
            })?
            .with_model_from_file(&config.model_path)
            .map_err(|e| {
                AugmentedNetOnnxError::SessionBootstrap(format!(
                    "failed loading ONNX model {}: {e}",
                    config.model_path.display()
                ))
            })?;

        validate_session_contract(&manifest, &session)?;
        if session.outputs.is_empty() {
            return Err(AugmentedNetOnnxError::SessionBootstrap(
                "model has zero outputs".to_string(),
            ));
        }

        Ok(Self {
            _environment: environment,
            manifest,
            session,
            fixed_t,
        })
    }

    pub fn manifest(&self) -> &AugmentedNetModelManifest {
        &self.manifest
    }

    pub fn fixed_t(&self) -> usize {
        self.fixed_t
    }

    pub fn infer(
        &self,
        tensors: &AugmentedNetInputTensors,
    ) -> AugmentedNetOnnxResult<AugmentedNetInferenceOutput> {
        tensors.validate_contract(self.fixed_t)?;
        let effective_steps = tensors.effective_steps();

        let bass = flatten_matrix(&tensors.x_bass19, "X_Bass19")?;
        let chroma = flatten_matrix(&tensors.x_chromagram19, "X_Chromagram19")?;
        let onset = flatten_matrix(&tensors.x_measure_note_onset14, "X_MeasureNoteOnset14")?;

        let bass_array = ndarray::Array::from_shape_vec((1usize, self.fixed_t, 19usize), bass)
            .map_err(|e| {
                AugmentedNetOnnxError::InputContract(format!(
                    "failed creating ndarray for X_Bass19: {e}"
                ))
            })?
            .into_dyn();
        let chroma_array = ndarray::Array::from_shape_vec((1usize, self.fixed_t, 19usize), chroma)
            .map_err(|e| {
                AugmentedNetOnnxError::InputContract(format!(
                    "failed creating ndarray for X_Chromagram19: {e}"
                ))
            })?
            .into_dyn();
        let onset_array = ndarray::Array::from_shape_vec((1usize, self.fixed_t, 14usize), onset)
            .map_err(|e| {
                AugmentedNetOnnxError::InputContract(format!(
                    "failed creating ndarray for X_MeasureNoteOnset14: {e}"
                ))
            })?
            .into_dyn();
        let bass_cow: ndarray::CowArray<'_, f32, ndarray::IxDyn> = bass_array.view().into();
        let chroma_cow: ndarray::CowArray<'_, f32, ndarray::IxDyn> = chroma_array.view().into();
        let onset_cow: ndarray::CowArray<'_, f32, ndarray::IxDyn> = onset_array.view().into();

        let v1 = Value::from_array(self.session.allocator(), &bass_cow).map_err(|e| {
            AugmentedNetOnnxError::InputContract(format!(
                "failed creating input value X_Bass19: {e}"
            ))
        })?;
        let v2 = Value::from_array(self.session.allocator(), &chroma_cow).map_err(|e| {
            AugmentedNetOnnxError::InputContract(format!(
                "failed creating input value X_Chromagram19: {e}"
            ))
        })?;
        let v3 = Value::from_array(self.session.allocator(), &onset_cow).map_err(|e| {
            AugmentedNetOnnxError::InputContract(format!(
                "failed creating input value X_MeasureNoteOnset14: {e}"
            ))
        })?;

        let outputs = self
            .session
            .run(vec![v1, v2, v3])
            .map_err(|e| AugmentedNetOnnxError::Inference(format!("ORT run failed: {e}")))?;

        let session_output_order: Vec<String> = self
            .session
            .outputs
            .iter()
            .map(|output| output.name.clone())
            .collect();
        if session_output_order != self.manifest.signature.onnx_output_names {
            return Err(AugmentedNetOnnxError::OutputContract(format!(
                "output name/order mismatch at runtime: expected={:?} actual={:?}",
                self.manifest.signature.onnx_output_names, session_output_order
            )));
        }

        let mut output_map = BTreeMap::new();
        for (output_spec, output_value) in self.session.outputs.iter().zip(outputs.iter()) {
            let head_name = output_spec.name.as_str();
            let output = output_value.try_extract::<f32>().map_err(|e| {
                AugmentedNetOnnxError::OutputContract(format!(
                    "failed to extract logits for head {head_name}: {e}"
                ))
            })?;
            let output_view = output.view();
            let shape = output_view.shape();
            if shape.len() != 3 {
                return Err(AugmentedNetOnnxError::OutputContract(format!(
                    "head {head_name} has rank {} (expected 3)",
                    shape.len()
                )));
            }
            let batch = shape[0];
            let t = shape[1];
            let classes = shape[2];
            if batch != 1 {
                return Err(AugmentedNetOnnxError::OutputContract(format!(
                    "head {head_name} batch dimension={batch}, expected=1"
                )));
            }
            if t != self.fixed_t {
                return Err(AugmentedNetOnnxError::OutputContract(format!(
                    "head {head_name} time dimension={t}, expected={}",
                    self.fixed_t
                )));
            }
            let expected_classes = parse_head_cardinality(head_name).ok_or_else(|| {
                AugmentedNetOnnxError::OutputContract(format!(
                    "unable to infer class count from head name: {head_name}"
                ))
            })?;
            if classes != expected_classes {
                return Err(AugmentedNetOnnxError::OutputContract(format!(
                    "head {head_name} class dimension={classes}, expected={expected_classes}"
                )));
            }

            let mut logits = Vec::with_capacity(effective_steps);
            let mut argmax = Vec::with_capacity(effective_steps);
            for step in 0..effective_steps {
                let row = output_view
                    .slice(ndarray::s![0, step, ..])
                    .iter()
                    .copied()
                    .collect::<Vec<f32>>();
                let mut max_idx = 0usize;
                let mut max_value = row[0];
                for (i, v) in row.iter().enumerate().skip(1) {
                    if *v > max_value {
                        max_value = *v;
                        max_idx = i;
                    }
                }
                logits.push(row);
                argmax.push(max_idx);
            }

            output_map.insert(
                normalize_head_name(head_name),
                AugmentedNetHeadOutput {
                    shape: [effective_steps, classes],
                    raw_logits: logits,
                    argmax,
                },
            );
        }

        let typed_outputs = AugmentedNetTypedOutputs::from_head_map(&output_map)?;
        Ok(AugmentedNetInferenceOutput {
            effective_steps,
            output_order: self.manifest.signature.onnx_output_names.clone(),
            typed_outputs,
        })
    }

    pub fn infer_preprocessed_chunk(
        &self,
        chunk: &crate::augnet_preprocess::AugmentedNetPreprocessChunk,
    ) -> AugmentedNetOnnxResult<AugmentedNetInferenceOutput> {
        self.infer(&chunk.tensors)
    }

    pub fn infer_preprocessed_chunks(
        &self,
        chunks: &[crate::augnet_preprocess::AugmentedNetPreprocessChunk],
    ) -> AugmentedNetOnnxResult<Vec<AugmentedNetInferenceOutput>> {
        let mut outputs = Vec::with_capacity(chunks.len());
        for chunk in chunks {
            outputs.push(self.infer_preprocessed_chunk(chunk)?);
        }
        Ok(outputs)
    }
}

#[cfg(feature = "augnet_onnx_backend")]
pub fn run_onnx_boundary_parity_gate(
    backend: &mut AugmentedNetOnnxBackend,
    fixtures: &[OnnxBoundaryParityFixture],
    options: &OnnxBoundaryParityOptions,
) -> AugmentedNetOnnxResult<OnnxBoundaryParityReport> {
    for fixture in fixtures {
        let actual = backend.infer(&fixture.stage_b)?.to_stage_c_artifact();
        verify_stage_c_parity(
            &fixture.fixture_id,
            &fixture.stage_c,
            &actual,
            options.logits_atol,
            options.diff_artifact_dir.as_deref(),
        )?;
    }
    Ok(OnnxBoundaryParityReport {
        fixtures_checked: fixtures.len(),
        logits_atol: options.logits_atol,
    })
}

pub fn load_parity_fixture(path: &Path) -> AugmentedNetOnnxResult<OnnxBoundaryParityFixture> {
    #[derive(Debug, Deserialize)]
    struct FixtureFile {
        fixture_id: String,
        stage_b: AugmentedNetInputTensors,
        stage_c: StageCArtifact,
    }
    let text = fs::read_to_string(path).map_err(|source| AugmentedNetOnnxError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let parsed: FixtureFile =
        serde_json::from_str(&text).map_err(|source| AugmentedNetOnnxError::Json {
            path: path.to_path_buf(),
            source,
        })?;
    Ok(OnnxBoundaryParityFixture {
        fixture_id: parsed.fixture_id,
        stage_b: parsed.stage_b,
        stage_c: parsed.stage_c,
    })
}

fn load_manifest(path: &Path) -> AugmentedNetOnnxResult<AugmentedNetModelManifest> {
    let text = fs::read_to_string(path).map_err(|source| AugmentedNetOnnxError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_str(&text).map_err(|source| AugmentedNetOnnxError::Json {
        path: path.to_path_buf(),
        source,
    })
}

fn validate_manifest_fields(
    manifest: &AugmentedNetModelManifest,
    config: &AugmentedNetOnnxConfig,
) -> AugmentedNetOnnxResult<()> {
    if manifest.model_id != config.expected_model_id {
        return Err(AugmentedNetOnnxError::ManifestValidation(format!(
            "model_id mismatch: expected={}, actual={}",
            config.expected_model_id, manifest.model_id
        )));
    }
    if manifest.onnx.opset != config.expected_opset {
        return Err(AugmentedNetOnnxError::ManifestValidation(format!(
            "opset mismatch: expected={}, actual={}",
            config.expected_opset, manifest.onnx.opset
        )));
    }
    if !manifest.signature.output_head_order_match {
        return Err(AugmentedNetOnnxError::ManifestValidation(
            "signature.output_head_order_match=false; refusing to continue".to_string(),
        ));
    }
    if manifest.signature.onnx_input_names != AUGNET_INPUT_ORDER {
        return Err(AugmentedNetOnnxError::ManifestValidation(format!(
            "input names/order mismatch: expected={:?}, actual={:?}",
            AUGNET_INPUT_ORDER, manifest.signature.onnx_input_names
        )));
    }
    if manifest.signature.onnx_output_heads != AUGNET_HEAD_ORDER {
        return Err(AugmentedNetOnnxError::ManifestValidation(format!(
            "output heads/order mismatch: expected={:?}, actual={:?}",
            AUGNET_HEAD_ORDER, manifest.signature.onnx_output_heads
        )));
    }
    if manifest.signature.onnx_output_names != manifest.signature.onnx_output_heads {
        return Err(AugmentedNetOnnxError::ManifestValidation(format!(
            "output names do not align with output heads: names={:?} heads={:?}",
            manifest.signature.onnx_output_names, manifest.signature.onnx_output_heads
        )));
    }
    if !manifest.signature.fixed_time_axis_contract.enforced
        || !manifest.signature.fixed_time_axis_contract.all_inputs_fixed
        || manifest.signature.fixed_time_axis_contract.dimension != 1
    {
        return Err(AugmentedNetOnnxError::ManifestValidation(
            "fixed_time_axis_contract must be enforced, all_inputs_fixed=true, dimension=1"
                .to_string(),
        ));
    }
    if manifest
        .signature
        .fixed_time_axis_contract
        .lengths
        .is_empty()
    {
        return Err(AugmentedNetOnnxError::ManifestValidation(
            "fixed_time_axis_contract.lengths must not be empty".to_string(),
        ));
    }
    Ok(())
}

#[cfg(feature = "augnet_onnx_backend")]
fn validate_session_contract(
    manifest: &AugmentedNetModelManifest,
    session: &Session,
) -> AugmentedNetOnnxResult<()> {
    let input_names: Vec<String> = session.inputs.iter().map(|o| o.name.clone()).collect();
    if input_names != manifest.signature.onnx_input_names {
        return Err(AugmentedNetOnnxError::SessionBootstrap(format!(
            "session input names/order mismatch: expected={:?} actual={:?}",
            manifest.signature.onnx_input_names, input_names
        )));
    }
    let output_names: Vec<String> = session.outputs.iter().map(|o| o.name.clone()).collect();
    if output_names != manifest.signature.onnx_output_names {
        return Err(AugmentedNetOnnxError::SessionBootstrap(format!(
            "session output names/order mismatch: expected={:?} actual={:?}",
            manifest.signature.onnx_output_names, output_names
        )));
    }

    for (idx, input) in session.inputs.iter().enumerate() {
        let expected = manifest
            .signature
            .onnx_input_shapes
            .get(idx)
            .ok_or_else(|| {
                AugmentedNetOnnxError::SessionBootstrap(format!(
                    "manifest missing input shape metadata for index {idx}"
                ))
            })?;
        if input.input_type != TensorElementDataType::Float32 {
            return Err(AugmentedNetOnnxError::SessionBootstrap(format!(
                "input {} has non-f32 type {:?}",
                input.name, input.input_type
            )));
        }
        let actual: Vec<Option<i64>> = input
            .dimensions
            .iter()
            .map(|dim| dim.map(i64::from))
            .collect();
        if &actual != expected {
            return Err(AugmentedNetOnnxError::SessionBootstrap(format!(
                "input shape mismatch for {}: expected={expected:?} actual={actual:?}",
                input.name
            )));
        }
    }
    for output in &session.outputs {
        if output.output_type != TensorElementDataType::Float32 {
            return Err(AugmentedNetOnnxError::SessionBootstrap(format!(
                "output {} has non-f32 type {:?}",
                output.name, output.output_type
            )));
        }
    }
    Ok(())
}

#[cfg(feature = "augnet_onnx_backend")]
fn sha256_hex(path: &Path) -> AugmentedNetOnnxResult<String> {
    let bytes = fs::read(path).map_err(|source| AugmentedNetOnnxError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let hash = Sha256::digest(bytes);
    let mut rendered = String::with_capacity(64);
    for b in hash {
        let _ = write!(&mut rendered, "{b:02x}");
    }
    Ok(rendered)
}

fn parse_head_cardinality(head_name: &str) -> Option<usize> {
    let digits_start = head_name
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_ascii_digit())
        .last()
        .map(|(idx, _)| idx)?;
    head_name[digits_start..].parse::<usize>().ok()
}

fn normalize_head_name(raw: &str) -> String {
    raw.split('/').next().unwrap_or(raw).to_string()
}

fn flatten_matrix(matrix: &[Vec<f32>], name: &str) -> AugmentedNetOnnxResult<Vec<f32>> {
    let width = matrix.first().map_or(0usize, Vec::len);
    if width == 0 {
        return Err(AugmentedNetOnnxError::InputContract(format!(
            "{name} matrix has zero-width rows"
        )));
    }
    let mut out = Vec::with_capacity(matrix.len() * width);
    for (idx, row) in matrix.iter().enumerate() {
        if row.len() != width {
            return Err(AugmentedNetOnnxError::InputContract(format!(
                "{name} row {idx} has width {}, expected {width}",
                row.len()
            )));
        }
        out.extend_from_slice(row);
    }
    Ok(out)
}

fn validate_matrix(
    name: &str,
    matrix: &[Vec<f32>],
    expected_rows: usize,
    expected_cols: usize,
) -> AugmentedNetOnnxResult<()> {
    if matrix.len() != expected_rows {
        return Err(AugmentedNetOnnxError::InputContract(format!(
            "{name} row count {} does not match expected {}",
            matrix.len(),
            expected_rows
        )));
    }
    for (row_idx, row) in matrix.iter().enumerate() {
        if row.len() != expected_cols {
            return Err(AugmentedNetOnnxError::InputContract(format!(
                "{name} row {} has {} columns, expected {}",
                row_idx,
                row.len(),
                expected_cols
            )));
        }
    }
    Ok(())
}

fn required_head(
    heads: &BTreeMap<String, AugmentedNetHeadOutput>,
    name: &str,
) -> AugmentedNetOnnxResult<AugmentedNetHeadOutput> {
    heads.get(name).cloned().ok_or_else(|| {
        AugmentedNetOnnxError::OutputContract(format!(
            "missing typed output head after mapping: {name}"
        ))
    })
}

fn round6(v: f32) -> f32 {
    ((v as f64 * 1_000_000.0).round() / 1_000_000.0) as f32
}

fn verify_stage_c_parity(
    fixture_id: &str,
    expected: &StageCArtifact,
    actual: &StageCArtifact,
    logits_atol: f32,
    diff_artifact_dir: Option<&Path>,
) -> AugmentedNetOnnxResult<()> {
    if expected.heads.keys().collect::<Vec<_>>() != actual.heads.keys().collect::<Vec<_>>() {
        return Err(AugmentedNetOnnxError::ParityMismatch(format!(
            "fixture {fixture_id}: output head names/order mismatch: expected={:?} actual={:?}",
            expected.heads.keys().collect::<Vec<_>>(),
            actual.heads.keys().collect::<Vec<_>>()
        )));
    }
    if expected.effective_steps != actual.effective_steps {
        return Err(AugmentedNetOnnxError::ParityMismatch(format!(
            "fixture {fixture_id}: effective_steps mismatch expected={} actual={}",
            expected.effective_steps, actual.effective_steps
        )));
    }

    for (head, expected_head) in &expected.heads {
        let actual_head = actual.heads.get(head).ok_or_else(|| {
            AugmentedNetOnnxError::ParityMismatch(format!(
                "fixture {fixture_id}: missing head in actual outputs: {head}"
            ))
        })?;
        let diff = compute_head_diff(fixture_id, head, expected_head, actual_head, logits_atol)?;
        if diff.first_exceedance.is_some() || !diff.argmax_equal {
            if let Some(dir) = diff_artifact_dir {
                emit_diff_artifact(dir, &diff)?;
            }
            return Err(AugmentedNetOnnxError::ParityMismatch(format!(
                "fixture {fixture_id} head {head}: max_abs_diff={} argmax_equal={}",
                diff.max_abs_diff, diff.argmax_equal
            )));
        }
    }
    Ok(())
}

fn compute_head_diff(
    fixture_id: &str,
    head: &str,
    expected: &StageCHeadArtifact,
    actual: &StageCHeadArtifact,
    logits_atol: f32,
) -> AugmentedNetOnnxResult<HeadDiffArtifact> {
    if expected.shape != actual.shape {
        return Err(AugmentedNetOnnxError::ParityMismatch(format!(
            "fixture {fixture_id} head {head}: shape mismatch expected={:?} actual={:?}",
            expected.shape, actual.shape
        )));
    }
    if expected.argmax.len() != actual.argmax.len() {
        return Err(AugmentedNetOnnxError::ParityMismatch(format!(
            "fixture {fixture_id} head {head}: argmax length mismatch expected={} actual={}",
            expected.argmax.len(),
            actual.argmax.len()
        )));
    }
    if expected.logits.len() != actual.logits.len() {
        return Err(AugmentedNetOnnxError::ParityMismatch(format!(
            "fixture {fixture_id} head {head}: logits row-count mismatch expected={} actual={}",
            expected.logits.len(),
            actual.logits.len()
        )));
    }

    let mut max_abs_diff = 0.0f32;
    let mut first_exceedance = None;
    for (t, (e_row, a_row)) in expected.logits.iter().zip(&actual.logits).enumerate() {
        if e_row.len() != a_row.len() {
            return Err(AugmentedNetOnnxError::ParityMismatch(format!(
                "fixture {fixture_id} head {head}: logits width mismatch at t={t} expected={} actual={}",
                e_row.len(),
                a_row.len()
            )));
        }
        for (c, (ev, av)) in e_row.iter().zip(a_row).enumerate() {
            let d = (ev - av).abs();
            if d > max_abs_diff {
                max_abs_diff = d;
            }
            if d > logits_atol && first_exceedance.is_none() {
                first_exceedance = Some([t, c]);
            }
        }
    }

    Ok(HeadDiffArtifact {
        fixture_id: fixture_id.to_string(),
        head: head.to_string(),
        expected_shape: expected.shape,
        actual_shape: actual.shape,
        argmax_equal: expected.argmax == actual.argmax,
        max_abs_diff,
        first_exceedance,
    })
}

fn emit_diff_artifact(dir: &Path, diff: &HeadDiffArtifact) -> AugmentedNetOnnxResult<()> {
    fs::create_dir_all(dir).map_err(|source| AugmentedNetOnnxError::Io {
        path: dir.to_path_buf(),
        source,
    })?;
    let file = dir.join(format!("{}_{}.json", diff.fixture_id, diff.head));
    let payload = serde_json::to_string_pretty(diff).map_err(|e| {
        AugmentedNetOnnxError::ParityMismatch(format!(
            "failed to serialize head diff artifact {}: {e}",
            file.display()
        ))
    })?;
    fs::write(&file, payload).map_err(|source| AugmentedNetOnnxError::Io { path: file, source })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn head_name_normalization_strips_biasadd_suffix() {
        assert_eq!(normalize_head_name("Alto35/BiasAdd"), "Alto35");
        assert_eq!(normalize_head_name("RomanNumeral31"), "RomanNumeral31");
    }

    #[test]
    fn parse_head_cardinality_works_for_all_augnet_heads() {
        for name in AUGNET_HEAD_ORDER {
            assert!(parse_head_cardinality(name).is_some(), "name={name}");
        }
        assert_eq!(parse_head_cardinality("NoDigits"), None);
    }

    #[test]
    fn tensor_input_contract_rejects_shape_mismatches() {
        let valid = AugmentedNetInputTensors {
            schema_version: 1,
            fixed_offset: 0.25,
            max_steps: 4,
            active_steps: 4,
            x_bass19: vec![vec![0.0; 19]; 4],
            x_chromagram19: vec![vec![0.0; 19]; 4],
            x_measure_note_onset14: vec![vec![0.0; 14]; 4],
        };
        valid.validate_contract(4).expect("valid");

        let mut bad = valid.clone();
        bad.x_bass19.pop();
        let err = bad.validate_contract(4).expect_err("must fail");
        assert!(err
            .to_string()
            .contains("X_Bass19 row count 3 does not match expected 4"));
    }

    #[test]
    fn typed_output_mapping_requires_all_heads() {
        let mut heads = BTreeMap::new();
        heads.insert(
            "Alto35".to_string(),
            AugmentedNetHeadOutput {
                shape: [1, 35],
                raw_logits: vec![vec![0.0; 35]],
                argmax: vec![0],
            },
        );
        let err = AugmentedNetTypedOutputs::from_head_map(&heads).expect_err("must fail");
        assert!(err.to_string().contains("missing typed output head"));
    }

    #[test]
    fn head_diff_detects_first_exceedance_and_argmax_mismatch() {
        let expected = StageCHeadArtifact {
            shape: [2, 3],
            logits: vec![vec![0.0, 0.1, 0.2], vec![0.2, 0.3, 0.4]],
            argmax: vec![2, 2],
        };
        let actual = StageCHeadArtifact {
            shape: [2, 3],
            logits: vec![vec![0.0, 0.1, 0.8], vec![0.2, 0.3, 0.4]],
            argmax: vec![2, 1],
        };
        let diff = compute_head_diff("fixture", "Alto35", &expected, &actual, 1e-5).expect("ok");
        assert_eq!(diff.first_exceedance, Some([0, 2]));
        assert!(!diff.argmax_equal);
    }
}
