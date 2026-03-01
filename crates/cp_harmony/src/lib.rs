use cp_core::{
    interval_pc, ticks_per_measure, AnalysisRequest, HarmonicRhythm, HarmonicSlice, NctTag,
    NormalizedScore, NoteEvent, ScaleMode,
};
use indexmap::IndexMap;
use std::cmp::Ordering;

#[derive(Debug, Clone)]
struct ChordTemplate {
    quality: &'static str,
    intervals: &'static [u8],
}

const CHORD_TEMPLATES: [ChordTemplate; 5] = [
    ChordTemplate {
        quality: "major",
        intervals: &[0, 4, 7],
    },
    ChordTemplate {
        quality: "minor",
        intervals: &[0, 3, 7],
    },
    ChordTemplate {
        quality: "diminished",
        intervals: &[0, 3, 6],
    },
    ChordTemplate {
        quality: "augmented",
        intervals: &[0, 4, 8],
    },
    ChordTemplate {
        quality: "dominant7",
        intervals: &[0, 4, 7, 10],
    },
];

#[derive(Debug, Clone)]
struct ChordHypothesis {
    root_pc: u8,
    quality: String,
    inversion: String,
    score: f32,
    inferred_root: bool,
    missing_tones: Vec<String>,
    chord_form: String,
}

#[derive(Debug, Clone)]
pub struct ChordIdResult {
    pub pitch_classes: Vec<u8>,
    pub root_pc: Option<u8>,
    pub quality: Option<String>,
    pub inversion: Option<String>,
    pub confidence: f32,
    pub inferred_root: Option<bool>,
    pub missing_tones: Vec<String>,
    pub chord_form: Option<String>,
}

pub fn analyze_harmony(req: &AnalysisRequest) -> (Vec<HarmonicSlice>, Vec<NctTag>, Vec<String>) {
    let score = &req.score;
    let mut warnings = Vec::new();
    let (note_starts, end_tick) = collect_note_starts_and_end_tick(score);
    if note_starts.is_empty() {
        return (
            Vec::new(),
            Vec::new(),
            vec!["score contains no notes".to_string()],
        );
    }
    let starts = harmonic_slice_starts(
        score,
        &req.config.harmonic_rhythm,
        &note_starts,
        end_tick,
        &mut warnings,
    );

    let mut slices = Vec::new();
    for (ix, start) in starts.iter().enumerate() {
        let end = starts
            .get(ix + 1)
            .copied()
            .unwrap_or_else(|| end_tick.max(start.saturating_add(1)));
        let active = active_notes_at(score, *start);
        if active.is_empty() {
            continue;
        }
        let chord = identify_chord(score, &active);
        let roman = chord
            .root_pc
            .map(|r| roman_in_key(score, r, chord.quality.as_deref()));
        if chord.root_pc.is_none() {
            warnings.push(format!("ambiguous harmony at tick {}", start));
        }
        slices.push(HarmonicSlice {
            slice_id: slices.len() as u32,
            start_tick: *start,
            end_tick: end,
            pitch_classes: chord.pitch_classes,
            root_pc: chord.root_pc,
            quality: chord.quality,
            inversion: chord.inversion,
            roman_numeral: roman,
            confidence: chord.confidence,
            inferred_root: chord.inferred_root,
            missing_tones: chord.missing_tones,
            chord_form: chord.chord_form,
        });
    }

    let nct_tags = detect_nct(score);
    (slices, nct_tags, warnings)
}

fn score_tpq(score: &NormalizedScore) -> u32 {
    if score.meta.ticks_per_quarter == 0 {
        480
    } else {
        score.meta.ticks_per_quarter
    }
}

fn collect_note_starts_and_end_tick(score: &NormalizedScore) -> (Vec<u32>, u32) {
    let mut starts = Vec::new();
    let mut end_tick = 0u32;
    for voice in &score.voices {
        for note in &voice.notes {
            starts.push(note.start_tick);
            end_tick = end_tick.max(note.start_tick.saturating_add(note.duration_ticks));
        }
    }
    starts.sort_unstable();
    starts.dedup();
    (starts, end_tick.max(1))
}

