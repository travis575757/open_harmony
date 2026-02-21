# Species 2 (Second Species) — Practical Rules Guide

## What This Mode Is
Second species means **two counterpoint notes against one cantus note** (2:1 rhythm). Think of each bar as a strong beat plus a weak beat.

In this mode, the strong beat carries the structure, so it must be consonant. A weak beat may be dissonant, but only as a passing tone that moves by step between consonances.

Preset truth for this mode:
- Preset: `species2` (base profile: strict species)
- Active rule groups: core general rules + species 2 rules
- Excluded groups: tonal general rules + deferred advanced rules
- Active total: 27 rules (21 general + 6 species-2 specific)

Severity is simple: some findings are **errors** (must fix), some are **warnings** (strongly recommended). In species presets, direct perfects are treated strictly as an **error**.

## Quick Checklist
- Keep a strict 2:1 note ratio against the cantus, with an allowed compressed cadence in the final cantus window.
- Make every downbeat interval consonant.
- If a weak beat is dissonant, make it a passing tone by step in one direction.
- Check downbeats as a skeleton: avoid parallel perfects across consecutive downbeats.
- Avoid downbeat unisons unless you have a clear musical reason.
- Follow general voice-leading basics: clean motion, reasonable spacing, singable melody, proper cadence, valid input setup.

## Core Rules You Must Pass
Species 2 behavior in practice:
- **Rhythm profile:** two notes against one in non-final windows; the final cadence window may use fewer note onsets (for long-note cadence variants).
- **Downbeat consonance:** the first note of each pair must form a consonance with the cantus.
- **Weak-beat dissonance license:** dissonance is allowed only as a passing move by step, continuing in the same direction.
- **Downbeat skeleton check:** compare only downbeats for structural parallels; do not chain perfect intervals in parallel.
- **Downbeat unison caution:** unison on strong beats is discouraged and usually weakens independence.

Shared general rules (grouped for musicians):
- **Input/setup:** one exercise, supported species/time/voice setup, stable key signature, usable note lengths.
- **Motion:** avoid parallel perfects, avoid direct/hidden perfects in strict style, favor contrary/oblique motion.
- **Spacing and independence:** keep voices in practical ranges and spacing, avoid crossing/overlap, limit interior unisons.
- **Melody quality:** avoid unsingable leaps, avoid dissonant leaps, compensate big leaps, avoid repeated large leaps in one direction, keep one clear high point.
- **Cadence and final sonority:** end with a proper perfect consonance closure.

### Count Rules (Total and Consecutive Notes/Intervals)
- Total notes: counterpoint keeps `2:1` through non-final windows; cadence endings may reduce onset count in the final window.
- Consecutive notes (local weak-beat check): if a weak beat is dissonant, previous-current-next counterpoint notes must form stepwise passing motion in one direction, and the surrounding notes must be consonant against the cantus.
- Total vertical intervals: two interval events per cantus note in normal 2:1 writing (strong + weak).
- Consecutive intervals (structural): downbeats form a skeleton checked for consecutive parallel perfects.
- Consecutive intervals (global): general motion rules still evaluate successive sampled intervals for parallel/direct perfect behavior.
- Consecutive imperfect parallels: no more than 3 consecutive parallel generic 3rds or 6ths in the same voice pair across successive interval events (weak beats included).

### Leap Rules (Clear Defaults)
These leap rules are active in Species 2 (default values):
- Maximum leap size: larger than 12 semitones is an error.
- Dissonant melodic leaps: 6, 10, or 11 semitones are errors.
- Consecutive large leaps: two 5+ semitone leaps in a row, same direction, are warned.
- Leap recovery: after a 5+ semitone leap, next move should be contrary and stepwise (<= 2 semitones), otherwise warning.
- Species 2-specific addition: weak-beat dissonance must be passing-stepwise, so a weak-beat dissonance cannot be approached/departed by leap.

Advanced invertible-counterpoint rules are intentionally deferred in this preset.

## Common Mistakes and How to Fix Them
1. **Wrong rhythm count (not 2:1)**
What to do: `CF: whole | whole` and `CP: half half | half half`
What to avoid: `CP: half half half | half` (ratio breaks)

2. **Dissonant downbeat**
What to do: `Downbeat interval = 3rd/6th/5th/8ve`
What to avoid: `Downbeat interval = 4th above bass or tritone`

3. **Weak-beat dissonance by leap**
What to do: `E - F - G` over one cantus note if F is the weak-beat dissonance
What to avoid: `E - B - A` when B is weak-beat dissonance (leap into/out of it)

