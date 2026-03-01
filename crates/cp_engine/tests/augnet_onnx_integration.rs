use cp_engine::augnet_onnx::{
    load_parity_fixture, run_onnx_boundary_parity_gate, AugmentedNetOnnxBackend,
    AugmentedNetOnnxConfig, AugmentedNetOnnxError, OnnxBoundaryParityFixture,
    OnnxBoundaryParityOptions, AUGNET_HEAD_ORDER,
};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tempfile::tempdir;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn manifest_path() -> PathBuf {
    repo_root().join("models/augnet/model-manifest.json")
}

fn model_path() -> PathBuf {
    repo_root().join("models/augnet/AugmentedNet.onnx")
}

fn baseline_fixture_path(fixture_id: &str) -> PathBuf {
    repo_root().join(format!(
        "tests/augnet_parity/music21_baseline/{fixture_id}.json"
    ))
}

fn backend_config() -> AugmentedNetOnnxConfig {
    AugmentedNetOnnxConfig {
        model_path: model_path(),
        manifest_path: manifest_path(),
        ..AugmentedNetOnnxConfig::default()
    }
}

fn backend() -> AugmentedNetOnnxBackend {
    AugmentedNetOnnxBackend::new(backend_config()).expect("onnx backend should bootstrap")
}

fn fixture(fixture_id: &str) -> OnnxBoundaryParityFixture {
    load_parity_fixture(&baseline_fixture_path(fixture_id)).expect("baseline fixture")
}

