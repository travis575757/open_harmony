use cp_core::{ticks_per_measure, AnalysisRequest, NoteEvent};
use cp_engine::analyze;
use cp_engine::augnet_onnx::{AugmentedNetInputTensors, StageCArtifact};
use cp_engine::augnet_postprocess::decode_stage_d_from_stage_c;
use cp_engine::augnet_preprocess::{
    preprocess_musicxml_to_chunks, AugmentedNetPreprocessConfig, AugmentedNetPreprocessMode,
};
use cp_io::to_response_json;
use cp_music21_compat::{
    augnet_reindex_frames, encode_stage_b_inputs, simple_interval_name, AugnetScoreFrame,
    PitchSpelling,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

mod musicxml_import;

#[derive(Debug, Serialize, Deserialize)]
struct AugnetWebChunk {
    chunk_index: usize,
    global_start_step: usize,
    global_end_step_exclusive: usize,
    tensors: AugmentedNetInputTensors,
}

#[derive(Debug, Serialize, Deserialize)]
struct AugnetWebPrepArtifact {
    schema_version: u32,
    fixed_offset: f64,
    max_steps: usize,
    step_ticks: u32,
    chunks: Vec<AugnetWebChunk>,
}

pub fn analyze_json(input: &str) -> String {
    match parse_analysis_request(input)
        .map_err(|e| e.to_string())
        .and_then(|req| analyze(&req).map_err(|e| e.to_string()))
        .and_then(|resp| to_response_json(&resp).map_err(|e| e.to_string()))
    {
        Ok(s) => s,
        Err(e) => format!("{{\"error\":\"{}\"}}", e.replace('"', "\\\"")),
    }
}

pub fn prepare_augnet_chunks_json(input: &str) -> String {
    let out = (|| -> Result<String, String> {
        let (req, note_spellings, source_musicxml) = parse_augnet_prepare_payload(input)?;
        let fixed_offset = req.config.augnet_backend.fixed_offset;
        let max_steps = req.config.augnet_backend.max_steps.max(1);
        if !fixed_offset.is_finite() || fixed_offset <= 0.0 {
            return Err(format!(
                "invalid augnet_backend.fixed_offset (must be finite and > 0): {fixed_offset}"
            ));
        }

        let step_ticks = score_step_ticks(&req, fixed_offset);
        let chunks = if let Some(xml) = source_musicxml {
            let preprocess = preprocess_musicxml_to_chunks(
                &xml,
                &AugmentedNetPreprocessConfig {
                    fixed_offset,
                    max_steps,
                    mode: AugmentedNetPreprocessMode::Parity,
                },
            )
            .map_err(|e| e.to_string())?;
            preprocess
                .chunks
                .into_iter()
                .map(|chunk| AugnetWebChunk {
                    chunk_index: chunk.chunk_index,
                    global_start_step: chunk.global_start_step,
                    global_end_step_exclusive: chunk.global_end_step_exclusive,
                    tensors: chunk.tensors,
                })
                .collect()
        } else {
            let frames = build_augnet_frames_from_score(&req, fixed_offset, &note_spellings);
            encode_chunks(&frames, fixed_offset, max_steps)
        };
        let artifact = AugnetWebPrepArtifact {
            schema_version: 1,
            fixed_offset,
            max_steps,
            step_ticks,
            chunks,
        };
        serde_json::to_string(&artifact).map_err(|e| e.to_string())
    })();

    match out {
        Ok(s) => s,
        Err(e) => format!("{{\"error\":\"{}\"}}", e.replace('"', "\\\"")),
    }
}

fn parse_analysis_request(input: &str) -> Result<AnalysisRequest, serde_json::Error> {
    let raw: serde_json::Value = serde_json::from_str(input)?;
    serde_json::from_value(raw)
}

fn parse_augnet_prepare_payload(
    input: &str,
) -> Result<(AnalysisRequest, BTreeMap<String, String>, Option<String>), String> {
    let raw: serde_json::Value = serde_json::from_str(input).map_err(|e| e.to_string())?;
    let req: AnalysisRequest = serde_json::from_value(raw.clone()).map_err(|e| e.to_string())?;
    let mut note_spellings = BTreeMap::new();
    if let Some(obj) = raw.get("augnet_note_spellings").and_then(|v| v.as_object()) {
        for (note_id, value) in obj {
            if let Some(name) = value.as_str().map(str::trim).filter(|s| !s.is_empty()) {
                note_spellings.insert(note_id.clone(), name.to_string());
            }
        }
    }
    let source_musicxml = raw
        .get("augnet_source_musicxml")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned);
    Ok((req, note_spellings, source_musicxml))
}

