# Species 3 (Third Species) — Practical Rules Guide

## What This Mode Is
Species 3 means a 4:1 texture: four counterpoint notes for every one cantus note.

In this preset, you are checked by two rule sets: shared core voice-leading rules plus Species 3 rules. Tonal harmony extras are not included here, and advanced invertible-counterpoint rules are deferred.

Think of this mode as: keep the downbeat stable, let weak beats move, and control dissonance by pattern.

## Quick Checklist
- Write 4 counterpoint notes for each non-final cantus note; allow cadence compression in the final cantus window.
- Make every downbeat interval consonant.
- Use dissonance only on weak beats, and only in licensed patterns.
- Licensed weak-beat dissonance patterns are passing, neighbor, and cambiata (with double-neighbor recognized through the pattern rule).
- If you leap away from a dissonant note, it should be a valid cambiata shape.
- Avoid downbeat unisons unless you have a strong musical reason.
- Avoid parallel perfects and direct perfect approaches.
- End with a clear perfect consonance at the cadence.

## Core Rules You Must Pass
### Species 3 essentials
- Rhythm profile: the line stays 4:1 in non-final windows; the final cadence window may use fewer onsets.
- Downbeats: beat 1 of each group should be consonant.
- Weak beats: dissonance is allowed only when the motion pattern licenses it.
- Leap from dissonance: generally avoided; the main limited exception is a proper cambiata.
- Downbeat unison: treated as a caution sign, not a model texture.

### Shared general checks (grouped)
- Input/setup checks: valid exercise format, supported species, supported meter and voice count.
- Harmonic legality checks: supported sonorities and cadence closure requirements.
- Motion-between-voices checks: no parallel perfects; direct perfects are treated strictly in species presets.
- Spacing/texture checks: spacing limits, crossing/overlap limits, and interior unison cautions.
- Melody-shape checks: leap limits, dissonant melodic leap bans, leap recovery, climax shape, and repeated large leap control.

### Count Rules (Total and Consecutive Notes/Intervals)
- Total notes: counterpoint keeps `4:1` through non-final windows; cadence endings may reduce onset count in the final window.
- Consecutive notes (local weak-beat checks): weak-beat dissonance is legal only when its surrounding note-to-note motion fits licensed passing/neighbor/cambiata behavior.
- Total vertical intervals: four interval events per cantus note in normal 4:1 writing.
- Consecutive intervals (structural): beat 1 of each four-note group is the structural interval and must be consonant.
- Consecutive intervals (global): successive sampled intervals are still checked by general motion rules (parallel/direct perfect handling), plus species-specific downbeat-unison caution.
- Consecutive imperfect parallels: no more than 3 consecutive parallel generic 3rds or 6ths in the same voice pair.

### Leap Rules (Clear Defaults)
Active leap behavior in Species 3 (default values):
- Maximum leap size: larger than 12 semitones is an error.
- Dissonant melodic leaps: 6, 10, or 11 semitones are errors.
- Consecutive large leaps: two 5+ semitone leaps in a row, same direction, are warned.
- Leap recovery: after a 5+ semitone leap, next move should be contrary and stepwise (<= 2 semitones), otherwise warning.
- Species 3-specific addition: leap-from-dissonance is generally not allowed, except the licensed cambiata exception shape.

Severity is simple in practice: errors must be fixed; warnings are strong improvement signals. In species presets, direct-perfect motion is enforced as an error.

## Common Mistakes and How to Fix Them
1. Rhythm mismatch (not 4:1)
What to do: `Cantus: C | D` and `Counterpoint: E F G A | F E D C`.
What to avoid: `Cantus: C | D` and `Counterpoint: E F G | F E D C`.
Fix: recount every bar so each cantus note carries exactly four counterpoint notes.

2. Dissonant downbeat
What to do: over cantus `C`, start the group on `E` or `G` (consonant).
What to avoid: over cantus `C`, start the group on `D` or `Bb` (dissonant).
Fix: move dissonance to a weak beat and keep beat 1 stable.

3. Weak-beat dissonance without a licensed shape
What to do (passing): `E D C B` over a held cantus tone (stepwise flow).
What to do (neighbor): `E F E D` (step away, then back).
What to avoid: `E G F E` when the dissonant note is entered by leap.
Fix: rework the local figure so dissonance is step-connected as passing or neighbor.