fn measure_segment_starts(
    score: &NormalizedScore,
    end_tick: u32,
    chords_per_bar: &[u8],
) -> Vec<u32> {
    let tpq = score_tpq(score);
    let tpm = ticks_per_measure(&score.meta.time_signature, tpq).max(1);
    let measure_count = ((end_tick.saturating_sub(1)) / tpm).saturating_add(1);
    let mut starts = Vec::new();

    for measure in 0..measure_count {
        let raw = chords_per_bar
            .get(measure as usize)
            .copied()
            .or_else(|| chords_per_bar.last().copied())
            .unwrap_or(1);
        let cpb = raw.max(1);
        let base = measure.saturating_mul(tpm);
        for sub in 0..cpb {
            let offset = ((u64::from(sub) * u64::from(tpm)) / u64::from(cpb)) as u32;
            let tick = base.saturating_add(offset);
            if tick < end_tick {
                starts.push(tick);
            }
        }
    }

    starts.sort_unstable();
    starts.dedup();
    starts
}

fn bars_per_chord_starts(score: &NormalizedScore, end_tick: u32, bars_per_chord: u8) -> Vec<u32> {
    let tpq = score_tpq(score);
    let tpm = ticks_per_measure(&score.meta.time_signature, tpq).max(1);
    let bars = bars_per_chord.max(1);
    let step = (u64::from(tpm) * u64::from(bars)).max(1);
    let mut starts = Vec::new();
    let mut tick = 0u64;
    let end = u64::from(end_tick);
    while tick < end {
        starts.push(tick as u32);
        tick = tick.saturating_add(step);
    }
    starts
}

fn harmonic_slice_starts(
    score: &NormalizedScore,
    rhythm: &HarmonicRhythm,
    note_starts: &[u32],
    end_tick: u32,
    warnings: &mut Vec<String>,
) -> Vec<u32> {
    match rhythm {
        HarmonicRhythm::NoteOnset => note_starts.to_vec(),
        HarmonicRhythm::FixedPerBar { chords_per_bar } => {
            if *chords_per_bar == 0 {
                warnings.push(
                    "harmonic rhythm fixed_per_bar requires chords_per_bar >= 1; using 1"
                        .to_string(),
                );
            }
            measure_segment_starts(score, end_tick, &[(*chords_per_bar).max(1)])
        }
        HarmonicRhythm::FixedBarsPerChord { bars_per_chord } => {
            if *bars_per_chord == 0 {
                warnings.push(
                    "harmonic rhythm fixed_bars_per_chord requires bars_per_chord >= 1; using 1"
                        .to_string(),
                );
            }
            bars_per_chord_starts(score, end_tick, (*bars_per_chord).max(1))
        }
        HarmonicRhythm::PerMeasure { chords_per_bar } => {
            if chords_per_bar.is_empty() {
                warnings.push(
                    "harmonic rhythm per_measure requires at least one value; using [1]"
                        .to_string(),
                );
                return measure_segment_starts(score, end_tick, &[1]);
            }
            if chords_per_bar.iter().any(|v| *v == 0) {
                warnings.push(
                    "harmonic rhythm per_measure values must be >= 1; zero entries were clamped to 1"
                        .to_string(),
                );
            }
            let clamped: Vec<u8> = chords_per_bar.iter().map(|v| (*v).max(1)).collect();
            measure_segment_starts(score, end_tick, &clamped)
        }
    }
}