pub fn decode_augnet_stage_d_json(input: &str) -> String {
    let out = (|| -> Result<String, String> {
        let stage_c: StageCArtifact = serde_json::from_str(input).map_err(|e| e.to_string())?;
        let stage_d = decode_stage_d_from_stage_c(&stage_c).map_err(|e| e.to_string())?;
        serde_json::to_string(&stage_d).map_err(|e| e.to_string())
    })();

    match out {
        Ok(s) => s,
        Err(e) => format!("{{\"error\":\"{}\"}}", e.replace('"', "\\\"")),
    }
}

pub fn import_musicxml_json(input: &str) -> String {
    musicxml_import::import_musicxml_json(input)
}

fn encode_chunks(
    frames: &[AugnetScoreFrame],
    fixed_offset: f64,
    max_steps: usize,
) -> Vec<AugnetWebChunk> {
    if frames.is_empty() {
        let stage_b = encode_stage_b_inputs(&[], fixed_offset, max_steps);
        return vec![AugnetWebChunk {
            chunk_index: 0,
            global_start_step: 0,
            global_end_step_exclusive: 0,
            tensors: AugmentedNetInputTensors {
                schema_version: stage_b.schema_version,
                fixed_offset: stage_b.fixed_offset,
                max_steps: stage_b.max_steps,
                active_steps: stage_b.active_steps,
                x_bass19: stage_b.x_bass19,
                x_chromagram19: stage_b.x_chromagram19,
                x_measure_note_onset14: stage_b.x_measure_note_onset14,
            },
        }];
    }

    let mut chunks = Vec::new();
    for (chunk_index, start) in (0..frames.len()).step_by(max_steps).enumerate() {
        let end = (start + max_steps).min(frames.len());
        let stage_b = encode_stage_b_inputs(&frames[start..end], fixed_offset, max_steps);
        chunks.push(AugnetWebChunk {
            chunk_index,
            global_start_step: start,
            global_end_step_exclusive: end,
            tensors: AugmentedNetInputTensors {
                schema_version: stage_b.schema_version,
                fixed_offset: stage_b.fixed_offset,
                max_steps: stage_b.max_steps,
                active_steps: stage_b.active_steps,
                x_bass19: stage_b.x_bass19,
                x_chromagram19: stage_b.x_chromagram19,
                x_measure_note_onset14: stage_b.x_measure_note_onset14,
            },
        });
    }
    chunks
}

fn score_step_ticks(req: &AnalysisRequest, fixed_offset: f64) -> u32 {
    let tpq = if req.score.meta.ticks_per_quarter == 0 {
        480
    } else {
        req.score.meta.ticks_per_quarter
    };
    ((tpq as f64) * fixed_offset).round().max(1.0) as u32
}

