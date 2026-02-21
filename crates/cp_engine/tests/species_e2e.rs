use cp_core::{
    AnalysisConfig, AnalysisRequest, HarmonicRhythm, KeySignature, NormalizedScore, NoteEvent,
    PresetId, ScaleMode, ScoreMeta, TimeSignature, Voice,
};
use cp_engine::{analyze, resolve_preset};
use std::collections::{BTreeMap, BTreeSet, HashSet};

#[derive(Clone)]
struct Case {
    name: &'static str,
    score: NormalizedScore,
    keep_rules: Vec<&'static str>,
    expected_rule_ids: Vec<&'static str>,
}

fn mk_two_voice_score(
    cp: &[(i16, u32, bool, bool)],
    cf: &[(i16, u32, bool, bool)],
    numerator: u8,
    denominator: u8,
) -> NormalizedScore {
    fn mk_voice(voice_index: u8, name: &str, spec: &[(i16, u32, bool, bool)]) -> Voice {
        let mut tick = 0u32;
        let mut notes = Vec::new();
        for (i, (midi, dur, tie_start, tie_end)) in spec.iter().copied().enumerate() {
            notes.push(NoteEvent {
                note_id: format!("{}_{}", name, i),
                voice_index,
                midi,
                start_tick: tick,
                duration_ticks: dur,
                tie_start,
                tie_end,
            });
            tick += dur;
        }
        Voice {
            voice_index,
            name: name.to_string(),
            notes,
        }
    }

    NormalizedScore {
        meta: ScoreMeta {
            exercise_count: 1,
            key_signature: KeySignature {
                tonic_pc: 0,
                mode: ScaleMode::Major,
            },
            time_signature: TimeSignature {
                numerator,
                denominator,
            },
            ticks_per_quarter: 480,
        },
        voices: vec![mk_voice(0, "cp", cp), mk_voice(1, "cf", cf)],
    }
}

fn mk_request(score: NormalizedScore, preset_id: PresetId, keep_rules: &[&str]) -> AnalysisRequest {
    let mut req = AnalysisRequest {
        score,
        config: AnalysisConfig {
            preset_id,
            enabled_rule_ids: Vec::new(),
            disabled_rule_ids: Vec::new(),
            severity_overrides: BTreeMap::new(),
            rule_params: BTreeMap::new(),
            harmonic_rhythm: HarmonicRhythm::NoteOnset,
        },
    };

    if !keep_rules.is_empty() {
        let keep: HashSet<&str> = keep_rules.iter().copied().collect();
        let resolved = resolve_preset(&req).expect("resolve preset");
        req.config.disabled_rule_ids = resolved
            .active_rules
            .iter()
            .filter(|rid| !keep.contains(rid.as_str()))
            .cloned()
            .collect();
    }

    req
}

fn diagnostic_rule_id_set(req: AnalysisRequest) -> BTreeSet<String> {
    let res = analyze(&req).expect("analyze");
    res.diagnostics.iter().map(|d| d.rule_id.clone()).collect()
}

fn run_cases(preset_id: PresetId, cases: Vec<Case>) {
    for c in cases {
        let req = mk_request(c.score, preset_id.clone(), &c.keep_rules);
        let got = diagnostic_rule_id_set(req);
        let expected: BTreeSet<String> =
            c.expected_rule_ids.iter().map(|x| x.to_string()).collect();
        assert_eq!(got, expected, "case={}", c.name);
    }
}

