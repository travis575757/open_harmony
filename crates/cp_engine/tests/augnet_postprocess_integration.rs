use cp_engine::augnet_onnx::{AugmentedNetOnnxBackend, AugmentedNetOnnxConfig};
use cp_engine::augnet_onnx::{StageCArtifact, StageCHeadArtifact};
use cp_engine::augnet_postprocess::{
    decode_stage_d_from_inference, decode_stage_d_from_stage_c, run_postprocess_parity_gate,
    PostprocessParityFixture, PostprocessParityOptions, StageDArtifact,
};
use cp_engine::augnet_preprocess::{preprocess_musicxml_to_chunks, AugmentedNetPreprocessConfig};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn manifest_path() -> PathBuf {
    repo_root().join("tests/augnet_parity/fixtures_manifest.json")
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

fn manifest() -> FixturesManifest {
    serde_json::from_str(
        &fs::read_to_string(manifest_path())
            .expect("read tests/augnet_parity/fixtures_manifest.json"),
    )
    .expect("parse fixtures manifest")
}

fn load_baseline(fixture: &ManifestFixture) -> BaselineFixture {
    let path = repo_root()
        .join("tests/augnet_parity/music21_baseline")
        .join(&fixture.baseline_artifact);
    serde_json::from_str(
        &fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display())),
    )
    .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

#[derive(Debug, Clone, Deserialize)]
struct FixturesManifest {
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
struct BaselineFixture {
    fixture_id: String,
    stage_c: StageCArtifact,
    stage_d: StageDArtifact,
}

#[derive(Debug, Clone, Deserialize)]
struct DecodeAssets {
    spellings: Vec<String>,
    keys: Vec<String>,
    roman_numerals: Vec<String>,
    pcsets: Vec<Vec<u8>>,
}

fn decode_assets() -> DecodeAssets {
    let path = repo_root().join("crates/cp_engine/src/augnet_decode_assets.json");
    serde_json::from_str(&fs::read_to_string(path).expect("read decode assets"))
        .expect("parse decode assets")
}

fn index_of<'a>(items: &'a [String], wanted: &str, field: &str) -> usize {
    items
        .iter()
        .position(|v| v == wanted)
        .unwrap_or_else(|| panic!("{field} missing label {wanted}"))
}

fn index_of_pcset(items: &[Vec<u8>], wanted: &[u8]) -> usize {
    items
        .iter()
        .position(|v| v == wanted)
        .unwrap_or_else(|| panic!("pcset missing {:?}", wanted))
}

fn synthetic_edge_case_stage_c() -> StageCArtifact {
    let assets = decode_assets();
    let mut heads = BTreeMap::new();

    let idx_c = index_of(&assets.keys, "C", "keys");
    let idx_g = index_of(&assets.keys, "G", "keys");
    let idx_cad = index_of(&assets.roman_numerals, "Cad", "roman_numerals");
    let idx_v7 = index_of(&assets.roman_numerals, "V7", "roman_numerals");
    let idx_i = index_of(&assets.roman_numerals, "I", "roman_numerals");
    let idx_pcset_c_maj = index_of_pcset(&assets.pcsets, &[0, 4, 7]);
    let idx_pcset_d7 = index_of_pcset(&assets.pcsets, &[0, 2, 6, 9]);
    let idx_hr0 = 0usize;

    let idx_bass_g = index_of(&assets.spellings, "G", "spellings");
    let idx_tenor_c = index_of(&assets.spellings, "C", "spellings");
    let idx_alto_e = index_of(&assets.spellings, "E", "spellings");
    let idx_soprano_g = index_of(&assets.spellings, "G", "spellings");

    let idx_bass_d = index_of(&assets.spellings, "D", "spellings");
    let idx_tenor_fs = index_of(&assets.spellings, "F#", "spellings");
    let idx_alto_a = index_of(&assets.spellings, "A", "spellings");
    let idx_soprano_c = index_of(&assets.spellings, "C", "spellings");

    let idx_bass_css = index_of(&assets.spellings, "C##", "spellings");
    let idx_tenor_e = index_of(&assets.spellings, "E", "spellings");
    let idx_alto_g = index_of(&assets.spellings, "G", "spellings");
    let idx_soprano_c2 = index_of(&assets.spellings, "C", "spellings");

    fn head_3step(classes: usize, idxs: [usize; 3]) -> StageCHeadArtifact {
        let mut rows = Vec::new();
        for idx in idxs {
            let mut row = vec![-9.0f32; classes];
            row[idx] = 9.0;
            rows.push(row);
        }
        StageCHeadArtifact {
            shape: [3, classes],
            logits: rows,
            argmax: idxs.to_vec(),
        }
    }

    heads.insert(
        "Alto35".to_string(),
        head_3step(35, [idx_alto_e, idx_alto_a, idx_alto_g]),
    );
    heads.insert(
        "Bass35".to_string(),
        head_3step(35, [idx_bass_g, idx_bass_d, idx_bass_css]),
    );
    heads.insert(
        "HarmonicRhythm7".to_string(),
        head_3step(7, [idx_hr0, idx_hr0, idx_hr0]),
    );
    heads.insert(
        "LocalKey38".to_string(),
        head_3step(38, [idx_c, idx_c, idx_c]),
    );
    heads.insert(
        "PitchClassSet121".to_string(),
        head_3step(121, [idx_pcset_c_maj, idx_pcset_d7, idx_pcset_c_maj]),
    );
    heads.insert(
        "RomanNumeral31".to_string(),
        head_3step(31, [idx_cad, idx_v7, idx_i]),
    );
    heads.insert(
        "Soprano35".to_string(),
        head_3step(35, [idx_soprano_g, idx_soprano_c, idx_soprano_c2]),
    );
    heads.insert(
        "Tenor35".to_string(),
        head_3step(35, [idx_tenor_c, idx_tenor_fs, idx_tenor_e]),
    );
    heads.insert(
        "TonicizedKey38".to_string(),
        head_3step(38, [idx_c, idx_g, idx_c]),
    );

    StageCArtifact {
        schema_version: 1,
        effective_steps: 3,
        heads,
    }
}

