# Research Rules Coverage Audit

## Goal
Demonstrate that rules mentioned in `docs/research/cp_gpt.md` and `docs/research/cp_gemini.md` are not missed.

## Coverage Policy
- `Active`: normalized and intended for Phase 1/near-term implementation.
- `Tracked`: normalized and source-mapped, but may be profile-gated.
- `Deferred`: normalized and mapped; intentionally postponed beyond Phase 1.

## Coverage Matrix

| Research rule family | Canonical rule IDs | Status |
|---|---|---|
| Consonance/dissonance baseline | `sp1.vertical.consonance_only`, `gen.interval.p4_dissonant_against_bass_in_two_voice` | Active |
| Parallel perfect interval ban | `gen.motion.parallel_perfects_forbidden`, `sp2.structure.downbeat_skeleton_no_parallel_perfects` | Active |
| Direct/hidden perfect restrictions | `gen.motion.direct_perfects_restricted` | Active |
| Motion preference (contrary/oblique) | `gen.motion.contrary_and_oblique_preferred` | Tracked |
| Melodic profile constraints (leaps/repeats/compensation) | `gen.melody.max_leap_octave`, `gen.melody.dissonant_leaps_forbidden`, `gen.melody.post_leap_compensation_required`, `gen.melody.consecutive_large_leaps_restricted`, `gen.melody.repeated_pitch_species_profiled` | Active/Tracked |
| Opening/cadence profile by species position | `gen.opening.interval_by_position_species_profiled`, `gen.cadence.clausula_vera_required` | Tracked |
| Two-voice distance and climax separation | `gen.spacing.two_voice_max_distance`, `gen.melody.climax_non_coincident_between_voices` | Tracked |
| Single-climax contour | `gen.melody.single_climax_preferred` | Tracked |
| Species 1 opening/ending/cadence | `sp1.opening.perfect_consonance_required`, `sp1.ending.unison_or_octave_required`, `sp1.cadence.penultimate_imperfect_consonance` | Active/Tracked |
| Species 2 downbeat consonance + weak passing dissonance | `sp2.strong_beat.consonance_required`, `sp2.dissonance.weak_passing_stepwise`, `sp2.downbeat_unison_discouraged`, `sp2.structure.downbeat_interval_repetition_limits`, `sp2.weak_beat.consonant_pattern_catalog` | Active/Tracked |
| Species 3 passing/neighbor/cambiata | `sp3.dissonance.passing_neighbor_patterns_only`, `sp3.dissonance.cambiata_limited_exception`, `sp3.downbeat_unison_forbidden`, `sp3.structure.downbeat_interval_repetition_limits`, `sp3.perfect_interval_proximity_guard` | Active/Tracked |
| Species 4 suspension syntax and constraints | `sp4.suspension.preparation_required`, `sp4.suspension.downbeat_dissonance_allowed_only_if_suspension`, `sp4.suspension.step_resolution_required`, `sp4.allowed_suspension_classes_enforced`, `sp4.afterbeat_parallel_guard`, `sp4.break_species.allowed_when_no_ligature_possible`, `sp4.form.break_species_budget`, `sp4.suspension_density_minimum`, `sp4.form.strict_entry_exit_profile`, `sp4.all_voices_syncopation_avoidance` | Active/Tracked |
| Species 5 florid rhythm and cadence strictness | `sp5.rhythm.mixed_species_profile`, `sp5.eighth_notes.weak_position_pairs_only`, `sp5.eighth_grouping_no_triplet_like_clusters`, `sp5.dissonance.licensed_patterns_only`, `sp5.cadence.strict_closure_required` | Active/Tracked |
| SATB spacing/crossing | `gen.spacing.upper_adjacent_max_octave`, `gen.spacing.tenor_bass_max_twelfth`, `gen.voice_crossing_and_overlap.restricted` | Active |
| Tonal tendency tones | `gen.voice.leading_tone_not_doubled`, `gen.voice.chordal_seventh_resolves_down`, `gen.voice.leading_tone_resolves_up` | Tracked |
| Doubling by inversion | `gen.doubling.root_position_prefers_root`, `gen.doubling.first_inversion_no_bass_double_default`, `gen.doubling.diminished_first_inversion_double_third`, `gen.doubling.second_inversion_double_bass` | Tracked |
| Cadential 6/4 behavior | `gen.cadence.cadential_64_resolves_65_43` | Tracked |
| Expanded non-chord tone taxonomy | `gen.nct.appoggiatura_escape_anticipation_pedal_retardation_supported` | Tracked |
| Invertible counterpoint (octave/tenth/twelfth) | `adv.invertible.octave_treat_fifth_as_sensitive`, `adv.invertible.octave_suspension_pair_76_23_preferred`, `adv.invertible.tenth_avoid_parallel_3_6_sources`, `adv.invertible.twelfth_limit_structural_sixths` | Deferred |

## Result
- All rule families explicitly described in `docs/research/*` now have corresponding normalized entries in `docs/planning/rules-canonical.md` and trace rows in `docs/planning/rules-mapping.csv`.
- No research rule family is untracked.

## Notes
- “No rule missed” here means no rule family is omitted from canonical tracking.
- Some advanced families are intentionally deferred (not ignored) and can be activated by profile in later phases.