#[test]
fn species1_examples_expected_errors() {
    run_cases(
        PresetId::Species1,
        vec![
            Case {
                name: "vertical-dissonance",
                score: mk_two_voice_score(
                    &[(63, 1920, false, false)],
                    &[(53, 1920, false, false)],
                    4,
                    4,
                ),
                keep_rules: vec!["sp1.vertical.consonance_only"],
                expected_rule_ids: vec!["sp1.vertical.consonance_only"],
            },
            Case {
                name: "opening-not-perfect",
                score: mk_two_voice_score(
                    &[(64, 1920, false, false), (65, 1920, false, false)],
                    &[(53, 1920, false, false), (55, 1920, false, false)],
                    4,
                    4,
                ),
                keep_rules: vec!["sp1.opening.perfect_consonance_required"],
                expected_rule_ids: vec!["sp1.opening.perfect_consonance_required"],
            },
            Case {
                name: "ending-not-unison-or-octave",
                score: mk_two_voice_score(
                    &[(60, 1920, false, false), (62, 1920, false, false)],
                    &[(53, 1920, false, false), (55, 1920, false, false)],
                    4,
                    4,
                ),
                keep_rules: vec!["sp1.ending.unison_or_octave_required"],
                expected_rule_ids: vec!["sp1.ending.unison_or_octave_required"],
            },
            Case {
                name: "repeated-melodic-note-forbidden",
                score: mk_two_voice_score(
                    &[(60, 1920, false, false), (60, 1920, false, false)],
                    &[(53, 1920, false, false), (55, 1920, false, false)],
                    4,
                    4,
                ),
                keep_rules: vec!["gen.melody.repeated_pitch_species_profiled"],
                expected_rule_ids: vec!["gen.melody.repeated_pitch_species_profiled"],
            },
            Case {
                name: "opening-position-invalid-below",
                score: mk_two_voice_score(
                    &[(55, 1920, false, false), (57, 1920, false, false)],
                    &[(60, 1920, false, false), (62, 1920, false, false)],
                    4,
                    4,
                ),
                keep_rules: vec!["gen.opening.interval_by_position_species_profiled"],
                expected_rule_ids: vec!["gen.opening.interval_by_position_species_profiled"],
            },
            Case {
                name: "clausula-vera-invalid",
                score: mk_two_voice_score(
                    &[(69, 1920, false, false), (72, 1920, false, false)],
                    &[(62, 1920, false, false), (60, 1920, false, false)],
                    4,
                    4,
                ),
                keep_rules: vec!["gen.cadence.clausula_vera_required"],
                expected_rule_ids: vec!["gen.cadence.clausula_vera_required"],
            },
        ],
    );
}