pub fn identify_chord(score: &NormalizedScore, notes: &[&NoteEvent]) -> ChordIdResult {
    let (pitch_classes, pc_counts, bass_pc) = pitch_class_data(notes);
    if notes.is_empty() || pitch_classes.is_empty() {
        return ChordIdResult {
            pitch_classes,
            root_pc: None,
            quality: None,
            inversion: None,
            confidence: 0.0,
            inferred_root: None,
            missing_tones: Vec::new(),
            chord_form: None,
        };
    }

    if pitch_classes.len() == 1 {
        let root_pc = pitch_classes[0];
        let quality = infer_single_tone_quality(score, root_pc).to_string();
        return ChordIdResult {
            pitch_classes,
            root_pc: Some(root_pc),
            quality: Some(quality),
            inversion: Some("root".to_string()),
            confidence: 0.62,
            inferred_root: Some(false),
            missing_tones: vec!["third".to_string(), "fifth".to_string()],
            chord_form: Some("single_tone_assumed_root".to_string()),
        };
    }

    let tonic_pc = score.meta.key_signature.tonic_pc % 12;
    let mut candidates = Vec::new();
    for root in 0..12u8 {
        for template in &CHORD_TEMPLATES {
            if let Some(h) = evaluate_template(
                root,
                template,
                &pitch_classes,
                &pc_counts,
                bass_pc,
                tonic_pc,
            ) {
                candidates.push(h);
            }
        }
        if let Some(h) = evaluate_open_fifth(root, &pitch_classes, &pc_counts, bass_pc, tonic_pc) {
            candidates.push(h);
        }
    }

    if candidates.is_empty() {
        return ChordIdResult {
            pitch_classes,
            root_pc: None,
            quality: None,
            inversion: None,
            confidence: 0.35,
            inferred_root: None,
            missing_tones: Vec::new(),
            chord_form: Some("ambiguous".to_string()),
        };
    }

    candidates.sort_by(compare_hypothesis_desc);
    let best = &candidates[0];
    let confidence = confidence_from_score(best.score);

    if best.score < 0.50 {
        return ChordIdResult {
            pitch_classes,
            root_pc: None,
            quality: None,
            inversion: None,
            confidence,
            inferred_root: None,
            missing_tones: Vec::new(),
            chord_form: Some("ambiguous".to_string()),
        };
    }

    ChordIdResult {
        pitch_classes,
        root_pc: Some(best.root_pc),
        quality: Some(best.quality.clone()),
        inversion: Some(best.inversion.clone()),
        confidence,
        inferred_root: Some(best.inferred_root),
        missing_tones: best.missing_tones.clone(),
        chord_form: Some(best.chord_form.clone()),
    }
}

fn compare_hypothesis_desc(a: &ChordHypothesis, b: &ChordHypothesis) -> Ordering {
    b.score
        .partial_cmp(&a.score)
        .unwrap_or(Ordering::Equal)
        .then_with(|| form_rank(&a.chord_form).cmp(&form_rank(&b.chord_form)))
        .then_with(|| a.inferred_root.cmp(&b.inferred_root))
        .then_with(|| a.missing_tones.len().cmp(&b.missing_tones.len()))
}

fn form_rank(form: &str) -> u8 {
    match form {
        "complete_seventh" => 0,
        "complete_triad" => 1,
        "incomplete_seventh_omit_fifth" => 2,
        "incomplete_triad_root_third" => 3,
        "incomplete_triad_implied_root" => 4,
        "incomplete_triad_open_fifth" => 5,
        _ => 10,
    }
}

fn confidence_from_score(score: f32) -> f32 {
    if score >= 1.00 {
        0.94
    } else if score >= 0.82 {
        0.86
    } else if score >= 0.67 {
        0.76
    } else if score >= 0.54 {
        0.64
    } else if score >= 0.45 {
        0.52
    } else {
        0.35
    }
}

fn pitch_class_data(notes: &[&NoteEvent]) -> (Vec<u8>, [u8; 12], u8) {
    let mut counts = [0u8; 12];
    let mut bass = i16::MAX;
    for n in notes {
        let pc = n.midi.rem_euclid(12) as usize;
        counts[pc] = counts[pc].saturating_add(1);
        bass = bass.min(n.midi);
    }
    let mut pcs = Vec::new();
    for (pc, cnt) in counts.iter().enumerate() {
        if *cnt > 0 {
            pcs.push(pc as u8);
        }
    }
    (pcs, counts, bass.rem_euclid(12) as u8)
}

fn is_pc_present(pitch_classes: &[u8], pc: u8) -> bool {
    pitch_classes.contains(&(pc % 12))
}

fn inversion_from_bass(root_pc: u8, bass_pc: u8) -> String {
    let i = interval_pc(bass_pc as i16, root_pc as i16);
    match i {
        0 => "root",
        3 | 4 => "first",
        6 | 7 | 8 => "second",
        10 | 11 => "third",
        _ => "other",
    }
    .to_string()
}

fn template_pcs(root_pc: u8, intervals: &[u8]) -> Vec<u8> {
    intervals.iter().map(|i| (root_pc + *i) % 12).collect()
}