#[test]
fn postprocess_a_exact_fixture_parity_matches_python_decoded_outputs() {
    let manifest = manifest();
    let fixtures: Vec<PostprocessParityFixture> = manifest
        .fixtures
        .iter()
        .map(|fixture| {
            let baseline = load_baseline(fixture);
            assert_eq!(baseline.fixture_id, fixture.id);

            let decoded = decode_stage_d_from_stage_c(&baseline.stage_c).expect("decode stage_c");
            assert_eq!(
                decoded.labels.len(),
                baseline.stage_d.labels.len(),
                "fixture={}",
                fixture.id
            );
            for (actual, expected) in decoded.labels.iter().zip(baseline.stage_d.labels.iter()) {
                assert_eq!(
                    actual.time_index, expected.time_index,
                    "fixture={}",
                    fixture.id
                );
                assert_eq!(
                    actual.components, expected.components,
                    "fixture={}",
                    fixture.id
                );
                assert_eq!(
                    actual.component_labels, expected.component_labels,
                    "fixture={}",
                    fixture.id
                );
                assert_eq!(
                    actual.roman_numeral_resolved, expected.roman_numeral_resolved,
                    "fixture={}",
                    fixture.id
                );
                assert_eq!(
                    actual.local_key, expected.local_key,
                    "fixture={}",
                    fixture.id
                );
                assert_eq!(
                    actual.tonicized_key_resolved, expected.tonicized_key_resolved,
                    "fixture={}",
                    fixture.id
                );
                assert_eq!(
                    actual.chord_quality, expected.chord_quality,
                    "fixture={}",
                    fixture.id
                );
                assert_eq!(
                    actual.inversion_figure, expected.inversion_figure,
                    "fixture={}",
                    fixture.id
                );
            }

            PostprocessParityFixture {
                fixture_id: fixture.id.clone(),
                stage_c: baseline.stage_c,
                stage_d: baseline.stage_d,
            }
        })
        .collect();

    let report = run_postprocess_parity_gate(
        &fixtures,
        &PostprocessParityOptions {
            float_atol: 5e-6,
            diff_artifact_dir: None,
        },
    )
    .expect("stage_d parity gate");
    assert_eq!(report.fixtures_checked, fixtures.len());
}

