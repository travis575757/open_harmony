# Species 1 (First Species) — Practical Rules Guide

## What This Mode Is
Species 1 is strict note-against-note counterpoint: one counterpoint note for each cantus firmus note (1:1 rhythm).

In this preset, every vertical sonority must be consonant. You open with a perfect consonance, close on unison or octave, and usually place an imperfect consonance on the penultimate sonority.

This mode includes the shared core counterpoint checks plus Species 1-specific checks. Tonal-only and advanced deferred rule groups are not active here.

## Quick Checklist
- Keep a strict 1:1 rhythm for both voices.
- Use only consonant vertical intervals at each note pair.
- Start with a perfect consonance (unison, fifth, or octave class).
- End on unison or octave.
- Aim for an imperfect consonance (3rd or 6th class) on the penultimate sonority.
- Avoid parallel perfects and avoid direct perfect approaches by similar motion.
- Keep melody singable: no giant awkward leaps, and balance leaps with stepwise recovery.

Severity in plain terms:
- Hard errors: must be fixed to pass.
- Style warnings: musically recommended improvements.
- In species presets, direct perfects are treated strictly (hard error, not just style).

## Core Rules You Must Pass
Species 1 has two layers of checks: species-specific rules and shared core rules.

Species 1-specific expectations:
- Rhythm is one note against one note, all the way through.
- Vertical intervals are consonant only.
- Opening sonority is perfect.
- Final sonority is unison or octave.
- Penultimate sonority should be imperfect (usually a 3rd or 6th).

Shared core expectations (grouped):
- Input validity: one exercise at a time, supported species/time signature/voice setup, and usable note lengths.
- Motion: no parallel perfects; direct perfect approaches are strict in this preset; contrary/oblique motion is preferred.
- Spacing and texture: avoid awkward spacing, voice crossing/overlap, and overused interior unisons.
- Melody shape: avoid dissonant or oversized leaps, avoid chains of large leaps, and keep one clear high point.
- Cadence discipline: finish with a proper perfect close.

### Count Rules (Total and Consecutive Notes/Intervals)
- Total notes: write 1 counterpoint note for each cantus note (1:1 profile).
- Consecutive notes: paired notes should use equal durations at each aligned position.
- Total vertical intervals: one structural interval per note pair in this species.
- Consecutive intervals: every successive interval pair is still checked by general motion rules (especially no parallel perfects and strict direct-perfect handling).
- Consecutive imperfect parallels: no more than 3 consecutive parallel generic 3rds or 6ths in the same voice pair.
- Final interval sequence: last two sonorities are cadence-critical (penultimate usually imperfect, final perfect unison/octave class).

### Leap Rules (Clear Defaults)
These are the active leap rules in this mode (default values):
- Maximum leap size: larger than 12 semitones (more than an octave) is an error.
- Dissonant melodic leaps: 6, 10, or 11 semitones are errors.
- Consecutive large leaps: two large leaps in a row (default large = 5+ semitones), same direction, are warned.
- Leap recovery: after a large leap (default 5+), the next move should be contrary and stepwise (default step <= 2 semitones), otherwise warning.
- Species 1 practical takeaway: keep lines mostly stepwise, use larger leaps sparingly, and recover immediately.

Short note on deferred advanced rules:
- Advanced invertible-counterpoint constraints are intentionally deferred in this mode and not part of pass/fail here.

## Common Mistakes and How to Fix Them
What to avoid:
```text
CF: C4  D4  E4  F4
CP: G4  A4  B4  C5  D5
```
Two CP notes against one CF note breaks Species 1 rhythm.

What to do:
```text
CF: C4  D4  E4  F4
CP: G4  F4  G4  A4
```
One note in CP for each CF note.

What to avoid:
```text
CF: C4  D4  E4  F4
CP: F4  G4  A4  Bb4
```
Opening on a 4th above bass (C-F) is treated as dissonant here.

