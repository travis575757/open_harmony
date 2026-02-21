# Rules Test Fixtures (Task 1)

## Fixture Sources
Primary fixture pool:
- `docs/demos/AiHarmony/musicxml/ca3-examples/*.xml`
- `docs/demos/AiHarmony/musicxml/good-cp1.xml`
- `docs/demos/AiHarmony/musicxml/good-cp2.xml`
- `docs/demos/AiHarmony/musicxml/good-cp3.xml`
- `docs/demos/AiHarmony/musicxml/good-cp4.xml`
- `docs/demos/AiHarmony/musicxml/2018-04-norm-cp5.xml`
- `docs/demos/AiHarmony/musicxml/2018-04-bad-cp5.xml`

## Baseline Fixture Catalog
- Positive baseline:
  - `docs/demos/AiHarmony/musicxml/good-cp1.xml`
  - `docs/demos/AiHarmony/musicxml/good-cp2.xml`
  - `docs/demos/AiHarmony/musicxml/good-cp3.xml`
  - `docs/demos/AiHarmony/musicxml/good-cp4.xml`
  - `docs/demos/AiHarmony/musicxml/2018-04-norm-cp5.xml`
- Negative baseline:
  - `docs/demos/AiHarmony/musicxml/2018-04-bad-cp5.xml`

## Rule-to-Fixture Matrix

| Rule family | Positive fixtures | Negative fixtures | Notes |
|---|---|---|---|
| `gen.input.*` | `good-cp1.xml`, `good-cp2.xml` | `2018-04-bad-cp5.xml` | Input validation negatives need synthetic files for each parser error case. |
| `gen.harmony.supported_sonorities` | `good-cp1.xml`, `good-cp4.xml` | `2018-04-bad-cp5.xml` | Add explicit unsupported-sonority synthetic negatives (9th/extended sonorities). |
| `gen.interval.p4_dissonant_against_bass_in_two_voice` | `ca3-examples/Species-1-exercise.xml` | `2018-04-bad-cp5.xml` | Add isolated two-voice P4-against-bass negative fixture. |
| `gen.motion.parallel_perfects_forbidden` | `good-cp1.xml`, `gallon-v2sp2-1.xml` | `2018-04-bad-cp5.xml` | Add synthetic minimal pair to isolate pure parallel 5th/8ve. |
| `gen.motion.direct_perfects_restricted` | `gallon-v3s1-2.xml` | `2018-04-bad-cp5.xml` | Requires dedicated direct-interval edge fixture. |
| `gen.motion.contrary_and_oblique_preferred` | `good-cp2.xml`, `gallon-v3s1-2.xml` | `2018-04-bad-cp5.xml` | Add excessive-similar-motion synthetic negative fixture. |
| `gen.spacing.*` | `gallon-v4s1-1.xml`, `gallon-v4s5-1.xml` | `2018-04-bad-cp5.xml` | Add explicit spacing-violation synthetic fixtures. |
| `gen.voice_crossing_and_overlap.restricted` | `good-cp2.xml` | `2018-04-bad-cp5.xml` | Add crossing-specific synthetic fixture. |
| `gen.unison.interior_restricted` | `good-cp1.xml` | `2018-04-bad-cp5.xml` | Needs explicit interior-unison case. |
| `gen.melody.*` | `good-cp1.xml`, `gallon-v2sp3-1.xml` | `2018-04-bad-cp5.xml` | Add leap-specific synthetic negatives. |
| `gen.melody.single_climax_preferred` | `good-cp3.xml`, `gallon-v3s3-4.xml` | `2018-04-bad-cp5.xml` | Add multi-climax contour synthetic negative fixture. |
| `gen.melody.consecutive_large_leaps_restricted` | `good-cp2.xml`, `gallon-v2sp3-2.xml` | `2018-04-bad-cp5.xml` | Add repeated-large-leap synthetic negative fixture. |
| `gen.cadence.*` | `good-cp1.xml`, `good-cp4.xml` | `2018-04-bad-cp5.xml` | Add cadence-only failing sample to reduce noise. |
| `sp1.*` | `ca3-examples/Species-1-exercise.xml`, `gallon-v3s1-2.xml` | `2018-04-bad-cp5.xml` | Add first-species focused invalid rhythm fixture. |
| `sp2.*` | `gallon-v2sp2-1.xml`, `gallon-v3s2-2.xml` | `2018-04-bad-cp5.xml` | Add weak-beat leap-from-dissonance negative. |
| `sp3.*` | `gallon-v2sp3-1.xml`, `gallon-v2sp3-2.xml`, `gallon-v3s3-4.xml` | `2018-04-bad-cp5.xml` | Add dedicated cambiata-invalid and accented dissonance negatives. |
| `sp4.*` | `gallon-v2sp4-1.xml`, `gallon-v2sp4-3.xml`, `gallon-v3s4-4.xml` | `2018-04-bad-cp5.xml` | Add suspension prep/resolution isolated negatives. |
| `sp5.*` | `ca3-examples/2018-04-ideal-cp5.xml`, `ca3-examples/good-cp5-extract.xml`, `gallon-v4s5-1.xml` | `2018-04-bad-cp5.xml` | Add eighth-position and cadence-specific negatives. |

## Scenario Packs for Task 2
- SP-A First species strict: `Species-1-exercise.xml`, `good-cp1.xml`.
- SP-B Weak-beat dissonance legality: `gallon-v2sp2-1.xml`, `gallon-v2sp3-1.xml`.
- SP-C Suspension syntax: `gallon-v2sp4-1.xml`, `gallon-v2sp4-4.xml`.
- SP-D Florid composition: `ca3-examples/2018-04-ideal-cp5.xml`, `ca3-examples/short.xml`, `ca3-examples/good-cp5-extract.xml`.
- SP-E Regression noisy negative: `2018-04-bad-cp5.xml`.

## Coverage Gaps (Known)
- No guaranteed one-rule-only failing fixture for every active rule.
- Need synthetic micro-fixtures for:
  - direct perfects (outer vs inner voices)
  - isolated crossing/overlap
  - interior unison
  - suspension without preparation
  - suspension wrong-direction resolution
  - illegal species-specific rhythm tokens

## Acceptance for Task 1 Output
- Every active rule family has at least one mapped positive and one mapped negative candidate.
- Gaps are explicitly listed so Task 2 can close them with generated fixtures.
