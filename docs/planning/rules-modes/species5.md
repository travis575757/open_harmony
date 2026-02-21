# Species 5 (Florid Counterpoint) — Practical Rules Guide

## What This Mode Is
Species 5 is the mixed-rhythm species. You combine the feel of earlier species in one line: stable consonant points, controlled dissonance, and light ornament.

Think of it as “free-looking, but not free.” The line should sound musical and varied, while still following strict counterpoint discipline.

In this preset, you are checked by shared core strict rules plus Species 5 rules; tonal harmony-only rules are excluded, and advanced invertible checks are deferred.

## Quick Checklist
- Use mixed note values, not one rhythm all the way through.
- Keep strong beats consonant, unless they are part of a real prepared suspension.
- Use eighth notes as weak-position pairs.
- Use dissonance only in licensed patterns.
- Close with a strict final cadence ending in a perfect consonance.
- Avoid long runs of consecutive eighth notes.
- Read severity simply: errors must be fixed; warnings are style problems you should usually improve.
- Similar-motion approach to perfect intervals is treated strictly in species presets (it is an error here).
- Advanced invertible-counterpoint checks are deferred in this mode.

## Core Rules You Must Pass
Start by building a clean framework: clear key/time/species setup, supported voice count, no illegal note lengths, and a valid one-exercise submission.

Then protect vertical clarity: avoid forbidden perfect parallels, avoid crossing/overlap problems, keep spacing reasonable, and land on a proper final perfect sonority.

Keep your melody singable: no oversized or harsh dissonant leaps, compensate larger leaps properly, avoid too many big leaps in a row, and shape the line toward a clear high point.

For Species 5 style specifically:
- Mix rhythmic values in a musically intentional way.
- Treat accented dissonance as suspension-only territory.
- Keep eighth notes on weak positions and usually in pairs.
- Use only licensed dissonance motions on weak positions.
- Keep cadence closure strict.
- Do not let ornament turn into extended eighth-note chains.

### Count Rules (Total and Consecutive Notes/Intervals)
- Total notes: no fixed overall ratio is enforced; species 5 is mixed-rhythm by design.
- Consecutive notes (rhythm profile): at least two different duration values must appear in the counterpoint line.
- Consecutive notes (eighth handling): eighth notes should be on weak subdivisions, should occur in adjacent pairs, and runs longer than two consecutive eighths are discouraged.
- Total vertical intervals: every simultaneous event is analyzed for consonance/dissonance legality relative to beat strength.
- Consecutive intervals: strong-beat intervals must stay consonant unless suspension-licensed; weak-beat dissonances must follow licensed consecutive-note patterns (passing/neighbor/double-neighbor/cambiata/escape).
- Consecutive imperfect parallels: no more than 3 consecutive parallel generic 3rds or 6ths in the same voice pair.

### Leap Rules (Clear Defaults)
Active leap behavior in Species 5 (default values):
- Maximum leap size: larger than 12 semitones is an error.
- Dissonant melodic leaps: 6, 10, or 11 semitones are errors.
- Consecutive large leaps: two 5+ semitone leaps in a row, same direction, are warned.
- Leap recovery: after a 5+ semitone leap, next move should be contrary and stepwise (<= 2 semitones), otherwise warning.
- Species 5-specific addition: weak-beat dissonance may use licensed florid patterns only; unlicensed leap behavior is rejected.

Practical valid/invalid note-language examples:
1. Valid: bar uses half + quarter + paired eighths with smooth contour. Invalid: every bar uses only quarters.
2. Valid: strong beat is consonant 3rd/6th/10th. Invalid: strong beat attacks a dissonance without suspension context.
3. Valid: two eighths on a weak part of the beat as passing decoration. Invalid: a lone eighth on a strong position.
4. Valid: weak-beat dissonance moves by a recognized passing/neighbor-like pattern. Invalid: weak-beat dissonance approached and left by leap with no license.
5. Valid: final sonority is perfect (unison or octave class). Invalid: final sonority is imperfect or dissonant.
6. Valid: occasional eighth-note pair separated by longer values. Invalid: three or more consecutive eighths that feel like a run.

## Common Mistakes and How to Fix Them
Too rhythmically uniform:
Write one short plan before composing each phrase: choose where halves, quarters, and eighth-pairs will appear.

Accented dissonance used as decoration:
Move that dissonance off the strong beat, or convert it into a properly prepared/resolved suspension.