#[test]
fn species2_examples_expected_errors() {
    run_cases(
        PresetId::Species2,
        vec![
            Case {
                name: "valid-weak-dissonant-passing",
                score: mk_two_voice_score(
                    &[
                        (62, 960, false, false),
                        (63, 960, false, false),
                        (64, 960, false, false),
                    ],
                    &[(53, 1920, false, false), (55, 1920, false, false)],
                    4,
                    4,
                ),
                keep_rules: vec!["sp2.dissonance.weak_passing_stepwise"],
                expected_rule_ids: vec![],
            },
            Case {
                name: "invalid-weak-dissonant-leap",
                score: mk_two_voice_score(
                    &[
                        (60, 960, false, false),
                        (66, 960, false, false),
                        (64, 960, false, false),
                    ],
                    &[(53, 1920, false, false), (53, 1920, false, false)],
                    4,
                    4,
                ),
                keep_rules: vec!["sp2.dissonance.weak_passing_stepwise"],
                expected_rule_ids: vec!["sp2.dissonance.weak_passing_stepwise"],
            },
            Case {
                name: "invalid-weak-dissonant-direction-change",
                score: mk_two_voice_score(
                    &[
                        (62, 960, false, false),
                        (63, 960, false, false),
                        (62, 960, false, false),
                    ],
                    &[(53, 1920, false, false), (55, 1920, false, false)],
                    4,
                    4,
                ),
                keep_rules: vec!["sp2.dissonance.weak_passing_stepwise"],
                expected_rule_ids: vec!["sp2.dissonance.weak_passing_stepwise"],
            },
            Case {
                name: "invalid-downbeat-dissonance",
                score: mk_two_voice_score(
                    &[(64, 960, false, false), (65, 960, false, false)],
                    &[(53, 1920, false, false)],
                    4,
                    4,
                ),
                keep_rules: vec!["sp2.strong_beat.consonance_required"],
                expected_rule_ids: vec!["sp2.strong_beat.consonance_required"],
            },
            Case {
                name: "downbeat-skeleton-parallel-perfects",
                score: mk_two_voice_score(
                    &[
                        (60, 960, false, false),
                        (61, 960, false, false),
                        (62, 960, false, false),
                        (63, 960, false, false),
                    ],
                    &[(53, 1920, false, false), (55, 1920, false, false)],
                    4,
                    4,
                ),
                keep_rules: vec!["sp2.structure.downbeat_skeleton_no_parallel_perfects"],
                expected_rule_ids: vec!["sp2.structure.downbeat_skeleton_no_parallel_perfects"],
            },
            Case {
                name: "repeated-melodic-note-discouraged",
                score: mk_two_voice_score(
                    &[
                        (60, 960, false, false),
                        (60, 960, false, false),
                        (62, 960, false, false),
                        (64, 960, false, false),
                    ],
                    &[(53, 1920, false, false), (55, 1920, false, false)],
                    4,
                    4,
                ),
                keep_rules: vec!["gen.melody.repeated_pitch_species_profiled"],
                expected_rule_ids: vec!["gen.melody.repeated_pitch_species_profiled"],
            },
            Case {
                name: "downbeat-interval-repetition-limits",
                score: mk_two_voice_score(
                    &[
                        (60, 960, false, false),
                        (62, 960, false, false),
                        (62, 960, false, false),
                        (64, 960, false, false),
                    ],
                    &[(53, 1920, false, false), (55, 1920, false, false)],
                    4,
                    4,
                ),
                keep_rules: vec!["sp2.structure.downbeat_interval_repetition_limits"],
                expected_rule_ids: vec!["sp2.structure.downbeat_interval_repetition_limits"],
            },
            Case {
                name: "weak-beat-consonant-pattern-catalog-warning",
                score: mk_two_voice_score(
                    &[
                        (60, 960, false, false),
                        (68, 960, false, false),
                        (63, 960, false, false),
                    ],
                    &[(53, 1920, false, false), (55, 1920, false, false)],
                    4,
                    4,
                ),
                keep_rules: vec!["sp2.weak_beat.consonant_pattern_catalog"],
                expected_rule_ids: vec!["sp2.weak_beat.consonant_pattern_catalog"],
            },
            Case {
                name: "user-example-no-consecutive-imperfect-run",
                score: mk_two_voice_score(
                    &[
                        (65, 960, false, false),
                        (67, 960, false, false),
                        (69, 960, false, false),
                        (65, 960, false, false),
                        (67, 960, false, false),
                        (64, 960, false, false),
                        (65, 960, false, false),
                        (69, 960, false, false),
                        (71, 960, false, false),
                        (67, 960, false, false),
                        (69, 960, false, false),
                        (67, 960, false, false),
                        (65, 960, false, false),
                        (69, 960, false, false),
                        (71, 960, false, false),
                        (67, 960, false, false),
                        (60, 960, false, false),
                        (62, 960, false, false),
                        (64, 960, false, false),
                        (61, 960, false, false),
                        (62, 960, false, false),
                        (62, 960, false, false),
                    ],
                    &[
                        (50, 1920, false, false),
                        (53, 1920, false, false),
                        (52, 1920, false, false),
                        (50, 1920, false, false),
                        (55, 1920, false, false),
                        (53, 1920, false, false),
                        (57, 1920, false, false),
                        (55, 1920, false, false),
                        (53, 1920, false, false),
                        (52, 1920, false, false),
                        (50, 1920, false, false),
                    ],
                    4,
                    4,
                ),
                keep_rules: vec!["gen.motion.consecutive_parallel_imperfects_limited"],
                expected_rule_ids: vec![],
            },
        ],
    );
}

