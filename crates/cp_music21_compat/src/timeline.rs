use serde::{Deserialize, Serialize};

use crate::interval::simple_interval_name;
use crate::musicxml::{MeasureSpan, ParsedNote, ParsedScore};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimelineArtifact {
    pub schema_version: u32,
    pub source_id: String,
    pub measure_number_shift: i32,
    pub slices: Vec<TimelineSlice>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimelineSlice {
    pub index: usize,
    pub start_div: u32,
    pub end_div: u32,
    pub measure_number: i32,
    pub notes: Vec<SliceNote>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SliceNote {
    pub part_id: String,
    pub voice_id: String,
    pub spelling: String,
    pub midi: i16,
    pub onset: bool,
    pub hold: bool,
    pub tie_start: bool,
    pub tie_stop: bool,
    pub interval_from_bass: String,
}

fn measure_number_at(measures: &[MeasureSpan], at: u32) -> i32 {
    for m in measures {
        if at >= m.start_div && at < m.end_div {
            return m.number;
        }
    }
    measures.last().map(|m| m.number).unwrap_or(1)
}

fn sort_notes(notes: &mut [ParsedNote]) {
    notes.sort_by(|a, b| {
        a.pitch
            .midi()
            .cmp(&b.pitch.midi())
            .then_with(|| a.part_id.cmp(&b.part_id))
            .then_with(|| a.voice_id.cmp(&b.voice_id))
            .then_with(|| {
                a.pitch
                    .m21_name_with_octave()
                    .cmp(&b.pitch.m21_name_with_octave())
            })
    });
}

/// Builds deterministic vertical slices from note events.
///
/// Maps to the baseline concept of `music21.stream.Stream.chordify()` plus offset iteration.
pub fn build_timeline(score: &ParsedScore, source_id: &str) -> TimelineArtifact {
    let mut boundaries: Vec<u32> = score
        .notes
        .iter()
        .flat_map(|n| [n.start_div, n.end_div])
        .collect();
    boundaries.sort_unstable();
    boundaries.dedup();

    let mut slices = Vec::new();
    for window in boundaries.windows(2) {
        let start = window[0];
        let end = window[1];
        if end <= start {
            continue;
        }

        let mut active: Vec<ParsedNote> = score
            .notes
            .iter()
            .filter(|n| n.start_div <= start && n.end_div > start)
            .cloned()
            .collect();

        if active.is_empty() {
            continue;
        }

        sort_notes(&mut active);
        let bass = active[0].pitch.clone();

        let notes = active
            .into_iter()
            .map(|n| {
                // music21-style tie continuation: tied stop notes at a slice boundary are held,
                // not fresh onsets, even though a new note object exists in the XML stream.
                let onset = n.start_div == start && !n.tie_stop;
                let interval = if n.pitch.midi() == bass.midi()
                    && n.pitch.m21_name_with_octave() == bass.m21_name_with_octave()
                {
                    "P1".to_string()
                } else {
                    simple_interval_name(&bass, &n.pitch)
                };
                SliceNote {
                    part_id: n.part_id,
                    voice_id: n.voice_id,
                    spelling: n.pitch.m21_name_with_octave(),
                    midi: n.pitch.midi(),
                    onset,
                    hold: !onset,
                    tie_start: n.tie_start,
                    tie_stop: n.tie_stop,
                    interval_from_bass: interval,
                }
            })
            .collect();

        let raw_measure = measure_number_at(&score.measures, start);
        slices.push(TimelineSlice {
            index: slices.len(),
            start_div: start,
            end_div: end,
            measure_number: raw_measure - score.measure_number_shift,
            notes,
        });
    }

    TimelineArtifact {
        schema_version: 1,
        source_id: source_id.to_string(),
        measure_number_shift: score.measure_number_shift,
        slices,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::musicxml::parse_musicxml;

    #[test]
    fn tie_stop_note_marks_hold_not_onset() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<score-partwise version="3.1">
  <part-list><score-part id="P1"><part-name>Music</part-name></score-part></part-list>
  <part id="P1">
    <measure number="1">
      <attributes><divisions>4</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes>
      <note>
        <pitch><step>C</step><octave>4</octave></pitch><duration>16</duration><tie type="start"/>
      </note>
    </measure>
    <measure number="2">
      <note>
        <pitch><step>C</step><octave>4</octave></pitch><duration>16</duration><tie type="stop"/>
      </note>
    </measure>
  </part>
</score-partwise>"#;

        let parsed = parse_musicxml(xml).expect("parse");
        let artifact = build_timeline(&parsed, "unit");

        let second_measure_slice = artifact
            .slices
            .iter()
            .find(|s| s.measure_number == 2)
            .expect("slice at shifted measure 2");
        assert!(!second_measure_slice.notes[0].onset);
        assert!(second_measure_slice.notes[0].hold);
    }

    #[test]
    fn slice_construction_splits_on_note_boundaries_and_labels_intervals() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<score-partwise version="3.1">
  <part-list><score-part id="P1"><part-name>Music</part-name></score-part></part-list>
  <part id="P1">
    <measure number="1">
      <attributes><divisions>4</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes>
      <note><pitch><step>C</step><octave>4</octave></pitch><duration>8</duration><voice>1</voice></note>
      <note><pitch><step>E</step><octave>4</octave></pitch><duration>8</duration><voice>1</voice></note>
      <backup><duration>16</duration></backup>
      <note><pitch><step>G</step><octave>3</octave></pitch><duration>16</duration><voice>2</voice></note>
    </measure>
  </part>
</score-partwise>"#;

        let parsed = parse_musicxml(xml).expect("parse");
        let artifact = build_timeline(&parsed, "poly");
        assert_eq!(artifact.slices.len(), 2);
        assert_eq!(artifact.slices[0].start_div, 0);
        assert_eq!(artifact.slices[0].end_div, 2);
        assert_eq!(artifact.slices[1].start_div, 2);
        assert_eq!(artifact.slices[1].end_div, 4);
        assert_eq!(artifact.slices[0].notes[1].interval_from_bass, "P4");
        assert_eq!(artifact.slices[1].notes[1].interval_from_bass, "M6");
    }
}