Eighth notes placed randomly:
Audit every eighth note. If it is not on a weak position and paired, rewrite the figure.

Weak-beat dissonance that sounds forced:
Check approach and departure. If it is not a clear passing/neighbor/cambiata/escape-type motion, simplify.

Cadence sounds unfinished:
Sketch the final two measures first, then compose backward to guarantee strict closure.

Texture gets “busy” near the end:
Replace long eighth-note chains with quarters/halves so the cadence can breathe.

## How Diagnostics Map to the Music
In the web editor, each diagnostic points to a specific note location. Start there first.

If a second note is shown, treat it as the partner note creating the problem (for example, the simultaneous note in the other voice).

Use severity to prioritize:
- Error: rule failure; fix before finalizing.
- Warning: stylistic risk; often acceptable only with a clear musical reason.

A practical workflow:
1. Fix all cadence and strong-beat dissonance errors first.
2. Fix dissonance-pattern and parallel-motion errors next.
3. Clean up warning-level rhythm/style issues (especially isolated or overused eighths).

## Rule ID Reference
| Rule ID | Human meaning |
|---|---|
| gen.input.single_exercise_per_file | Submit one exercise per file. |
| gen.input.key_signature_required_and_stable | Use a valid, stable key signature. |
| gen.input.timesig_supported | Use a supported time signature for this mode. |
| gen.input.species_supported | Species selection must be valid. |
| gen.input.voice_count_supported | Keep voice count within supported limits. |
| gen.input_min_note_length.eighth_or_longer | Do not use note values shorter than eighth notes. |
| gen.harmony.supported_sonorities | Keep sonorities within supported harmonic types. |
| gen.motion.parallel_perfects_forbidden | Avoid parallel perfect fifths and octaves. |
| gen.motion.direct_perfects_restricted | Similar-motion entry to perfect intervals is strictly restricted (error here). |
| gen.motion.consecutive_parallel_imperfects_limited | Limit consecutive parallel generic 3rds/6ths to at most 3. |
| gen.spacing.upper_adjacent_max_octave | Keep adjacent upper voices within normal spacing. |
| gen.spacing.tenor_bass_max_twelfth | Keep bass-to-tenor spacing within normal range. |
| gen.voice_crossing_and_overlap.restricted | Avoid voice crossing and overlap issues. |
| gen.unison.interior_restricted | Avoid interior unisons except where justified. |
| gen.melody.max_leap_octave | Avoid melodic leaps larger than an octave. |
| gen.melody.dissonant_leaps_forbidden | Avoid dissonant melodic leaps. |
| gen.melody.post_leap_compensation_required | Balance larger leaps with proper compensation. |
| gen.cadence.final_perfect_consonance_required | End with a perfect final consonance. |
| gen.interval.p4_dissonant_against_bass_in_two_voice | Treat fourth above the bass as dissonant in strict two-voice use. |
| gen.motion.contrary_and_oblique_preferred | Favor contrary/oblique motion over excessive similar motion. |
| gen.melody.single_climax_preferred | Prefer one clear melodic high point. |
| gen.melody.consecutive_large_leaps_restricted | Limit repeated large leaps, especially in one direction. |
| sp5.rhythm.mixed_species_profile | Use mixed rhythmic values in florid style. |
| sp5.strong_beat.consonance_or_prepared_suspension_only | Strong beats must be consonant or suspension-licensed. |
| sp5.eighth_notes.weak_position_pairs_only | Eighth notes belong on weak positions and in pairs. |
| sp5.dissonance.licensed_patterns_only | Allow dissonance only in licensed florid patterns. |
| sp5.cadence.strict_closure_required | Apply strict species closure at the cadence. |
| sp5.eighth_grouping_no_triplet_like_clusters | Avoid extended consecutive eighth-note clusters. |
| adv.invertible.octave_treat_fifth_as_sensitive | Deferred advanced invertible rule (not enforced in this mode). |
| adv.invertible.octave_suspension_pair_76_23_preferred | Deferred advanced invertible rule (not enforced in this mode). |
| adv.invertible.tenth_avoid_parallel_3_6_sources | Deferred advanced invertible rule (not enforced in this mode). |
| adv.invertible.twelfth_limit_structural_sixths | Deferred advanced invertible rule (not enforced in this mode). |

## Sources
- `docs/planning/rules-presets.json` (preset membership, groups, severity override, deferred list)
- `docs/planning/rules-canonical.md` (human musical intent behind rule meanings)