fn fixture_ids_manifest() -> Vec<String> {
    let manifest_path = repo_root().join("tests/augnet_parity/fixtures_manifest.json");
    let manifest: Value = serde_json::from_str(
        &fs::read_to_string(&manifest_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", manifest_path.display())),
    )
    .unwrap_or_else(|e| panic!("parse {}: {e}", manifest_path.display()));
    manifest["fixtures"]
        .as_array()
        .expect("fixtures manifest array")
        .iter()
        .map(|fixture| fixture["id"].as_str().expect("fixture id").to_string())
        .collect()
}

fn write_json(path: &Path, value: &Value) {
    let rendered = serde_json::to_string_pretty(value).expect("serialize");
    fs::write(path, rendered).expect("write json");
}

#[test]
fn onnx_adapter_4a_session_bootstrap_acceptance_loads_model_and_manifest_contract() {
    let engine = backend();
    assert_eq!(engine.manifest().model_id, "augmentednet-v1");
    assert_eq!(engine.manifest().onnx.opset, 13);
    assert_eq!(
        engine.manifest().onnx.onnx_sha256,
        "05f15a09c300dd7d65e05ec199a42d74210f0cdf992c2a39c65c3c4deebd3016"
    );
    assert_eq!(engine.fixed_t(), 640);
}

#[test]
fn onnx_adapter_4a_session_bootstrap_acceptance_rejects_corrupt_manifest_artifact() {
    let tmp = tempdir().expect("tmp");
    let corrupt_manifest = tmp.path().join("model-manifest.corrupt.json");
    let mut manifest: Value = serde_json::from_str(
        &fs::read_to_string(manifest_path()).expect("read model-manifest.json"),
    )
    .expect("parse manifest");
    manifest["onnx"]["onnx_sha256"] = Value::String("deadbeef".to_string());
    write_json(&corrupt_manifest, &manifest);

    let mut cfg = backend_config();
    cfg.manifest_path = corrupt_manifest;
    let err = match AugmentedNetOnnxBackend::new(cfg) {
        Ok(_) => panic!("corrupt manifest must fail"),
        Err(err) => err,
    };
    assert!(matches!(err, AugmentedNetOnnxError::ManifestValidation(_)));
    assert!(err.to_string().contains("onnx sha256 mismatch"));
}

#[test]
fn onnx_adapter_4b_input_tensor_contract_acceptance_binds_fixed_t_ingress() {
    let engine = backend();
    let dense = fixture("dense_poly");
    dense
        .stage_b
        .validate_contract(engine.fixed_t())
        .expect("fixture stage_b contract");
    let output = engine.infer(&dense.stage_b).expect("infer");
    assert_eq!(output.effective_steps, dense.stage_b.active_steps);
}

#[test]
fn onnx_adapter_4b_input_tensor_contract_acceptance_fails_before_execution_on_mismatch() {
    let engine = backend();
    let mut bad = fixture("dense_poly").stage_b;
    bad.x_bass19.pop();
    let err = engine.infer(&bad).expect_err("shape mismatch must fail");
    assert!(matches!(err, AugmentedNetOnnxError::InputContract(_)));
    assert!(err.to_string().contains("row count"));
}

#[test]
fn onnx_adapter_smoke_one_score_inference_has_non_empty_outputs() {
    let engine = backend();
    let dense = fixture("dense_poly");
    let output = engine.infer(&dense.stage_b).expect("infer");
    assert!(output.effective_steps > 0);
    let mapped = output.typed_outputs.as_head_map();
    assert_eq!(mapped.len(), AUGNET_HEAD_ORDER.len());
    for (head, data) in mapped {
        assert!(!data.raw_logits.is_empty(), "head={head}");
        assert!(!data.argmax.is_empty(), "head={head}");
    }
}

#[test]
fn onnx_adapter_head_name_order_mismatch_fails_immediately() {
    let tmp = tempdir().expect("tmp");
    let manifest_override = tmp.path().join("model-manifest.bad-output-order.json");
    let mut manifest: Value = serde_json::from_str(
        &fs::read_to_string(manifest_path()).expect("read model-manifest.json"),
    )
    .expect("parse manifest");
    manifest["signature"]["onnx_output_heads"] = Value::Array(vec![
        Value::String("Bass35".to_string()),
        Value::String("Alto35".to_string()),
        Value::String("HarmonicRhythm7".to_string()),
        Value::String("LocalKey38".to_string()),
        Value::String("PitchClassSet121".to_string()),
        Value::String("RomanNumeral31".to_string()),
        Value::String("Soprano35".to_string()),
        Value::String("Tenor35".to_string()),
        Value::String("TonicizedKey38".to_string()),
    ]);
    write_json(&manifest_override, &manifest);

    let mut cfg = backend_config();
    cfg.manifest_path = manifest_override;
    let err = match AugmentedNetOnnxBackend::new(cfg) {
        Ok(_) => panic!("head order mismatch must fail"),
        Err(err) => err,
    };
    assert!(matches!(err, AugmentedNetOnnxError::ManifestValidation(_)));
    assert!(err.to_string().contains("output heads/order mismatch"));
}

#[test]
fn onnx_adapter_4c_output_head_mapping_acceptance_maps_all_heads_and_captures_logits() {
    let engine = backend();
    let chunk = fixture("chunk_boundary");
    let output = engine.infer(&chunk.stage_b).expect("infer");
    let heads = output.typed_outputs.as_head_map();
    let head_set: BTreeSet<String> = heads.keys().cloned().collect();
    let expected_set: BTreeSet<String> = AUGNET_HEAD_ORDER.iter().map(|s| s.to_string()).collect();
    assert_eq!(head_set, expected_set);

    for (head, data) in &heads {
        assert_eq!(data.shape[0], output.effective_steps, "head={head}");
        assert_eq!(data.shape[1], data.raw_logits[0].len(), "head={head}");
        assert_eq!(data.raw_logits.len(), output.effective_steps, "head={head}");
        assert_eq!(data.argmax.len(), output.effective_steps, "head={head}");
    }

    let stage_c_a = output.to_stage_c_artifact();
    let stage_c_b = output.to_stage_c_artifact();
    let a = serde_json::to_string(&stage_c_a).expect("serialize a");
    let b = serde_json::to_string(&stage_c_b).expect("serialize b");
    assert_eq!(a, b);
}

#[test]
fn onnx_adapter_4d_onnx_boundary_parity_acceptance_passes_fixture_corpus() {
    let mut engine = backend();
    let fixture_ids = fixture_ids_manifest();
    let fixtures: Vec<_> = fixture_ids.iter().map(|id| fixture(id)).collect();
    let report = run_onnx_boundary_parity_gate(
        &mut engine,
        &fixtures,
        &OnnxBoundaryParityOptions {
            logits_atol: 1e-5,
            diff_artifact_dir: None,
        },
    )
    .expect("parity gate");
    assert_eq!(report.fixtures_checked, fixture_ids.len());
}

#[test]
fn onnx_adapter_4d_onnx_boundary_parity_acceptance_emits_head_diff_artifact_on_failure() {
    let mut engine = backend();
    let mut dense = fixture("dense_poly");
    dense
        .stage_c
        .heads
        .get_mut("Alto35")
        .expect("alto head")
        .logits[0][0] += 10.0;

    let diff_dir = tempdir().expect("diff dir");
    let err = run_onnx_boundary_parity_gate(
        &mut engine,
        &[dense],
        &OnnxBoundaryParityOptions {
            logits_atol: 1e-5,
            diff_artifact_dir: Some(diff_dir.path().to_path_buf()),
        },
    )
    .expect_err("mismatch should fail");
    assert!(matches!(err, AugmentedNetOnnxError::ParityMismatch(_)));
    assert!(err.to_string().contains("max_abs_diff"));
    let artifact = diff_dir.path().join("dense_poly_Alto35.json");
    assert!(
        artifact.exists(),
        "expected diff artifact at {}",
        artifact.display()
    );
}

#[derive(Debug, Serialize)]
struct PerfRecord {
    fixture_id: String,
    active_steps: usize,
    cold_ms: u128,
    warm_ms: Vec<u128>,
    deterministic: bool,
}

#[derive(Debug, Serialize)]
struct PerfReport {
    intra_threads: usize,
    inter_threads: usize,
    records: Vec<PerfRecord>,
}

#[test]
fn onnx_adapter_4e_performance_and_determinism_acceptance() {
    let selected = ["dense_poly", "tied_barlines", "chunk_boundary"];
    let mut records = Vec::new();

    for id in selected {
        let sample = fixture(id);
        let cold_start = Instant::now();
        let cold_backend = backend();
        let cold_output = cold_backend.infer(&sample.stage_b).expect("cold infer");
        let cold_ms = cold_start.elapsed().as_millis();

        let warm_backend = backend();
        let mut warm_ms = Vec::new();
        let first = warm_backend
            .infer(&sample.stage_b)
            .expect("warm infer first")
            .to_stage_c_artifact();
        for _ in 0..3 {
            let start = Instant::now();
            let _ = warm_backend.infer(&sample.stage_b).expect("warm infer");
            warm_ms.push(start.elapsed().as_millis());
        }
        let second = warm_backend
            .infer(&sample.stage_b)
            .expect("warm infer second")
            .to_stage_c_artifact();
        let deterministic = first == second;

        assert!(cold_ms > 0, "cold latency should be measurable for {id}");
        assert!(
            warm_ms.iter().all(|ms| *ms > 0),
            "warm latency should be measurable for {id}"
        );
        assert!(deterministic, "determinism failed for {id}");
        assert_eq!(
            cold_output.effective_steps,
            sample.stage_b.active_steps.max(1)
        );

        records.push(PerfRecord {
            fixture_id: id.to_string(),
            active_steps: sample.stage_b.active_steps,
            cold_ms,
            warm_ms,
            deterministic,
        });
    }

    let report = PerfReport {
        intra_threads: 1,
        inter_threads: 1,
        records,
    };
    let report_dir = repo_root().join("target/augnet");
    fs::create_dir_all(&report_dir).expect("create report dir");
    let report_path = report_dir.join("augnet_performance_determinism.json");
    let payload = serde_json::to_string_pretty(&report).expect("serialize report");
    fs::write(&report_path, payload).expect("write report");
    assert!(report_path.exists());
}

#[test]
fn onnx_adapter_backend_unavailable_is_fatal_no_fallback() {
    let mut cfg = backend_config();
    cfg.model_path = repo_root().join("models/augnet/DOES_NOT_EXIST.onnx");
    let err = match AugmentedNetOnnxBackend::new(cfg) {
        Ok(_) => panic!("missing model must fail"),
        Err(err) => err,
    };
    assert!(
        matches!(
            err,
            AugmentedNetOnnxError::Io { .. } | AugmentedNetOnnxError::SessionBootstrap(_)
        ),
        "unexpected error variant: {err}"
    );
}