#[test]
fn species3_examples_expected_errors() {
    run_cases(
        PresetId::Species3,
        vec![
            Case {
                name: "cadential-long-note-allowed",
                score: mk_two_voice_score(
                    &[
                        (62, 480, false, false),
                        (64, 480, false, false),
                        (65, 480, false, false),
                        (67, 480, false, false),
                        (69, 1920, false, false),
                    ],
                    &[(53, 1920, false, false), (55, 1920, false, false)],
                    4,
                    4,
                ),
                keep_rules: vec!["sp3.rhythm.four_to_one_only"],
                expected_rule_ids: vec![],
            },
            Case {
                name: "user-example-cadential-whole-note-allowed",
                score: mk_two_voice_score(
                    &[
                        (62, 480, false, false),
                        (60, 480, false, false),
                        (57, 480, false, false),
                        (59, 480, false, false),
                        (60, 480, false, false),
                        (62, 480, false, false),
                        (60, 480, false, false),
                        (57, 480, false, false),
                        (55, 480, false, false),
                        (57, 480, false, false),
                        (59, 480, false, false),
                        (55, 480, false, false),
                        (53, 480, false, false),
                        (55, 480, false, false),
                        (57, 480, false, false),
                        (53, 480, false, false),
                        (55, 480, false, false),
                        (57, 480, false, false),
                        (59, 480, false, false),
                        (60, 480, false, false),
                        (62, 480, false, false),
                        (60, 480, false, false),
                        (59, 480, false, false),
                        (57, 480, false, false),
                        (60, 480, false, false),
                        (59, 480, false, false),
                        (60, 480, false, false),
                        (62, 480, false, false),
                        (64, 480, false, false),
                        (62, 480, false, false),
                        (60, 480, false, false),
                        (59, 480, false, false),
                        (57, 480, false, false),
                        (55, 480, false, false),
                        (57, 480, false, false),
                        (53, 480, false, false),
                        (55, 480, false, false),
                        (57, 480, false, false),
                        (59, 480, false, false),
                        (61, 480, false, false),
                        (62, 1920, false, false),
                    ],
                    &[
                        (50, 1920, false, false),
                        (53, 1920, false, false),
                        (52, 1920, false, false),
                        (50, 1920, false, false),
                        (55, 1920, false, false),
                        (53, 1920, false, false),
                        (57, 1920, false, false),
                        (55, 1920, false, false),
                        (53, 1920, false, false),
                        (52, 1920, false, false),
                        (50, 1920, false, false),
                    ],
                    4,
                    4,
                ),
                keep_rules: vec!["sp3.rhythm.four_to_one_only"],
                expected_rule_ids: vec![],
            },
            Case {
                name: "valid-passing",
                score: mk_two_voice_score(
                    &[
                        (62, 480, false, false),
                        (64, 480, false, false),
                        (65, 480, false, false),
                        (69, 480, false, false),
                    ],
                    &[(53, 1920, false, false)],
                    4,
                    4,
                ),
                keep_rules: vec!["sp3.dissonance.passing_neighbor_patterns_only"],
                expected_rule_ids: vec![],
            },
            Case {
                name: "repeated-melodic-note-discouraged",
                score: mk_two_voice_score(
                    &[
                        (62, 480, false, false),
                        (62, 480, false, false),
                        (64, 480, false, false),
                        (65, 480, false, false),
                    ],
                    &[(53, 1920, false, false)],
                    4,
                    4,
                ),
                keep_rules: vec!["gen.melody.repeated_pitch_species_profiled"],
                expected_rule_ids: vec!["gen.melody.repeated_pitch_species_profiled"],
            },
            Case {
                name: "downbeat-interval-repetition-limits",
                score: mk_two_voice_score(
                    &[
                        (60, 480, false, false),
                        (61, 480, false, false),
                        (62, 480, false, false),
                        (63, 480, false, false),
                        (62, 480, false, false),
                        (63, 480, false, false),
                        (64, 480, false, false),
                        (65, 480, false, false),
                        (64, 480, false, false),
                        (65, 480, false, false),
                        (66, 480, false, false),
                        (67, 480, false, false),
                    ],
                    &[
                        (53, 1920, false, false),
                        (55, 1920, false, false),
                        (57, 1920, false, false),
                    ],
                    4,
                    4,
                ),
                keep_rules: vec!["sp3.structure.downbeat_interval_repetition_limits"],
                expected_rule_ids: vec!["sp3.structure.downbeat_interval_repetition_limits"],
            },
            Case {
                name: "perfect-interval-proximity-guard",
                score: mk_two_voice_score(
                    &[
                        (60, 480, false, false),
                        (61, 480, false, false),
                        (62, 480, false, false),
                        (60, 480, false, false),
                        (62, 480, false, false),
                        (63, 480, false, false),
                        (64, 480, false, false),
                        (65, 480, false, false),
                    ],
                    &[(53, 1920, false, false), (55, 1920, false, false)],
                    4,
                    4,
                ),
                keep_rules: vec!["sp3.perfect_interval_proximity_guard"],
                expected_rule_ids: vec!["sp3.perfect_interval_proximity_guard"],
            },
            Case {
                name: "valid-neighbor",
                score: mk_two_voice_score(
                    &[
                        (62, 480, false, false),
                        (64, 480, false, false),
                        (62, 480, false, false),
                        (60, 480, false, false),
                    ],
                    &[(53, 1920, false, false)],
                    4,
                    4,
                ),
                keep_rules: vec!["sp3.dissonance.passing_neighbor_patterns_only"],
                expected_rule_ids: vec![],
            },
            Case {
                name: "valid-double-neighbor",
                score: mk_two_voice_score(
                    &[
                        (62, 480, false, false),
                        (64, 480, false, false),
                        (61, 480, false, false),
                        (62, 480, false, false),
                    ],
                    &[(53, 1920, false, false)],
                    4,
                    4,
                ),
                keep_rules: vec!["sp3.dissonance.passing_neighbor_patterns_only"],
                expected_rule_ids: vec![],
            },
            Case {
                name: "valid-cambiata",
                score: mk_two_voice_score(
                    &[
                        (65, 480, false, false),
                        (64, 480, false, false),
                        (60, 480, false, false),
                        (62, 480, false, false),
                        (64, 480, false, false),
                    ],
                    &[(53, 1920, false, false), (53, 1920, false, false)],
                    4,
                    4,
                ),
                keep_rules: vec!["sp3.dissonance.cambiata_limited_exception"],
                expected_rule_ids: vec![],
            },
            Case {
                name: "invalid-leap-from-dissonance-not-cambiata",
                score: mk_two_voice_score(
                    &[
                        (65, 480, false, false),
                        (64, 480, false, false),
                        (61, 480, false, false),
                        (60, 480, false, false),
                        (59, 480, false, false),
                    ],
                    &[(53, 1920, false, false), (53, 1920, false, false)],
                    4,
                    4,
                ),
                keep_rules: vec!["sp3.dissonance.cambiata_limited_exception"],
                expected_rule_ids: vec!["sp3.dissonance.cambiata_limited_exception"],
            },
        ],
    );
}

