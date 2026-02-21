# Species 4 (Fourth Species) — Practical Rules Guide

## What This Mode Is
Fourth species is suspension counterpoint: the line is mostly tied across barlines (syncopated ligatures), so tension happens on strong beats and resolves after.

In this preset, you are checked by two rule sets together: shared core counterpoint rules plus fourth-species suspension rules. Tonal-style rules are not part of this mode.

Severity is simple: `error` means you must fix it, `warning` means musical advice. In species presets, direct motion into perfect intervals is treated strictly as an `error`.

## Quick Checklist
- Keep a clear syncopated profile: ties should cross barlines.
- Prepare each suspension as a consonance before it becomes dissonant.
- Use accented dissonance only as a true suspension (prepared and tied).
- Resolve dissonant suspensions down by step.
- Stay within standard suspension families (4-3, 7-6, 9-8, 2-3).
- Watch weak-beat perfect intervals: do not chain parallel perfects after the beat.
- If you break species (temporary untied writing), do it only when a ligature is genuinely not possible.
- In multi-voice writing, keep at least one voice articulating the beat.
- Still pass the shared basics: no parallel perfects, valid spacing, clean cadence, singable melody.

## Core Rules You Must Pass
Suspension grammar comes first. In practice, each real dissonant suspension should follow this shape:
1) consonant preparation,
2) tied note across the barline,
3) dissonance on the strong beat,
4) downward step resolution.

Downbeat dissonance is not free color in this mode. If it is not a prepared suspension, it is wrong.

Allowed dissonant suspension classes are the standard set used here: 4-3, 7-6, 9-8, and 2-3 behavior.

You are also checked for hidden texture problems. The engine guards against repeated perfect intervals on consecutive afterbeats, and it warns if every voice is syncopated (no one marks the beat).

Shared general rules still matter and are grouped like this:
- Input and setup: supported species/voice count/time signature, one exercise per file, stable key, legal note lengths.
- Vertical control: supported sonorities, no parallel perfects, strict direct-perfect handling, two-voice fourth above bass treated as dissonant.
- Spacing and texture: spacing limits, crossing/overlap limits, interior unison caution, preferred contrary/oblique balance.
- Melody and cadence: leap limits, avoid dissonant leaps, compensate large leaps, avoid repeated large same-direction leaps, prefer a single climax, end on a perfect final consonance.

### Count Rules (Total and Consecutive Notes/Intervals)
- Total notes: there is no fixed global ratio like 1:1/2:1/4:1; this species is profile-based (syncopated ligatures across barlines are required).
- Consecutive notes (suspension chain): a suspension event depends on note sequence and tie behavior: preparation note -> held/tied downbeat note -> downward step resolution if the downbeat is dissonant.
- Total vertical intervals: interval checks run at each simultaneous onset where both voices are active.
- Consecutive intervals (afterbeat guard): consecutive weak-beat intervals are checked for repeated perfect classes (parallel-perfect guard on afterbeats).
- Consecutive intervals (global): standard motion checks (parallel/direct perfect restrictions) still apply to successive sampled events.
- Consecutive imperfect parallels: no more than 3 consecutive parallel generic 3rds or 6ths in the same voice pair.

### Leap Rules (Clear Defaults)
Active leap behavior in Species 4 (default values):
- Maximum leap size: larger than 12 semitones is an error.
- Dissonant melodic leaps: 6, 10, or 11 semitones are errors.
- Consecutive large leaps: two 5+ semitone leaps in a row, same direction, are warned.
- Leap recovery: after a 5+ semitone leap, next move should be contrary and stepwise (<= 2 semitones), otherwise warning.
- Species 4-specific addition: dissonant suspensions must resolve down by step (no leap resolution from the dissonant suspension point).

Advanced invertible-counterpoint rules are deferred in this preset.

## Common Mistakes and How to Fix Them
1. Valid 4-3 suspension
What to do: tie the upper note over the barline so it forms a 4th above the bass on the strong beat, then resolve down to the 3rd by step.
What to avoid: attacking the dissonant 4th freshly on the downbeat with no tie.

2. Valid 7-6 suspension
What to do: prepare the 7th as consonant first, hold it across the barline, then step down to the 6th.
What to avoid: leaping away from the 7th instead of resolving by step.

3. Valid 9-8 suspension
What to do: in a higher voice, hold the 9th into the bar and resolve down to the octave.
What to avoid: resolving the 9th upward or repeating it without resolution.

4. Invalid: unprepared accented dissonance
What to do: make sure the note before the tie is consonant against the other voice.
What to avoid: preparing the suspension with a dissonance, then tying it forward.