4. Cambiata done correctly
What to do: `F E C D E` as a cambiata-style cell (step to dissonance, leap, then stepwise recovery).
What to avoid: random leap patterns that only resemble cambiata by accident.
Fix: keep the recognizable cambiata contour when using leap-from-dissonance behavior.

5. Invalid leap from dissonance
What to do: if a dissonant weak beat must leap away, use a clean cambiata shape.
What to avoid: `F E C# B A` where the dissonant note leaves by leap into an unrelated contour.
Fix: either convert to stepwise passing/neighbor motion or rewrite as a proper cambiata.

6. Too many downbeat unisons
What to do: prefer thirds, sixths, and other stable consonant intervals on downbeats.
What to avoid: repeated bar openings where both voices land on the same pitch.
Fix: revoice downbeats to keep independence between lines.

## How Diagnostics Map to the Music
- `error` means the submission fails until you fix that spot.
- `warning` means musically weak or stylistically risky; improve it even if playback still works.
- Most diagnostics point to one note (the problem) and sometimes a second related note (the vertical pair or motion partner).

Quick interpretation guide:
- “expects 4:1 note ratio” -> your note counts are off.
- “beat 1 must be consonant” -> a downbeat interval is dissonant.
- “dissonance must be passing/neighbor pattern” -> weak-beat dissonance contour is not licensed.
- “leap-from-dissonance should follow cambiata schema” -> leap exception was used, but not as a real cambiata.
- “downbeat unison is discouraged” -> musical independence is thinning out.

## Rule ID Reference
| Rule ID | Human meaning |
|---|---|
| `sp3.rhythm.four_to_one_only` | Species 3 stays 4:1 in non-final windows; cadence endings may compress the final window. |
| `sp3.strong_beat.consonance_required` | Each downbeat vertical interval must be consonant. |
| `sp3.dissonance.passing_neighbor_patterns_only` | Weak-beat dissonance is limited to licensed passing/neighbor-type patterns. |
| `sp3.dissonance.cambiata_limited_exception` | Leap-from-dissonance is only acceptable in valid cambiata behavior. |
| `sp3.downbeat_unison_forbidden` | Downbeat unison is discouraged in this texture. |
| `gen.input.single_exercise_per_file` | One exercise per file. |
| `gen.input.key_signature_required_and_stable` | Key signature must be present and stable. |
| `gen.input.timesig_supported` | Time signature must be supported. |
| `gen.input.species_supported` | Species value must be recognized. |
| `gen.input.voice_count_supported` | Voice count must be within supported limits. |
| `gen.input_min_note_length.eighth_or_longer` | Notes must not be shorter than the minimum supported value. |
| `gen.harmony.supported_sonorities` | Vertical sonorities must stay within supported complexity. |
| `gen.motion.parallel_perfects_forbidden` | Parallel perfect intervals are not allowed. |
| `gen.motion.direct_perfects_restricted` | Direct approach to perfect intervals is restricted (strict in species presets). |
| `gen.motion.consecutive_parallel_imperfects_limited` | Limit consecutive parallel generic 3rds/6ths to at most 3. |
| `gen.spacing.upper_adjacent_max_octave` | Adjacent upper voices should stay within an octave. |
| `gen.spacing.tenor_bass_max_twelfth` | Tenor-bass spacing should stay within a twelfth. |
| `gen.voice_crossing_and_overlap.restricted` | Voice crossing and overlap are restricted. |
| `gen.unison.interior_restricted` | Interior unisons are discouraged. |
| `gen.melody.max_leap_octave` | Very large melodic leaps are restricted. |
| `gen.melody.dissonant_leaps_forbidden` | Dissonant melodic leaps are forbidden. |
| `gen.melody.post_leap_compensation_required` | Large leaps should be compensated by contrary stepwise motion. |
| `gen.cadence.final_perfect_consonance_required` | Final sonority must be a perfect consonance. |
| `gen.interval.p4_dissonant_against_bass_in_two_voice` | Perfect fourth above bass is treated as dissonant in two-voice logic. |
| `gen.motion.contrary_and_oblique_preferred` | Prefer contrary/oblique motion over too much similar motion. |
| `gen.melody.single_climax_preferred` | Prefer one clear high point in a line. |
| `gen.melody.consecutive_large_leaps_restricted` | Repeated large leaps in the same direction are restricted. |

Deferred advanced note: invertible-counterpoint advanced rules (`adv.invertible.*`) are intentionally deferred in this preset.

## Sources
- `docs/planning/rules-presets.json`
- `docs/planning/rules-canonical.md`