#[test]
fn postprocess_b_edge_case_snapshot_covers_cadential_tonicization_and_root_fallback() {
    let stage_c = synthetic_edge_case_stage_c();
    let stage_d = decode_stage_d_from_stage_c(&stage_c).expect("decode synthetic stage_c");
    let actual = serde_json::to_value(&stage_d).expect("serialize actual");
    let snapshot_path = repo_root()
        .join("crates/cp_engine/tests/fixtures/postprocess_edge_snapshots.expected.json");
    let expected: Value = serde_json::from_str(
        &fs::read_to_string(&snapshot_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", snapshot_path.display())),
    )
    .expect("parse snapshot json");
    assert_eq!(actual, expected);
}

#[test]
fn postprocess_c_confidence_is_correct_per_head_and_non_decision_affecting() {
    let mut stage_c = synthetic_edge_case_stage_c();
    let local_key = stage_c
        .heads
        .get_mut("LocalKey38")
        .expect("LocalKey38 head");
    local_key.logits[0][0] = 10.0;
    local_key.logits[0][1] = 9.0;
    local_key.argmax[0] = 1;

    let stage_d = decode_stage_d_from_stage_c(&stage_c).expect("decode with manual argmax");
    let head = stage_d.heads.get("LocalKey38").expect("decoded head");
    assert_eq!(
        head.argmax[0], 1,
        "decode must respect preselected class index"
    );

    let row = &head.raw_logits[0];
    let max = row.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let exp: Vec<f64> = row.iter().map(|v| (*v - max).exp() as f64).collect();
    let denom: f64 = exp.iter().sum();
    let chosen = exp[1] / denom;
    let second = exp[0] / denom;
    let expected_top1 = (chosen * 1_000_000.0).round() / 1_000_000.0;
    let expected_margin = ((chosen - second) * 1_000_000.0).round() / 1_000_000.0;
    assert_eq!(head.confidence_top1[0], expected_top1);
    assert_eq!(head.confidence_margin[0], expected_margin);
    assert!(head.confidence_margin[0] < 0.0);
}

#[test]
fn postprocess_d_schema_preserves_raw_logits_and_is_deterministic() {
    let manifest = manifest();
    let fixture = manifest
        .fixtures
        .iter()
        .find(|f| f.id == "dense_poly")
        .expect("dense_poly fixture");
    let config = AugmentedNetPreprocessConfig {
        fixed_offset: manifest.fixed_offset,
        max_steps: manifest.max_steps,
        ..AugmentedNetPreprocessConfig::default()
    };
    let xml_path = repo_root().join(&fixture.musicxml_path);
    let xml = fs::read_to_string(&xml_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", xml_path.display()));
    let chunks = preprocess_musicxml_to_chunks(&xml, &config).expect("preprocess");
    let backend = AugmentedNetOnnxBackend::new(backend_config()).expect("backend");
    let inference = backend
        .infer(&chunks.chunks[0].tensors)
        .expect("infer first chunk");

    let d_a = decode_stage_d_from_inference(&inference).expect("decode A");
    let d_b = decode_stage_d_from_inference(&inference).expect("decode B");
    assert_eq!(d_a, d_b);
    for (head_name, head) in &d_a.heads {
        assert!(
            !head.raw_logits.is_empty(),
            "raw logits must be present for {head_name}"
        );
        assert_eq!(
            head.raw_logits.len(),
            d_a.effective_steps,
            "time rows for {head_name}"
        );
        assert_eq!(
            head.argmax.len(),
            d_a.effective_steps,
            "argmax rows for {head_name}"
        );
        assert_eq!(
            head.confidence_top1.len(),
            d_a.effective_steps,
            "confidence rows for {head_name}"
        );
    }
}

#[test]
fn postprocess_e_postprocess_parity_gate_emits_actionable_diff_artifact_on_mismatch() {
    let manifest = manifest();
    let fixture = manifest
        .fixtures
        .iter()
        .find(|f| f.id == "tied_barlines")
        .expect("tied_barlines fixture");
    let mut baseline = load_baseline(fixture);
    baseline.stage_d.labels[0].roman_numeral_resolved = "BROKEN".to_string();

    let tmp = tempdir().expect("tmp dir");
    let err = run_postprocess_parity_gate(
        &[PostprocessParityFixture {
            fixture_id: fixture.id.clone(),
            stage_c: baseline.stage_c,
            stage_d: baseline.stage_d,
        }],
        &PostprocessParityOptions {
            float_atol: 0.0,
            diff_artifact_dir: Some(tmp.path().to_path_buf()),
        },
    )
    .expect_err("mismatch must fail");
    assert!(err.to_string().contains("stage_d mismatch"));
    let artifact = tmp.path().join("tied_barlines_stage_d_diff.json");
    assert!(
        artifact.exists(),
        "expected diff artifact at {}",
        artifact.display()
    );
}

#[test]
fn postprocess_e_postprocess_parity_gate_passes_fixture_corpus() {
    let manifest = manifest();
    let fixtures: Vec<PostprocessParityFixture> = manifest
        .fixtures
        .iter()
        .map(|fixture| {
            let baseline = load_baseline(fixture);
            PostprocessParityFixture {
                fixture_id: fixture.id.clone(),
                stage_c: baseline.stage_c,
                stage_d: baseline.stage_d,
            }
        })
        .collect();
    let report = run_postprocess_parity_gate(&fixtures, &PostprocessParityOptions::default())
        .expect("parity gate");
    assert_eq!(report.fixtures_checked, fixtures.len());
}

#[test]
fn postprocess_f_confidence_and_schema_hold_on_full_fixture_corpus_decode() {
    let manifest = manifest();
    for fixture in &manifest.fixtures {
        let baseline = load_baseline(fixture);
        let decoded = decode_stage_d_from_stage_c(&baseline.stage_c).expect("decode fixture");
        for head in decoded.heads.values() {
            for (top1, margin) in head
                .confidence_top1
                .iter()
                .zip(head.confidence_margin.iter())
            {
                assert!((*top1 >= 0.0) && (*top1 <= 1.0));
                assert!(*margin <= 1.0);
            }
        }
    }
}

#[test]
fn postprocess_g_decode_rejects_missing_required_head() {
    let mut stage_c = synthetic_edge_case_stage_c();
    stage_c.heads.remove("TonicizedKey38");

    let err = decode_stage_d_from_stage_c(&stage_c).expect_err("missing required head must fail");
    assert!(err
        .to_string()
        .contains("missing required output head for Stage D decode: TonicizedKey38"));
}

#[test]
fn postprocess_h_decode_rejects_out_of_bounds_argmax_index() {
    let mut stage_c = synthetic_edge_case_stage_c();
    let bass = stage_c.heads.get_mut("Bass35").expect("Bass35 head");
    bass.argmax[0] = bass.logits[0].len();

    let err = decode_stage_d_from_stage_c(&stage_c).expect_err("argmax out of bounds must fail");
    assert!(err.to_string().contains("Bass35 argmax[0]"));
    assert!(err.to_string().contains("out of bounds"));
}

#[test]
fn postprocess_i_decode_rejects_empty_logit_row() {
    let mut stage_c = synthetic_edge_case_stage_c();
    let bass = stage_c.heads.get_mut("Bass35").expect("Bass35 head");
    bass.logits[0].clear();
    bass.argmax[0] = 0;

    let err = decode_stage_d_from_stage_c(&stage_c).expect_err("empty logits row must fail");
    assert!(err.to_string().contains("Bass35 logits row 0 is empty"));
}

#[test]
fn postprocess_j_parity_gate_allows_small_float_drift_within_tolerance() {
    let manifest = manifest();
    let fixture = manifest
        .fixtures
        .iter()
        .find(|f| f.id == "tied_barlines")
        .expect("tied_barlines fixture");
    let mut baseline = load_baseline(fixture);
    baseline
        .stage_d
        .heads
        .get_mut("Bass35")
        .expect("Bass35 in baseline")
        .raw_logits[0][0] += 1e-6;

    let report = run_postprocess_parity_gate(
        &[PostprocessParityFixture {
            fixture_id: fixture.id.clone(),
            stage_c: baseline.stage_c,
            stage_d: baseline.stage_d,
        }],
        &PostprocessParityOptions {
            float_atol: 5e-6,
            diff_artifact_dir: None,
        },
    )
    .expect("small float drift should pass parity gate");
    assert_eq!(report.fixtures_checked, 1);
}

#[test]
fn postprocess_k_parity_gate_rejects_large_float_drift_and_reports_field_path() {
    let manifest = manifest();
    let fixture = manifest
        .fixtures
        .iter()
        .find(|f| f.id == "tied_barlines")
        .expect("tied_barlines fixture");
    let mut baseline = load_baseline(fixture);
    baseline
        .stage_d
        .heads
        .get_mut("Bass35")
        .expect("Bass35 in baseline")
        .raw_logits[0][0] += 1e-2;

    let err = run_postprocess_parity_gate(
        &[PostprocessParityFixture {
            fixture_id: fixture.id.clone(),
            stage_c: baseline.stage_c,
            stage_d: baseline.stage_d,
        }],
        &PostprocessParityOptions {
            float_atol: 5e-6,
            diff_artifact_dir: None,
        },
    )
    .expect_err("large float drift must fail parity gate");

    let msg = err.to_string();
    assert!(msg.contains("stage_d mismatch"));
    assert!(msg.contains("raw_logits"));
}