What to do:
```text
CF: C4  D4  E4  F4
CP: G4  F4  G4  A4
```
Open with a perfect consonance (C-G).

What to avoid:
```text
CF: D4  C4
CP: A4  G4
```
Penultimate is a perfect 5th (D-A), which is weak for this cadence style.

What to do:
```text
CF: D4  C4
CP: F4  C5
```
Penultimate is an imperfect consonance (D-F), then close on octave/unison class (C-C).

## How Diagnostics Map to the Music
Each diagnostic points to a place in the score and, when relevant, the matching note in the other voice.

How to read it quickly:
- Location (primary note): where the issue is reported.
- Related note: the other voice note involved in the same interval/motion check.
- Message meaning: musical intent (for example, dissonance, parallel perfect, cadence issue).
- Severity: error means required fix; warning means stylistic guidance.

Practical workflow in the editor:
- Jump to the reported note pair.
- Check the vertical interval first.
- Then check how you approached it melodically (especially if perfect intervals are involved).
- Re-test after each small fix so you do not introduce a new motion issue elsewhere.

## Rule ID Reference
| Rule ID | Human meaning |
|---|---|
| `gen.input.single_exercise_per_file` | Keep one exercise per file/session. |
| `gen.input.key_signature_required_and_stable` | Use a valid key signature and keep it stable. |
| `gen.input.timesig_supported` | Use a supported time signature. |
| `gen.input.species_supported` | Exercise must declare a supported species mode. |
| `gen.input.voice_count_supported` | Use a supported number of voices. |
| `gen.input_min_note_length.eighth_or_longer` | Do not use durations shorter than an eighth note. |
| `gen.harmony.supported_sonorities` | Keep sonorities within supported harmonic vocabulary. |
| `gen.motion.parallel_perfects_forbidden` | No parallel perfect fifths or octaves. |
| `gen.motion.direct_perfects_restricted` | Direct/similar-motion approaches to perfects are restricted (strict here). |
| `gen.motion.consecutive_parallel_imperfects_limited` | Limit consecutive parallel generic 3rds/6ths to at most 3. |
| `gen.spacing.upper_adjacent_max_octave` | Keep adjacent upper voices within reasonable spacing. |
| `gen.spacing.tenor_bass_max_twelfth` | Avoid excessive tenor-bass distance. |
| `gen.voice_crossing_and_overlap.restricted` | Avoid voice crossing and overlap. |
| `gen.unison.interior_restricted` | Limit interior unisons away from opening/closing function. |
| `gen.melody.max_leap_octave` | Do not leap more than an octave melodically. |
| `gen.melody.dissonant_leaps_forbidden` | Avoid melodically dissonant leaps. |
| `gen.melody.post_leap_compensation_required` | After a large leap, recover with contrary stepwise motion. |
| `gen.cadence.final_perfect_consonance_required` | End with a perfect final sonority. |
| `gen.interval.p4_dissonant_against_bass_in_two_voice` | Treat a fourth above the bass as dissonant in two voices. |
| `gen.motion.contrary_and_oblique_preferred` | Prefer contrary/oblique motion over too much similar motion. |
| `gen.melody.single_climax_preferred` | Aim for one clear melodic high point. |
| `gen.melody.consecutive_large_leaps_restricted` | Avoid strings of large leaps. |
| `sp1.rhythm.one_to_one_only` | Species 1 is strict 1:1 rhythm. |
| `sp1.vertical.consonance_only` | Every vertical sonority must be consonant. |
| `sp1.opening.perfect_consonance_required` | Opening sonority must be perfect. |
| `sp1.ending.unison_or_octave_required` | Final sonority must be unison or octave class. |
| `sp1.cadence.penultimate_imperfect_consonance` | Penultimate sonority should be an imperfect consonance. |

## Sources
- `docs/planning/rules-presets.json` (preset composition, included/excluded groups, severity override, deferred rules)
- `docs/planning/rules-canonical.md` (canonical rule meanings)
