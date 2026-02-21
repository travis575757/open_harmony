# Canonical Rules Corpus (Task 1)

## Purpose
This document defines the canonical, implementation-ready counterpoint rule corpus for Phase 1 analysis.

Authority order:
1. `docs/demos/AiHarmony/md/pdf/Artinfuser_Counterpoint_rules.pdf`
2. `docs/demos/AiHarmony/md/xls/rules2.xlsm` (sheet 1)
3. `docs/demos/AiHarmony/js/data/rules_paragraphs.js`
4. `docs/research/cp_gpt.md`, `docs/research/cp_gemini.md` (secondary clarification only)

## Rule Model
Each rule entry must be treated as an atomic check:
- `rule_id`: stable identifier
- `severity_default`: `error` or `warning`
- `scope`: species/voices/metric positions
- `condition_formal`: exact trigger/failure logic
- `exceptions`: explicitly allowed variants

## Rule ID Convention
`<domain>.<topic>.<constraint>[.<variant>]`
- `domain`: `gen`, `sp1`, `sp2`, `sp3`, `sp4`, `sp5`
- Example: `sp2.dissonance.weak_passing_stepwise`

## Normalized Canonical Rules

### General Input/Domain Rules
1. `gen.input.single_exercise_per_file`
- Severity: `error`
- Scope: all species, all voices
- Condition: input file contains exactly one exercise; reject multi-exercise files.

2. `gen.input.key_signature_required_and_stable`
- Severity: `error`
- Scope: all species
- Condition: key signature must appear before exercise and remain unchanged.

3. `gen.input.timesig_supported`
- Severity: `error`
- Scope: all species
- Condition: timesig must be in supported set `{2/4,3/4,4/4,2/2,5/4,6/4,3/2}` with species-specific limits.

4. `gen.input.species_supported`
- Severity: `error`
- Scope: all voices
- Condition: species must be in `{1,2,3,4,5}`.

5. `gen.input.voice_count_supported`
- Severity: `error`
- Scope: all species
- Condition: source supports 1..9 voices; product profile for editor limits execution profile to <=4 voices.

6. `gen.input_min_note_length.eighth_or_longer`
- Severity: `error`
- Scope: all species
- Condition: durations shorter than 1/8 are invalid; tuplets unsupported.

7. `gen.harmony.supported_sonorities`
- Severity: `error`
- Scope: harmonic analysis layer
- Condition: triads and seventh chords supported; higher extensions unsupported in strict profile.

### General Voice-Leading / Counterpoint Rules
8. `gen.motion.parallel_perfects_forbidden`
- Severity: `error`
- Scope: all species
- Condition: parallel perfect fifths/octaves between same voice pair across structural positions are forbidden.

9. `gen.motion.direct_perfects_restricted`
- Severity: `warning`
- Scope: all species; `error` in strict species profile
- Condition: similar motion into perfect intervals is forbidden/restricted, especially in outer voices.

10. `gen.spacing.upper_adjacent_max_octave`
- Severity: `warning`
- Scope: 3+ voices
- Condition: adjacent upper voices must not exceed octave spacing.

11. `gen.spacing.tenor_bass_max_twelfth`
- Severity: `warning`
- Scope: 3+ voices
- Condition: tenor-bass spacing should not exceed twelfth.

12. `gen.voice_crossing_and_overlap.restricted`
- Severity: `error`
- Scope: 2+ voices
- Condition: voice crossing/overlap violations are disallowed in strict profile.

13. `gen.unison.interior_restricted`
- Severity: `warning`
- Scope: strict species
- Condition: interior unisons are restricted; typically only opening/closing unison allowed.

14. `gen.melody.max_leap_octave`
- Severity: `error`
- Scope: all species
- Condition: melodic leaps > octave forbidden.

15. `gen.melody.dissonant_leaps_forbidden`
- Severity: `error`
- Scope: all species
- Condition: tritone/seventh melodic leaps forbidden in strict profile.

16. `gen.melody.post_leap_compensation_required`
- Severity: `warning`
- Scope: all species
- Condition: larger leaps require stepwise contrary compensation.