#[test]
fn species4_examples_expected_errors() {
    run_cases(
        PresetId::Species4,
        vec![
            Case {
                name: "valid-43",
                score: mk_two_voice_score(
                    &[
                        (62, 960, false, false),
                        (60, 960, true, false),
                        (60, 960, false, true),
                        (59, 960, false, false),
                    ],
                    &[(53, 1920, false, false), (55, 1920, false, false)],
                    4,
                    4,
                ),
                keep_rules: vec![
                    "sp4.allowed_suspension_classes_enforced",
                    "sp4.suspension.step_resolution_required",
                ],
                expected_rule_ids: vec![],
            },
            Case {
                name: "valid-76",
                score: mk_two_voice_score(
                    &[
                        (67, 960, false, false),
                        (65, 960, true, false),
                        (65, 960, false, true),
                        (64, 960, false, false),
                    ],
                    &[(53, 1920, false, false), (55, 1920, false, false)],
                    4,
                    4,
                ),
                keep_rules: vec![
                    "sp4.allowed_suspension_classes_enforced",
                    "sp4.suspension.step_resolution_required",
                ],
                expected_rule_ids: vec![],
            },
            Case {
                name: "valid-98",
                score: mk_two_voice_score(
                    &[
                        (59, 960, false, false),
                        (57, 960, true, false),
                        (57, 960, false, true),
                        (55, 960, false, false),
                    ],
                    &[(53, 1920, false, false), (55, 1920, false, false)],
                    4,
                    4,
                ),
                keep_rules: vec![
                    "sp4.allowed_suspension_classes_enforced",
                    "sp4.suspension.step_resolution_required",
                ],
                expected_rule_ids: vec![],
            },
            Case {
                name: "valid-23-below",
                score: mk_two_voice_score(
                    &[
                        (60, 960, false, false),
                        (58, 960, true, false),
                        (58, 960, false, true),
                        (57, 960, false, false),
                    ],
                    &[(62, 1920, false, false), (60, 1920, false, false)],
                    4,
                    4,
                ),
                keep_rules: vec![
                    "sp4.allowed_suspension_classes_enforced",
                    "sp4.suspension.step_resolution_required",
                ],
                expected_rule_ids: vec![],
            },
            Case {
                name: "invalid-suspension-class",
                score: mk_two_voice_score(
                    &[
                        (60, 960, false, false),
                        (61, 960, true, false),
                        (61, 960, false, true),
                        (60, 960, false, false),
                    ],
                    &[(53, 1920, false, false), (55, 1920, false, false)],
                    4,
                    4,
                ),
                keep_rules: vec!["sp4.allowed_suspension_classes_enforced"],
                expected_rule_ids: vec!["sp4.allowed_suspension_classes_enforced"],
            },
            Case {
                name: "invalid-resolution-direction",
                score: mk_two_voice_score(
                    &[
                        (62, 960, false, false),
                        (60, 960, true, false),
                        (60, 960, false, true),
                        (62, 960, false, false),
                    ],
                    &[(53, 1920, false, false), (55, 1920, false, false)],
                    4,
                    4,
                ),
                keep_rules: vec!["sp4.suspension.step_resolution_required"],
                expected_rule_ids: vec!["sp4.suspension.step_resolution_required"],
            },
            Case {
                name: "non-tied-repeat-forbidden",
                score: mk_two_voice_score(
                    &[
                        (62, 960, false, false),
                        (60, 960, false, false),
                        (60, 960, false, false),
                        (59, 960, false, false),
                    ],
                    &[(53, 1920, false, false), (55, 1920, false, false)],
                    4,
                    4,
                ),
                keep_rules: vec!["gen.melody.repeated_pitch_species_profiled"],
                expected_rule_ids: vec!["gen.melody.repeated_pitch_species_profiled"],
            },
            Case {
                name: "strict-entry-exit-profile",
                score: mk_two_voice_score(
                    &[
                        (62, 960, false, false),
                        (60, 960, true, false),
                        (60, 960, false, true),
                        (59, 960, false, false),
                    ],
                    &[(53, 1920, false, false), (55, 1920, false, false)],
                    4,
                    4,
                ),
                keep_rules: vec!["sp4.form.strict_entry_exit_profile"],
                expected_rule_ids: vec!["sp4.form.strict_entry_exit_profile"],
            },
            Case {
                name: "break-species-budget-overused",
                score: mk_two_voice_score(
                    &[
                        (60, 960, false, false),
                        (62, 960, false, false),
                        (64, 960, false, false),
                        (65, 960, false, false),
                        (67, 960, false, false),
                        (69, 960, false, false),
                        (67, 960, false, false),
                        (65, 960, false, false),
                        (64, 960, false, false),
                        (62, 960, false, false),
                    ],
                    &[
                        (53, 1920, false, false),
                        (55, 1920, false, false),
                        (57, 1920, false, false),
                        (55, 1920, false, false),
                        (53, 1920, false, false),
                    ],
                    4,
                    4,
                ),
                keep_rules: vec!["sp4.form.break_species_budget"],
                expected_rule_ids: vec!["sp4.form.break_species_budget"],
            },
            Case {
                name: "suspension-density-minimum",
                score: mk_two_voice_score(
                    &[
                        (64, 960, false, false),
                        (62, 960, true, false),
                        (62, 960, false, true),
                        (60, 960, true, false),
                        (60, 960, false, true),
                        (59, 960, false, false),
                    ],
                    &[
                        (53, 1920, false, false),
                        (55, 1920, false, false),
                        (57, 1920, false, false),
                    ],
                    4,
                    4,
                ),
                keep_rules: vec!["sp4.suspension_density_minimum"],
                expected_rule_ids: vec!["sp4.suspension_density_minimum"],
            },
        ],
    );
}