fn apply_common_scoring(
    mut score: f32,
    quality: &str,
    inversion: &str,
    root_pc: u8,
    bass_pc: u8,
    pc_counts: &[u8; 12],
    tonic_pc: u8,
) -> f32 {
    let root_cnt = pc_counts[root_pc as usize] as usize;
    let bass_cnt = pc_counts[bass_pc as usize] as usize;

    if inversion == "root" {
        if root_cnt >= 2 {
            score += 0.03;
        } else {
            score -= 0.03;
        }
    } else if inversion == "first" {
        if quality == "diminished" {
            if bass_cnt >= 2 {
                score += 0.04;
            } else {
                score -= 0.08;
            }
        } else if bass_cnt > 1 {
            score -= 0.05;
        }
    } else if inversion == "second" {
        if bass_cnt >= 2 {
            score += 0.05;
        } else {
            score -= 0.06;
        }
    }

    let leading_tone_pc = ((tonic_pc + 11) % 12) as usize;
    if pc_counts[leading_tone_pc] > 1 {
        score -= 0.05;
    }
    score
}

fn evaluate_template(
    root_pc: u8,
    template: &ChordTemplate,
    pitch_classes: &[u8],
    pc_counts: &[u8; 12],
    bass_pc: u8,
    tonic_pc: u8,
) -> Option<ChordHypothesis> {
    let pcs = template_pcs(root_pc, template.intervals);
    let has_root = is_pc_present(pitch_classes, pcs[0]);
    let has_third = is_pc_present(pitch_classes, pcs[1]);
    let has_fifth = is_pc_present(pitch_classes, pcs[2]);
    let has_seventh = if template.intervals.len() > 3 {
        is_pc_present(pitch_classes, pcs[3])
    } else {
        false
    };

    let (chord_form, inferred_root, missing_tones) = if template.intervals.len() == 3 {
        if has_root && has_third && has_fifth {
            ("complete_triad".to_string(), false, Vec::new())
        } else if has_root && has_third && !has_fifth {
            (
                "incomplete_triad_root_third".to_string(),
                false,
                vec!["fifth".to_string()],
            )
        } else if !has_root
            && has_third
            && has_fifth
            && (template.quality == "major" || template.quality == "minor")
        {
            (
                "incomplete_triad_implied_root".to_string(),
                true,
                vec!["root".to_string()],
            )
        } else {
            return None;
        }
    } else if has_root && has_third && has_fifth && has_seventh {
        ("complete_seventh".to_string(), false, Vec::new())
    } else if has_root && has_third && !has_fifth && has_seventh {
        (
            "incomplete_seventh_omit_fifth".to_string(),
            false,
            vec!["fifth".to_string()],
        )
    } else {
        return None;
    };

    let mut score = 0.40;
    score += if chord_form.starts_with("complete") {
        0.28
    } else {
        0.06
    };
    score += if has_root { 0.15 } else { -0.12 };
    score += if has_third { 0.18 } else { -0.30 };
    score += if has_fifth { 0.07 } else { -0.05 };
    if template.intervals.len() > 3 {
        score += if has_seventh { 0.12 } else { -0.25 };
    }
    if chord_form == "incomplete_triad_implied_root" {
        score -= 0.08;
    }
    if chord_form == "incomplete_seventh_omit_fifth" {
        score += 0.05;
    }

    let template_set = template_pcs(root_pc, template.intervals);
    let extra_count = pitch_classes
        .iter()
        .filter(|pc| !template_set.contains(pc))
        .count();
    score -= 0.12 * extra_count as f32;

    let inversion = inversion_from_bass(root_pc, bass_pc);
    if inversion == "other" {
        score -= 0.10;
    } else {
        score += 0.04;
    }
    if inversion == "first" && !has_third {
        score -= 0.12;
    }
    if inversion == "second" && !has_fifth {
        score -= 0.12;
    }
    if inversion == "third" && !has_seventh {
        score -= 0.12;
    }

    score = apply_common_scoring(
        score,
        template.quality,
        &inversion,
        root_pc,
        bass_pc,
        pc_counts,
        tonic_pc,
    );

    Some(ChordHypothesis {
        root_pc,
        quality: template.quality.to_string(),
        inversion,
        score,
        inferred_root,
        missing_tones,
        chord_form,
    })
}

