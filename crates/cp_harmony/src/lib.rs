use cp_core::{
    interval_pc, AnalysisRequest, HarmonicSlice, NctTag, NoteEvent, NormalizedScore, ScaleMode,
};
use indexmap::IndexMap;

#[derive(Debug, Clone)]
struct ActiveNote<'a> {
    note: &'a NoteEvent,
}

pub fn analyze_harmony(req: &AnalysisRequest) -> (Vec<HarmonicSlice>, Vec<NctTag>, Vec<String>) {
    let score = &req.score;
    let mut warnings = Vec::new();
    let mut starts: Vec<u32> = score
        .voices
        .iter()
        .flat_map(|v| v.notes.iter().map(|n| n.start_tick))
        .collect();
    starts.sort_unstable();
    starts.dedup();
    if starts.is_empty() {
        return (Vec::new(), Vec::new(), vec!["score contains no notes".to_string()]);
    }

    let mut slices = Vec::new();
    for (ix, start) in starts.iter().enumerate() {
        let end = starts.get(ix + 1).copied().unwrap_or(*start + 1);
        let mut active: Vec<ActiveNote<'_>> = Vec::new();
        for v in &score.voices {
            for n in &v.notes {
                if n.start_tick <= *start && *start < n.start_tick + n.duration_ticks {
                    active.push(ActiveNote { note: n });
                }
            }
        }
        if active.is_empty() {
            continue;
        }
        let pcs: Vec<u8> = {
            let mut p: Vec<u8> = active.iter().map(|a| a.note.midi.rem_euclid(12) as u8).collect();
            p.sort_unstable();
            p.dedup();
            p
        };
        let (root, quality) = detect_quality(&pcs);
        let inversion = root.and_then(|r| detect_inversion(r, &active));
        let roman = root.map(|r| roman_in_key(score, r, quality.as_deref()));
        let conf = if root.is_some() { 0.9 } else { 0.3 };
        if root.is_none() {
            warnings.push(format!("ambiguous harmony at tick {}", start));
        }
        slices.push(HarmonicSlice {
            slice_id: ix as u32,
            start_tick: *start,
            end_tick: end,
            pitch_classes: pcs,
            root_pc: root,
            quality,
            inversion,
            roman_numeral: roman,
            confidence: conf,
        });
    }

    let nct_tags = detect_nct(score);
    (slices, nct_tags, warnings)
}

fn detect_quality(pcs: &[u8]) -> (Option<u8>, Option<String>) {
    if pcs.is_empty() {
        return (None, None);
    }
    for &root in pcs {
        let mut intervals: Vec<u8> = pcs.iter().map(|pc| interval_pc(*pc as i16, root as i16)).collect();
        intervals.sort_unstable();
        if intervals.contains(&4) && intervals.contains(&7) && intervals.contains(&10) {
            return (Some(root), Some("dominant7".to_string()));
        }
        if intervals.contains(&4) && intervals.contains(&7) {
            return (Some(root), Some("major".to_string()));
        }
        if intervals.contains(&3) && intervals.contains(&7) {
            return (Some(root), Some("minor".to_string()));
        }
        if intervals.contains(&3) && intervals.contains(&6) {
            return (Some(root), Some("diminished".to_string()));
        }
        if intervals.contains(&4) && intervals.contains(&8) {
            return (Some(root), Some("augmented".to_string()));
        }
    }
    (Some(pcs[0]), Some("other".to_string()))
}

fn detect_inversion(root_pc: u8, active: &[ActiveNote<'_>]) -> Option<String> {
    let bass = active.iter().map(|a| a.note.midi).min()?;
    let bass_pc = bass.rem_euclid(12) as u8;
    let i = interval_pc(bass_pc as i16, root_pc as i16);
    let inv = match i {
        0 => "root",
        3 | 4 => "first",
        6 | 7 | 8 => "second",
        10 | 11 => "third",
        _ => "other",
    };
    Some(inv.to_string())
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
    use cp_core::{AnalysisConfig, AnalysisRequest, KeySignature, PresetId, ScoreMeta, TimeSignature, Voice};
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
    }
}