17. `gen.cadence.final_perfect_consonance_required`
- Severity: `error`
- Scope: species-focused modes
- Condition: final sonority must be perfect (unison/octave).

### Species 1 (1:1)
18. `sp1.rhythm.one_to_one_only`
- Severity: `error`
- Condition: one CP note per cantus note.

19. `sp1.vertical.consonance_only`
- Severity: `error`
- Condition: all vertical intervals consonant (strict two-voice treatment includes 4th as dissonant against bass).

20. `sp1.opening.perfect_consonance_required`
- Severity: `error`
- Condition: opening interval must be perfect consonance.

21. `sp1.ending.unison_or_octave_required`
- Severity: `error`
- Condition: ending interval must be unison or octave.

### Species 2 (2:1)
22. `sp2.rhythm.two_to_one_only`
- Severity: `error`
- Condition: two CP notes per cantus note in non-terminal CF windows; terminal cadence window may compress to fewer CP note onsets.

23. `sp2.strong_beat.consonance_required`
- Severity: `error`
- Condition: downbeats must be consonant.

24. `sp2.dissonance.weak_passing_stepwise`
- Severity: `error`
- Condition: weak-beat dissonance allowed only as passing tone approached and left by step.

25. `sp2.structure.downbeat_skeleton_no_parallel_perfects`
- Severity: `error`
- Condition: evaluate parallels using structural downbeats.

### Species 3 (4:1)
26. `sp3.rhythm.four_to_one_only`
- Severity: `error`
- Condition: four CP notes per cantus note in non-terminal CF windows; terminal cadence window may compress to fewer CP note onsets.

27. `sp3.strong_beat.consonance_required`
- Severity: `error`
- Condition: beat 1 consonant; no accented dissonance unless licensed pattern.

28. `sp3.dissonance.passing_neighbor_patterns_only`
- Severity: `error`
- Condition: dissonances must fit passing/neighbor-like stepwise patterns.

29. `sp3.dissonance.cambiata_limited_exception`
- Severity: `warning`
- Condition: cambiata permits specific leap-from-dissonance exception only in defined schema.

### Species 4 (syncopation/suspension)
31. `sp4.rhythm.syncopated_ligature_profile`
- Severity: `error`
- Condition: offset tied pattern required for suspension species baseline.

32. `sp4.suspension.preparation_required`
- Severity: `error`
- Condition: suspension note must be prepared as consonance.

33. `sp4.suspension.downbeat_dissonance_allowed_only_if_suspension`
- Severity: `error`
- Condition: strong-beat dissonance legal only as proper suspension.

34. `sp4.suspension.step_resolution_required`
- Severity: `error`
- Condition: suspension resolves by step (typically downward in strict profile).

35. `sp4.break_species.allowed_when_no_ligature_possible`
- Severity: `warning`
- Condition: temporary break to second-species behavior allowed when needed.

### Species 5 (florid)
36. `sp5.rhythm.mixed_species_profile`
- Severity: `error`
- Condition: mixed durations allowed while preserving structural species constraints.

37. `sp5.strong_beat.consonance_or_prepared_suspension_only`
- Severity: `error`
- Condition: structural beats must be consonant unless suspension-licensed.

38. `sp5.eighth_notes.weak_position_pairs_only`
- Severity: `warning`
- Condition: eighth-note usage restricted to weak-position paired decoration in strict profile.

39. `sp5.dissonance.licensed_patterns_only`
- Severity: `error`
- Condition: dissonance allowed only by recognized pattern class.

40. `sp5.cadence.strict_closure_required`
- Severity: `error`
- Condition: cadence must satisfy strict closure logic even in florid context.

## Canonical-to-Engine Notes
- Rules 1-17 are shared checks used by all species profiles.
- Rules 18-40 are species-gated checks.
- Severity overrides can be profile-driven (`strict`, `pedagogical`, `lenient`) but conditions stay fixed.

## Deferred / Ambiguous Rules
1. `gen.motion.direct_perfects_restricted`: strictness varies by outer/inner voices and pedagogy.
- Default: strict profile treats as `error`; common-practice profile downgrades to `warning`.

