use serde::{Deserialize, Serialize};

use crate::interval::simple_interval_name;
use crate::musicxml::{MeasureSpan, ParsedNote, ParsedScore};

const FLOAT_SCALE: f64 = 10_000.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AugnetScoreFrame {
    pub s_offset: f64,
    pub s_duration: f64,
    pub s_measure: i32,
    pub s_notes: Option<Vec<String>>,
    pub s_intervals: Option<Vec<String>>,
    pub s_is_onset: Option<Vec<bool>>,
}

fn round4(v: f64) -> f64 {
    (v * FLOAT_SCALE).round() / FLOAT_SCALE
}

fn to_scaled(v: f64) -> i64 {
    (v * FLOAT_SCALE).round() as i64
}

fn from_scaled(v: i64) -> f64 {
    round4(v as f64 / FLOAT_SCALE)
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

fn is_onset(note: &ParsedNote, start_div: u32) -> bool {
    if note.start_div != start_div {
        return false;
    }
    !note.tie_stop
}

fn score_last_offset(score: &ParsedScore) -> f64 {
    score
        .measures
        .last()
        .map(|m| m.end_div as f64 / score.grid_divisions_per_quarter as f64)
        .unwrap_or(0.0)
}

/// Equivalent to AugmentedNet score parser `_initialDataFrame` from a parsed score.
pub fn augnet_initial_frames(score: &ParsedScore) -> Vec<AugnetScoreFrame> {
    if score.measures.is_empty() {
        return Vec::new();
    }

    let mut boundaries: Vec<u32> = score
        .notes
        .iter()
        .flat_map(|n| [n.start_div, n.end_div])
        .collect();
    boundaries.extend(score.measures.iter().flat_map(|m| [m.start_div, m.end_div]));
    boundaries.sort_unstable();
    boundaries.dedup();

    let mut rows = Vec::new();
    for w in boundaries.windows(2) {
        let start = w[0];
        let end = w[1];
        if end <= start {
            continue;
        }

        let mut active: Vec<ParsedNote> = score
            .notes
            .iter()
            .filter(|n| n.start_div <= start && n.end_div > start)
            .cloned()
            .collect();

        let offset = round4(start as f64 / score.grid_divisions_per_quarter as f64);
        let duration = round4((end - start) as f64 / score.grid_divisions_per_quarter as f64);
        let measure = measure_number_at(&score.measures, start) - score.measure_number_shift;

        if active.is_empty() {
            rows.push(AugnetScoreFrame {
                s_offset: offset,
                s_duration: duration,
                s_measure: measure,
                s_notes: None,
                s_intervals: None,
                s_is_onset: None,
            });
            continue;
        }

        sort_notes(&mut active);
        let bass = active[0].pitch.clone();
        let notes: Vec<String> = active
            .iter()
            .map(|n| n.pitch.m21_name_with_octave())
            .collect();
        let intervals: Vec<String> = active
            .iter()
            .skip(1)
            .map(|n| simple_interval_name(&bass, &n.pitch))
            .collect();
        let onsets: Vec<bool> = active.iter().map(|n| is_onset(n, start)).collect();

        rows.push(AugnetScoreFrame {
            s_offset: offset,
            s_duration: duration,
            s_measure: measure,
            s_notes: Some(notes),
            s_intervals: Some(intervals),
            s_is_onset: Some(onsets),
        });
    }

    if let Some(last) = rows.last_mut() {
        let current_last_offset = round4(last.s_offset + last.s_duration);
        let delta = round4(score_last_offset(score) - current_last_offset);
        if delta != 0.0 {
            last.s_duration = round4(last.s_duration + delta);
        }
    }

    // Equivalent to pandas duplicate-index keep-first behavior.
    rows.dedup_by(|a, b| to_scaled(a.s_offset) == to_scaled(b.s_offset));
    rows
}

fn fill_forward<T: Clone, FGet, FSet>(rows: &mut [AugnetScoreFrame], mut get: FGet, mut set: FSet)
where
    FGet: FnMut(&AugnetScoreFrame) -> Option<T>,
    FSet: FnMut(&mut AugnetScoreFrame, T),
{
    let mut carry: Option<T> = None;
    for row in rows.iter_mut() {
        if let Some(v) = get(row) {
            carry = Some(v);
        } else if let Some(v) = carry.clone() {
            set(row, v);
        }
    }
}

fn fill_backward<T: Clone, FGet, FSet>(rows: &mut [AugnetScoreFrame], mut get: FGet, mut set: FSet)
where
    FGet: FnMut(&AugnetScoreFrame) -> Option<T>,
    FSet: FnMut(&mut AugnetScoreFrame, T),
{
    let mut carry: Option<T> = None;
    for row in rows.iter_mut().rev() {
        if let Some(v) = get(row) {
            carry = Some(v);
        } else if let Some(v) = carry.clone() {
            set(row, v);
        }
    }
}

/// Equivalent to AugmentedNet `_reindexDataFrame` fixed-grid behavior.
pub fn augnet_reindex_frames(
    initial: &[AugnetScoreFrame],
    fixed_offset: f64,
) -> Vec<AugnetScoreFrame> {
    if initial.is_empty() {
        return Vec::new();
    }

    let min_offset_i = to_scaled(initial.first().expect("non-empty").s_offset);
    let max_offset_i = to_scaled(
        initial.last().expect("non-empty").s_offset + initial.last().expect("non-empty").s_duration,
    );
    let step_i = to_scaled(fixed_offset).max(1);

    let mut new_index: Vec<i64> = Vec::new();
    let mut cur = min_offset_i;
    while cur < max_offset_i {
        new_index.push(cur);
        cur += step_i;
    }

    let mut all_index = new_index.clone();
    all_index.extend(initial.iter().map(|r| to_scaled(r.s_offset)));
    all_index.sort_unstable();
    all_index.dedup();

    let mut by_offset = std::collections::BTreeMap::new();
    for row in initial {
        by_offset
            .entry(to_scaled(row.s_offset))
            .or_insert_with(|| row.clone());
    }

    let mut rows: Vec<AugnetScoreFrame> = all_index
        .iter()
        .map(|idx| {
            by_offset.get(idx).cloned().unwrap_or(AugnetScoreFrame {
                s_offset: from_scaled(*idx),
                s_duration: f64::NAN,
                s_measure: i32::MIN,
                s_notes: None,
                s_intervals: None,
                s_is_onset: None,
            })
        })
        .collect();

    // s_notes ffill+bfill
    fill_forward(
        &mut rows,
        |r| r.s_notes.clone(),
        |r, v| {
            r.s_notes = Some(v);
        },
    );
    fill_backward(
        &mut rows,
        |r| r.s_notes.clone(),
        |r, v| {
            r.s_notes = Some(v);
        },
    );

    // Fill missing onset vectors with all-False according to note cardinality.
    for row in &mut rows {
        if row.s_is_onset.is_none() {
            let n = row.s_notes.as_ref().map(|x| x.len()).unwrap_or(0);
            row.s_is_onset = Some(vec![false; n]);
        }
    }

    // Equivalent to fillna(ffill), fillna(bfill) on remaining columns.
    fill_forward(
        &mut rows,
        |r| {
            if r.s_duration.is_finite() {
                Some(r.s_duration)
            } else {
                None
            }
        },
        |r, v| {
            r.s_duration = v;
        },
    );
    fill_backward(
        &mut rows,
        |r| {
            if r.s_duration.is_finite() {
                Some(r.s_duration)
            } else {
                None
            }
        },
        |r, v| {
            r.s_duration = v;
        },
    );

    fill_forward(
        &mut rows,
        |r| {
            if r.s_measure != i32::MIN {
                Some(r.s_measure)
            } else {
                None
            }
        },
        |r, v| {
            r.s_measure = v;
        },
    );
    fill_backward(
        &mut rows,
        |r| {
            if r.s_measure != i32::MIN {
                Some(r.s_measure)
            } else {
                None
            }
        },
        |r, v| {
            r.s_measure = v;
        },
    );

    fill_forward(
        &mut rows,
        |r| r.s_intervals.clone(),
        |r, v| {
            r.s_intervals = Some(v);
        },
    );
    fill_backward(
        &mut rows,
        |r| r.s_intervals.clone(),
        |r, v| {
            r.s_intervals = Some(v);
        },
    );

    let new_index_set: std::collections::BTreeSet<i64> = new_index.into_iter().collect();
    let mut out: Vec<AugnetScoreFrame> = rows
        .into_iter()
        .filter(|r| new_index_set.contains(&to_scaled(r.s_offset)))
        .collect();
    out.sort_by_key(|r| to_scaled(r.s_offset));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::musicxml::parse_musicxml;

    #[test]
    fn initial_frames_capture_partial_onsets_in_vertical_slices() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<score-partwise version="3.1">
  <part-list><score-part id="P1"><part-name>Music</part-name></score-part></part-list>
  <part id="P1">
    <measure number="1">
      <attributes><divisions>4</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes>
      <note><pitch><step>C</step><octave>4</octave></pitch><duration>16</duration><tie type="start"/></note>
    </measure>
    <measure number="2">
      <note><pitch><step>C</step><octave>4</octave></pitch><duration>8</duration><tie type="stop"/></note>
      <note><pitch><step>E</step><octave>4</octave></pitch><duration>8</duration></note>
    </measure>
  </part>
</score-partwise>"#;

        let parsed = parse_musicxml(xml).expect("parse");
        let rows = augnet_initial_frames(&parsed);
        assert_eq!(rows.len(), 3);
        assert_eq!(
            rows[1].s_notes.as_ref().expect("notes"),
            &vec!["C4".to_string()]
        );
        assert_eq!(rows[1].s_is_onset.as_ref().expect("onset"), &vec![false]);
        assert_eq!(
            rows[2].s_notes.as_ref().expect("notes"),
            &vec!["E4".to_string()]
        );
        assert_eq!(rows[2].s_is_onset.as_ref().expect("onset"), &vec![true]);
    }

    #[test]
    fn reindex_frames_produces_fixed_grid_and_false_holds() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<score-partwise version="3.1">
  <part-list><score-part id="P1"><part-name>Music</part-name></score-part></part-list>
  <part id="P1">
    <measure number="1">
      <attributes><divisions>4</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes>
      <note><pitch><step>C</step><octave>4</octave></pitch><duration>16</duration></note>
    </measure>
  </part>
</score-partwise>"#;

        let parsed = parse_musicxml(xml).expect("parse");
        let rows = augnet_initial_frames(&parsed);
        let grid = augnet_reindex_frames(&rows, 0.25);
        assert_eq!(grid.len(), 16);
        assert_eq!(grid[0].s_is_onset.as_ref().expect("onset"), &vec![true]);
        assert_eq!(grid[1].s_is_onset.as_ref().expect("onset"), &vec![false]);
    }

    #[test]
    fn reindex_preserves_pickup_measure_zero() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<score-partwise version="3.1">
  <part-list><score-part id="P1"><part-name>Music</part-name></score-part></part-list>
  <part id="P1">
    <measure number="1">
      <attributes><divisions>4</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes>
      <note><pitch><step>G</step><octave>4</octave></pitch><duration>4</duration></note>
    </measure>
    <measure number="2">
      <note><pitch><step>C</step><octave>4</octave></pitch><duration>16</duration></note>
    </measure>
  </part>
</score-partwise>"#;

        let parsed = parse_musicxml(xml).expect("parse");
        let initial = augnet_initial_frames(&parsed);
        assert_eq!(initial[0].s_measure, 0);
        let grid = augnet_reindex_frames(&initial, 0.25);
        assert_eq!(grid[0].s_measure, 0);
        assert_eq!(grid[3].s_measure, 0);
        assert_eq!(grid[4].s_measure, 1);
    }
}