fn evaluate_open_fifth(
    root_pc: u8,
    pitch_classes: &[u8],
    pc_counts: &[u8; 12],
    bass_pc: u8,
    tonic_pc: u8,
) -> Option<ChordHypothesis> {
    let has_root = is_pc_present(pitch_classes, root_pc);
    let has_fifth = is_pc_present(pitch_classes, (root_pc + 7) % 12);
    let has_major_third = is_pc_present(pitch_classes, (root_pc + 4) % 12);
    let has_minor_third = is_pc_present(pitch_classes, (root_pc + 3) % 12);
    if !(has_root && has_fifth && !has_major_third && !has_minor_third) {
        return None;
    }

    let mut score = 0.40;
    score += 0.15; // root present
    score += 0.07; // fifth present
    score -= 0.30; // no third
    score -= 0.22; // explicit penalty for open fifth

    let template_set = [root_pc, (root_pc + 7) % 12];
    let extra_count = pitch_classes
        .iter()
        .filter(|pc| !template_set.contains(pc))
        .count();
    score -= 0.12 * extra_count as f32;

    let inversion = inversion_from_bass(root_pc, bass_pc);
    if inversion == "other" {
        score -= 0.10;
    } else {
        score += 0.04;
    }

    score = apply_common_scoring(
        score, "other", &inversion, root_pc, bass_pc, pc_counts, tonic_pc,
    );

    Some(ChordHypothesis {
        root_pc,
        quality: "other".to_string(),
        inversion,
        score,
        inferred_root: false,
        missing_tones: vec!["third".to_string()],
        chord_form: "incomplete_triad_open_fifth".to_string(),
    })
}

fn active_notes_at<'a>(score: &'a NormalizedScore, tick: u32) -> Vec<&'a NoteEvent> {
    let mut out = Vec::new();
    for v in &score.voices {
        for n in &v.notes {
            if n.start_tick <= tick && tick < n.start_tick + n.duration_ticks {
                out.push(n);
            }
        }
    }
    out
}

fn mode_intervals(mode: &ScaleMode) -> [u8; 7] {
    match mode {
        ScaleMode::Minor | ScaleMode::Aeolian => [0, 2, 3, 5, 7, 8, 10],
        ScaleMode::Dorian => [0, 2, 3, 5, 7, 9, 10],
        ScaleMode::Phrygian => [0, 1, 3, 5, 7, 8, 10],
        ScaleMode::Lydian => [0, 2, 4, 6, 7, 9, 11],
        ScaleMode::Mixolydian => [0, 2, 4, 5, 7, 9, 10],
        ScaleMode::Ionian | ScaleMode::Major => [0, 2, 4, 5, 7, 9, 11],
    }
}

fn infer_single_tone_quality(score: &NormalizedScore, root_pc: u8) -> &'static str {
    let tonic = score.meta.key_signature.tonic_pc % 12;
    let intervals = mode_intervals(&score.meta.key_signature.mode);
    let degree = intervals
        .iter()
        .position(|i| ((tonic + *i) % 12) == root_pc);
    let Some(i) = degree else {
        return "other";
    };
    let root_i = intervals[i] as i16;
    let third_i = intervals[(i + 2) % 7] as i16 + if i + 2 >= 7 { 12 } else { 0 };
    let fifth_i = intervals[(i + 4) % 7] as i16 + if i + 4 >= 7 { 12 } else { 0 };
    let i3 = third_i - root_i;
    let i5 = fifth_i - root_i;
    match (i3, i5) {
        (4, 7) => "major",
        (3, 7) => "minor",
        (3, 6) => "diminished",
        (4, 8) => "augmented",
        _ => "other",
    }
}

fn roman_in_key(score: &NormalizedScore, root_pc: u8, quality: Option<&str>) -> String {
    let tonic = score.meta.key_signature.tonic_pc % 12;
    let degree = interval_pc(root_pc as i16, tonic as i16);
    let (major_set, minor_set): (IndexMap<u8, &str>, IndexMap<u8, &str>) = (
        IndexMap::from([
            (0, "I"),
            (2, "II"),
            (4, "III"),
            (5, "IV"),
            (7, "V"),
            (9, "VI"),
            (11, "VII"),
        ]),
        IndexMap::from([
            (0, "i"),
            (2, "ii"),
            (3, "III"),
            (5, "iv"),
            (7, "v"),
            (8, "VI"),
            (10, "VII"),
        ]),
    );
    let base = match score.meta.key_signature.mode {
        ScaleMode::Minor | ScaleMode::Aeolian => minor_set.get(&degree).copied().unwrap_or("?"),
        _ => major_set.get(&degree).copied().unwrap_or("?"),
    };
    match quality.unwrap_or("other") {
        "diminished" => format!("{}o", base),
        "augmented" => format!("{}+", base),
        "dominant7" => format!("{}7", base),
        _ => base.to_string(),
    }
}

