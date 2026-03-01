# cp_music21_compat

Phase 2 compatibility layer for the full `music21` behavior surface exercised by AugmentedNet.

## Compatibility matrix (implemented vs out of scope)
### Implemented
| Status | Rust API | Python baseline counterpart concept | Notes |
| --- | --- | --- | --- |
| Implemented | `parse_musicxml` | `music21.converter.parse(...)` on `score-partwise` MusicXML | Parses the subset required by AugmentedNet fixtures and preprocessing flow. |
| Implemented | `build_timeline` | `stream.chordify()` + vertical offset iteration | Deterministic vertical slices with onset/hold/tie flags and interval-from-bass labels. |
| Implemented | `augnet_initial_frames` | `score_parser._initialDataFrame` | Event-based rows (`s_offset`, `s_duration`, `s_measure`, `s_notes`, `s_intervals`, `s_is_onset`). |
| Implemented | `augnet_reindex_frames` | `score_parser._reindexDataFrame` | Fixed-grid reindex behavior with fill semantics and deterministic ordering. |
| Implemented | `interval_label`, `simple_interval_name` | `music21.interval.Interval(...).name/.simpleName` | Spelling-sensitive interval quality with enharmonic and double accidental behavior. |
| Implemented | `parse_interval_spec`, `interval_class_info` | `music21.interval.Interval(x)` fields used by AugNet caches | Supports AugmentedNet interval class parsing semantics (including uppercase diminished alias). |
| Implemented | `transpose_pitch_m21`, `transpose_key_m21`, `transpose_pcset` | AugmentedNet transposition cache helpers | String spellings follow music21/AugNet conventions (e.g. `-` for flats). |
| Implemented | `weber_euclidean` | `keydistance.weberEuclidean` | Weber distance metric used by AugNet tonicization/keydistance logic. |
| Implemented | `tonicization_denominator`, `tonicization_scale_degree` | AugmentedNet tonicization denominator helpers | Includes minor-mode case handling used in baseline behavior. |
| Implemented | `serialize_timeline_artifact`, `serialize_augnet_frames` | Parity artifact snapshots | Stable JSON output for deterministic regression fixtures. |

### Out of scope (intentional)
| Status | Area | Why out of scope |
| --- | --- | --- |
| Out of scope | Full generic `music21` API parity | Phase 2 targets only AugmentedNet call surface, not all library capabilities. |
| Out of scope | Counterpoint/species analysis toolkits | Not used by AugmentedNet preprocessing/postprocessing path. |
| Out of scope | Schenkerian/phrase/form/cadence analysis modules | Higher-level musicological analysis is not required by AugmentedNet inference. |
| Out of scope | Post-tonal set-class/Forte analytical pipelines | Not part of AugmentedNet tonal feature and decode path. |
| Out of scope | Engraving/layout/ornament/articulation semantics | Rendering/performance details are outside compatibility requirements. |
| Out of scope | Non-MusicXML ingest (`midi`, `abc`, `humdrum`, etc.) | Current compatibility target is MusicXML-oriented AugmentedNet path. |
| Out of scope | Training/model architecture logic | This crate is preprocessing/postprocessing compatibility only. |

## API boundary
Use `Music21CompatApi` as the integration seam. Future native replacements should implement this trait so ONNX adapter and UI layers can remain unchanged.

## Direct music21 chord parity tests
The repository includes direct parity tests that run `music21` chordify/interval behavior and compare against `cp_music21_compat` timeline output on the same fixtures.

Run:

```bash
uv run --python 3.10 --with-requirements tools/augnet/requirements-diff.txt \
  pytest tests/music21_compat/test_chord_functions_parity.py -q
```

These tests validate:
- slice boundaries and measure-number handling,
- onset/hold/tie semantics,
- enharmonic interval labels,
- deterministic repeated export behavior.

## When-in-Rome corpus parity (real music)
Pinned CI corpus manifest:
- `tests/corpora/when_in_rome_ci_manifest.txt` (40 real scores)

Run direct corpus parity (music21 baseline vs Rust timeline export):

```bash
WHEN_IN_ROME_ROOT=/path/to/When-in-Rome \
uv run --python 3.10 --with-requirements tools/augnet/requirements-diff.txt \
  pytest tests/music21_compat/test_when_in_rome_parity.py -q
```

Optional:
- `WHEN_IN_ROME_MAX_PIECES` to run a smaller subset (default `40`).

## Baseline concept mapping
- `parse_musicxml` -> `music21.converter.parse(...)` (subset).
- `augnet_initial_frames` -> `score_parser._initialDataFrame`.
- `augnet_reindex_frames` -> `score_parser._reindexDataFrame`.
- `build_timeline` -> `stream.chordify()` + vertical offset iteration.
- tie onset/hold behavior -> tied continuation semantics in chordified streams.
- pickup shift -> pickup/anacrusis measure-number shift behavior.
- `interval_label` / `simple_interval_name` -> `music21.interval.Interval(...).name/simpleName`.
- `transpose_pitch_m21` / `transpose_key_m21` / `transpose_pcset` -> AugmentedNet cache transposition helpers.
- `tonicization_scale_degree` -> AugmentedNet tonicization denominator helper (`keydistance.getTonicizationScaleDegree` concept).