#[test]
fn species5_examples_expected_errors() {
    run_cases(
        PresetId::Species5,
        vec![
            Case {
                name: "valid-passing",
                score: mk_two_voice_score(
                    &[
                        (62, 480, false, false),
                        (64, 480, false, false),
                        (65, 480, false, false),
                        (69, 480, false, false),
                    ],
                    &[(53, 1920, false, false)],
                    4,
                    4,
                ),
                keep_rules: vec!["sp5.dissonance.licensed_patterns_only"],
                expected_rule_ids: vec![],
            },
            Case {
                name: "valid-neighbor",
                score: mk_two_voice_score(
                    &[
                        (62, 480, false, false),
                        (64, 480, false, false),
                        (62, 480, false, false),
                        (60, 480, false, false),
                    ],
                    &[(53, 1920, false, false)],
                    4,
                    4,
                ),
                keep_rules: vec!["sp5.dissonance.licensed_patterns_only"],
                expected_rule_ids: vec![],
            },
            Case {
                name: "valid-escape-like",
                score: mk_two_voice_score(
                    &[
                        (62, 480, false, false),
                        (64, 480, false, false),
                        (60, 480, false, false),
                        (62, 480, false, false),
                    ],
                    &[(53, 1920, false, false)],
                    4,
                    4,
                ),
                keep_rules: vec!["sp5.dissonance.licensed_patterns_only"],
                expected_rule_ids: vec![],
            },
            Case {
                name: "valid-cambiata",
                score: mk_two_voice_score(
                    &[
                        (65, 480, false, false),
                        (64, 480, false, false),
                        (60, 480, false, false),
                        (62, 480, false, false),
                        (64, 480, false, false),
                    ],
                    &[(53, 1920, false, false), (55, 1920, false, false)],
                    4,
                    4,
                ),
                keep_rules: vec!["sp5.dissonance.licensed_patterns_only"],
                expected_rule_ids: vec![],
            },
            Case {
                name: "invalid-unlicensed-weak-dissonance",
                score: mk_two_voice_score(
                    &[
                        (62, 480, false, false),
                        (64, 480, false, false),
                        (67, 480, false, false),
                        (69, 480, false, false),
                    ],
                    &[(53, 1920, false, false)],
                    4,
                    4,
                ),
                keep_rules: vec!["sp5.dissonance.licensed_patterns_only"],
                expected_rule_ids: vec!["sp5.dissonance.licensed_patterns_only"],
            },
            Case {
                name: "invalid-downbeat-dissonance-no-suspension",
                score: mk_two_voice_score(
                    &[(64, 1920, false, false), (65, 1920, false, false)],
                    &[(53, 1920, false, false), (55, 1920, false, false)],
                    4,
                    4,
                ),
                keep_rules: vec!["sp5.strong_beat.consonance_or_prepared_suspension_only"],
                expected_rule_ids: vec!["sp5.strong_beat.consonance_or_prepared_suspension_only"],
            },
            Case {
                name: "repeated-melodic-note-discouraged",
                score: mk_two_voice_score(
                    &[
                        (62, 960, false, false),
                        (60, 960, false, false),
                        (60, 960, false, false),
                        (59, 960, false, false),
                    ],
                    &[(53, 1920, false, false), (55, 1920, false, false)],
                    4,
                    4,
                ),
                keep_rules: vec!["gen.melody.repeated_pitch_species_profiled"],
                expected_rule_ids: vec!["gen.melody.repeated_pitch_species_profiled"],
            },
        ],
    );
}
