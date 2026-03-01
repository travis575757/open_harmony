use std::cmp::Ordering;
use std::collections::HashMap;

use roxmltree::{Document, Node};

use crate::error::CompatError;
use crate::pitch::PitchSpelling;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Frac {
    num: i64,
    den: i64,
}

impl Frac {
    fn zero() -> Self {
        Self { num: 0, den: 1 }
    }

    fn new(num: i64, den: i64) -> Result<Self, CompatError> {
        if den == 0 {
            return Err(CompatError::InvalidValue {
                field: "duration.denominator",
                value: den.to_string(),
            });
        }
        let mut num = num;
        let mut den = den;
        if den < 0 {
            num = -num;
            den = -den;
        }
        let g = gcd_i64(num.unsigned_abs(), den as u64) as i64;
        Ok(Self {
            num: num / g,
            den: den / g,
        })
    }

    fn add(self, rhs: Self) -> Self {
        let num = self.num * rhs.den + rhs.num * self.den;
        let den = self.den * rhs.den;
        Self::new(num, den).expect("valid fraction add")
    }

    fn sub(self, rhs: Self) -> Self {
        let num = self.num * rhs.den - rhs.num * self.den;
        let den = self.den * rhs.den;
        Self::new(num, den).expect("valid fraction sub")
    }

    fn to_div(self, grid: u32) -> Result<u32, CompatError> {
        if self.num < 0 {
            return Err(CompatError::InvalidValue {
                field: "offset",
                value: self.num.to_string(),
            });
        }
        let scale = i64::from(grid) / self.den;
        Ok((self.num * scale) as u32)
    }
}