fn build_augnet_frames_from_score(
    req: &AnalysisRequest,
    fixed_offset: f64,
    note_spellings: &BTreeMap<String, String>,
) -> Vec<AugnetScoreFrame> {
    let end_tick = req
        .score
        .voices
        .iter()
        .flat_map(|v| v.notes.iter())
        .map(|n| n.start_tick.saturating_add(n.duration_ticks))
        .max();
    let tpq = if req.score.meta.ticks_per_quarter == 0 {
        480
    } else {
        req.score.meta.ticks_per_quarter
    };
    let tpm = ticks_per_measure(&req.score.meta.time_signature, tpq).max(1);

    let mut boundaries: Vec<u32> = req
        .score
        .voices
        .iter()
        .flat_map(|v| v.notes.iter())
        .flat_map(|n| [n.start_tick, n.start_tick.saturating_add(n.duration_ticks)])
        .collect();
    if let Some(last_tick) = end_tick {
        let mut m = 0u32;
        while m <= last_tick {
            boundaries.push(m);
            m = m.saturating_add(tpm);
            if m == u32::MAX {
                break;
            }
        }
        boundaries.push(last_tick);
    }
    boundaries.sort_unstable();
    boundaries.dedup();
    if boundaries.len() < 2 {
        return Vec::new();
    }

    let mut initial = Vec::with_capacity(boundaries.len().saturating_sub(1));
    for window in boundaries.windows(2) {
        let tick = window[0];
        let next = window[1];
        if next <= tick {
            continue;
        }
        let mut active: Vec<&NoteEvent> = req
            .score
            .voices
            .iter()
            .flat_map(|voice| voice.notes.iter())
            .filter(|note| {
                note.start_tick <= tick
                    && tick < note.start_tick.saturating_add(note.duration_ticks)
            })
            .collect();
        active.sort_by(|a, b| {
            (a.midi, a.voice_index, a.note_id.as_str()).cmp(&(
                b.midi,
                b.voice_index,
                b.note_id.as_str(),
            ))
        });
        let measure = (tick / tpm) as i32 + 1;
        let offset = (tick as f64) / (tpq as f64);
        let duration = (next.saturating_sub(tick) as f64) / (tpq as f64);
        if active.is_empty() {
            initial.push(AugnetScoreFrame {
                s_offset: offset,
                s_duration: duration,
                s_measure: measure,
                s_notes: None,
                s_intervals: None,
                s_is_onset: None,
            });
            continue;
        }

        let notes: Vec<String> = active
            .iter()
            .map(|note| {
                note_spellings
                    .get(&note.note_id)
                    .cloned()
                    .unwrap_or_else(|| midi_to_m21_name(note.midi))
            })
            .collect();
        let intervals = intervals_from_m21_note_names(&notes);
        let onsets: Vec<bool> = active
            .iter()
            .map(|note| note.start_tick == tick && !note.tie_end)
            .collect();
        initial.push(AugnetScoreFrame {
            s_offset: offset,
            s_duration: duration,
            s_measure: measure,
            s_notes: Some(notes),
            s_intervals: Some(intervals),
            s_is_onset: Some(onsets),
        });
    }

    augnet_reindex_frames(&initial, fixed_offset)
}

fn midi_to_m21_name(midi: i16) -> String {
    const M21_PC_NAMES: [&str; 12] = [
        "C", "C#", "D", "E-", "E", "F", "F#", "G", "A-", "A", "B-", "B",
    ];
    let pc = midi.rem_euclid(12) as usize;
    let octave = midi.div_euclid(12) - 1;
    format!("{}{}", M21_PC_NAMES[pc], octave)
}

fn intervals_from_m21_note_names(notes: &[String]) -> Vec<String> {
    if notes.len() <= 1 {
        return Vec::new();
    }
    let Ok((bass, _)) = PitchSpelling::parse_m21_pitch_name(&notes[0]) else {
        return vec!["P1".to_string(); notes.len().saturating_sub(1)];
    };
    notes
        .iter()
        .skip(1)
        .map(|name| {
            PitchSpelling::parse_m21_pitch_name(name)
                .map(|(pitch, _)| simple_interval_name(&bass, &pitch))
                .unwrap_or_else(|_| "P1".to_string())
        })
        .collect()
}

#[cfg(target_arch = "wasm32")]
mod wasm_export {
    use super::{
        analyze_json, decode_augnet_stage_d_json, import_musicxml_json, prepare_augnet_chunks_json,
    };
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    pub fn analyze_json_wasm(input: &str) -> String {
        analyze_json(input)
    }

    #[wasm_bindgen]
    pub fn prepare_augnet_chunks_json_wasm(input: &str) -> String {
        prepare_augnet_chunks_json(input)
    }

    #[wasm_bindgen]
    pub fn decode_augnet_stage_d_json_wasm(input: &str) -> String {
        decode_augnet_stage_d_json(input)
    }

    #[wasm_bindgen]
    pub fn import_musicxml_json_wasm(input: &str) -> String {
        import_musicxml_json(input)
    }
}

