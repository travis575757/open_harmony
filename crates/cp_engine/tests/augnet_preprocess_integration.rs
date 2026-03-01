use cp_engine::augnet_onnx::{
    AugmentedNetInputTensors, AugmentedNetOnnxBackend, AugmentedNetOnnxConfig,
};
use cp_engine::augnet_preprocess::{
    compare_stage_b_tensors, infer_musicxml_chunks, preprocess_musicxml_to_chunks,
    stage_b_inputs_to_onnx_tensors, AugmentedNetPreprocessConfig, StageBTensorParityMismatch,
    PREPROCESS_SCHEMA_VERSION,
};
use cp_music21_compat::{encode_stage_b_inputs, AugnetScoreFrame};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn manifest_path() -> PathBuf {
    repo_root().join("tests/augnet_parity/fixtures_manifest.json")
}

fn manifest() -> FixturesManifest {
    serde_json::from_str(
        &fs::read_to_string(manifest_path())
            .expect("read tests/augnet_parity/fixtures_manifest.json"),
    )
    .expect("parse fixtures manifest")
}

fn model_path() -> PathBuf {
    repo_root().join("models/augnet/AugmentedNet.onnx")
}

fn model_manifest_path() -> PathBuf {
    repo_root().join("models/augnet/model-manifest.json")
}

fn backend_config() -> AugmentedNetOnnxConfig {
    AugmentedNetOnnxConfig {
        model_path: model_path(),
        manifest_path: model_manifest_path(),
        ..AugmentedNetOnnxConfig::default()
    }
}

fn load_musicxml(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
}

fn baseline_path(fixture: &ManifestFixture) -> PathBuf {
    repo_root()
        .join("tests/augnet_parity/music21_baseline")
        .join(&fixture.baseline_artifact)
}

fn load_baseline_fixture(fixture: &ManifestFixture) -> BaselineFixture {
    let path = baseline_path(fixture);
    serde_json::from_str(
        &fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display())),
    )
    .unwrap_or_else(|e| panic!("failed to parse {}: {e}", path.display()))
}

fn config_from_manifest(manifest: &FixturesManifest) -> AugmentedNetPreprocessConfig {
    AugmentedNetPreprocessConfig {
        fixed_offset: manifest.fixed_offset,
        max_steps: manifest.max_steps,
        ..AugmentedNetPreprocessConfig::default()
    }
}

fn assert_stage_b_parity(
    fixture_id: &str,
    expected: &AugmentedNetInputTensors,
    actual: &AugmentedNetInputTensors,
) {
    match compare_stage_b_tensors(expected, actual, 1e-6) {
        Ok(_) => {}
        Err(mismatch) => panic!("fixture {fixture_id} stage_b mismatch: {mismatch}"),
    }
}

fn assert_segment_equal(
    tensor_name: &str,
    full: &[Vec<f32>],
    chunk: &[Vec<f32>],
    global_start: usize,
    active_steps: usize,
) {
    for local_row in 0..active_steps {
        let full_row = &full[global_start + local_row];
        let chunk_row = &chunk[local_row];
        assert_eq!(
            full_row,
            chunk_row,
            "{tensor_name} mismatch at global_step={} local_step={}",
            global_start + local_row,
            local_row
        );
    }
}

#[derive(Debug, Clone, Deserialize)]
struct FixturesManifest {
    schema_version: u32,
    fixed_offset: f64,
    max_steps: usize,
    fixtures: Vec<ManifestFixture>,
}

#[derive(Debug, Clone, Deserialize)]
struct ManifestFixture {
    id: String,
    musicxml_path: String,
    baseline_artifact: String,
}

#[derive(Debug, Clone, Deserialize)]
struct BaselineStageA {
    event_frames: Vec<AugnetScoreFrame>,
    grid_frames: Vec<AugnetScoreFrame>,
}

#[derive(Debug, Clone, Deserialize)]
struct BaselineFixture {
    fixture_id: String,
    stage_a: BaselineStageA,
    stage_b: AugmentedNetInputTensors,
}

#[derive(Debug, Clone, Serialize)]
struct FixtureParityResult {
    fixture_id: String,
    status: String,
    checked_values: usize,
    mismatch: Option<StageBTensorParityMismatch>,
}