impl PartialOrd for Frac {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Frac {
    fn cmp(&self, other: &Self) -> Ordering {
        let lhs = self.num as i128 * other.den as i128;
        let rhs = other.num as i128 * self.den as i128;
        lhs.cmp(&rhs)
    }
}

fn gcd_i64(a: u64, b: u64) -> u64 {
    let mut a = a;
    let mut b = b;
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a.max(1)
}

fn lcm_u32(a: u32, b: u32) -> u32 {
    (a / gcd_i64(a as u64, b as u64) as u32).saturating_mul(b)
}

#[derive(Debug, Clone)]
struct RawNote {
    part_id: String,
    voice_id: String,
    pitch: PitchSpelling,
    start_q: Frac,
    end_q: Frac,
    tie_start: bool,
    tie_stop: bool,
}

#[derive(Debug, Clone)]
struct RawMeasure {
    number: i32,
    start_q: Frac,
    end_q: Frac,
    nominal_q: Frac,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedNote {
    pub part_id: String,
    pub voice_id: String,
    pub pitch: PitchSpelling,
    pub start_div: u32,
    pub end_div: u32,
    pub tie_start: bool,
    pub tie_stop: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeasureSpan {
    pub number: i32,
    pub start_div: u32,
    pub end_div: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedScore {
    pub grid_divisions_per_quarter: u32,
    pub measure_number_shift: i32,
    pub notes: Vec<ParsedNote>,
    pub measures: Vec<MeasureSpan>,
}

fn child_text<'a>(node: Node<'a, 'a>, tag: &str) -> Option<&'a str> {
    node.children()
        .find(|c| c.is_element() && c.tag_name().name() == tag)
        .and_then(|n| n.text())
        .map(str::trim)
}

fn has_tie_type(node: Node<'_, '_>, tie_type: &str) -> bool {
    let direct = node
        .children()
        .filter(|c| c.is_element() && c.tag_name().name() == "tie")
        .any(|t| t.attribute("type") == Some(tie_type));

    let nested = node
        .children()
        .find(|c| c.is_element() && c.tag_name().name() == "notations")
        .into_iter()
        .flat_map(|n| n.children())
        .filter(|c| c.is_element() && c.tag_name().name() == "tied")
        .any(|t| t.attribute("type") == Some(tie_type));

    direct || nested
}

fn parse_u32_field(node: Node<'_, '_>, field: &'static str) -> Result<u32, CompatError> {
    node.text()
        .ok_or(CompatError::MissingField(field))?
        .trim()
        .parse::<u32>()
        .map_err(|_| CompatError::InvalidValue {
            field,
            value: node.text().unwrap_or_default().to_string(),
        })
}

fn parse_measure_number(raw: Option<&str>, fallback: i32, previous_number: Option<i32>) -> i32 {
    let Some(raw) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return fallback;
    };
    if let Ok(value) = raw.parse::<i32>() {
        return value;
    }

    // MusicXML exports can encode alternate endings as measure numbers like `8X1`.
    // Match music21 behavior by using the leading signed integer token when present.
    let bytes = raw.as_bytes();
    let mut start = 0usize;
    if matches!(bytes.first(), Some(b'+') | Some(b'-')) {
        start = 1;
    }
    let mut end = start;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    if end > start {
        return raw[..end].parse::<i32>().unwrap_or(fallback);
    }

    // Some MusicXML exports encode alternate endings as `X1`, `X2`, ... without
    // a leading numeric measure. music21 keeps these attached to the prior measure
    // number rather than inventing a new positional number.
    if raw.starts_with('X') && raw[1..].chars().all(|c| c.is_ascii_digit()) {
        if let Some(prev) = previous_number {
            return prev;
        }
    }

    if let Some(prev) = previous_number {
        return prev;
    }
    fallback
}

fn parse_pitch(note: Node<'_, '_>) -> Result<PitchSpelling, CompatError> {
    let pitch_node = note
        .children()
        .find(|c| c.is_element() && c.tag_name().name() == "pitch")
        .ok_or(CompatError::MissingField("note.pitch"))?;
    let step = child_text(pitch_node, "step").ok_or(CompatError::MissingField("pitch.step"))?;
    let alter = child_text(pitch_node, "alter");
    let octave =
        child_text(pitch_node, "octave").ok_or(CompatError::MissingField("pitch.octave"))?;
    PitchSpelling::parse(step, alter, octave)
}

fn strip_doctype(xml: &str) -> String {
    if !xml.contains("<!DOCTYPE") {
        return xml.to_string();
    }

    let bytes = xml.as_bytes();
    let mut out = String::with_capacity(xml.len());
    let mut i = 0usize;

    while let Some(rel) = xml[i..].find("<!DOCTYPE") {
        let start = i + rel;
        out.push_str(&xml[i..start]);

        let mut j = start;
        let mut bracket_depth = 0i32;
        while j < bytes.len() {
            match bytes[j] {
                b'[' => bracket_depth += 1,
                b']' if bracket_depth > 0 => bracket_depth -= 1,
                b'>' if bracket_depth == 0 => {
                    j += 1;
                    break;
                }
                _ => {}
            }
            j += 1;
        }
        i = j.min(bytes.len());
    }

    out.push_str(&xml[i..]);
    out
}

/// Parses the strict MusicXML subset needed for AugmentedNet compatibility.
///
/// Maps to the baseline concept of `music21.converter.parse(...)` for partwise scores.
pub fn parse_musicxml(xml: &str) -> Result<ParsedScore, CompatError> {
    let sanitized = strip_doctype(xml);
    let doc =
        Document::parse(&sanitized).map_err(|e| CompatError::InvalidMusicXml(e.to_string()))?;
    let root = doc.root_element();
    if root.tag_name().name() != "score-partwise" {
        return Err(CompatError::InvalidMusicXml(
            "only score-partwise input is supported".to_string(),
        ));
    }

    let part_nodes: Vec<Node<'_, '_>> = root
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "part")
        .collect();
    if part_nodes.is_empty() {
        return Err(CompatError::MissingField("score.part"));
    }

    let mut part_name_by_id: HashMap<String, String> = HashMap::new();
    if let Some(part_list) = root
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == "part-list")
    {
        for score_part in part_list
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "score-part")
        {
            let Some(id) = score_part.attribute("id").map(str::to_string) else {
                continue;
            };
            let Some(name) = child_text(score_part, "part-name").map(str::to_string) else {
                continue;
            };
            part_name_by_id.insert(id, name);
        }
    }

    let use_part_name_for_identity = part_nodes.len() > 1;

    let mut raw_notes: Vec<RawNote> = Vec::new();
    let mut raw_measures: Vec<RawMeasure> = Vec::new();
    let mut first_measure_pickup = false;
    let mut max_part_end_q = Frac::zero();

