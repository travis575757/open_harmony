use serde::{Deserialize, Serialize};

use crate::error::CompatError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Step {
    C,
    D,
    E,
    F,
    G,
    A,
    B,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PitchSpelling {
    pub step: Step,
    pub alter: i8,
    pub octave: i8,
}

impl PitchSpelling {
    pub fn parse(step: &str, alter: Option<&str>, octave: &str) -> Result<Self, CompatError> {
        let step = match step {
            "C" => Step::C,
            "D" => Step::D,
            "E" => Step::E,
            "F" => Step::F,
            "G" => Step::G,
            "A" => Step::A,
            "B" => Step::B,
            other => {
                return Err(CompatError::InvalidValue {
                    field: "pitch.step",
                    value: other.to_string(),
                })
            }
        };
        let alter = alter
            .unwrap_or("0")
            .parse::<i8>()
            .map_err(|_| CompatError::InvalidValue {
                field: "pitch.alter",
                value: alter.unwrap_or("<none>").to_string(),
            })?;
        let octave = octave
            .parse::<i8>()
            .map_err(|_| CompatError::InvalidValue {
                field: "pitch.octave",
                value: octave.to_string(),
            })?;

        Ok(Self {
            step,
            alter,
            octave,
        })
    }

    pub fn midi(&self) -> i16 {
        let base = match self.step {
            Step::C => 0,
            Step::D => 2,
            Step::E => 4,
            Step::F => 5,
            Step::G => 7,
            Step::A => 9,
            Step::B => 11,
        };
        (base + self.alter as i16) + (self.octave as i16 + 1) * 12
    }

    pub fn pitch_class(&self) -> u8 {
        self.midi().rem_euclid(12) as u8
    }

    pub fn spelling(&self) -> String {
        let mut s = match self.step {
            Step::C => "C",
            Step::D => "D",
            Step::E => "E",
            Step::F => "F",
            Step::G => "G",
            Step::A => "A",
            Step::B => "B",
        }
        .to_string();

        if self.alter > 0 {
            s.push_str(&"#".repeat(self.alter as usize));
        } else if self.alter < 0 {
            s.push_str(&"b".repeat((-self.alter) as usize));
        }
        s.push_str(&self.octave.to_string());
        s
    }

    pub fn m21_name(&self) -> String {
        let mut s = match self.step {
            Step::C => "C",
            Step::D => "D",
            Step::E => "E",
            Step::F => "F",
            Step::G => "G",
            Step::A => "A",
            Step::B => "B",
        }
        .to_string();
        if self.alter > 0 {
            s.push_str(&"#".repeat(self.alter as usize));
        } else if self.alter < 0 {
            s.push_str(&"-".repeat((-self.alter) as usize));
        }
        s
    }

    pub fn m21_name_with_octave(&self) -> String {
        format!("{}{}", self.m21_name(), self.octave)
    }

    pub fn parse_m21_pitch_name(input: &str) -> Result<(Self, bool), CompatError> {
        let trimmed = input.trim();
        let first = trimmed.chars().next().ok_or(CompatError::InvalidValue {
            field: "pitch.name",
            value: input.to_string(),
        })?;
        let step = match first.to_ascii_uppercase() {
            'C' => Step::C,
            'D' => Step::D,
            'E' => Step::E,
            'F' => Step::F,
            'G' => Step::G,
            'A' => Step::A,
            'B' => Step::B,
            _ => {
                return Err(CompatError::InvalidValue {
                    field: "pitch.step",
                    value: input.to_string(),
                })
            }
        };

        let chars: Vec<char> = trimmed.chars().collect();
        let mut i = 1usize;
        let mut alter = 0i8;
        while i < chars.len() {
            match chars[i] {
                '#' => {
                    alter += 1;
                    i += 1;
                }
                '-' | 'b' => {
                    alter -= 1;
                    i += 1;
                }
                _ => break,
            }
        }

        let octave_str: String = chars[i..].iter().collect();
        let has_octave = !octave_str.is_empty();
        let octave = if has_octave {
            octave_str
                .parse::<i8>()
                .map_err(|_| CompatError::InvalidValue {
                    field: "pitch.octave",
                    value: octave_str.clone(),
                })?
        } else {
            // music21 defaults to octave 4 when omitted for transposition behavior.
            4
        };

        Ok((
            Self {
                step,
                alter,
                octave,
            },
            has_octave,
        ))
    }

    pub(crate) fn step_index(&self) -> i32 {
        match self.step {
            Step::C => 0,
            Step::D => 1,
            Step::E => 2,
            Step::F => 3,
            Step::G => 4,
            Step::A => 5,
            Step::B => 6,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pitch_to_midi_and_spelling_handles_double_accidental() {
        let p = PitchSpelling {
            step: Step::F,
            alter: 2,
            octave: 4,
        };
        assert_eq!(p.spelling(), "F##4");
        assert_eq!(p.midi(), 67);
        assert_eq!(p.pitch_class(), 7);
    }

    #[test]
    fn parse_rejects_unknown_step() {
        let err = PitchSpelling::parse("H", None, "4").expect_err("must fail");
        assert!(matches!(
            err,
            CompatError::InvalidValue {
                field: "pitch.step",
                ..
            }
        ));
    }

    #[test]
    fn parse_m21_pitch_name_round_trip_hyphen_flats() {
        let (p, has_octave) = PitchSpelling::parse_m21_pitch_name("B--3").expect("parse");
        assert!(has_octave);
        assert_eq!(p.m21_name_with_octave(), "B--3");
        assert_eq!(p.midi(), 57);
    }

    #[test]
    fn parse_m21_pitch_name_without_octave_defaults_to_4() {
        let (p, has_octave) = PitchSpelling::parse_m21_pitch_name("F#").expect("parse");
        assert!(!has_octave);
        assert_eq!(p.m21_name(), "F#");
        assert_eq!(p.octave, 4);
    }
}