#[derive(Debug, Clone, Serialize)]
struct CorpusParityReport {
    schema_version: u32,
    fixtures_total: usize,
    fixtures_passed: usize,
    pass_rate: f64,
    results: Vec<FixtureParityResult>,
}

#[test]
fn preprocessing_feature_level_golden_exact_channels_and_tensor_order() {
    let manifest = manifest();
    assert_eq!(manifest.schema_version, 1);
    let config = config_from_manifest(&manifest);

    for fixture in &manifest.fixtures {
        let xml_path = repo_root().join(&fixture.musicxml_path);
        let xml = load_musicxml(&xml_path);
        let baseline = load_baseline_fixture(fixture);
        let artifact =
            preprocess_musicxml_to_chunks(&xml, &config).expect("preprocess fixture to chunks");

        assert_eq!(artifact.schema_version, PREPROCESS_SCHEMA_VERSION);
        assert_eq!(artifact.event_frames, baseline.stage_a.event_frames);
        assert_eq!(artifact.grid_frames, baseline.stage_a.grid_frames);
        assert!(!artifact.chunks.is_empty(), "fixture {}", fixture.id);

        let first = &artifact.chunks[0];
        assert_eq!(first.tensors.max_steps, manifest.max_steps);
        assert_eq!(first.tensors.x_bass19.len(), manifest.max_steps);
        assert_eq!(first.tensors.x_chromagram19.len(), manifest.max_steps);
        assert_eq!(
            first.tensors.x_measure_note_onset14.len(),
            manifest.max_steps
        );
        for row in &first.tensors.x_bass19 {
            assert_eq!(
                row.len(),
                19,
                "fixture {} X_Bass19 ordering/shape",
                fixture.id
            );
        }
        for row in &first.tensors.x_chromagram19 {
            assert_eq!(
                row.len(),
                19,
                "fixture {} X_Chromagram19 ordering/shape",
                fixture.id
            );
        }
        for row in &first.tensors.x_measure_note_onset14 {
            assert_eq!(
                row.len(),
                14,
                "fixture {} X_MeasureNoteOnset14 ordering/shape",
                fixture.id
            );
        }

        assert_stage_b_parity(&fixture.id, &baseline.stage_b, &first.tensors);
    }
}

#[test]
fn preprocessing_corpus_preprocessing_parity_report_is_100_percent() {
    let manifest = manifest();
    let config = config_from_manifest(&manifest);
    let mut results = Vec::new();
    let mut passed = 0usize;

    for fixture in &manifest.fixtures {
        let xml_path = repo_root().join(&fixture.musicxml_path);
        let xml = load_musicxml(&xml_path);
        let baseline = load_baseline_fixture(fixture);
        let artifact =
            preprocess_musicxml_to_chunks(&xml, &config).expect("preprocess fixture to chunks");
        let first = &artifact.chunks[0];

        match compare_stage_b_tensors(&baseline.stage_b, &first.tensors, 1e-6) {
            Ok(stats) => {
                passed += 1;
                results.push(FixtureParityResult {
                    fixture_id: fixture.id.clone(),
                    status: "ok".to_string(),
                    checked_values: stats.value_count,
                    mismatch: None,
                });
            }
            Err(mismatch) => {
                results.push(FixtureParityResult {
                    fixture_id: fixture.id.clone(),
                    status: "mismatch".to_string(),
                    checked_values: 0,
                    mismatch: Some(mismatch),
                });
            }
        }
    }

    let total = manifest.fixtures.len();
    let pass_rate = if total == 0 {
        0.0
    } else {
        passed as f64 / total as f64
    };
    let report = CorpusParityReport {
        schema_version: PREPROCESS_SCHEMA_VERSION,
        fixtures_total: total,
        fixtures_passed: passed,
        pass_rate,
        results,
    };
    let report_dir = repo_root().join("target/augnet");
    fs::create_dir_all(&report_dir).expect("create target/augnet");
    let report_path = report_dir.join("preprocessing_parity_report.json");
    fs::write(
        &report_path,
        serde_json::to_string_pretty(&report).expect("serialize report"),
    )
    .expect("write preprocessing parity report");

    assert_eq!(passed, total, "report={}", report_path.display());
    assert!(
        (report.pass_rate - 1.0).abs() < f64::EPSILON,
        "expected 100% pass rate, got {}",
        report.pass_rate
    );
}