    for (part_idx, part) in part_nodes.iter().enumerate() {
        let part_id = part
            .attribute("id")
            .map(|id| {
                if use_part_name_for_identity {
                    part_name_by_id
                        .get(id)
                        .cloned()
                        .unwrap_or_else(|| id.to_string())
                } else {
                    id.to_string()
                }
            })
            .unwrap_or_else(|| format!("P{}", part_idx + 1));

        let mut divisions: u32 = 1;
        let mut beats: u32 = 4;
        let mut beat_type: u32 = 4;
        let mut measure_start = Frac::zero();

        let measures: Vec<Node<'_, '_>> = part
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "measure")
            .collect();

        let mut previous_measure_number: Option<i32> = None;
        for (measure_idx, measure) in measures.iter().enumerate() {
            let number = parse_measure_number(
                measure.attribute("number"),
                (measure_idx + 1) as i32,
                previous_measure_number,
            );
            previous_measure_number = Some(number);

            let mut cursor = measure_start;
            let mut max_cursor = measure_start;
            let mut last_note_start: Option<Frac> = None;

            for child in measure.children().filter(|n| n.is_element()) {
                match child.tag_name().name() {
                    "attributes" => {
                        if let Some(d) = child
                            .children()
                            .find(|n| n.is_element() && n.tag_name().name() == "divisions")
                        {
                            divisions = parse_u32_field(d, "attributes.divisions")?;
                            if divisions == 0 {
                                return Err(CompatError::InvalidValue {
                                    field: "attributes.divisions",
                                    value: "0".to_string(),
                                });
                            }
                        }

                        if let Some(time_node) = child
                            .children()
                            .find(|n| n.is_element() && n.tag_name().name() == "time")
                        {
                            if let Some(beats_node) = time_node
                                .children()
                                .find(|n| n.is_element() && n.tag_name().name() == "beats")
                            {
                                beats = parse_u32_field(beats_node, "time.beats")?;
                            }
                            if let Some(beat_type_node) = time_node
                                .children()
                                .find(|n| n.is_element() && n.tag_name().name() == "beat-type")
                            {
                                beat_type = parse_u32_field(beat_type_node, "time.beat_type")?;
                            }
                        }
                    }
                    "backup" | "forward" => {
                        let duration_node = child
                            .children()
                            .find(|n| n.is_element() && n.tag_name().name() == "duration")
                            .ok_or(CompatError::MissingField("backup.duration"))?;
                        let duration = parse_u32_field(duration_node, "backup.duration")?;
                        let delta = Frac::new(duration as i64, divisions as i64)?;
                        if child.tag_name().name() == "backup" {
                            cursor = cursor.sub(delta);
                        } else {
                            cursor = cursor.add(delta);
                            if cursor > max_cursor {
                                max_cursor = cursor;
                            }
                        }
                    }
                    "note" => {
                        let is_rest = child
                            .children()
                            .any(|n| n.is_element() && n.tag_name().name() == "rest");
                        let is_chord = child
                            .children()
                            .any(|n| n.is_element() && n.tag_name().name() == "chord");

                        let duration = child
                            .children()
                            .find(|n| n.is_element() && n.tag_name().name() == "duration")
                            .map(|n| parse_u32_field(n, "note.duration"))
                            .transpose()?
                            .unwrap_or(0);

                        let dur_q = Frac::new(duration as i64, divisions as i64)?;
                        let start_q = if is_chord {
                            last_note_start.unwrap_or(cursor)
                        } else {
                            cursor
                        };
                        let end_q = start_q.add(dur_q);

                        if !is_rest {
                            let pitch = parse_pitch(child)?;
                            let voice = child_text(child, "voice").unwrap_or("1").to_string();
                            let tie_start = has_tie_type(child, "start");
                            let tie_stop = has_tie_type(child, "stop") && !tie_start;
                            raw_notes.push(RawNote {
                                part_id: part_id.clone(),
                                voice_id: voice,
                                pitch,
                                start_q,
                                end_q,
                                tie_start,
                                tie_stop,
                            });
                        }

                        if !is_chord {
                            cursor = end_q;
                            last_note_start = Some(start_q);
                        }
                        if end_q > max_cursor {
                            max_cursor = end_q;
                        }
                    }
                    _ => {}
                }
            }

            let nominal_q = Frac::new((beats * 4) as i64, beat_type as i64)?;
            let measured_span = max_cursor.sub(measure_start);
            if part_idx == 0 {
                // music21 pickup behavior equivalent: if the first bar is short, downstream
                // consumers should treat subsequent measure numbers as shifted by -1.
                if measure_idx == 0 && measured_span < nominal_q && measured_span > Frac::zero() {
                    first_measure_pickup = true;
                }
                raw_measures.push(RawMeasure {
                    number,
                    start_q: measure_start,
                    end_q: if max_cursor > measure_start {
                        max_cursor
                    } else {
                        measure_start.add(nominal_q)
                    },
                    nominal_q,
                });
            }

            measure_start = if max_cursor > measure_start {
                max_cursor
            } else {
                measure_start.add(nominal_q)
            };
        }