5. Invalid: wrong resolution direction or size
What to do: resolve dissonant suspensions down by one scale step.
What to avoid: upward resolution or downward leap.

6. Invalid texture handling (afterbeat perfects / all-voices syncopation)
What to do: vary weak-beat intervals so perfect 5ths/8ves do not repeat in parallel, and let at least one voice attack on the beat in multi-voice textures.
What to avoid: consecutive weak-beat perfect parallels, or making every voice tied across every barline.

## How Diagnostics Map to the Music
Read diagnostics as rehearsal notes:
- `error`: structural problem; fix before moving on.
- `warning`: stylistic or texture issue; usually improve unless you have a strong reason.

When a message mentions preparation, check the harmony right before the tie. When it mentions downbeat dissonance, check whether the strong-beat dissonance is truly tied from before. When it mentions step resolution, inspect the next note after the suspension and confirm a downward step.

If a diagnostic points to afterbeat parallels, compare consecutive weak beats for repeated perfect 5ths or 8ves. If it warns about all-voice syncopation, leave one voice beat-articulated so the meter stays clear.

## Rule ID Reference
| Rule ID | Human meaning |
|---|---|
| `gen.input.single_exercise_per_file` | One exercise per file. |
| `gen.input.key_signature_required_and_stable` | Key signature must exist and stay stable. |
| `gen.input.timesig_supported` | Use a supported time signature. |
| `gen.input.species_supported` | Species must be in supported set. |
| `gen.input.voice_count_supported` | Voice count must be supported. |
| `gen.input_min_note_length.eighth_or_longer` | No note values shorter than an eighth. |
| `gen.harmony.supported_sonorities` | Vertical sonorities must be from supported set. |
| `gen.motion.parallel_perfects_forbidden` | No parallel perfect 5ths or 8ves. |
| `gen.motion.direct_perfects_restricted` | Direct motion into perfect intervals is restricted (strict here). |
| `gen.motion.consecutive_parallel_imperfects_limited` | Limit consecutive parallel generic 3rds/6ths to at most 3. |
| `gen.spacing.upper_adjacent_max_octave` | Upper adjacent voices should stay within an octave. |
| `gen.spacing.tenor_bass_max_twelfth` | Tenor-bass spacing should stay within a twelfth. |
| `gen.voice_crossing_and_overlap.restricted` | Avoid restricted crossing and overlap. |
| `gen.unison.interior_restricted` | Interior unisons are limited. |
| `gen.melody.max_leap_octave` | Do not leap more than an octave melodically. |
| `gen.melody.dissonant_leaps_forbidden` | Avoid dissonant melodic leaps. |
| `gen.melody.post_leap_compensation_required` | Compensate large leaps with contrary stepwise motion. |
| `gen.cadence.final_perfect_consonance_required` | End on a perfect final consonance. |
| `gen.interval.p4_dissonant_against_bass_in_two_voice` | In two voices, a 4th above bass is dissonant. |
| `gen.motion.contrary_and_oblique_preferred` | Prefer contrary/oblique over too much similar motion. |
| `gen.melody.single_climax_preferred` | Prefer one clear melodic high point. |
| `gen.melody.consecutive_large_leaps_restricted` | Restrict repeated large leaps in same direction. |
| `sp4.rhythm.syncopated_ligature_profile` | Fourth-species line should show syncopated barline ties. |
| `sp4.suspension.preparation_required` | Suspensions must be prepared consonantly. |
| `sp4.suspension.downbeat_dissonance_allowed_only_if_suspension` | Strong-beat dissonance is allowed only as prepared suspension. |
| `sp4.suspension.step_resolution_required` | Dissonant suspensions resolve downward by step. |
| `sp4.break_species.allowed_when_no_ligature_possible` | Break species only when ligature continuation is not possible. |
| `sp4.allowed_suspension_classes_enforced` | Only allowed suspension classes are accepted. |
| `sp4.afterbeat_parallel_guard` | Guard against parallel perfects on consecutive afterbeats. |
| `sp4.all_voices_syncopation_avoidance` | Warn if all voices are syncopated. |

Deferred advanced note: invertible-counterpoint rules are intentionally deferred in this preset (`adv.invertible.octave_treat_fifth_as_sensitive`, `adv.invertible.octave_suspension_pair_76_23_preferred`, `adv.invertible.tenth_avoid_parallel_3_6_sources`, `adv.invertible.twelfth_limit_structural_sixths`).

## Sources
- `docs/planning/rules-presets.json`
- `docs/planning/rules-canonical.md`
- `docs/planning/rules-mapping.csv`
- `crates/cp_rules/src/lib.rs`