fn detect_nct(score: &NormalizedScore) -> Vec<NctTag> {
    let mut tags = Vec::new();
    for v in &score.voices {
        let notes = &v.notes;
        for i in 1..notes.len().saturating_sub(1) {
            let prev = notes[i - 1].midi;
            let cur = notes[i].midi;
            let next = notes[i + 1].midi;
            let up1 = cur - prev;
            let up2 = next - cur;
            if up1.abs() == 1 && up2.abs() == 1 && up1.signum() == up2.signum() {
                tags.push(NctTag {
                    note_id: notes[i].note_id.clone(),
                    tag_type: "passing".to_string(),
                    justification: "stepwise same-direction approach/departure".to_string(),
                });
            } else if up1.abs() == 1 && up2.abs() == 1 && up1.signum() == -up2.signum() {
                tags.push(NctTag {
                    note_id: notes[i].note_id.clone(),
                    tag_type: "neighbor".to_string(),
                    justification: "step out and return".to_string(),
                });
            }
            if notes[i - 1].tie_start && notes[i].midi == notes[i - 1].midi && (next - cur) == -1 {
                tags.push(NctTag {
                    note_id: notes[i].note_id.clone(),
                    tag_type: "suspension".to_string(),
                    justification: "tied tone resolving downward by step".to_string(),
                });
            }
        }
    }
    tags
}

#[cfg(test)]
mod tests {
    use super::*;
    use cp_core::{
        AnalysisBackend, AnalysisConfig, AnalysisRequest, AugmentedNetBackendConfig,
        HarmonicRhythm, KeySignature, PresetId, ScoreMeta, TimeSignature, Voice,
    };
    use std::collections::BTreeMap;