2. `gen.spacing.*`: historical corpus shows exceptions.
- Default: retain as warnings unless explicit strict SATB mode is selected.

3. `sp3.dissonance.cambiata_limited_exception`: multiple schema variants in literature.
- Default: enable one canonical strict variant first; add alternatives as named subrules.

4. `sp5.eighth_notes.weak_position_pairs_only`: style-dependent.
- Default: strict profile enforces; broader profile warns.

## Research-Derived Additional Rules (Tracked)
These rules are explicitly tracked from `docs/research/*` so they are not missed. Some are deferred beyond Phase 1 implementation but remain normalized here.

### Additional General / SATB / Tonal Rules
41. `gen.interval.p4_dissonant_against_bass_in_two_voice`
- Severity: `error`
- Scope: strict two-voice species
- Condition: harmonic perfect fourth against bass is treated as dissonance.

42. `gen.motion.contrary_and_oblique_preferred`
- Severity: `warning`
- Scope: all species
- Condition: excessive similar motion triggers independence warning.

43. `gen.melody.single_climax_preferred`
- Severity: `warning`
- Scope: all species
- Condition: multiple local maxima in one line produce contour warning.

44. `gen.melody.consecutive_large_leaps_restricted`
- Severity: `warning`
- Scope: all species
- Condition: repeated large leaps in same direction restricted unless triadic compensation pattern.

44b. `gen.melody.repeated_pitch_species_profiled`
- Severity: `error` in species 1 and species 4 (when not tie-linked), `warning` in species 2/3/5
- Scope: species presets
- Condition: consecutive repeated melodic pitch is forbidden in species 1; discouraged in species 2/3/5; in species 4 repeated pitch must be tie-linked (preparation/suspension) to be valid.

44c. `gen.opening.interval_by_position_species_profiled`
- Severity: `error`
- Scope: species presets
- Condition: opening pitch class follows species position logic (CP above starts on tonic/dominant; CP below starts on tonic).

44d. `gen.cadence.clausula_vera_required`
- Severity: `error`
- Scope: species presets
- Condition: cadence follows clausula-vera profile (tonic final, contrary stepwise approach, penultimate formula where applicable).

44e. `gen.spacing.two_voice_max_distance`
- Severity: `warning` above 10th, `error` above 12th
- Scope: two-voice textures
- Condition: vertical spacing between the two voices should stay within a tenth by default and must not exceed a twelfth.

44f. `gen.melody.climax_non_coincident_between_voices`
- Severity: `warning`
- Scope: multi-voice species presets
- Condition: primary climaxes in distinct voices should not coincide at the same metric instant.

44a. `gen.motion.consecutive_parallel_imperfects_limited`
- Severity: `error`
- Scope: all profiles (species and general voice-leading)
- Condition: no more than 3 consecutive parallel generic 3rds or 6ths between the same voice pair across successive interval events.

45. `gen.voice.leading_tone_not_doubled`
- Severity: `error`
- Scope: tonal/common-practice profile
- Condition: doubled leading tone is forbidden.

46. `gen.voice.chordal_seventh_resolves_down`
- Severity: `error`
- Scope: tonal/common-practice profile
- Condition: chordal 7th must resolve down by step.

47. `gen.voice.leading_tone_resolves_up`
- Severity: `warning`
- Scope: tonal/common-practice profile
- Condition: unresolved leading tone flagged unless accepted exception profile.

48. `gen.doubling.root_position_prefers_root`
- Severity: `warning`
- Scope: 4-voice tonal profile
- Condition: root doubling preferred in root position triads.

49. `gen.doubling.first_inversion_no_bass_double_default`
- Severity: `warning`
- Scope: 4-voice tonal profile
- Condition: first inversion generally avoids bass doubling except specified exceptions.

50. `gen.doubling.diminished_first_inversion_double_third`
- Severity: `error`
- Scope: 4-voice tonal profile
- Condition: diminished triad in first inversion doubles third (bass), not root.

51. `gen.doubling.second_inversion_double_bass`
- Severity: `error`
- Scope: 4-voice tonal profile
- Condition: second inversion sonority doubles bass.

