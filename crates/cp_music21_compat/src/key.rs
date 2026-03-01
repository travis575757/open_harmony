use serde::{Deserialize, Serialize};

use crate::error::CompatError;
use crate::interval::parse_interval_spec;
use crate::pitch::PitchSpelling;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyMode {
    Major,
    Minor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyContext {
    pub tonic: PitchSpelling,
    pub mode: KeyMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyName {
    pub tonic: PitchSpelling,
    pub mode: KeyMode,
}

const WEBER_DIAGONAL: [&str; 40] = [
    "B--", "c-", "F-", "g-", "C-", "d-", "G-", "a-", "D-", "e-", "A-", "b-", "E-", "f", "B-", "c",
    "F", "g", "C", "d", "G", "a", "D", "e", "A", "b", "E", "f#", "B", "c#", "F#", "g#", "C#", "d#",
    "G#", "a#", "D#", "e#", "A#", "b#",
];

fn mode_scale(mode: KeyMode) -> [i32; 7] {
    match mode {
        KeyMode::Major => [0, 2, 4, 5, 7, 9, 11],
        KeyMode::Minor => [0, 2, 3, 5, 7, 8, 10],
    }
}

fn step_char(step: i32, mode: KeyMode) -> char {
    let upper = match step.rem_euclid(7) {
        0 => 'C',
        1 => 'D',
        2 => 'E',
        3 => 'F',
        4 => 'G',
        5 => 'A',
        _ => 'B',
    };
    match mode {
        KeyMode::Major => upper,
        KeyMode::Minor => upper.to_ascii_lowercase(),
    }
}

fn base_pc_for_step(step: i32) -> i32 {
    match step.rem_euclid(7) {
        0 => 0,
        1 => 2,
        2 => 4,
        3 => 5,
        4 => 7,
        5 => 9,
        _ => 11,
    }
}

fn accidental_prefix(delta: i32) -> String {
    if delta > 0 {
        "#".repeat(delta as usize)
    } else if delta < 0 {
        "b".repeat((-delta) as usize)
    } else {
        String::new()
    }
}

fn to_signed_small_interval(mut delta: i32) -> i32 {
    while delta > 6 {
        delta -= 12;
    }
    while delta < -6 {
        delta += 12;
    }
    delta
}

fn parse_key_name(input: &str) -> Result<KeyName, CompatError> {
    let trimmed = input.trim();
    let first = trimmed.chars().next().ok_or(CompatError::InvalidValue {
        field: "key",
        value: input.to_string(),
    })?;
    if !matches!(first.to_ascii_uppercase(), 'A'..='G') {
        return Err(CompatError::InvalidValue {
            field: "key",
            value: input.to_string(),
        });
    }

    let mode = if first.is_ascii_lowercase() {
        KeyMode::Minor
    } else {
        KeyMode::Major
    };
    let step = first.to_ascii_uppercase().to_string();
    let accidental_part = &trimmed[1..];
    let mut alter = 0i8;
    for ch in accidental_part.chars() {
        match ch {
            '#' => alter += 1,
            '-' | 'b' => alter -= 1,
            _ => {
                return Err(CompatError::InvalidValue {
                    field: "key",
                    value: input.to_string(),
                })
            }
        }
    }

    Ok(KeyName {
        tonic: PitchSpelling::parse(&step, Some(&alter.to_string()), "4")?,
        mode,
    })
}

fn format_key_name(k: &KeyName) -> String {
    let letter = step_char(k.tonic.step_index(), k.mode);
    let mut out = letter.to_string();
    if k.tonic.alter > 0 {
        out.push_str(&"#".repeat(k.tonic.alter as usize));
    } else if k.tonic.alter < 0 {
        out.push_str(&"-".repeat((-k.tonic.alter) as usize));
    }
    out
}

fn transpose_pitch_spelling(
    pitch: &PitchSpelling,
    interval: &str,
) -> Result<PitchSpelling, CompatError> {
    let spec = parse_interval_spec(interval)?;
    let diatonic_steps = i32::from(spec.generic_number) - 1;

    let source_step = pitch.step_index();
    let target_step = source_step + diatonic_steps;

    let source_pc = pitch.pitch_class() as i32;
    let target_pc = (source_pc + i32::from(spec.semitones)).rem_euclid(12);

    let target_octave = pitch.octave as i32 + target_step.div_euclid(7);
    let target_step_mod = target_step.rem_euclid(7);
    let natural_pc = base_pc_for_step(target_step_mod);
    let mut alter = target_pc - natural_pc;
    while alter > 6 {
        alter -= 12;
    }
    while alter < -6 {
        alter += 12;
    }

    let step = match target_step_mod {
        0 => crate::pitch::Step::C,
        1 => crate::pitch::Step::D,
        2 => crate::pitch::Step::E,
        3 => crate::pitch::Step::F,
        4 => crate::pitch::Step::G,
        5 => crate::pitch::Step::A,
        _ => crate::pitch::Step::B,
    };

    Ok(PitchSpelling {
        step,
        alter: alter as i8,
        octave: target_octave as i8,
    })
}

/// Converts a pitch spelling into its tonic-relative chromatic class.
///
/// Maps to measuring pitch classes in `music21.key.Key` context.
pub fn tonic_relative_pc(pitch: &PitchSpelling, key: &KeyContext) -> u8 {
    (pitch.midi() - key.tonic.midi()).rem_euclid(12) as u8
}

/// Returns denominator label used by tonicization representations.
///
/// Maps to AugmentedNet's tonicization denominator extraction concept.
pub fn tonicization_denominator(pitch: &PitchSpelling, key: &KeyContext) -> String {
    const ROMAN: [&str; 7] = ["I", "II", "III", "IV", "V", "VI", "VII"];

    let degree = (pitch.step_index() - key.tonic.step_index()).rem_euclid(7) as usize;
    let expected = mode_scale(key.mode)[degree];
    let actual = tonic_relative_pc(pitch, key) as i32;
    let accidental = to_signed_small_interval(actual - expected);

    let prefix = accidental_prefix(accidental);
    format!("{}{}", prefix, ROMAN[degree])
}

/// Equivalent to AugmentedNet cache `TransposePitch`.
pub fn transpose_pitch_m21(pitch: &str, interval: &str) -> Result<String, CompatError> {
    let (parsed, has_octave) = PitchSpelling::parse_m21_pitch_name(pitch)?;
    let transposed = transpose_pitch_spelling(&parsed, interval)?;
    Ok(if has_octave {
        transposed.m21_name_with_octave()
    } else {
        transposed.m21_name()
    })
}

/// Equivalent to AugmentedNet cache `TransposeKey`.
pub fn transpose_key_m21(key: &str, interval: &str) -> Result<String, CompatError> {
    let parsed = parse_key_name(key)?;
    let tonic = transpose_pitch_spelling(&parsed.tonic, interval)?;
    Ok(format_key_name(&KeyName {
        tonic,
        mode: parsed.mode,
    }))
}

/// Equivalent to AugmentedNet cache `TransposePcSet`.
pub fn transpose_pcset(pcset: &[u8], interval: &str) -> Result<Vec<u8>, CompatError> {
    let semitones = i32::from(parse_interval_spec(interval)?.semitones);
    let mut out: Vec<u8> = pcset
        .iter()
        .map(|x| ((*x as i32 + semitones).rem_euclid(12)) as u8)
        .collect();
    out.sort_unstable();
    out.dedup();
    Ok(out)
}

/// Equivalent to AugmentedNet `keydistance.weberEuclidean`.
pub fn weber_euclidean(k1: &str, k2: &str) -> Result<f64, CompatError> {
    let i1 = WEBER_DIAGONAL
        .iter()
        .position(|k| *k == k1)
        .ok_or(CompatError::InvalidValue {
            field: "key",
            value: k1.to_string(),
        })? as i32;
    let i2 = WEBER_DIAGONAL
        .iter()
        .position(|k| *k == k2)
        .ok_or(CompatError::InvalidValue {
            field: "key",
            value: k2.to_string(),
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

/// Equivalent to AugmentedNet `keydistance.getTonicizationScaleDegree`.
pub fn tonicization_scale_degree(
    local_key: &str,
    tonicized_key: &str,
) -> Result<String, CompatError> {
    let local = parse_key_name(local_key)?;
    let tonicized = parse_key_name(tonicized_key)?;
    let context = KeyContext {
        tonic: local.tonic,
        mode: local.mode,
    };

    let mut degree = tonicization_denominator(&tonicized.tonic, &context);
    if tonicized.mode == KeyMode::Minor {
        let prefix_len = degree
            .chars()
            .take_while(|c| *c == '#' || *c == 'b')
            .count();
        let (prefix, roman) = degree.split_at(prefix_len);
        degree = format!("{}{}", prefix, roman.to_ascii_lowercase());
    }

    if matches!(local.mode, KeyMode::Minor) && degree == "bVI" {
        degree = "VI".to_string();
    }
    Ok(degree)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pitch::{PitchSpelling, Step};

    #[test]
    fn tonic_relative_pc_tracks_key_context() {
        let key = KeyContext {
            tonic: PitchSpelling {
                step: Step::C,
                alter: 0,
                octave: 4,
            },
            mode: KeyMode::Major,
        };
        let a = PitchSpelling {
            step: Step::A,
            alter: 0,
            octave: 4,
        };
        assert_eq!(tonic_relative_pc(&a, &key), 9);
    }

    #[test]
    fn tonicization_denominator_handles_accidentals() {
        let key = KeyContext {
            tonic: PitchSpelling {
                step: Step::C,
                alter: 0,
                octave: 4,
            },
            mode: KeyMode::Major,
        };
        let fs = PitchSpelling {
            step: Step::F,
            alter: 1,
            octave: 4,
        };
        let bb = PitchSpelling {
            step: Step::B,
            alter: -1,
            octave: 4,
        };
        assert_eq!(tonicization_denominator(&fs, &key), "#IV");
        assert_eq!(tonicization_denominator(&bb, &key), "bVII");
    }

    #[test]
    fn tonicization_denominator_minor_mode_uses_minor_scale() {
        let key = KeyContext {
            tonic: PitchSpelling {
                step: Step::A,
                alter: 0,
                octave: 4,
            },
            mode: KeyMode::Minor,
        };
        let f = PitchSpelling {
            step: Step::F,
            alter: 0,
            octave: 4,
        };
        assert_eq!(tonicization_denominator(&f, &key), "VI");
    }

    #[test]
    fn transpose_pitch_matches_augnet_style_output() {
        assert_eq!(transpose_pitch_m21("B-3", "m2").expect("transpose"), "C-4");
        assert_eq!(transpose_pitch_m21("A", "M2").expect("transpose"), "B");
    }

    #[test]
    fn transpose_key_preserves_mode_case() {
        assert_eq!(transpose_key_m21("a", "m2").expect("transpose"), "b-");
        assert_eq!(transpose_key_m21("C", "P5").expect("transpose"), "G");
    }

    #[test]
    fn transpose_pcset_is_sorted_and_wrapped() {
        let got = transpose_pcset(&[0, 4, 9], "m2").expect("transpose");
        assert_eq!(got, vec![1, 5, 10]);
    }

    #[test]
    fn weber_distance_examples_match_augnet_reference() {
        assert_eq!(
            weber_euclidean("C", "G").expect("distance").round() as i32,
            1
        );
        assert_eq!(
            weber_euclidean("C", "f#").expect("distance").round() as i32,
            3
        );
    }

    #[test]
    fn tonicization_scale_degree_matches_case_behavior() {
        assert_eq!(tonicization_scale_degree("a", "C").expect("degree"), "III");
        assert_eq!(tonicization_scale_degree("C", "e").expect("degree"), "iii");
    }

    #[test]
    fn weber_distance_vector_matches_augmentednet_reference() {
        let gt = vec![
            ("C", 0.0),
            ("G", 1.0),
            ("F", 1.0),
            ("c", 1.0),
            ("a", 1.0),
            ("g", 1.41),
            ("e", 1.41),
            ("d", 1.41),
            ("f", 1.41),
            ("D", 2.0),
            ("E-", 2.0),
            ("B-", 2.0),
            ("A", 2.0),
            ("b", 2.24),
            ("E", 2.24),
            ("A-", 2.24),
            ("b-", 2.24),
            ("D-", 2.83),
            ("B", 2.83),
            ("e-", 3.0),
            ("f#", 3.0),
            ("c#", 3.16),
            ("a-", 3.16),
            ("d-", 3.61),
            ("G-", 3.61),
            ("F#", 3.61),
            ("g#", 3.61),
            ("C-", 4.12),
            ("C#", 4.12),
            ("d#", 4.24),
            ("G#", 4.47),
            ("F-", 4.47),
            ("a#", 5.0),
            ("e#", 5.39),
        ];
        let got: Vec<f64> = gt
            .iter()
            .map(|(k, _)| (weber_euclidean("C", k).expect("distance") * 100.0).round() / 100.0)
            .collect();
        let exp: Vec<f64> = gt.iter().map(|(_, d)| *d).collect();
        assert_eq!(got, exp);
    }
}
