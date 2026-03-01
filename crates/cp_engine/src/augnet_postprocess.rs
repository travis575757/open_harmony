use crate::augnet_onnx::{
    AugmentedNetHeadOutput, AugmentedNetInferenceOutput, AugmentedNetOnnxError,
    AugmentedNetOnnxResult, StageCArtifact,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const DECODE_ASSET_JSON: &str = include_str!("augnet_decode_assets.json");
const WEBER_DIAGONAL: [&str; 40] = [
    "B--", "c-", "F-", "g-", "C-", "d-", "G-", "a-", "D-", "e-", "A-", "b-", "E-", "f", "B-", "c",
    "F", "g", "C", "d", "G", "a", "D", "e", "A", "b", "E", "f#", "B", "c#", "F#", "g#", "C#", "d#",
    "G#", "a#", "D#", "e#", "A#", "b#",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AugmentedNetHeadConfidence {
    pub confidence_top1: f64,
    pub confidence_margin: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StageDHeadArtifact {
    pub shape: [usize; 2],
    pub raw_logits: Vec<Vec<f32>>,
    pub argmax: Vec<usize>,
    pub decoded_labels: Vec<String>,
    pub confidence_top1: Vec<f64>,
    pub confidence_margin: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StageDResolvedLabel {
    pub time_index: usize,
    pub components: BTreeMap<String, usize>,
    pub component_labels: BTreeMap<String, String>,
    pub component_confidence: BTreeMap<String, AugmentedNetHeadConfidence>,
    pub local_key: String,
    pub tonicized_key_predicted: String,
    pub tonicized_key_resolved: String,
    pub tonicization: Option<String>,
    pub roman_numeral_predicted: String,
    pub roman_numeral_resolved: String,
    pub roman_numeral_formatted: String,
    pub pitch_class_set_predicted: Vec<u8>,
    pub pitch_class_set_resolved: Vec<u8>,
    pub chord_pitch_names: Vec<String>,
    pub chord_root: String,
    pub chord_quality: String,
    pub chord_bass: String,
    pub inversion_index: usize,
    pub inversion_figure: String,
    pub chord_label_raw: String,
    pub chord_label_formatted: String,
    pub harmonic_rhythm: i32,
    pub is_cadential_64: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StageDArtifact {
    pub schema_version: u32,
    pub effective_steps: usize,
    pub heads: BTreeMap<String, StageDHeadArtifact>,
    pub labels: Vec<StageDResolvedLabel>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PostprocessParityFixture {
    pub fixture_id: String,
    pub stage_c: StageCArtifact,
    pub stage_d: StageDArtifact,
}

#[derive(Debug, Clone)]
pub struct PostprocessParityOptions {
    pub float_atol: f32,
    pub diff_artifact_dir: Option<PathBuf>,
}

impl Default for PostprocessParityOptions {
    fn default() -> Self {
        Self {
            float_atol: 5e-6,
            diff_artifact_dir: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PostprocessParityReport {
    pub fixtures_checked: usize,
    pub float_atol: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PostprocessDiffArtifact {
    pub fixture_id: String,
    pub field_path: String,
    pub expected_summary: serde_json::Value,
    pub actual_summary: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
struct DecodeAssets {
    schema_version: u32,
    spellings: Vec<String>,
    keys: Vec<String>,
    roman_numerals: Vec<String>,
    qualities: Vec<String>,
    pcsets: Vec<Vec<u8>>,
    pcset_key_entries: Vec<Vec<PcsetKeyEntry>>,
    numerator_pitch_classes: Vec<Vec<Vec<u8>>>,
    tonicization_scale_degrees: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
struct PcsetKeyEntry {
    key_index: usize,
    rn_index: usize,
    quality_index: usize,
    chord_spelling_indices: Vec<usize>,
}

#[derive(Debug)]
struct DecodeRuntime {
    assets: DecodeAssets,
    pcset_vectors: Vec<[f64; 12]>,
    weber_index: HashMap<&'static str, usize>,
}

#[derive(Debug)]
struct ResolvedChord {
    resolved_rn: String,
    formatted_rn: String,
    resolved_tonicized_key: String,
    tonicization: Option<String>,
    resolved_pcset: Vec<u8>,
    chord: Vec<String>,
    chord_root: String,
    chord_quality: String,
    chord_bass: String,
    inversion_index: usize,
    inversion_figure: String,
    chord_label_raw: String,
    chord_label_formatted: String,
    is_cadential_64: bool,
}

pub fn decode_stage_d_from_inference(
    output: &AugmentedNetInferenceOutput,
) -> AugmentedNetOnnxResult<StageDArtifact> {
    decode_from_head_map(output.effective_steps, &output.typed_outputs.as_head_map())
}

pub fn decode_stage_d_from_stage_c(
    stage_c: &StageCArtifact,
) -> AugmentedNetOnnxResult<StageDArtifact> {
    let mut head_map = BTreeMap::new();
    for (head, data) in &stage_c.heads {
        head_map.insert(
            head.clone(),
            AugmentedNetHeadOutput {
                shape: data.shape,
                raw_logits: data.logits.clone(),
                argmax: data.argmax.clone(),
            },
        );
    }
    decode_from_head_map(stage_c.effective_steps, &head_map)
}

pub fn run_postprocess_parity_gate(
    fixtures: &[PostprocessParityFixture],
    options: &PostprocessParityOptions,
) -> AugmentedNetOnnxResult<PostprocessParityReport> {
    for fixture in fixtures {
        let actual = decode_stage_d_from_stage_c(&fixture.stage_c)?;
        verify_stage_d_parity(
            &fixture.fixture_id,
            &fixture.stage_d,
            &actual,
            options.float_atol,
            options.diff_artifact_dir.as_deref(),
        )?;
    }
    Ok(PostprocessParityReport {
        fixtures_checked: fixtures.len(),
        float_atol: options.float_atol,
    })
}

fn decode_from_head_map(
    effective_steps: usize,
    head_map: &BTreeMap<String, AugmentedNetHeadOutput>,
) -> AugmentedNetOnnxResult<StageDArtifact> {
    let runtime = decode_runtime()?;
    validate_head_contract(effective_steps, head_map)?;

    let mut heads = BTreeMap::new();
    for (head_name, head) in head_map {
        let mut decoded_labels = Vec::with_capacity(effective_steps);
        let mut confidence_top1: Vec<f64> = Vec::with_capacity(effective_steps);
        let mut confidence_margin: Vec<f64> = Vec::with_capacity(effective_steps);
        for t in 0..effective_steps {
            let idx = head.argmax[t];
            decoded_labels.push(head_label_to_string(&runtime.assets, head_name, idx)?);
            let (top1, margin) = confidence_for_decision(&head.raw_logits[t], idx)?;
            confidence_top1.push(round6f64(top1));
            confidence_margin.push(round6f64(margin));
        }
        heads.insert(
            head_name.clone(),
            StageDHeadArtifact {
                shape: head.shape,
                raw_logits: head.raw_logits.clone(),
                argmax: head.argmax.clone(),
                decoded_labels,
                confidence_top1,
                confidence_margin,
            },
        );
    }

    let mut labels = Vec::with_capacity(effective_steps);
    for t in 0..effective_steps {
        let mut components = BTreeMap::new();
        let mut component_labels = BTreeMap::new();
        let mut component_confidence = BTreeMap::new();
        for (head_name, head) in &heads {
            let class_index = head.argmax[t];
            components.insert(head_name.clone(), class_index);
            component_labels.insert(head_name.clone(), head.decoded_labels[t].clone());
            component_confidence.insert(
                head_name.clone(),
                AugmentedNetHeadConfidence {
                    confidence_top1: head.confidence_top1[t],
                    confidence_margin: head.confidence_margin[t],
                },
            );
        }

        let local_idx = required_component(&components, "LocalKey38")?;
        let tonicized_idx = required_component(&components, "TonicizedKey38")?;
        let pcs_idx = required_component(&components, "PitchClassSet121")?;
        let rn_idx = required_component(&components, "RomanNumeral31")?;
        let hr_idx = required_component(&components, "HarmonicRhythm7")?;
        let bass_idx = required_component(&components, "Bass35")?;
        let tenor_idx = required_component(&components, "Tenor35")?;
        let alto_idx = required_component(&components, "Alto35")?;
        let soprano_idx = required_component(&components, "Soprano35")?;

        let local_key = required_idx(&runtime.assets.keys, local_idx, "LocalKey38")?.to_string();
        let tonicized_predicted =
            required_idx(&runtime.assets.keys, tonicized_idx, "TonicizedKey38")?.to_string();
        let predicted_rn =
            required_idx(&runtime.assets.roman_numerals, rn_idx, "RomanNumeral31")?.to_string();
        let predicted_pcs =
            required_idx(&runtime.assets.pcsets, pcs_idx, "PitchClassSet121")?.clone();
        let bass = required_idx(&runtime.assets.spellings, bass_idx, "Bass35")?.to_string();
        let tenor = required_idx(&runtime.assets.spellings, tenor_idx, "Tenor35")?.to_string();
        let alto = required_idx(&runtime.assets.spellings, alto_idx, "Alto35")?.to_string();
        let soprano =
            required_idx(&runtime.assets.spellings, soprano_idx, "Soprano35")?.to_string();
        let harmonic_rhythm = i32::try_from(hr_idx).map_err(|_| {
            AugmentedNetOnnxError::OutputContract(format!(
                "HarmonicRhythm7 index out of range for i32: {hr_idx}"
            ))
        })?;

        let resolved = resolve_roman_numeral_cosine(
            runtime,
            &bass,
            &tenor,
            &alto,
            &soprano,
            &predicted_pcs,
            local_idx,
            rn_idx,
            tonicized_idx,
        )?;

        labels.push(StageDResolvedLabel {
            time_index: t,
            components,
            component_labels,
            component_confidence,
            local_key,
            tonicized_key_predicted: tonicized_predicted,
            tonicized_key_resolved: resolved.resolved_tonicized_key,
            tonicization: resolved.tonicization,
            roman_numeral_predicted: predicted_rn,
            roman_numeral_resolved: resolved.resolved_rn,
            roman_numeral_formatted: resolved.formatted_rn,
            pitch_class_set_predicted: predicted_pcs,
            pitch_class_set_resolved: resolved.resolved_pcset,
            chord_pitch_names: resolved.chord,
            chord_root: resolved.chord_root,
            chord_quality: resolved.chord_quality,
            chord_bass: resolved.chord_bass,
            inversion_index: resolved.inversion_index,
            inversion_figure: resolved.inversion_figure,
            chord_label_raw: resolved.chord_label_raw,
            chord_label_formatted: resolved.chord_label_formatted,
            harmonic_rhythm,
            is_cadential_64: resolved.is_cadential_64,
        });
    }

    Ok(StageDArtifact {
        schema_version: runtime.assets.schema_version,
        effective_steps,
        heads,
        labels,
    })
}

fn validate_head_contract(
    effective_steps: usize,
    head_map: &BTreeMap<String, AugmentedNetHeadOutput>,
) -> AugmentedNetOnnxResult<()> {
    for required in [
        "Alto35",
        "Bass35",
        "HarmonicRhythm7",
        "LocalKey38",
        "PitchClassSet121",
        "RomanNumeral31",
        "Soprano35",
        "Tenor35",
        "TonicizedKey38",
    ] {
        if !head_map.contains_key(required) {
            return Err(AugmentedNetOnnxError::OutputContract(format!(
                "missing required output head for Stage D decode: {required}"
            )));
        }
    }
    for (head, data) in head_map {
        if data.raw_logits.len() != effective_steps {
            return Err(AugmentedNetOnnxError::OutputContract(format!(
                "{head} logits row count {} does not match effective_steps {}",
                data.raw_logits.len(),
                effective_steps
            )));
        }
        if data.argmax.len() != effective_steps {
            return Err(AugmentedNetOnnxError::OutputContract(format!(
                "{head} argmax row count {} does not match effective_steps {}",
                data.argmax.len(),
                effective_steps
            )));
        }
        for (t, row) in data.raw_logits.iter().enumerate() {
            if row.is_empty() {
                return Err(AugmentedNetOnnxError::OutputContract(format!(
                    "{head} logits row {t} is empty"
                )));
            }
            if data.argmax[t] >= row.len() {
                return Err(AugmentedNetOnnxError::OutputContract(format!(
                    "{head} argmax[{t}]={} out of bounds for class width {}",
                    data.argmax[t],
                    row.len()
                )));
            }
        }
    }
    Ok(())
}

fn required_component(
    components: &BTreeMap<String, usize>,
    head: &str,
) -> AugmentedNetOnnxResult<usize> {
    components.get(head).copied().ok_or_else(|| {
        AugmentedNetOnnxError::OutputContract(format!(
            "missing component {head} during Stage D reconstruction"
        ))
    })
}

fn required_idx<'a, T>(items: &'a [T], idx: usize, name: &str) -> AugmentedNetOnnxResult<&'a T> {
    items.get(idx).ok_or_else(|| {
        AugmentedNetOnnxError::OutputContract(format!(
            "{name} index out of range: idx={idx}, len={}",
            items.len()
        ))
    })
}

fn head_label_to_string(
    assets: &DecodeAssets,
    head: &str,
    idx: usize,
) -> AugmentedNetOnnxResult<String> {
    match head {
        "Alto35" | "Bass35" | "Soprano35" | "Tenor35" => {
            Ok(required_idx(&assets.spellings, idx, head)?.to_string())
        }
        "LocalKey38" | "TonicizedKey38" => Ok(required_idx(&assets.keys, idx, head)?.to_string()),
        "RomanNumeral31" => Ok(required_idx(&assets.roman_numerals, idx, head)?.to_string()),
        "PitchClassSet121" => {
            let pcs = required_idx(&assets.pcsets, idx, head)?;
            Ok(format!(
                "[{}]",
                pcs.iter()
                    .map(std::string::ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            ))
        }
        "HarmonicRhythm7" => Ok(idx.to_string()),
        _ => Err(AugmentedNetOnnxError::OutputContract(format!(
            "unknown head for label decode: {head}"
        ))),
    }
}

fn resolve_roman_numeral_cosine(
    runtime: &DecodeRuntime,
    bass: &str,
    tenor: &str,
    alto: &str,
    soprano: &str,
    predicted_pcs: &[u8],
    local_idx: usize,
    rn_idx: usize,
    tonicized_idx: usize,
) -> AugmentedNetOnnxResult<ResolvedChord> {
    let assets = &runtime.assets;
    let local_key = required_idx(&assets.keys, local_idx, "LocalKey38")?;
    let _predicted_tonicized = required_idx(&assets.keys, tonicized_idx, "TonicizedKey38")?;
    let numerator = required_idx(&assets.roman_numerals, rn_idx, "RomanNumeral31")?;

    let mut pcset_vector = [0.0f64; 12];
    for pitch in [bass, tenor, alto, soprano] {
        let pc = spelling_to_pc(pitch).ok_or_else(|| {
            AugmentedNetOnnxError::OutputContract(format!(
                "unable to parse SATB pitch spelling: {pitch}"
            ))
        })?;
        pcset_vector[pc as usize] += 1.0;
    }
    for pc in predicted_pcs {
        pcset_vector[*pc as usize] += 1.0;
    }
    for pc in required_idx(
        required_idx(
            &assets.numerator_pitch_classes,
            tonicized_idx,
            "numerator_pitch_classes",
        )?,
        rn_idx,
        "numerator_pitch_classes",
    )? {
        pcset_vector[*pc as usize] += 1.0;
    }

    let mut best_pcset_idx = 0usize;
    let mut best_similarity = f64::NEG_INFINITY;
    for (idx, candidate) in runtime.pcset_vectors.iter().enumerate() {
        let sim = cosine_similarity(&pcset_vector, candidate);
        if sim > best_similarity {
            best_similarity = sim;
            best_pcset_idx = idx;
        }
    }

    let mut resolved_tonicized_idx = tonicized_idx;
    if !has_pcset_key_entry(assets, best_pcset_idx, tonicized_idx) {
        let candidates: Vec<usize> = assets.pcset_key_entries[best_pcset_idx]
            .iter()
            .map(|entry| entry.key_index)
            .collect();
        resolved_tonicized_idx = force_tonicization(runtime, local_idx, &candidates)?;
    }
    let resolved_tonicized_key =
        required_idx(&assets.keys, resolved_tonicized_idx, "TonicizedKey38")?;
    let entry = entry_for_pcset_key(assets, best_pcset_idx, resolved_tonicized_idx)?;

    let mut rn_figure =
        required_idx(&assets.roman_numerals, entry.rn_index, "RomanNumeral31")?.to_string();
    let chord: Vec<String> = entry
        .chord_spelling_indices
        .iter()
        .map(|idx| {
            required_idx(&assets.spellings, *idx, "spellings").map(std::string::ToString::to_string)
        })
        .collect::<AugmentedNetOnnxResult<_>>()?;
    let quality = required_idx(&assets.qualities, entry.quality_index, "qualities")?.to_string();
    let chord_type = if assets.pcsets[best_pcset_idx].len() == 4 {
        "seventh"
    } else {
        "triad"
    };
    let inversion = chord.iter().position(|p| p == bass).unwrap_or(0);
    let inversion_figure = inversion_figure(chord_type, inversion)?;

    if matches!(inversion_figure.as_str(), "65" | "43" | "2") {
        rn_figure = rn_figure.replace('7', &inversion_figure);
    } else if matches!(inversion_figure.as_str(), "6" | "64") {
        rn_figure.push_str(&inversion_figure);
    }

    let mut resolved_rn = rn_figure;
    if numerator == "Cad" && inversion == 2 {
        resolved_rn = "Cad64".to_string();
    }

    let mut tonicization = None;
    if resolved_tonicized_key != local_key {
        let degree = required_idx(
            required_idx(
                &assets.tonicization_scale_degrees,
                local_idx,
                "tonicization_scale_degrees",
            )?,
            resolved_tonicized_idx,
            "tonicization_scale_degrees",
        )?
        .to_string();
        resolved_rn = format!("{resolved_rn}/{degree}");
        tonicization = Some(degree);
    }

    let mut chord_label_raw = format!("{}{}", chord[0], quality);
    if inversion != 0 {
        chord_label_raw.push('/');
        chord_label_raw.push_str(&chord[inversion]);
    }
    let chord_label_formatted = format_chord_label(&chord_label_raw);

    Ok(ResolvedChord {
        formatted_rn: format_roman_numeral(&resolved_rn),
        resolved_rn: resolved_rn.clone(),
        resolved_tonicized_key: resolved_tonicized_key.to_string(),
        tonicization,
        resolved_pcset: assets.pcsets[best_pcset_idx].clone(),
        chord_root: chord[0].clone(),
        chord_quality: quality,
        chord_bass: chord[inversion].clone(),
        inversion_index: inversion,
        inversion_figure: inversion_figure.clone(),
        chord_label_raw,
        chord_label_formatted,
        chord,
        is_cadential_64: resolved_rn.starts_with("Cad64"),
    })
}

fn inversion_figure(chord_type: &str, inversion: usize) -> AugmentedNetOnnxResult<String> {
    match chord_type {
        "triad" => match inversion {
            0 => Ok("".to_string()),
            1 => Ok("6".to_string()),
            2 => Ok("64".to_string()),
            _ => Err(AugmentedNetOnnxError::OutputContract(format!(
                "invalid triad inversion index: {inversion}"
            ))),
        },
        "seventh" => match inversion {
            0 => Ok("7".to_string()),
            1 => Ok("65".to_string()),
            2 => Ok("43".to_string()),
            3 => Ok("2".to_string()),
            _ => Err(AugmentedNetOnnxError::OutputContract(format!(
                "invalid seventh inversion index: {inversion}"
            ))),
        },
        _ => Err(AugmentedNetOnnxError::OutputContract(format!(
            "unknown chord type: {chord_type}"
        ))),
    }
}

fn has_pcset_key_entry(assets: &DecodeAssets, pcset_idx: usize, key_idx: usize) -> bool {
    assets.pcset_key_entries[pcset_idx]
        .iter()
        .any(|entry| entry.key_index == key_idx)
}

fn entry_for_pcset_key(
    assets: &DecodeAssets,
    pcset_idx: usize,
    key_idx: usize,
) -> AugmentedNetOnnxResult<&PcsetKeyEntry> {
    assets.pcset_key_entries[pcset_idx]
        .iter()
        .find(|entry| entry.key_index == key_idx)
        .ok_or_else(|| {
            AugmentedNetOnnxError::OutputContract(format!(
                "missing chord vocabulary entry for pcset_idx={pcset_idx} key_idx={key_idx}"
            ))
        })
}

fn force_tonicization(
    runtime: &DecodeRuntime,
    local_idx: usize,
    candidates: &[usize],
) -> AugmentedNetOnnxResult<usize> {
    let assets = &runtime.assets;
    let local_key = required_idx(&assets.keys, local_idx, "LocalKey38")?;
    let mut best_candidate = None;
    let mut best_distance = f64::INFINITY;
    for candidate_idx in candidates {
        let candidate_key = required_idx(&assets.keys, *candidate_idx, "TonicizedKey38")?;
        let mut distance = weber_euclidean(runtime, local_key, candidate_key)?;
        let degree = required_idx(
            required_idx(
                &assets.tonicization_scale_degrees,
                local_idx,
                "tonicization_scale_degrees",
            )?,
            *candidate_idx,
            "tonicization_scale_degrees",
        )?;
        if degree != "i" && degree != "III" {
            distance *= 1.05;
        }
        if !matches!(degree.as_str(), "i" | "I" | "III" | "iv" | "IV" | "v" | "V") {
            distance *= 1.05;
        }
        if distance < best_distance {
            best_distance = distance;
            best_candidate = Some(*candidate_idx);
        }
    }
    best_candidate.ok_or_else(|| {
        AugmentedNetOnnxError::OutputContract(format!(
            "force_tonicization received empty candidate list for local key {local_key}"
        ))
    })
}

fn weber_euclidean(runtime: &DecodeRuntime, k1: &str, k2: &str) -> AugmentedNetOnnxResult<f64> {
    let i1 = runtime.weber_index.get(k1).copied().ok_or_else(|| {
        AugmentedNetOnnxError::OutputContract(format!("unknown key in Weber map: {k1}"))
    })? as i32;
    let i2 = runtime.weber_index.get(k2).copied().ok_or_else(|| {
        AugmentedNetOnnxError::OutputContract(format!("unknown key in Weber map: {k2}"))
    })? as i32;
    let (flatter, sharper) = if i1 <= i2 { (i1, i2) } else { (i2, i1) };
    let mut best = f64::INFINITY;
    for i in 0..(WEBER_DIAGONAL.len() as i32 / 2) {
        let new_x = flatter + 2 * i;
        let new_y = flatter + 3 * i;
        let dx = (sharper - new_x) as f64;
        let dy = (sharper - new_y) as f64;
        let d = (dx * dx + dy * dy).sqrt();
        if d < best {
            best = d;
        }
    }
    Ok(best)
}

fn spelling_to_pc(spelling: &str) -> Option<u8> {
    let mut chars = spelling.chars();
    let step = chars.next()?;
    let mut base: i32 = match step {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    for c in chars {
        match c {
            '#' => base += 1,
            '-' => base -= 1,
            _ => {}
        }
    }
    Some(base.rem_euclid(12) as u8)
}

fn cosine_similarity(left: &[f64; 12], right: &[f64; 12]) -> f64 {
    let mut dot = 0.0f64;
    let mut left_norm = 0.0f64;
    let mut right_norm = 0.0f64;
    for i in 0..12 {
        dot += left[i] * right[i];
        left_norm += left[i] * left[i];
        right_norm += right[i] * right[i];
    }
    if left_norm == 0.0 || right_norm == 0.0 {
        return -1.0;
    }
    dot / (left_norm.sqrt() * right_norm.sqrt())
}

fn confidence_for_decision(
    logits: &[f32],
    chosen_idx: usize,
) -> AugmentedNetOnnxResult<(f64, f64)> {
    if logits.is_empty() {
        return Err(AugmentedNetOnnxError::OutputContract(
            "confidence cannot be computed from empty logits".to_string(),
        ));
    }
    if chosen_idx >= logits.len() {
        return Err(AugmentedNetOnnxError::OutputContract(format!(
            "confidence chosen index out of bounds: chosen_idx={chosen_idx}, classes={}",
            logits.len()
        )));
    }

    let max_logit = logits
        .iter()
        .fold(f32::NEG_INFINITY, |acc, value| acc.max(*value));
    let mut denom = 0.0f64;
    for value in logits {
        denom += ((*value - max_logit).exp()) as f64;
    }
    let chosen_prob = (logits[chosen_idx] - max_logit).exp() as f64 / denom;
    let mut next_best_logit = f32::NEG_INFINITY;
    for (idx, value) in logits.iter().enumerate() {
        if idx == chosen_idx {
            continue;
        }
        if *value > next_best_logit {
            next_best_logit = *value;
        }
    }
    let second_prob = if logits.len() > 1 {
        (next_best_logit - max_logit).exp() as f64 / denom
    } else {
        0.0
    };
    Ok((chosen_prob, chosen_prob - second_prob))
}

fn format_chord_label(raw: &str) -> String {
    let mut label = raw.to_string();
    if label.ends_with("maj") {
        let len = label.len();
        label.truncate(len - 3);
    }
    label.replace('-', "b")
}

fn format_roman_numeral(raw: &str) -> String {
    if raw == "I/I" {
        "I".to_string()
    } else {
        raw.to_string()
    }
}

fn decode_runtime() -> AugmentedNetOnnxResult<&'static DecodeRuntime> {
    static RUNTIME: OnceLock<Result<DecodeRuntime, String>> = OnceLock::new();
    let result = RUNTIME.get_or_init(|| build_decode_runtime().map_err(|e| e.to_string()));
    match result {
        Ok(runtime) => Ok(runtime),
        Err(message) => Err(AugmentedNetOnnxError::OutputContract(format!(
            "failed to initialize decode runtime: {message}"
        ))),
    }
}

fn build_decode_runtime() -> AugmentedNetOnnxResult<DecodeRuntime> {
    let assets: DecodeAssets = serde_json::from_str(DECODE_ASSET_JSON).map_err(|e| {
        AugmentedNetOnnxError::OutputContract(format!("decode asset JSON parse failed: {e}"))
    })?;
    if assets.schema_version != 1 {
        return Err(AugmentedNetOnnxError::OutputContract(format!(
            "unsupported decode asset schema_version={}, expected=1",
            assets.schema_version
        )));
    }
    if assets.pcset_key_entries.len() != assets.pcsets.len() {
        return Err(AugmentedNetOnnxError::OutputContract(format!(
            "decode asset mismatch: pcset_key_entries={} pcsets={}",
            assets.pcset_key_entries.len(),
            assets.pcsets.len()
        )));
    }
    if assets.numerator_pitch_classes.len() != assets.keys.len() {
        return Err(AugmentedNetOnnxError::OutputContract(format!(
            "decode asset mismatch: numerator_pitch_classes={} keys={}",
            assets.numerator_pitch_classes.len(),
            assets.keys.len()
        )));
    }
    let mut pcset_vectors = Vec::with_capacity(assets.pcsets.len());
    for pcs in &assets.pcsets {
        let mut vec = [0.0f64; 12];
        for pc in pcs {
            vec[*pc as usize] = 1.0;
        }
        pcset_vectors.push(vec);
    }
    let mut weber_index = HashMap::new();
    for (idx, key) in WEBER_DIAGONAL.iter().enumerate() {
        weber_index.insert(*key, idx);
    }
    Ok(DecodeRuntime {
        assets,
        pcset_vectors,
        weber_index,
    })
}

fn round6f64(v: f64) -> f64 {
    (v * 1_000_000.0).round() / 1_000_000.0
}

fn verify_stage_d_parity(
    fixture_id: &str,
    expected: &StageDArtifact,
    actual: &StageDArtifact,
    float_atol: f32,
    diff_artifact_dir: Option<&Path>,
) -> AugmentedNetOnnxResult<()> {
    let expected_json = serde_json::to_value(expected).map_err(|e| {
        AugmentedNetOnnxError::ParityMismatch(format!(
            "failed to serialize expected stage_d for fixture {fixture_id}: {e}"
        ))
    })?;
    let actual_json = serde_json::to_value(actual).map_err(|e| {
        AugmentedNetOnnxError::ParityMismatch(format!(
            "failed to serialize actual stage_d for fixture {fixture_id}: {e}"
        ))
    })?;
    if let Some((path, exp, act)) =
        first_mismatch_value("", &expected_json, &actual_json, float_atol)
    {
        let diff = PostprocessDiffArtifact {
            fixture_id: fixture_id.to_string(),
            field_path: path.clone(),
            expected_summary: summarize_value(&exp),
            actual_summary: summarize_value(&act),
        };
        if let Some(dir) = diff_artifact_dir {
            emit_postprocess_diff_artifact(dir, &diff)?;
        }
        return Err(AugmentedNetOnnxError::ParityMismatch(format!(
            "fixture {fixture_id} stage_d mismatch at {path}"
        )));
    }
    Ok(())
}

fn emit_postprocess_diff_artifact(
    dir: &Path,
    diff: &PostprocessDiffArtifact,
) -> AugmentedNetOnnxResult<()> {
    fs::create_dir_all(dir).map_err(|source| AugmentedNetOnnxError::Io {
        path: dir.to_path_buf(),
        source,
    })?;
    let path = dir.join(format!("{}_stage_d_diff.json", diff.fixture_id));
    let payload = serde_json::to_string_pretty(diff).map_err(|e| {
        AugmentedNetOnnxError::ParityMismatch(format!(
            "failed to serialize Stage D diff artifact {}: {e}",
            path.display()
        ))
    })?;
    fs::write(&path, payload).map_err(|source| AugmentedNetOnnxError::Io { path, source })?;
    Ok(())
}

fn first_mismatch_value(
    path: &str,
    expected: &serde_json::Value,
    actual: &serde_json::Value,
    float_atol: f32,
) -> Option<(String, serde_json::Value, serde_json::Value)> {
    use serde_json::Value;
    match (expected, actual) {
        (Value::Number(en), Value::Number(an)) => {
            let ev = en.as_f64()?;
            let av = an.as_f64()?;
            if (ev - av).abs() <= float_atol as f64 {
                None
            } else {
                Some((path.to_string(), expected.clone(), actual.clone()))
            }
        }
        (Value::Array(ea), Value::Array(aa)) => {
            if ea.len() != aa.len() {
                return Some((
                    format!("{path}.<len>"),
                    serde_json::json!(ea.len()),
                    serde_json::json!(aa.len()),
                ));
            }
            for (idx, (ev, av)) in ea.iter().zip(aa).enumerate() {
                let next = format!("{path}[{idx}]");
                if let Some(mismatch) = first_mismatch_value(&next, ev, av, float_atol) {
                    return Some(mismatch);
                }
            }
            None
        }
        (Value::Object(eo), Value::Object(ao)) => {
            let e_keys: Vec<_> = eo.keys().cloned().collect();
            let a_keys: Vec<_> = ao.keys().cloned().collect();
            if e_keys != a_keys {
                return Some((
                    format!("{path}.<keys>"),
                    serde_json::json!(e_keys),
                    serde_json::json!(a_keys),
                ));
            }
            for key in eo.keys() {
                let next = if path.is_empty() {
                    key.to_string()
                } else {
                    format!("{path}.{key}")
                };
                if let Some(mismatch) = first_mismatch_value(&next, &eo[key], &ao[key], float_atol)
                {
                    return Some(mismatch);
                }
            }
            None
        }
        _ => {
            if expected == actual {
                None
            } else {
                Some((path.to_string(), expected.clone(), actual.clone()))
            }
        }
    }
}

fn summarize_value(value: &serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match value {
        Value::Array(arr) => {
            if arr.is_empty() {
                serde_json::json!({ "type": "array", "len": 0 })
            } else {
                serde_json::json!({
                    "type": "array",
                    "len": arr.len(),
                    "first": summarize_value(&arr[0]),
                })
            }
        }
        Value::Object(map) => {
            let mut keys: Vec<_> = map.keys().cloned().collect();
            keys.sort();
            serde_json::json!({
                "type": "object",
                "keys": keys.into_iter().take(8).collect::<Vec<_>>(),
                "key_count": map.len(),
            })
        }
        _ => value.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_assets_load_expected_cardinalities() {
        let runtime = decode_runtime().expect("decode assets");
        assert_eq!(runtime.assets.spellings.len(), 35);
        assert_eq!(runtime.assets.keys.len(), 38);
        assert_eq!(runtime.assets.roman_numerals.len(), 31);
        assert_eq!(runtime.assets.pcsets.len(), 121);
    }

    #[test]
    fn confidence_uses_decision_index_not_redecoding() {
        let logits = vec![10.0, 9.0, 8.0];
        let (top1, margin) = confidence_for_decision(&logits, 1).expect("confidence");
        assert!(
            top1 < 0.5,
            "chosen class should not become top1 probability"
        );
        assert!(
            margin < 0.0,
            "margin should reflect chosen class vs true top competitor"
        );
    }

    #[test]
    fn confidence_rejects_empty_logits() {
        let err = confidence_for_decision(&[], 0).expect_err("empty logits must fail");
        assert!(err
            .to_string()
            .contains("confidence cannot be computed from empty logits"));
    }

    #[test]
    fn confidence_rejects_out_of_bounds_choice() {
        let err = confidence_for_decision(&[1.0, 2.0], 2).expect_err("oob choice must fail");
        assert!(err.to_string().contains("chosen index out of bounds"));
    }

    #[test]
    fn confidence_single_class_has_full_probability_and_margin() {
        let (top1, margin) = confidence_for_decision(&[3.0], 0).expect("single class confidence");
        assert_eq!(top1, 1.0);
        assert_eq!(margin, 1.0);
    }

    #[test]
    fn chord_and_rn_display_formatting_matches_augnet() {
        assert_eq!(format_chord_label("Cmaj"), "C");
        assert_eq!(format_chord_label("B-maj7"), "Bbmaj7");
        assert_eq!(format_roman_numeral("I/I"), "I");
        assert_eq!(format_roman_numeral("V7/ii"), "V7/ii");
    }
}