52. `gen.cadence.cadential_64_resolves_65_43`
- Severity: `error`
- Scope: tonal/common-practice profile
- Condition: cadential 6/4 resolves with expected 6-5 and 4-3 behavior.

53. `gen.nct.appoggiatura_escape_anticipation_pedal_retardation_supported`
- Severity: `warning`
- Scope: tonal/common-practice profile
- Condition: non-chord tones outside species baseline must match supported pattern classes and resolution paths.

### Additional Species-Specific Rules
54. `sp1.cadence.penultimate_imperfect_consonance`
- Severity: `warning`
- Scope: species 1
- Condition: penultimate sonority follows clausula-compatible imperfect consonance logic.

55. `sp2.downbeat_unison_discouraged`
- Severity: `warning`
- Scope: species 2
- Condition: downbeat unison discouraged except explicit opening/closing contexts.

56. `sp3.downbeat_unison_forbidden`
- Severity: `warning`
- Scope: species 3
- Condition: beat-1 unison avoided in strict profile.

57. `sp4.allowed_suspension_classes_enforced`
- Severity: `error`
- Scope: species 4
- Condition: permitted suspension classes restricted (e.g., 7-6, 4-3, 9-8 above; 2-3 below).

58. `sp4.form.strict_entry_exit_profile`
- Severity: `error`
- Scope: species 4
- Condition: strict species-4 framing: half-rest opening profile and constrained cadential ending profile.

59. `sp4.form.break_species_budget`
- Severity: `warning`
- Scope: species 4
- Condition: break-species passages should be short and limited in count/segments.

60. `sp4.suspension_density_minimum`
- Severity: `warning`
- Scope: species 4
- Condition: dissonant suspensions should appear with minimum density in the exercise.

61. `sp4.afterbeat_parallel_guard`
- Severity: `error`
- Scope: species 4
- Condition: suspension chains must not generate forbidden after-beat parallels.

62. `sp4.all_voices_syncopation_avoidance`
- Severity: `warning`
- Scope: multi-voice species 4/5 textures
- Condition: avoid fully syncopated texture with no beat-articulating voice.

63. `sp2.structure.downbeat_interval_repetition_limits`
- Severity: `error`/`warning`
- Scope: species 2
- Condition: repeated same perfect interval on consecutive downbeats is forbidden; long runs of same imperfect class are discouraged.

64. `sp2.weak_beat.consonant_pattern_catalog`
- Severity: `warning`
- Scope: species 2
- Condition: consonant weak beats should match recognized pattern classes (passing, neighbor, substitution, skipped passing, interval subdivision, change of register, delay).

65. `sp3.structure.downbeat_interval_repetition_limits`
- Severity: `error`/`warning`
- Scope: species 3
- Condition: at most two consecutive downbeats in one perfect interval; long runs of same imperfect class are discouraged.

66. `sp3.perfect_interval_proximity_guard`
- Severity: `error`
- Scope: species 3
- Condition: structural perfect intervals on downbeats must not be too closely preceded by the same perfect class in the prior bar.

67. `sp5.eighth_grouping_no_triplet_like_clusters`
- Severity: `warning`
- Scope: species 5 strict profile
- Condition: eighth-note groups restricted to supported pairwise decorative shapes.

### Advanced Counterpoint (Tracked, Deferred by Default)
68. `adv.invertible.octave_treat_fifth_as_sensitive`
- Severity: `deferred`
- Scope: invertible counterpoint contexts
- Condition: fifth behavior constrained under octave inversion to avoid illicit fourth-against-bass outcomes.

62. `adv.invertible.octave_suspension_pair_76_23_preferred`
- Severity: `deferred`
- Scope: invertible counterpoint contexts
- Condition: suspension pairs selected for inversion compatibility.

63. `adv.invertible.tenth_avoid_parallel_3_6_sources`
- Severity: `deferred`
- Scope: invertible at tenth
- Condition: avoid interval chains that invert to perfect parallels.

64. `adv.invertible.twelfth_limit_structural_sixths`
- Severity: `deferred`
- Scope: invertible at twelfth
- Condition: structural sixths constrained due to inversion to sevenths.