        if measure_start > max_part_end_q {
            max_part_end_q = measure_start;
        }
    }

    if let Some(last_measure) = raw_measures.last_mut() {
        let mut latest_end_q = max_part_end_q;
        if let Some(max_note_end_q) = raw_notes.iter().map(|n| n.end_q).max() {
            if max_note_end_q > latest_end_q {
                latest_end_q = max_note_end_q;
            }
        }
        let nominal_end_q = last_measure.start_q.add(last_measure.nominal_q);
        if nominal_end_q > last_measure.end_q {
            last_measure.end_q = nominal_end_q;
        }
        if latest_end_q > last_measure.end_q {
            last_measure.end_q = latest_end_q;
        }
    }

    let mut grid = 1u32;
    for n in &raw_notes {
        grid = lcm_u32(grid, n.start_q.den as u32);
        grid = lcm_u32(grid, n.end_q.den as u32);
    }
    for m in &raw_measures {
        grid = lcm_u32(grid, m.start_q.den as u32);
        grid = lcm_u32(grid, m.end_q.den as u32);
    }

    let mut notes = raw_notes
        .into_iter()
        .map(|n| {
            Ok(ParsedNote {
                part_id: n.part_id,
                voice_id: n.voice_id,
                pitch: n.pitch,
                start_div: n.start_q.to_div(grid)?,
                end_div: n.end_q.to_div(grid)?,
                tie_start: n.tie_start,
                tie_stop: n.tie_stop,
            })
        })
        .collect::<Result<Vec<_>, CompatError>>()?;

    notes.sort_by(|a, b| {
        a.start_div
            .cmp(&b.start_div)
            .then_with(|| a.end_div.cmp(&b.end_div))
            .then_with(|| a.pitch.midi().cmp(&b.pitch.midi()))
            .then_with(|| a.part_id.cmp(&b.part_id))
            .then_with(|| a.voice_id.cmp(&b.voice_id))
    });

    let mut measures = raw_measures
        .into_iter()
        .map(|m| {
            Ok(MeasureSpan {
                number: m.number,
                start_div: m.start_q.to_div(grid)?,
                end_div: m.end_q.to_div(grid)?,
            })
        })
        .collect::<Result<Vec<_>, CompatError>>()?;

    measures.sort_by(|a, b| {
        a.start_div
            .cmp(&b.start_div)
            .then_with(|| a.number.cmp(&b.number))
    });

    Ok(ParsedScore {
        grid_divisions_per_quarter: grid,
        measure_number_shift: if first_measure_pickup { 1 } else { 0 },
        notes,
        measures,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pickup_and_sets_measure_shift() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<score-partwise version="3.1">
  <part-list><score-part id="P1"><part-name>Music</part-name></score-part></part-list>
  <part id="P1">
    <measure number="1">
      <attributes>
        <divisions>4</divisions>
        <time><beats>4</beats><beat-type>4</beat-type></time>
      </attributes>
      <note><pitch><step>C</step><octave>4</octave></pitch><duration>4</duration></note>
    </measure>
    <measure number="2">
      <note><pitch><step>D</step><octave>4</octave></pitch><duration>16</duration></note>
    </measure>
  </part>
</score-partwise>"#;

        let parsed = parse_musicxml(xml).expect("parse");
        assert_eq!(parsed.measure_number_shift, 1);
        assert_eq!(parsed.measures[0].start_div, 0);
        assert_eq!(parsed.measures[0].end_div, 1);
        assert_eq!(parsed.measures[1].start_div, 1);
        assert_eq!(parsed.measures[1].end_div, 5);
    }

    #[test]
    fn parses_measure_numbers_with_suffixes_using_numeric_prefix() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<score-partwise version="3.1">
  <part-list><score-part id="P1"><part-name>Music</part-name></score-part></part-list>
  <part id="P1">
    <measure number="7">
      <attributes>
        <divisions>4</divisions>
        <time><beats>3</beats><beat-type>4</beat-type></time>
      </attributes>
      <note><pitch><step>C</step><octave>4</octave></pitch><duration>12</duration></note>
    </measure>
    <measure number="8X1">
      <note><pitch><step>D</step><octave>4</octave></pitch><duration>12</duration></note>
    </measure>
    <measure number="9">
      <note><pitch><step>E</step><octave>4</octave></pitch><duration>12</duration></note>
    </measure>
  </part>
</score-partwise>"#;

        let parsed = parse_musicxml(xml).expect("parse");
        assert_eq!(parsed.measures.len(), 3);
        assert_eq!(parsed.measures[0].number, 7);
        assert_eq!(parsed.measures[1].number, 8);
        assert_eq!(parsed.measures[2].number, 9);
    }

    #[test]
    fn parses_x_only_measure_numbers_as_previous_measure_number() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<score-partwise version="3.1">
  <part-list><score-part id="P1"><part-name>Music</part-name></score-part></part-list>
  <part id="P1">
    <measure number="6">
      <attributes>
        <divisions>4</divisions>
        <time><beats>3</beats><beat-type>4</beat-type></time>
      </attributes>
      <note><pitch><step>C</step><octave>4</octave></pitch><duration>12</duration></note>
    </measure>
    <measure number="X1">
      <note><pitch><step>D</step><octave>4</octave></pitch><duration>12</duration></note>
    </measure>
    <measure number="7">
      <note><pitch><step>E</step><octave>4</octave></pitch><duration>12</duration></note>
    </measure>
  </part>
