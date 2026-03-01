use musicxml::datatypes::{Mode as MusicXmlMode, NoteTypeValue, StartStop, Step};
use musicxml::elements::{
    AudibleType, Attributes, KeyContents, Measure, MeasureElement, Note, NoteType, Part, PartElement, ScorePartwise,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

const DURATION_EIGHTH_MIN: f64 = 0.25;
const DURATION_EIGHTH_MAX: f64 = 16.0;
const DURATION_EIGHTH_STEP: f64 = 0.25;
const EIGHTH_EPS: f64 = 1e-6;

#[derive(Debug, Deserialize)]
struct MusicXmlImportPayload {
    xml_text: String,
    max_voices: Option<usize>,
    preset_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct ImportedScore {
    preset_id: String,
    key_tonic_pc: i32,
    mode: String,
    time_signature: ImportedTimeSignature,
    pickup_eighths: Option<f64>,
    voices: Vec<ImportedVoice>,
}

#[derive(Debug, Serialize)]
struct ImportedTimeSignature {
    numerator: u32,
    denominator: u32,
}

#[derive(Debug, Serialize)]
struct ImportedVoice {
    voice_index: usize,
    name: String,
    source_staff_num: u32,
    source_voice_num: u32,
    notes: Vec<ImportedNote>,
}

#[derive(Debug, Serialize)]
struct ImportedNote {
    note_id: String,
    midi: i16,
    is_rest: bool,
    start_eighths: f64,
    spelling_m21: Option<String>,
    spelling_midi: Option<i16>,
    duration_eighths: f64,
    tie_start: bool,
    tie_end: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct VoiceKey {
    part_num: usize,
    staff_num: u32,
    voice_num: u32,
}

#[derive(Debug, Clone)]
struct ParsedEventNote {
    start_quarters: f64,
    midi: i16,
    is_rest: bool,
    spelling_m21: Option<String>,
    duration_eighths: f64,
    tie_start: bool,
    tie_end: bool,
}

#[derive(Debug, Default)]
struct MetaState {
    beats: u32,
    beat_type: u32,
    mode: String,
    tonic_pc: i32,
    key_locked: bool,
    time_locked: bool,
}

impl MetaState {
    fn new() -> Self {
        Self {
            beats: 4,
            beat_type: 4,
            mode: "major".to_string(),
            tonic_pc: 0,
            key_locked: false,
            time_locked: false,
        }
    }
}

#[derive(Debug)]
struct RankedVoice {
    key: VoiceKey,
    lane_index: usize,
    notes: Vec<ParsedEventNote>,
    sounding_count: usize,
    duration_sum: f64,
    first_start: f64,
}

pub fn import_musicxml_json(input: &str) -> String {
    let out = (|| -> Result<String, String> {
        let payload: MusicXmlImportPayload = serde_json::from_str(input).map_err(|e| e.to_string())?;
        let parsed = import_musicxml_impl(&payload)?;
        serde_json::to_string(&parsed).map_err(|e| e.to_string())
    })();

    match out {
        Ok(json) => json,
        Err(err) => format!("{{\"error\":\"{}\"}}", err.replace('"', "\\\"")),
    }
}

fn import_musicxml_impl(payload: &MusicXmlImportPayload) -> Result<ImportedScore, String> {
    let preset_id = payload
        .preset_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("species1")
        .to_string();
    let max_voices = payload.max_voices.unwrap_or(4).clamp(1, 4);
    let xml = payload.xml_text.trim();
    if xml.is_empty() {
        return Err("MusicXML input is empty.".to_string());
    }

    let score: ScorePartwise =
        musicxml::read_score_data_partwise(xml.as_bytes().to_vec()).map_err(|e| format!("Invalid MusicXML: {e}"))?;
    if score.content.part.is_empty() {
        return Err("No <part> blocks found in MusicXML.".to_string());
    }

    let mut meta = MetaState::new();
    let mut by_voice: BTreeMap<VoiceKey, Vec<ParsedEventNote>> = BTreeMap::new();
    let mut first_measure_span_quarters = 0.0_f64;

    for (part_idx, part) in score.content.part.iter().enumerate() {
        process_part(
            part_idx,
            part,
            &mut meta,
            &mut by_voice,
            &mut first_measure_span_quarters,
        )?;
    }

    if by_voice.is_empty() {
        return Err("No notes were parsed from the MusicXML file.".to_string());
    }

    let mut ranked: Vec<RankedVoice> = Vec::new();
    for (key, notes) in by_voice {
        for (lane_index, lane_notes) in split_into_monophonic_lanes(&notes).into_iter().enumerate() {
            if lane_notes.is_empty() {
                continue;
            }
            let sounding_count = lane_notes.len();
            let duration_sum = lane_notes.iter().map(|n| n.duration_eighths).sum::<f64>();
            let first_start = lane_notes
                .iter()
                .map(|n| n.start_quarters)
                .fold(f64::INFINITY, |acc, v| acc.min(v));
            ranked.push(RankedVoice {
                key,
                lane_index,
                notes: lane_notes,
                sounding_count,
                duration_sum,
                first_start,
            });
        }
    }

    ranked.sort_by(|a, b| {
        b.sounding_count
            .cmp(&a.sounding_count)
            .then(
                b.duration_sum
                    .partial_cmp(&a.duration_sum)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then(
                a.first_start
                    .partial_cmp(&b.first_start)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then(a.key.part_num.cmp(&b.key.part_num))
            .then(a.key.staff_num.cmp(&b.key.staff_num))
            .then(a.key.voice_num.cmp(&b.key.voice_num))
            .then(a.lane_index.cmp(&b.lane_index))
    });

    let mut selected = ranked.into_iter().take(max_voices).collect::<Vec<_>>();
    selected.sort_by(|a, b| {
        a.key
            .part_num
            .cmp(&b.key.part_num)
            .then(a.key.staff_num.cmp(&b.key.staff_num))
            .then(a.key.voice_num.cmp(&b.key.voice_num))
            .then(a.lane_index.cmp(&b.lane_index))
    });

    let voices = selected
        .into_iter()
        .enumerate()
        .map(|(voice_index, rv)| ImportedVoice {
            voice_index,
            name: format!("Voice {}", voice_index + 1),
            source_staff_num: rv.key.staff_num,
            source_voice_num: rv.key.voice_num,
            notes: rv
                .notes
                .into_iter()
                .enumerate()
                .map(|(note_index, n)| ImportedNote {
                    note_id: format!("v{voice_index}_n{note_index}"),
                    midi: n.midi,
                    is_rest: n.is_rest,
                    start_eighths: n.start_quarters * 2.0,
                    spelling_m21: n.spelling_m21.clone(),
                    spelling_midi: if n.spelling_m21.is_some() {
                        Some(n.midi)
                    } else {
                        None
                    },
                    duration_eighths: n.duration_eighths,
                    tie_start: n.tie_start,
                    tie_end: n.tie_end,
                })
                .collect(),
        })
        .collect::<Vec<_>>();

    let full_measure_eighths = (meta.beats as f64 * 8.0) / (meta.beat_type.max(1) as f64);
    let pickup_candidate = normalize_duration_eighths(first_measure_span_quarters * 2.0, None);
    let pickup_eighths = if let Some(v) = pickup_candidate {
        if v > EIGHTH_EPS && v < full_measure_eighths - EIGHTH_EPS {
            Some(v)
        } else {
            None
        }
    } else {
        None
    };

    Ok(ImportedScore {
        preset_id,
        key_tonic_pc: meta.tonic_pc,
        mode: meta.mode,
        time_signature: ImportedTimeSignature {
            numerator: meta.beats,
            denominator: meta.beat_type,
        },
        pickup_eighths,
        voices,
    })
}

fn process_part(
    part_idx: usize,
    part: &Part,
    meta: &mut MetaState,
    by_voice: &mut BTreeMap<VoiceKey, Vec<ParsedEventNote>>,
    first_measure_span_quarters: &mut f64,
) -> Result<(), String> {
    let measures: Vec<&Measure> = part
        .content
        .iter()
        .filter_map(|el| {
            if let PartElement::Measure(m) = el {
                Some(m)
            } else {
                None
            }
        })
        .collect();
    if measures.is_empty() {
        return Err(format!("Part {} contains no <measure> elements.", part_idx + 1));
    }

    let mut default_divisions: u32 = 1;
    let mut part_cursor_quarters = 0.0_f64;
    let mut last_start_by_voice: HashMap<VoiceKey, f64> = HashMap::new();

    for (measure_idx, measure) in measures.iter().enumerate() {
        let mut measure_cursor_quarters = 0.0_f64;
        let mut measure_max_end_quarters = 0.0_f64;

        for event in &measure.content {
            match event {
                MeasureElement::Attributes(attrs) => {
                    if let Some(divisions) = attrs.content.divisions.as_ref() {
                        default_divisions = (*divisions.content).max(1);
                    }
                    apply_attributes_meta(attrs, meta);
                }
                MeasureElement::Backup(backup) => {
                    let duration_quarters = (*backup.content.duration.content as f64) / (default_divisions as f64);
                    measure_cursor_quarters = (measure_cursor_quarters - duration_quarters).max(0.0);
                }
                MeasureElement::Forward(forward) => {
                    let duration_quarters = (*forward.content.duration.content as f64) / (default_divisions as f64);
                    measure_cursor_quarters += duration_quarters;
                    measure_max_end_quarters = measure_max_end_quarters.max(measure_cursor_quarters);
                }
                MeasureElement::Note(note) => {
                    let parsed = parse_note(note, default_divisions)?;
                    if parsed.skip_timeline {
                        continue;
                    }

                    let key = VoiceKey {
                        part_num: part_idx,
                        staff_num: parsed.staff_num,
                        voice_num: parsed.voice_num,
                    };
                    let start_quarters = if parsed.is_chord_tone {
                        *last_start_by_voice
                            .get(&key)
                            .unwrap_or(&(part_cursor_quarters + measure_cursor_quarters))
                    } else {
                        part_cursor_quarters + measure_cursor_quarters
                    };
                    let end_quarters = start_quarters + parsed.duration_quarters;
                    measure_max_end_quarters = measure_max_end_quarters.max(end_quarters - part_cursor_quarters);

                    by_voice.entry(key).or_default().push(ParsedEventNote {
                        start_quarters,
                        midi: parsed.midi,
                        is_rest: parsed.is_rest,
                        spelling_m21: parsed.spelling_m21,
                        duration_eighths: parsed.duration_eighths,
                        tie_start: parsed.tie_start,
                        tie_end: parsed.tie_end,
                    });
                    last_start_by_voice.insert(key, start_quarters);

                    if !parsed.is_chord_tone {
                        measure_cursor_quarters += parsed.duration_quarters;
                        measure_max_end_quarters = measure_max_end_quarters.max(measure_cursor_quarters);
                    }
                }
                _ => {}
            }
        }

        let fallback_measure_quarters = ((meta.beats as f64) * 4.0 / (meta.beat_type.max(1) as f64)).max(0.25);
        let span = if measure_max_end_quarters > 0.0 {
            measure_max_end_quarters
        } else {
            fallback_measure_quarters
        };
        if measure_idx == 0 {
            *first_measure_span_quarters = first_measure_span_quarters.max(span);
        }
        part_cursor_quarters += span;
    }

    Ok(())
}

#[derive(Debug)]
struct ParsedNote {
    skip_timeline: bool,
    is_chord_tone: bool,
    is_rest: bool,
    midi: i16,
    spelling_m21: Option<String>,
    duration_quarters: f64,
    duration_eighths: f64,
    tie_start: bool,
    tie_end: bool,
    voice_num: u32,
    staff_num: u32,
}

fn parse_note(note: &Note, default_divisions: u32) -> Result<ParsedNote, String> {
    let voice_num = note
        .content
        .voice
        .as_ref()
        .and_then(|v| v.content.trim().parse::<u32>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(1);
    let staff_num = note
        .content
        .staff
        .as_ref()
        .map(|s| *s.content)
        .filter(|v| *v > 0)
        .unwrap_or(1);
    let dot_count = note.content.dot.len().min(2);
    let type_quarters = note
        .content
        .r#type
        .as_ref()
        .and_then(|t| duration_quarters_from_type(&t.content, dot_count));

    match &note.content.info {
        NoteType::Grace(_) => Ok(ParsedNote {
            skip_timeline: true,
            is_chord_tone: false,
            is_rest: true,
            midi: 60,
            spelling_m21: None,
            duration_quarters: 0.0,
            duration_eighths: 0.0,
            tie_start: false,
            tie_end: false,
            voice_num,
            staff_num,
        }),
        NoteType::Normal(info) => {
            let (is_rest, midi, spelling_m21) = extract_audible(&info.audible);
            let duration_quarters = (*info.duration.content as f64) / (default_divisions.max(1) as f64);
            let duration_quarters = if duration_quarters > 0.0 {
                duration_quarters
            } else {
                type_quarters.unwrap_or(1.0)
            };
            let duration_eighths = normalize_duration_eighths(duration_quarters * 2.0, Some(2.0)).unwrap_or(2.0);
            let tie_start = info
                .tie
                .iter()
                .any(|t| matches!(t.attributes.r#type, StartStop::Start));
            let tie_end = info
                .tie
                .iter()
                .any(|t| matches!(t.attributes.r#type, StartStop::Stop));
            Ok(ParsedNote {
                skip_timeline: false,
                is_chord_tone: info.chord.is_some(),
                is_rest,
                midi,
                spelling_m21,
                duration_quarters,
                duration_eighths,
                tie_start,
                tie_end,
                voice_num,
                staff_num,
            })
        }
        NoteType::Cue(info) => {
            let (is_rest, midi, spelling_m21) = extract_audible(&info.audible);
            let duration_quarters = (*info.duration.content as f64) / (default_divisions.max(1) as f64);
            let duration_quarters = if duration_quarters > 0.0 {
                duration_quarters
            } else {
                type_quarters.unwrap_or(1.0)
            };
            let duration_eighths = normalize_duration_eighths(duration_quarters * 2.0, Some(2.0)).unwrap_or(2.0);
            Ok(ParsedNote {
                skip_timeline: false,
                is_chord_tone: info.chord.is_some(),
                is_rest,
                midi,
                spelling_m21,
                duration_quarters,
                duration_eighths,
                tie_start: false,
                tie_end: false,
                voice_num,
                staff_num,
            })
        }
    }
}

fn split_into_monophonic_lanes(notes: &[ParsedEventNote]) -> Vec<Vec<ParsedEventNote>> {
    let mut sounding: Vec<ParsedEventNote> = notes.iter().filter(|n| !n.is_rest).cloned().collect();
    sounding.sort_by(|a, b| {
        a.start_quarters
            .partial_cmp(&b.start_quarters)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                b.duration_eighths
                    .partial_cmp(&a.duration_eighths)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then(a.midi.cmp(&b.midi))
    });

    if sounding.is_empty() {
        return Vec::new();
    }

    let mut lanes: Vec<Vec<ParsedEventNote>> = Vec::new();
    let mut lane_ends: Vec<f64> = Vec::new();
    for note in sounding {
        let note_end = note.start_quarters + note.duration_eighths / 2.0;
        let mut assigned = false;
        for lane_idx in 0..lane_ends.len() {
            if lane_ends[lane_idx] <= note.start_quarters + EIGHTH_EPS {
                lanes[lane_idx].push(note.clone());
                lane_ends[lane_idx] = note_end;
                assigned = true;
                break;
            }
        }
        if !assigned {
            lanes.push(vec![note]);
            lane_ends.push(note_end);
        }
    }
    for lane in &mut lanes {
        lane.sort_by(|a, b| {
            a.start_quarters
                .partial_cmp(&b.start_quarters)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.midi.cmp(&b.midi))
        });
    }
    lanes
}

fn extract_audible(audible: &AudibleType) -> (bool, i16, Option<String>) {
    match audible {
        AudibleType::Rest(_) => (true, 60, None),
        AudibleType::Unpitched(_) => (true, 60, None),
        AudibleType::Pitch(pitch) => {
            let step = &pitch.content.step.content;
            let alter = pitch.content.alter.as_ref().map(|a| *a.content).unwrap_or(0);
            let octave = *pitch.content.octave.content as i16;
            let midi = pitch_to_midi(step, alter, octave).unwrap_or(60);
            let spelling = m21_name_from_step_alter_octave(step, alter, octave);
            (false, midi, Some(spelling))
        }
    }
}

fn duration_quarters_from_type(note_type: &NoteTypeValue, dot_count: usize) -> Option<f64> {
    let base = match note_type {
        NoteTypeValue::Maxima => 32.0,
        NoteTypeValue::Long => 16.0,
        NoteTypeValue::Breve => 8.0,
        NoteTypeValue::Whole => 4.0,
        NoteTypeValue::Half => 2.0,
        NoteTypeValue::Quarter => 1.0,
        NoteTypeValue::Eighth => 0.5,
        NoteTypeValue::Sixteenth => 0.25,
        NoteTypeValue::ThirtySecond => 0.125,
        NoteTypeValue::SixtyFourth => 0.0625,
        NoteTypeValue::OneHundredTwentyEighth => 0.03125,
        NoteTypeValue::TwoHundredFiftySixth => 0.015625,
        NoteTypeValue::FiveHundredTwelfth => 0.0078125,
        NoteTypeValue::OneThousandTwentyFourth => 0.00390625,
    };
    let d = dot_count.min(2) as i32;
    let multiplier = 2.0 - 1.0 / 2_f64.powi(d);
    Some(base * multiplier)
}

fn pitch_to_midi(step: &Step, alter: i16, octave: i16) -> Option<i16> {
    let base = match step {
        Step::C => 0,
        Step::D => 2,
        Step::E => 4,
        Step::F => 5,
        Step::G => 7,
        Step::A => 9,
        Step::B => 11,
    };
    Some((octave + 1) * 12 + base + alter)
}

fn m21_name_from_step_alter_octave(step: &Step, alter: i16, octave: i16) -> String {
    let letter = match step {
        Step::A => "A",
        Step::B => "B",
        Step::C => "C",
        Step::D => "D",
        Step::E => "E",
        Step::F => "F",
        Step::G => "G",
    };
    let accidental = if alter > 0 {
        "#".repeat(alter as usize)
    } else if alter < 0 {
        "-".repeat((-alter) as usize)
    } else {
        String::new()
    };
    format!("{letter}{accidental}{octave}")
}

fn apply_attributes_meta(attrs: &Attributes, meta: &mut MetaState) {
    if !meta.time_locked {
        if let Some(first_time) = attrs.content.time.first() {
            if let Some(beat_pair) = first_time.content.beats.first() {
                if let (Ok(beats), Ok(beat_type)) = (
                    beat_pair.beats.content.trim().parse::<u32>(),
                    beat_pair.beat_type.content.trim().parse::<u32>(),
                ) {
                    if beats > 0 && beat_type > 0 {
                        meta.beats = beats;
                        meta.beat_type = beat_type;
                        meta.time_locked = true;
                    }
                }
            }
        }
    }

    if !meta.key_locked {
        if let Some(first_key) = attrs.content.key.first() {
            match &first_key.content {
                KeyContents::Explicit(explicit) => {
                    let mode = explicit
                        .mode
                        .as_ref()
                        .map(|m| normalize_mode(&m.content))
                        .unwrap_or_else(|| "major".to_string());
                    meta.mode = mode.clone();
                    meta.tonic_pc = tonic_pc_from_fifths(*explicit.fifths.content as i32, &mode);
                    meta.key_locked = true;
                }
                KeyContents::Relative(relative) => {
                    let step_pc = match relative.key_step.content {
                        Step::C => 0,
                        Step::D => 2,
                        Step::E => 4,
                        Step::F => 5,
                        Step::G => 7,
                        Step::A => 9,
                        Step::B => 11,
                    };
                    let alter = *relative.key_alter.content as i32;
                    meta.mode = "major".to_string();
                    meta.tonic_pc = (step_pc + alter).rem_euclid(12);
                    meta.key_locked = true;
                }
            }
        }
    }
}

fn normalize_mode(mode: &MusicXmlMode) -> String {
    let m = match mode {
        MusicXmlMode::Major => "major",
        MusicXmlMode::Minor => "minor",
        MusicXmlMode::Dorian => "dorian",
        MusicXmlMode::Phrygian => "phrygian",
        MusicXmlMode::Lydian => "lydian",
        MusicXmlMode::Mixolydian => "mixolydian",
        MusicXmlMode::Aeolian => "aeolian",
        MusicXmlMode::Ionian => "ionian",
        MusicXmlMode::Locrian => "major",
        MusicXmlMode::None => "major",
    };
    m.to_string()
}

fn tonic_pc_from_fifths(fifths: i32, mode: &str) -> i32 {
    let fifths_key = fifths.clamp(-7, 7);
    if mode == "minor" || mode == "aeolian" {
        match fifths_key {
            -7 => 8,
            -6 => 3,
            -5 => 10,
            -4 => 5,
            -3 => 0,
            -2 => 7,
            -1 => 2,
            0 => 9,
            1 => 4,
            2 => 11,
            3 => 6,
            4 => 1,
            5 => 8,
            6 => 3,
            7 => 10,
            _ => 0,
        }
    } else {
        match fifths_key {
            -7 => 11,
            -6 => 6,
            -5 => 1,
            -4 => 8,
            -3 => 3,
            -2 => 10,
            -1 => 5,
            0 => 0,
            1 => 7,
            2 => 2,
            3 => 9,
            4 => 4,
            5 => 11,
            6 => 6,
            7 => 1,
            _ => 0,
        }
    }
}

fn normalize_duration_eighths(value: f64, fallback: Option<f64>) -> Option<f64> {
    let parsed = if value.is_finite() { value } else { return fallback };
    if parsed <= 0.0 {
        return fallback;
    }
    let clamped = parsed.clamp(DURATION_EIGHTH_MIN, DURATION_EIGHTH_MAX);
    let stepped = (clamped / DURATION_EIGHTH_STEP).round() * DURATION_EIGHTH_STEP;
    Some(stepped.clamp(DURATION_EIGHTH_MIN, DURATION_EIGHTH_MAX))
}

#[cfg(test)]
mod tests {
    use super::{import_musicxml_impl, MusicXmlImportPayload};

    #[test]
    fn import_handles_backup_forward_alignment() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<score-partwise version="3.1">
  <part-list><score-part id="P1"><part-name>P1</part-name></score-part></part-list>
  <part id="P1">
    <measure number="1">
      <attributes><divisions>8</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes>
      <note><pitch><step>C</step><octave>4</octave></pitch><duration>8</duration><voice>1</voice><staff>1</staff></note>
      <backup><duration>8</duration></backup>
      <forward><duration>8</duration></forward>
      <note><pitch><step>E</step><octave>4</octave></pitch><duration>8</duration><voice>2</voice><staff>1</staff></note>
    </measure>
  </part>
</score-partwise>"#;
        let payload = MusicXmlImportPayload {
            xml_text: xml.to_string(),
            max_voices: Some(4),
            preset_id: Some("species1".to_string()),
        };
        let imported = import_musicxml_impl(&payload).expect("import xml");
        assert_eq!(imported.voices.len(), 2);
        assert!((imported.voices[0].notes[0].start_eighths - 0.0).abs() < 1e-6);
        assert!((imported.voices[1].notes[0].start_eighths - 2.0).abs() < 1e-6);
    }

    #[test]
    fn import_detects_pickup_measure() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<score-partwise version="3.1">
  <part-list><score-part id="P1"><part-name>P1</part-name></score-part></part-list>
  <part id="P1">
    <measure number="0" implicit="yes">
      <attributes><divisions>8</divisions><time><beats>3</beats><beat-type>4</beat-type></time></attributes>
      <note><pitch><step>C</step><octave>5</octave></pitch><duration>12</duration><voice>1</voice><staff>1</staff></note>
    </measure>
    <measure number="1">
      <note><pitch><step>D</step><octave>5</octave></pitch><duration>24</duration><voice>1</voice><staff>1</staff></note>
    </measure>
  </part>
</score-partwise>"#;
        let payload = MusicXmlImportPayload {
            xml_text: xml.to_string(),
            max_voices: Some(4),
            preset_id: Some("species1".to_string()),
        };
        let imported = import_musicxml_impl(&payload).expect("import xml");
        assert_eq!(imported.pickup_eighths, Some(3.0));
    }

    #[test]
    fn import_splits_polyphonic_source_voice_into_monophonic_lanes() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<score-partwise version="3.1">
  <part-list><score-part id="P1"><part-name>P1</part-name></score-part></part-list>
  <part id="P1">
    <measure number="1">
      <attributes><divisions>8</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes>
      <note><pitch><step>C</step><octave>4</octave></pitch><duration>16</duration><voice>1</voice><staff>1</staff></note>
      <backup><duration>16</duration></backup>
      <note><pitch><step>E</step><octave>4</octave></pitch><duration>8</duration><voice>1</voice><staff>1</staff></note>
      <forward><duration>8</duration></forward>
      <note><pitch><step>G</step><octave>4</octave></pitch><duration>8</duration><voice>1</voice><staff>1</staff></note>
    </measure>
  </part>
</score-partwise>"#;
        let payload = MusicXmlImportPayload {
            xml_text: xml.to_string(),
            max_voices: Some(4),
            preset_id: Some("general_voice_leading".to_string()),
        };
        let imported = import_musicxml_impl(&payload).expect("import xml");
        assert_eq!(imported.voices.len(), 2);

        for voice in &imported.voices {
            let mut end = 0.0;
            for note in &voice.notes {
                if note.is_rest {
                    continue;
                }
                assert!(
                    note.start_eighths + 1e-6 >= end,
                    "voice {} is not monophonic at start {} < previous end {}",
                    voice.voice_index,
                    note.start_eighths,
                    end
                );
                end = note.start_eighths + note.duration_eighths;
            }
        }
    }
}