4. **Weak-beat direction change (not passing)**
What to do: `G - A - B` (or `B - A - G`) for a true passing motion
What to avoid: `G - A - G` when A is dissonant (neighbor shape, not passing)

5. **Parallel perfects in the downbeat skeleton**
What to do: downbeats move with varied interval classes and contrary/oblique tendency
What to avoid: two consecutive downbeats that form perfect fifth then perfect fifth (or octave then octave) in similar motion

6. **Too many downbeat unisons**
What to do: reserve unison for exceptional structural moments
What to avoid: repeated strong-beat unisons that make the two lines sound like one line

## How Diagnostics Map to the Music
Read diagnostics as rehearsal notes, not code notes:
- **Error** means the line breaks a required species rule and must be rewritten.
- **Warning** means the line is legal enough to run but musically weaker; improve it if possible.

How to interpret locations:
- **Primary note**: the note to inspect first.
- **Related note**: the other voice note involved in the issue (if present).
- **Message text**: usually tells you the exact musical category (ratio, downbeat consonance, passing-tone handling, parallels, unison preference).

Fast triage order:
1. Fix rhythm ratio first.
2. Fix all downbeat consonance problems.
3. Fix weak-beat dissonance handling.
4. Fix structural parallel perfects on downbeats.
5. Address warnings (especially repeated unisons and stylistic motion warnings).

## Rule ID Reference
| Rule ID | Human meaning |
|---|---|
| `gen.input.single_exercise_per_file` | Exactly one exercise is expected per file. |
| `gen.input.key_signature_required_and_stable` | Key signature must be present and unchanged. |
| `gen.input.timesig_supported` | Time signature must be supported. |
| `gen.input.species_supported` | Species label must be supported. |
| `gen.input.voice_count_supported` | Voice count must be in supported range. |
| `gen.input_min_note_length.eighth_or_longer` | Notes shorter than an eighth are not allowed. |
| `gen.harmony.supported_sonorities` | Vertical sonorities must stay within supported types. |
| `gen.motion.parallel_perfects_forbidden` | Parallel fifths/octaves are forbidden. |
| `gen.motion.direct_perfects_restricted` | Direct approach to perfect intervals is restricted (strict in species presets). |
| `gen.motion.consecutive_parallel_imperfects_limited` | Limit consecutive parallel generic 3rds/6ths to at most 3. |
| `gen.spacing.upper_adjacent_max_octave` | Adjacent upper voices should stay within an octave. |
| `gen.spacing.tenor_bass_max_twelfth` | Tenor-bass spacing should stay within a twelfth. |
| `gen.voice_crossing_and_overlap.restricted` | Voice crossing/overlap is restricted. |
| `gen.unison.interior_restricted` | Interior unisons are restricted/discouraged. |
| `gen.melody.max_leap_octave` | Melodic leaps larger than an octave are forbidden. |
| `gen.melody.dissonant_leaps_forbidden` | Dissonant melodic leaps are forbidden. |
| `gen.melody.post_leap_compensation_required` | Large leaps should be compensated stepwise in contrary motion. |
| `gen.cadence.final_perfect_consonance_required` | Final sonority must be a perfect consonance. |
| `gen.interval.p4_dissonant_against_bass_in_two_voice` | A fourth above the bass is treated as dissonant in two voices. |
| `gen.motion.contrary_and_oblique_preferred` | Contrary/oblique motion is preferred over too much similar motion. |
| `gen.melody.single_climax_preferred` | A single clear melodic climax is preferred. |
| `gen.melody.consecutive_large_leaps_restricted` | Consecutive large leaps in one direction are restricted. |
| `sp2.rhythm.two_to_one_only` | Species 2 keeps 2:1 in non-final windows; cadence endings may compress the final window. |
| `sp2.strong_beat.consonance_required` | Every downbeat must be consonant. |
| `sp2.dissonance.weak_passing_stepwise` | Weak-beat dissonance is allowed only as stepwise passing motion. |
| `sp2.structure.downbeat_skeleton_no_parallel_perfects` | Downbeat skeleton cannot form parallel perfect intervals. |
| `sp2.downbeat_unison_discouraged` | Downbeat unison is discouraged. |

## Sources
- `docs/planning/rules-presets.json` (preset composition, groups, severity override, deferred list)
- `docs/planning/rules-canonical.md` (canonical rule meanings)
- `docs/planning/rules-mapping.csv` (rule mapping references)
- `docs/planning/rules-test-fixtures.md` (fixture coverage context)