</score-partwise>"#;

        let parsed = parse_musicxml(xml).expect("parse");
        assert_eq!(parsed.measures.len(), 3);
        assert_eq!(parsed.measures[0].number, 6);
        assert_eq!(parsed.measures[1].number, 6);
        assert_eq!(parsed.measures[2].number, 7);
    }

    #[test]
    fn normalizes_offsets_when_divisions_change() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<score-partwise version="3.1">
  <part-list><score-part id="P1"><part-name>Music</part-name></score-part></part-list>
  <part id="P1">
    <measure number="1">
      <attributes>
        <divisions>4</divisions>
        <time><beats>4</beats><beat-type>4</beat-type></time>
      </attributes>
      <note><pitch><step>C</step><octave>4</octave></pitch><duration>4</duration></note>
    </measure>
    <measure number="2">
      <attributes>
        <divisions>8</divisions>
      </attributes>
      <note><pitch><step>D</step><octave>4</octave></pitch><duration>8</duration></note>
    </measure>
  </part>
</score-partwise>"#;

        let parsed = parse_musicxml(xml).expect("parse");
        assert_eq!(parsed.grid_divisions_per_quarter, 1);
        assert_eq!(parsed.notes[0].start_div, 0);
        assert_eq!(parsed.notes[0].end_div, 1);
        assert_eq!(parsed.notes[1].start_div, 1);
        assert_eq!(parsed.notes[1].end_div, 2);
    }

    #[test]
    fn accepts_musicxml_with_doctype_declaration() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE score-partwise PUBLIC
    "-//Recordare//DTD MusicXML 3.1 Partwise//EN"
    "http://www.musicxml.org/dtds/partwise.dtd">
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
        assert_eq!(parsed.notes.len(), 1);
        assert_eq!(parsed.measures.len(), 1);
    }

    #[test]
    fn tie_continue_note_matches_music21_onset_semantics() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<score-partwise version="3.1">
  <part-list><score-part id="P1"><part-name>Music</part-name></score-part></part-list>
  <part id="P1">
    <measure number="1">
      <attributes><divisions>4</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes>
      <note><pitch><step>C</step><octave>4</octave></pitch><duration>16</duration><tie type="start"/></note>
    </measure>
    <measure number="2">
      <note>
        <pitch><step>C</step><octave>4</octave></pitch><duration>16</duration>
        <tie type="stop"/><tie type="start"/>
      </note>
    </measure>
  </part>