#[test]
fn preprocessing_chunk_boundary_parity_matches_python_baseline_and_chunk_joins() {
    let manifest = manifest();
    let fixture = manifest
        .fixtures
        .iter()
        .find(|f| f.id == "chunk_boundary")
        .expect("chunk_boundary fixture in manifest");
    let config = config_from_manifest(&manifest);

    let xml_path = repo_root().join(&fixture.musicxml_path);
    let xml = load_musicxml(&xml_path);
    let baseline = load_baseline_fixture(fixture);
    let artifact = preprocess_musicxml_to_chunks(&xml, &config).expect("preprocess chunk fixture");
    assert_eq!(baseline.fixture_id, "chunk_boundary");
    assert_eq!(artifact.grid_frames.len(), 656);
    assert_eq!(artifact.chunks.len(), 2);

    let first = &artifact.chunks[0];
    let second = &artifact.chunks[1];
    assert_eq!(first.global_start_step, 0);
    assert_eq!(first.global_end_step_exclusive, 640);
    assert_eq!(first.tensors.active_steps, 640);
    assert_eq!(second.global_start_step, 640);
    assert_eq!(second.global_end_step_exclusive, 656);
    assert_eq!(second.tensors.active_steps, 16);

    assert_stage_b_parity("chunk_boundary", &baseline.stage_b, &first.tensors);

    let unchunked = stage_b_inputs_to_onnx_tensors(encode_stage_b_inputs(
        &artifact.grid_frames,
        manifest.fixed_offset,
        artifact.grid_frames.len(),
    ));
    assert_eq!(unchunked.active_steps, artifact.grid_frames.len());
    assert_eq!(unchunked.max_steps, artifact.grid_frames.len());

    assert_segment_equal(
        "X_Bass19",
        &unchunked.x_bass19,
        &first.tensors.x_bass19,
        first.global_start_step,
        first.tensors.active_steps,
    );
    assert_segment_equal(
        "X_Chromagram19",
        &unchunked.x_chromagram19,
        &first.tensors.x_chromagram19,
        first.global_start_step,
        first.tensors.active_steps,
    );
    assert_segment_equal(
        "X_MeasureNoteOnset14",
        &unchunked.x_measure_note_onset14,
        &first.tensors.x_measure_note_onset14,
        first.global_start_step,
        first.tensors.active_steps,
    );
    assert_segment_equal(
        "X_Bass19",
        &unchunked.x_bass19,
        &second.tensors.x_bass19,
        second.global_start_step,
        second.tensors.active_steps,
    );
    assert_segment_equal(
        "X_Chromagram19",
        &unchunked.x_chromagram19,
        &second.tensors.x_chromagram19,
        second.global_start_step,
        second.tensors.active_steps,
    );
    assert_segment_equal(
        "X_MeasureNoteOnset14",
        &unchunked.x_measure_note_onset14,
        &second.tensors.x_measure_note_onset14,
        second.global_start_step,
        second.tensors.active_steps,
    );

    assert_eq!(unchunked.x_bass19[639], first.tensors.x_bass19[639]);
    assert_eq!(unchunked.x_bass19[640], second.tensors.x_bass19[0]);
    assert_eq!(
        unchunked.x_chromagram19[639],
        first.tensors.x_chromagram19[639]
    );
    assert_eq!(
        unchunked.x_chromagram19[640],
        second.tensors.x_chromagram19[0]
    );
    assert_eq!(
        unchunked.x_measure_note_onset14[639],
        first.tensors.x_measure_note_onset14[639]
    );
    assert_eq!(
        unchunked.x_measure_note_onset14[640],
        second.tensors.x_measure_note_onset14[0]
    );
}

#[test]
fn preprocessing_preprocessing_is_deterministic_for_tensors_and_serialized_artifacts() {
    let manifest = manifest();
    let config = config_from_manifest(&manifest);

    for fixture in &manifest.fixtures {
        let xml = load_musicxml(&repo_root().join(&fixture.musicxml_path));
        let a = preprocess_musicxml_to_chunks(&xml, &config).expect("preprocess A");
        let b = preprocess_musicxml_to_chunks(&xml, &config).expect("preprocess B");
        let c = preprocess_musicxml_to_chunks(&xml, &config).expect("preprocess C");

        assert_eq!(a, b, "fixture {}", fixture.id);
        assert_eq!(b, c, "fixture {}", fixture.id);
        let a_json = serde_json::to_string(&a).expect("serialize A");
        let b_json = serde_json::to_string(&b).expect("serialize B");
        let c_json = serde_json::to_string(&c).expect("serialize C");
        assert_eq!(a_json, b_json, "fixture {}", fixture.id);
        assert_eq!(b_json, c_json, "fixture {}", fixture.id);
    }
}

