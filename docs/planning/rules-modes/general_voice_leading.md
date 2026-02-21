# General Voice Leading (Common Practice) — Practical Rules Guide

## What This Mode Is
This mode is for common-practice voice leading rather than a single species exercise pattern.

You still get all core counterpoint safety checks (motion, spacing, melodic shape, cadence, input validity), plus tonal rules for tendencies and chord doubling.

Preset truth for this mode:
- Active groups: `core_general_rules` + `tonal_general_rules`
- Excluded group: `advanced_deferred_rules`
- Explicit exclusion: `gen.interval.p4_dissonant_against_bass_in_two_voice`
- Direct perfects are treated as a warning in this mode (not strict species error)

## Quick Checklist
- Keep voice-leading clean: avoid parallel perfects and reduce direct approaches to perfect intervals.
- Avoid long chains of parallel imperfect consonances (no more than 3 consecutive generic 3rds/6ths).
- Keep spacing practical and avoid crossing/overlap.
- Keep lines singable: avoid harsh leaps and recover from large leaps.
- Keep one clear melodic high point per line where possible.
- Resolve tendencies: leading tone up, chordal seventh down.
- Use sensible doubling based on inversion.
- Close cadences clearly, including cadential 6/4 handling.

Severity in plain terms:
- `error`: required fix
- `warning`: strong musical recommendation

## Core Rules You Must Pass
This mode combines two layers.

Core counterpoint layer (shared):
- Input/setup validity (supported species/time/voices, stable key, legal durations).
- Motion control (parallel perfects forbidden, direct perfects restricted).
- Spacing/texture control (adjacent spacing, crossing/overlap, interior unison caution).
- Melodic quality (leap limits, dissonant-leap bans, leap compensation, contour checks).
- Cadential closure (final perfect consonance requirement).

Tonal/common-practice layer:
- Do not double the leading tone.
- Resolve the chordal seventh downward.
- Prefer leading tone resolving upward.
- Prefer root doubling in root-position triads.
- Avoid bass doubling in first inversion by default.
- In diminished first inversion, double the third.
- In second inversion, double the bass.
- Treat cadential 6/4 as a resolving sonority, not a static resting chord.
- Use licensed non-chord-tone behavior in tonal contexts.

Deferred note:
- Advanced invertible-counterpoint rules are tracked but not active in this preset.

## Common Mistakes and How to Fix Them
1. Parallel perfects between voices
- What to do: vary interval class and prefer contrary motion.
- What to avoid: perfect 5th to perfect 5th in similar motion.

2. Direct approach into perfect interval in outer voices
- What to do: approach perfects by contrary or oblique motion when possible.
- What to avoid: both voices moving same direction into a perfect octave.

3. Tendency tones not resolving
- What to do: let leading tone rise by step and chordal seventh fall by step.
- What to avoid: unresolved leading tone dropping away, or seventh leaping upward.

4. Weak doubling choices
- What to do: choose doubling based on inversion role.
- What to avoid: doubling leading tone, doubling the wrong tone in diminished first inversion, or missing bass doubling in second inversion.

5. Cadential 6/4 treated like a stable block
- What to do: resolve cadential 6/4 into dominant function correctly.
- What to avoid: holding second inversion as a static sonority.

6. Melody is jagged or over-leaping
- What to do: balance leaps with stepwise contrary recovery and shape a clear peak.
- What to avoid: repeated large leaps in same direction.

## How Diagnostics Map to the Music
How to read messages quickly:
- Primary note: where the checker anchors the issue.
- Related note: the partner note (or previous/next event) involved in the same problem.
- Severity: tells you whether it blocks acceptance (`error`) or is guidance (`warning`).

Practical triage order:
1. Fix hard cadence and tendency-tone errors.
2. Fix parallel/direct-perfect and crossing issues.
3. Fix doubling/inversion errors.
4. Improve warning-level style issues (contour, spacing, excess similar motion).

## Rule ID Reference
| Rule ID | Human meaning |
|---|---|
| `gen.input.single_exercise_per_file` | One exercise per analysis request. |
| `gen.input.key_signature_required_and_stable` | Key signature must be present and stable. |
| `gen.input.timesig_supported` | Time signature must be supported. |
| `gen.input.species_supported` | Species value must be recognized. |
| `gen.input.voice_count_supported` | Voice count must be within supported limits. |
| `gen.input_min_note_length.eighth_or_longer` | No durations shorter than eighth notes. |
| `gen.harmony.supported_sonorities` | Sonorities must stay in supported harmonic classes. |
| `gen.motion.parallel_perfects_forbidden` | No parallel 5ths/8ves. |
| `gen.motion.direct_perfects_restricted` | Direct motion into perfect intervals is restricted (warning here). |
| `gen.motion.consecutive_parallel_imperfects_limited` | Limit consecutive parallel generic 3rds/6ths to at most 3. |
| `gen.spacing.upper_adjacent_max_octave` | Adjacent upper voices should stay within an octave. |
| `gen.spacing.tenor_bass_max_twelfth` | Tenor-bass spacing should stay within a twelfth. |
| `gen.voice_crossing_and_overlap.restricted` | Voice crossing/overlap is restricted. |
| `gen.unison.interior_restricted` | Interior unisons are discouraged. |
| `gen.melody.max_leap_octave` | Melodic leaps larger than an octave are forbidden. |
| `gen.melody.dissonant_leaps_forbidden` | Dissonant melodic leaps are forbidden. |
| `gen.melody.post_leap_compensation_required` | Large leaps should be compensated stepwise in contrary direction. |
| `gen.cadence.final_perfect_consonance_required` | Final sonority should be perfect consonance. |
| `gen.motion.contrary_and_oblique_preferred` | Prefer contrary/oblique motion over too much similar motion. |
| `gen.melody.single_climax_preferred` | Prefer one clear melodic climax. |
| `gen.melody.consecutive_large_leaps_restricted` | Repeated large leaps in same direction are restricted. |
| `gen.voice.leading_tone_not_doubled` | Do not double the leading tone. |
| `gen.voice.chordal_seventh_resolves_down` | Chordal seventh should resolve down by step. |
| `gen.voice.leading_tone_resolves_up` | Leading tone should resolve upward in normal cases. |
| `gen.doubling.root_position_prefers_root` | Root-position chords usually prefer root doubling. |
| `gen.doubling.first_inversion_no_bass_double_default` | First inversion usually avoids bass doubling. |
| `gen.doubling.diminished_first_inversion_double_third` | Diminished first inversion should double the third. |
| `gen.doubling.second_inversion_double_bass` | Second inversion should double the bass. |
| `gen.cadence.cadential_64_resolves_65_43` | Cadential 6/4 should resolve in standard cadential voice-leading. |
| `gen.nct.appoggiatura_escape_anticipation_pedal_retardation_supported` | Extended NCT classes are recognized in this tonal mode. |
| `adv.invertible.octave_treat_fifth_as_sensitive` | Deferred advanced invertible rule (not active). |
| `adv.invertible.octave_suspension_pair_76_23_preferred` | Deferred advanced invertible rule (not active). |
| `adv.invertible.tenth_avoid_parallel_3_6_sources` | Deferred advanced invertible rule (not active). |
| `adv.invertible.twelfth_limit_structural_sixths` | Deferred advanced invertible rule (not active). |

## Sources
- `docs/planning/rules-presets.json`
- `docs/planning/rules-canonical.md`
- `docs/planning/rules-mapping.csv`
- `docs/planning/rules-test-fixtures.md`
- `crates/cp_rules/src/lib.rs`