    fn req_with_chord(notes: &[(u8, i16)]) -> AnalysisRequest {
        let voice = |voice_index: u8, midi: i16| Voice {
            voice_index,
            name: format!("v{}", voice_index),
            notes: vec![NoteEvent {
                note_id: format!("n{}", voice_index),
                voice_index,
                midi,
                start_tick: 0,
                duration_ticks: 480,
                tie_start: false,
                tie_end: false,
            }],
        };
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
                voices: notes.iter().map(|(vi, m)| voice(*vi, *m)).collect(),
            },
            config: AnalysisConfig {
                preset_id: PresetId::Species1,
                enabled_rule_ids: vec![],
                disabled_rule_ids: vec![],
                severity_overrides: BTreeMap::new(),
                rule_params: BTreeMap::new(),
                harmonic_rhythm: HarmonicRhythm::NoteOnset,
                analysis_backend: AnalysisBackend::RuleBased,
                augnet_backend: AugmentedNetBackendConfig::default(),
            },
        }
    }

    fn req_with_two_measure_static_chord() -> AnalysisRequest {
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
                        name: "v0".to_string(),
                        notes: vec![NoteEvent {
                            note_id: "n0".to_string(),
                            voice_index: 0,
                            midi: 60,
                            start_tick: 0,
                            duration_ticks: 3840,
                            tie_start: false,
                            tie_end: false,
                        }],
                    },
                    Voice {
                        voice_index: 1,
                        name: "v1".to_string(),
                        notes: vec![NoteEvent {
                            note_id: "n1".to_string(),
                            voice_index: 1,
                            midi: 64,
                            start_tick: 0,
                            duration_ticks: 3840,
                            tie_start: false,
                            tie_end: false,
                        }],
                    },
                    Voice {
                        voice_index: 2,
                        name: "v2".to_string(),
                        notes: vec![NoteEvent {
                            note_id: "n2".to_string(),
                            voice_index: 2,
                            midi: 67,
                            start_tick: 0,
                            duration_ticks: 3840,
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
                augnet_backend: AugmentedNetBackendConfig::default(),
            },
        }
    }

    #[test]
    fn detects_major_triad_and_roman() {
        let req = req_with_chord(&[(0, 60), (1, 64), (2, 67)]);
        let (slices, _nct, _w) = analyze_harmony(&req);
        assert!(!slices.is_empty());
        assert_eq!(slices[0].quality.as_deref(), Some("major"));
        assert_eq!(slices[0].roman_numeral.as_deref(), Some("I"));
        assert_eq!(slices[0].inferred_root, Some(false));
    }

    #[test]
    fn detects_incomplete_root_third_triad() {
        let req = req_with_chord(&[(0, 60), (1, 64)]);
        let (slices, _nct, warnings) = analyze_harmony(&req);
        assert!(warnings.is_empty());
        assert_eq!(slices[0].quality.as_deref(), Some("major"));
        assert_eq!(slices[0].roman_numeral.as_deref(), Some("I"));
        assert_eq!(
            slices[0].chord_form.as_deref(),
            Some("incomplete_triad_root_third")
        );
        assert_eq!(slices[0].missing_tones, vec!["fifth".to_string()]);
    }

    #[test]
    fn supports_implied_root_hypothesis_generation() {
        let pcs = vec![4, 7];
        let mut counts = [0u8; 12];
        counts[4] = 1;
        counts[7] = 1;
        let major_template = CHORD_TEMPLATES
            .iter()
            .find(|t| t.quality == "major")
            .expect("major template");
        let hyp = evaluate_template(0, major_template, &pcs, &counts, 4, 0)
            .expect("implied-root candidate");
        assert_eq!(hyp.quality, "major");
        assert!(hyp.inferred_root);
        assert_eq!(hyp.chord_form, "incomplete_triad_implied_root");
        assert_eq!(hyp.missing_tones, vec!["root".to_string()]);
    }

    #[test]
    fn detects_incomplete_dominant_seventh_without_fifth() {
        let req = req_with_chord(&[(0, 67), (1, 71), (2, 77)]);
        let (slices, _nct, warnings) = analyze_harmony(&req);
        assert!(warnings.is_empty());
        assert_eq!(slices[0].quality.as_deref(), Some("dominant7"));
        assert_eq!(slices[0].roman_numeral.as_deref(), Some("V7"));
        assert_eq!(
            slices[0].chord_form.as_deref(),
            Some("incomplete_seventh_omit_fifth")
        );
    }

    #[test]
    fn open_fifth_is_reported_ambiguous() {
        let req = req_with_chord(&[(0, 60), (1, 67)]);
        let (slices, _nct, warnings) = analyze_harmony(&req);
        assert!(!warnings.is_empty());
        assert_eq!(slices[0].root_pc, None);
        assert_eq!(slices[0].quality, None);
        assert_eq!(slices[0].chord_form.as_deref(), Some("ambiguous"));
    }

    #[test]
    fn single_note_assumes_root_position_harmony() {
        let req = req_with_chord(&[(0, 60)]);
        let (slices, _nct, warnings) = analyze_harmony(&req);
        assert!(warnings.is_empty());
        assert_eq!(slices[0].root_pc, Some(0));
        assert_eq!(slices[0].inversion.as_deref(), Some("root"));
        assert_eq!(slices[0].quality.as_deref(), Some("major"));
        assert_eq!(slices[0].roman_numeral.as_deref(), Some("I"));
        assert_eq!(
            slices[0].chord_form.as_deref(),
            Some("single_tone_assumed_root")
        );
    }

    #[test]
    fn fixed_per_bar_harmonic_rhythm_slices_by_measure() {
        let mut req = req_with_two_measure_static_chord();
        req.config.harmonic_rhythm = HarmonicRhythm::FixedPerBar { chords_per_bar: 1 };
        let (slices, _nct, warnings) = analyze_harmony(&req);
        assert!(warnings.is_empty());
        let starts: Vec<u32> = slices.iter().map(|s| s.start_tick).collect();
        assert_eq!(starts, vec![0, 1920]);
    }

    #[test]
    fn per_measure_harmonic_rhythm_supports_variable_density() {
        let mut req = req_with_two_measure_static_chord();
        req.config.harmonic_rhythm = HarmonicRhythm::PerMeasure {
            chords_per_bar: vec![1, 2],
        };
        let (slices, _nct, warnings) = analyze_harmony(&req);
        assert!(warnings.is_empty());
        let starts: Vec<u32> = slices.iter().map(|s| s.start_tick).collect();
        assert_eq!(starts, vec![0, 1920, 2880]);
    }

    #[test]
    fn fixed_bars_per_chord_can_span_multiple_measures() {
        let mut req = req_with_two_measure_static_chord();
        req.config.harmonic_rhythm = HarmonicRhythm::FixedBarsPerChord { bars_per_chord: 2 };
        let (slices, _nct, warnings) = analyze_harmony(&req);
        assert!(warnings.is_empty());
        let starts: Vec<u32> = slices.iter().map(|s| s.start_tick).collect();
        assert_eq!(starts, vec![0]);
    }
}