#[test]
fn preprocessing_integration_path_feeds_preprocessing_chunks_into_onnx_adapter() {
    let manifest = manifest();
    let fixture = manifest
        .fixtures
        .iter()
        .find(|f| f.id == "dense_poly")
        .expect("dense_poly fixture");
    let config = config_from_manifest(&manifest);
    let xml = load_musicxml(&repo_root().join(&fixture.musicxml_path));

    let backend = AugmentedNetOnnxBackend::new(backend_config()).expect("backend");
    let artifact = preprocess_musicxml_to_chunks(&xml, &config).expect("preprocess");
    let via_chunks = backend
        .infer_preprocessed_chunks(&artifact.chunks)
        .expect("infer preprocessed chunks");
    let via_direct = backend
        .infer(&artifact.chunks[0].tensors)
        .expect("infer direct tensor");
    let via_helper = infer_musicxml_chunks(&backend, &xml, &config).expect("infer helper");

    assert_eq!(via_chunks.len(), artifact.chunks.len());
    assert_eq!(via_helper.len(), artifact.chunks.len());
    assert_eq!(
        via_chunks[0].to_stage_c_artifact(),
        via_direct.to_stage_c_artifact()
    );
    assert_eq!(
        via_helper[0].to_stage_c_artifact(),
        via_direct.to_stage_c_artifact()
    );
}

#[test]
fn preprocessing_additional_edge_fixtures_cover_divisions_rests_ties_and_multipart_spelling() {
    let manifest = manifest();
    let config = config_from_manifest(&manifest);

    let divisions_fixture = manifest
        .fixtures
        .iter()
        .find(|f| f.id == "divisions_change")
        .expect("divisions_change fixture");
    let divisions_xml = load_musicxml(&repo_root().join(&divisions_fixture.musicxml_path));
    let divisions =
        preprocess_musicxml_to_chunks(&divisions_xml, &config).expect("divisions preprocess");
    assert_eq!(divisions.event_frames[2].s_offset, 4.0);
    assert_eq!(divisions.event_frames[2].s_duration, 1.5);
    assert_eq!(divisions.event_frames[3].s_offset, 5.5);

    let rests_fixture = manifest
        .fixtures
        .iter()
        .find(|f| f.id == "rest_heavy")
        .expect("rest_heavy fixture");
    let rests_xml = load_musicxml(&repo_root().join(&rests_fixture.musicxml_path));
    let rests = preprocess_musicxml_to_chunks(&rests_xml, &config).expect("rests preprocess");
    assert!(rests.event_frames[1].s_notes.is_none());
    assert!(rests.event_frames[3].s_notes.is_none());

    let tie_fixture = manifest
        .fixtures
        .iter()
        .find(|f| f.id == "long_tie_chain")
        .expect("long_tie_chain fixture");
    let tie_xml = load_musicxml(&repo_root().join(&tie_fixture.musicxml_path));
    let ties = preprocess_musicxml_to_chunks(&tie_xml, &config).expect("long tie preprocess");
    assert_eq!(
        ties.event_frames[3].s_is_onset.as_ref().expect("onset vec"),
        &vec![false]
    );

    let multipart_fixture = manifest
        .fixtures
        .iter()
        .find(|f| f.id == "multi_part_enharmonic")
        .expect("multi_part_enharmonic fixture");
    let multipart_xml = load_musicxml(&repo_root().join(&multipart_fixture.musicxml_path));
    let multipart =
        preprocess_musicxml_to_chunks(&multipart_xml, &config).expect("multipart preprocess");
    assert_eq!(
        multipart.event_frames[0].s_notes.as_ref().expect("notes"),
        &vec!["G-3".to_string(), "F#3".to_string()]
    );
    assert_eq!(
        multipart.event_frames[0]
            .s_intervals
            .as_ref()
            .expect("intervals"),
        &vec!["d2".to_string()]
    );
}