</score-partwise>"#;

        let parsed = parse_musicxml(xml).expect("parse");
        assert_eq!(parsed.notes.len(), 2);
        assert!(parsed.notes[0].tie_start);
        assert!(!parsed.notes[0].tie_stop);
        assert!(parsed.notes[1].tie_start);
        assert!(!parsed.notes[1].tie_stop);
    }

    #[test]
    fn part_name_is_used_for_part_identity_ordering() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<score-partwise version="3.1">
  <part-list>
    <score-part id="P1"><part-name>Upper</part-name></score-part>
    <score-part id="P2"><part-name>Lower</part-name></score-part>
  </part-list>
  <part id="P1">
    <measure number="1">
      <attributes><divisions>4</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes>
      <note><pitch><step>F</step><alter>1</alter><octave>3</octave></pitch><duration>16</duration></note>
    </measure>
  </part>
  <part id="P2">
    <measure number="1">
      <attributes><divisions>4</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes>
      <note><pitch><step>G</step><alter>-1</alter><octave>3</octave></pitch><duration>16</duration></note>
    </measure>
  </part>
</score-partwise>"#;

        let parsed = parse_musicxml(xml).expect("parse");
        assert_eq!(parsed.notes.len(), 2);
        assert_eq!(parsed.notes[0].part_id, "Lower");
        assert_eq!(parsed.notes[1].part_id, "Upper");
    }

    #[test]
    fn last_measure_extends_to_latest_note_across_parts() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<score-partwise version="3.1">
  <part-list>
    <score-part id="P1"><part-name>Upper</part-name></score-part>
    <score-part id="P2"><part-name>Lower</part-name></score-part>
  </part-list>
  <part id="P1">
    <measure number="1">
      <attributes><divisions>4</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes>
      <note><pitch><step>C</step><octave>4</octave></pitch><duration>16</duration></note>
    </measure>
  </part>
  <part id="P2">
    <measure number="1">
      <attributes><divisions>4</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes>
      <note><pitch><step>C</step><octave>3</octave></pitch><duration>24</duration></note>
    </measure>
  </part>
</score-partwise>"#;

        let parsed = parse_musicxml(xml).expect("parse");
        assert_eq!(parsed.measures.len(), 1);
        assert_eq!(parsed.measures[0].start_div, 0);
        assert_eq!(parsed.measures[0].end_div, 6);
    }

    #[test]
    fn last_measure_extends_to_longest_part_span_when_other_part_has_only_rests() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<score-partwise version="3.1">
  <part-list>
    <score-part id="P1"><part-name>Upper</part-name></score-part>
    <score-part id="P2"><part-name>Lower</part-name></score-part>
  </part-list>
  <part id="P1">
    <measure number="1">
      <attributes><divisions>4</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes>
      <note><pitch><step>C</step><octave>4</octave></pitch><duration>16</duration></note>
    </measure>
  </part>
  <part id="P2">
    <measure number="1">
      <attributes><divisions>4</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes>
      <note><rest/><duration>24</duration></note>
    </measure>
  </part>
</score-partwise>"#;

        let parsed = parse_musicxml(xml).expect("parse");
        assert_eq!(parsed.measures.len(), 1);
        assert_eq!(parsed.measures[0].end_div, 6);
    }

    #[test]
    fn short_final_measure_extends_to_nominal_bar_length() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<score-partwise version="3.1">
  <part-list><score-part id="P1"><part-name>Music</part-name></score-part></part-list>
  <part id="P1">
    <measure number="1">
      <attributes><divisions>4</divisions><time><beats>3</beats><beat-type>4</beat-type></time></attributes>
      <note><pitch><step>C</step><octave>4</octave></pitch><duration>12</duration></note>
    </measure>
    <measure number="2">
      <note><pitch><step>D</step><octave>4</octave></pitch><duration>6</duration></note>
    </measure>
  </part>
</score-partwise>"#;

        let parsed = parse_musicxml(xml).expect("parse");
        assert_eq!(parsed.measures.len(), 2);
        assert_eq!(parsed.measures[1].start_div, 6);
        assert_eq!(parsed.measures[1].end_div, 12);
    }
}
