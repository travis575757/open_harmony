use crate::error::CompatError;
use crate::pitch::PitchSpelling;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntervalSpec {
    pub generic_number: u8,
    pub semitones: i16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntervalClassInfo {
    pub semitones: i16,
    pub chromatic_mod12: u8,
    pub generic_simple_undirected: u8,
}

fn base_major_or_perfect_semitones(simple_number: i32) -> i32 {
    match simple_number {
        1 => 0,
        2 => 2,
        3 => 4,
        4 => 5,
        5 => 7,
        6 => 9,
        7 => 11,
        _ => unreachable!("simple number must be in 1..=7"),
    }
}

fn is_perfect_class(simple_number: i32) -> bool {
    matches!(simple_number, 1 | 4 | 5)
}

fn quality_delta(simple_number: i32, quality: &str) -> Result<i32, CompatError> {
    if is_perfect_class(simple_number) {
        return match quality {
            "P" => Ok(0),
            q if q.chars().all(|c| c == 'A') => Ok(q.len() as i32),
            q if q.chars().all(|c| c == 'd' || c == 'D') => Ok(-(q.len() as i32)),
            _ => Err(CompatError::InvalidValue {
                field: "interval.quality",
                value: quality.to_string(),
            }),
        };
    }

    match quality {
        "M" => Ok(0),
        "m" => Ok(-1),
        q if q.chars().all(|c| c == 'A') => Ok(q.len() as i32),
        q if q.chars().all(|c| c == 'd' || c == 'D') => Ok(-(q.len() as i32) - 1),
        _ => Err(CompatError::InvalidValue {
            field: "interval.quality",
            value: quality.to_string(),
        }),
    }
}

fn quality_label(simple_number: i32, delta: i32) -> String {
    if is_perfect_class(simple_number) {
        if delta == 0 {
            return "P".to_string();
        }
        if delta > 0 {
            return "A".repeat(delta as usize);
        }
        return "d".repeat((-delta) as usize);
    }

    if delta == 0 {
        return "M".to_string();
    }
    if delta == -1 {
        return "m".to_string();
    }
    if delta > 0 {
        return "A".repeat(delta as usize);
    }
    "d".repeat((-delta - 1) as usize)
}

fn split_quality_and_number(interval: &str) -> Result<(&str, i32), CompatError> {
    let trimmed = interval.trim();
    let split_at = trimmed
        .find(|c: char| c.is_ascii_digit())
        .ok_or(CompatError::InvalidValue {
            field: "interval",
            value: interval.to_string(),
        })?;
    let (quality, number_str) = trimmed.split_at(split_at);
    let number = number_str
        .parse::<i32>()
        .map_err(|_| CompatError::InvalidValue {
            field: "interval.number",
            value: number_str.to_string(),
        })?;
    if number <= 0 {
        return Err(CompatError::InvalidValue {
            field: "interval.number",
            value: number_str.to_string(),
        });
    }
    Ok((quality, number))
}

/// Parses a music21-style interval name (e.g. `m3`, `AA4`, `d7`).
pub fn parse_interval_spec(interval: &str) -> Result<IntervalSpec, CompatError> {
    let (quality, number) = split_quality_and_number(interval)?;
    let simple_number = ((number - 1) % 7) + 1;
    let octaves = (number - 1) / 7;
    let delta = quality_delta(simple_number, quality)?;
    let semitones = base_major_or_perfect_semitones(simple_number) + octaves * 12 + delta;
    Ok(IntervalSpec {
        generic_number: number as u8,
        semitones: semitones as i16,
    })
}

/// Equivalent to `music21.interval.Interval(x)` fields used by AugmentedNet.
pub fn interval_class_info(interval: &str) -> Result<IntervalClassInfo, CompatError> {
    let spec = parse_interval_spec(interval)?;
    let simple_generic = ((i32::from(spec.generic_number) - 1) % 7) + 1;
    Ok(IntervalClassInfo {
        semitones: spec.semitones,
        chromatic_mod12: spec.semitones.rem_euclid(12) as u8,
        generic_simple_undirected: simple_generic as u8,
    })
}

/// Spelling-sensitive interval naming for compatibility with music21 interval labels.
///
/// Maps to `music21.interval.Interval(note1, note2).name`.
pub fn interval_label(lower: &PitchSpelling, upper: &PitchSpelling) -> String {
    let diatonic_steps =
        (upper.octave as i32 - lower.octave as i32) * 7 + (upper.step_index() - lower.step_index());
    let number = diatonic_steps.abs() + 1;
    let simple_number = ((number - 1) % 7) + 1;
    let octaves = (number - 1) / 7;

    let semitones = (upper.midi() - lower.midi()) as i32;
    let semitones_abs = semitones.abs();
    let expected = base_major_or_perfect_semitones(simple_number) + octaves * 12;
    let delta = semitones_abs - expected;

    let quality = quality_label(simple_number, delta);
    let prefix = if semitones < 0 { "-" } else { "" };
    format!("{}{}{}", prefix, quality, number)
}

/// Equivalent to `music21.interval.Interval(p1, p2).simpleName`.
pub fn simple_interval_name(lower: &PitchSpelling, upper: &PitchSpelling) -> String {
    let diatonic_steps =
        (upper.octave as i32 - lower.octave as i32) * 7 + (upper.step_index() - lower.step_index());
    let number = diatonic_steps.abs() + 1;
    let simple_number = ((number - 1) % 7) + 1;

    let semitones_abs = (upper.midi() - lower.midi()).abs() as i32;
    let semitones_simple = semitones_abs.rem_euclid(12);
    let expected_simple = base_major_or_perfect_semitones(simple_number);
    let mut delta = semitones_simple - expected_simple;
    while delta > 6 {
        delta -= 12;
    }
    while delta < -6 {
        delta += 12;
    }

    let quality = quality_label(simple_number, delta);
    format!("{}{}", quality, simple_number)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pitch::{PitchSpelling, Step};

    #[test]
    fn interval_label_is_spelling_sensitive_for_enharmonics() {
        let c4 = PitchSpelling {
            step: Step::C,
            alter: 0,
            octave: 4,
        };
        let fs4 = PitchSpelling {
            step: Step::F,
            alter: 1,
            octave: 4,
        };
        let gb4 = PitchSpelling {
            step: Step::G,
            alter: -1,
            octave: 4,
        };

        assert_eq!(interval_label(&c4, &fs4), "A4");
        assert_eq!(interval_label(&c4, &gb4), "d5");
    }

    #[test]
    fn interval_label_supports_double_augmented_and_double_diminished() {
        let c4 = PitchSpelling {
            step: Step::C,
            alter: 0,
            octave: 4,
        };
        let ex4 = PitchSpelling {
            step: Step::E,
            alter: 2,
            octave: 4,
        };
        let fbb4 = PitchSpelling {
            step: Step::F,
            alter: -2,
            octave: 4,
        };
        assert_eq!(interval_label(&c4, &ex4), "AA3");
        assert_eq!(interval_label(&c4, &fbb4), "dd4");
    }

    #[test]
    fn simple_interval_name_reduces_compounds_like_music21() {
        let c4 = PitchSpelling {
            step: Step::C,
            alter: 0,
            octave: 4,
        };
        let b5 = PitchSpelling {
            step: Step::B,
            alter: 0,
            octave: 5,
        };
        assert_eq!(simple_interval_name(&c4, &b5), "M7");
    }

    #[test]
    fn interval_class_info_matches_augnet_usage() {
        let info = interval_class_info("d5").expect("parse");
        assert_eq!(info.semitones, 6);
        assert_eq!(info.chromatic_mod12, 6);
        assert_eq!(info.generic_simple_undirected, 5);
    }

    #[test]
    fn parse_interval_spec_supports_uppercase_diminished_alias() {
        let info = parse_interval_spec("D3").expect("parse");
        assert_eq!(info.semitones, 2);
    }

    #[test]
    fn parses_full_augmentednet_interval_class_set() {
        let classes = [
            "dd2", "d2", "m2", "M2", "A2", "AA2", "dd3", "d3", "m3", "M3", "A3", "AA3", "dd6",
            "d6", "m6", "M6", "A6", "AA6", "dd7", "d7", "m7", "M7", "A7", "AA7", "dd1", "d1", "P1",
            "A1", "AA1", "dd4", "d4", "P4", "A4", "AA4", "dd5", "d5", "P5", "A5", "AA5",
        ];
        for i in classes {
            parse_interval_spec(i).expect("parse interval class");
            interval_class_info(i).expect("class info");
        }
    }
}