#[cfg(test)]
mod tests {
    use super::{analyze_json, prepare_augnet_chunks_json, AugnetWebPrepArtifact};
    use cp_core::{
        AnalysisBackend, AnalysisConfig, AnalysisRequest, AugmentedNetBackendConfig,
        HarmonicRhythm, KeySignature, NormalizedScore, NoteEvent, PresetId, ScaleMode, ScoreMeta,
        TimeSignature, Voice,
    };
    use cp_engine::augnet_preprocess::{
        preprocess_musicxml_to_chunks, AugmentedNetPreprocessConfig,
    };
    use cp_music21_compat::{encode_stage_b_inputs, AugnetScoreFrame};
    use serde_json::json;
    use std::collections::BTreeMap;

    #[test]
    fn returns_json_error_on_invalid_input() {
        let out = analyze_json("not json");
        assert!(out.contains("error"));
    }

    fn test_request() -> AnalysisRequest {
        AnalysisRequest {
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
                voices: vec![
                    Voice {
                        voice_index: 0,
                        name: "Upper".to_string(),
                        notes: vec![NoteEvent {
                            note_id: "u0".to_string(),
                            voice_index: 0,
                            midi: 64, // E4
                            start_tick: 0,
                            duration_ticks: 240,
                            tie_start: false,
                            tie_end: false,
                        }],
                    },
                    Voice {
                        voice_index: 1,
                        name: "Bass".to_string(),
                        notes: vec![NoteEvent {
                            note_id: "b0".to_string(),
                            voice_index: 1,
                            midi: 60, // C4
                            start_tick: 0,
                            duration_ticks: 480,
                            tie_start: false,
                            tie_end: false,
                        }],
                    },
                    Voice {
                        voice_index: 2,
                        name: "Top".to_string(),
                        notes: vec![NoteEvent {
                            note_id: "t0".to_string(),
                            voice_index: 2,
                            midi: 67, // G4
                            start_tick: 120,
                            duration_ticks: 120,
                            tie_start: false,
                            tie_end: false,
                        }],
                    },
                ],
            },
            config: AnalysisConfig {
                preset_id: PresetId::Species1,
                enabled_rule_ids: vec![],
                disabled_rule_ids: vec![],
                severity_overrides: BTreeMap::new(),
                rule_params: BTreeMap::new(),
                harmonic_rhythm: HarmonicRhythm::NoteOnset,
                analysis_backend: AnalysisBackend::RuleBased,
                augnet_backend: AugmentedNetBackendConfig {
                    fixed_offset: 0.25,
                    max_steps: 4,
                    model_path: None,
                    manifest_path: None,
                },
            },
        }
    }

    #[test]
    fn prepare_augnet_chunks_json_matches_expected_frames_end_to_end() {
        let req = test_request();
        let req_json = serde_json::to_string(&req).expect("serialize request");
        let raw = prepare_augnet_chunks_json(&req_json);
        let artifact: AugnetWebPrepArtifact =
            serde_json::from_str(&raw).expect("parse prep artifact json");
        assert_eq!(artifact.schema_version, 1);
        assert_eq!(artifact.fixed_offset, 0.25);
        assert_eq!(artifact.max_steps, 4);
        assert_eq!(artifact.step_ticks, 120);
        assert_eq!(artifact.chunks.len(), 1);

        let expected_frames = vec![
            AugnetScoreFrame {
                s_offset: 0.0,
                s_duration: 0.25,
                s_measure: 1,
                s_notes: Some(vec!["C4".to_string(), "E4".to_string()]),
                s_intervals: Some(vec!["M3".to_string()]),
                s_is_onset: Some(vec![true, true]),
            },
            AugnetScoreFrame {
                s_offset: 0.25,
                s_duration: 0.25,
                s_measure: 1,
                s_notes: Some(vec!["C4".to_string(), "E4".to_string(), "G4".to_string()]),
                s_intervals: Some(vec!["M3".to_string(), "P5".to_string()]),
                s_is_onset: Some(vec![false, false, true]),
            },
            AugnetScoreFrame {
                s_offset: 0.5,
                s_duration: 0.5,
                s_measure: 1,
                s_notes: Some(vec!["C4".to_string()]),
                s_intervals: Some(vec![]),
                s_is_onset: Some(vec![false]),
            },
            AugnetScoreFrame {
                s_offset: 0.75,
                s_duration: 0.5,
                s_measure: 1,
                s_notes: Some(vec!["C4".to_string()]),
                s_intervals: Some(vec![]),
                s_is_onset: Some(vec![false]),
            },
        ];
        let expected_stage_b = encode_stage_b_inputs(&expected_frames, 0.25, 4);
        let tensors = &artifact.chunks[0].tensors;
        assert_eq!(tensors.schema_version, expected_stage_b.schema_version);
        assert_eq!(tensors.fixed_offset, expected_stage_b.fixed_offset);
        assert_eq!(tensors.max_steps, expected_stage_b.max_steps);
        assert_eq!(tensors.active_steps, expected_stage_b.active_steps);
        assert_eq!(tensors.x_bass19, expected_stage_b.x_bass19);
        assert_eq!(tensors.x_chromagram19, expected_stage_b.x_chromagram19);
        assert_eq!(
            tensors.x_measure_note_onset14,
            expected_stage_b.x_measure_note_onset14
        );
    }

    #[test]
    fn prepare_augnet_chunks_json_matches_musicxml_preprocess_when_spellings_provided() {
        let xml = r#"
<score-partwise version="3.1">
  <part-list>
    <score-part id="P1"><part-name>Upper</part-name></score-part>
    <score-part id="P2"><part-name>Bass</part-name></score-part>
  </part-list>
  <part id="P1">
    <measure number="1">
      <attributes>
        <divisions>4</divisions>
        <key><fifths>-1</fifths><mode>major</mode></key>
        <time><beats>4</beats><beat-type>4</beat-type></time>
      </attributes>
      <note><pitch><step>E</step><alter>-1</alter><octave>4</octave></pitch><duration>8</duration><type>half</type></note>
      <note><pitch><step>F</step><octave>4</octave></pitch><duration>8</duration><type>half</type></note>
    </measure>
  </part>
  <part id="P2">
    <measure number="1">
      <attributes>
        <divisions>4</divisions>
        <key><fifths>-1</fifths><mode>major</mode></key>
        <time><beats>4</beats><beat-type>4</beat-type></time>
      </attributes>
      <note><pitch><step>C</step><octave>4</octave></pitch><duration>16</duration><type>whole</type></note>
    </measure>
  </part>
</score-partwise>
"#;
        let expected = preprocess_musicxml_to_chunks(
            xml,
            &AugmentedNetPreprocessConfig {
                fixed_offset: 0.25,
                max_steps: 16,
                mode: cp_engine::augnet_preprocess::AugmentedNetPreprocessMode::Parity,
            },
        )
        .expect("preprocess");
        assert!(!expected.chunks.is_empty());

        let req = AnalysisRequest {
            score: NormalizedScore {
                meta: ScoreMeta {
                    exercise_count: 1,
                    key_signature: KeySignature {
                        tonic_pc: 5,
                        mode: ScaleMode::Major,
                    },
                    time_signature: TimeSignature {
                        numerator: 4,
                        denominator: 4,
                    },
                    ticks_per_quarter: 480,
                },
                voices: vec![
                    Voice {
                        voice_index: 0,
                        name: "Upper".to_string(),
                        notes: vec![
                            NoteEvent {
                                note_id: "u0".to_string(),
                                voice_index: 0,
                                midi: 63,
                                start_tick: 0,
                                duration_ticks: 960,
                                tie_start: false,
                                tie_end: false,
                            },
                            NoteEvent {
                                note_id: "u1".to_string(),
                                voice_index: 0,
                                midi: 65,
                                start_tick: 960,
                                duration_ticks: 960,
                                tie_start: false,
                                tie_end: false,
                            },
                        ],
                    },
                    Voice {
                        voice_index: 1,
                        name: "Bass".to_string(),
                        notes: vec![NoteEvent {
                            note_id: "b0".to_string(),
                            voice_index: 1,
                            midi: 60,
                            start_tick: 0,
                            duration_ticks: 1920,
                            tie_start: false,
                            tie_end: false,
                        }],
                    },
                ],
            },
            config: AnalysisConfig {
                preset_id: PresetId::GeneralVoiceLeading,
                enabled_rule_ids: vec![],
                disabled_rule_ids: vec![],
                severity_overrides: BTreeMap::new(),
                rule_params: BTreeMap::new(),
                harmonic_rhythm: HarmonicRhythm::NoteOnset,
                analysis_backend: AnalysisBackend::AugnetOnnx,
                augnet_backend: AugmentedNetBackendConfig {
                    fixed_offset: 0.25,
                    max_steps: 16,
                    model_path: None,
                    manifest_path: None,
                },
            },
        };

        let mut payload = serde_json::to_value(&req).expect("serialize request");
        payload["augnet_note_spellings"] = json!({
            "u0": "E-4",
            "u1": "F4",
            "b0": "C4"
        });
        let raw = prepare_augnet_chunks_json(&payload.to_string());
        let artifact: AugnetWebPrepArtifact = serde_json::from_str(&raw).expect("parse artifact");

        assert_eq!(artifact.chunks.len(), expected.chunks.len());
        assert_eq!(
            artifact.chunks[0].tensors.x_bass19,
            expected.chunks[0].tensors.x_bass19
        );
        assert_eq!(
            artifact.chunks[0].tensors.x_chromagram19,
            expected.chunks[0].tensors.x_chromagram19
        );
        assert_eq!(
            artifact.chunks[0].tensors.x_measure_note_onset14,
            expected.chunks[0].tensors.x_measure_note_onset14
        );
    }

    #[test]
    fn prepare_augnet_chunks_json_uses_source_musicxml_when_provided() {
        let xml = r#"
<score-partwise version="3.1">
  <part-list>
    <score-part id="P1"><part-name>Upper</part-name></score-part>
    <score-part id="P2"><part-name>Bass</part-name></score-part>
  </part-list>
  <part id="P1">
    <measure number="1">
      <attributes>
        <divisions>4</divisions>
        <key><fifths>0</fifths><mode>major</mode></key>
        <time><beats>4</beats><beat-type>4</beat-type></time>
      </attributes>
      <note><pitch><step>E</step><octave>4</octave></pitch><duration>8</duration><type>half</type></note>
      <note><pitch><step>F</step><octave>4</octave></pitch><duration>8</duration><type>half</type></note>
    </measure>
  </part>
  <part id="P2">
    <measure number="1">
      <attributes>
        <divisions>4</divisions>
        <key><fifths>0</fifths><mode>major</mode></key>
        <time><beats>4</beats><beat-type>4</beat-type></time>
      </attributes>
      <note><pitch><step>C</step><octave>4</octave></pitch><duration>16</duration><type>whole</type></note>
    </measure>
  </part>
</score-partwise>
"#;
        let expected = preprocess_musicxml_to_chunks(
            xml,
            &AugmentedNetPreprocessConfig {
                fixed_offset: 0.25,
                max_steps: 4,
                mode: cp_engine::augnet_preprocess::AugmentedNetPreprocessMode::Parity,
            },
        )
        .expect("preprocess");

        let req = test_request();
        let mut payload = serde_json::to_value(&req).expect("serialize");
        payload["augnet_source_musicxml"] = json!(xml);
        let raw = prepare_augnet_chunks_json(&payload.to_string());
        let artifact: AugnetWebPrepArtifact = serde_json::from_str(&raw).expect("parse");

        assert_eq!(artifact.chunks.len(), expected.chunks.len());
        assert_eq!(
            artifact.chunks[0].tensors.x_bass19,
            expected.chunks[0].tensors.x_bass19
        );
        assert_eq!(
            artifact.chunks[0].tensors.x_chromagram19,
            expected.chunks[0].tensors.x_chromagram19
        );
        assert_eq!(
            artifact.chunks[0].tensors.x_measure_note_onset14,
            expected.chunks[0].tensors.x_measure_note_onset14
        );
    }
}
