# AugmentedNet ONNX Port Plan (Rust + ONNX Runtime)

## Purpose
Port AugmentedNet inference to a native Rust path using `tf2onnx` + ONNX Runtime (`ort`), with exact preprocessing/postprocessing parity and a strong regression/parity test harness.

This plan is scoped to inference only (no training) and supports selecting analysis backend at runtime.

## Confirmed Decisions
1. Use `tf2onnx` to convert Keras `.hdf5` model to ONNX.
2. Use ONNX Runtime via Rust `ort` crate.
3. Support all AugmentedNet output heads.
4. CPU-only runtime for now.
5. Port preprocessing exactly (not approximate).
6. Expose raw logits in engine output.
7. Allow selecting analysis backend (`rule-based`, `augnet-onnx`, `hybrid`).
8. Model size is acceptable.
9. Add sufficient corpus-level parity testing, including When-in-Rome driven checks.

## Clarification: Sequence Length (Frozen vs Dynamic)
- Frozen sequence length:
  - ONNX graph has fixed `T` (time axis length).
  - Inputs must be padded/truncated/windowed to exactly `T`.
  - Usually simpler and closer to original training/inference assumptions.
- Dynamic sequence length:
  - ONNX graph allows variable `T`.
  - Better ergonomics for arbitrary piece lengths.
  - Slightly more runtime/shape-handling complexity.

### Plan decision
Use **frozen/fixed `T` only** for this port. Dynamic-length ONNX export is out of scope for this effort.

## Architecture

### Components
1. `harmony_ml` Rust module/crate
   - ONNX session management.
   - AugmentedNet feature encoder (exact parity).
   - AugmentedNet decoder/postprocessor (exact parity).
2. `augnet_parity_bridge` validation tools
   - Direct Python `music21` + AugmentedNet baseline runner.
   - Stage-by-stage artifact export for parity checks (timeline, tensors, head decodes, final labels).
   - Deterministic fixture manifest and baseline snapshots.
3. Model artifacts
   - Versioned ONNX file.
   - Manifest (`model id`, `source hdf5`, `opset`, `sha256`).
4. Tools
   - Conversion scripts (`.hdf5 -> .onnx`).
   - Golden output generation scripts from official Python AugmentedNet.
5. Engine integration
   - Analyzer backend selection and unified output schema.

### Runtime flow
1. Internal score -> exact AugmentedNet feature tensors.
2. Tensors -> ONNX Runtime session.
3. Per-head logits + argmax decode.
4. Exact RN/chord reconstruction.
5. Emit normalized harmonic slices + logits + metadata.

### Validation flow (direct reference baseline)
1. Run Python baseline (`music21` + official AugmentedNet) on same fixture inputs.
2. Export stage artifacts:
   - chordified/sliced timeline snapshot
   - encoded input tensors
   - per-head logits/argmax decodes
   - final harmonic labels (RN/chord/key/inversion/tonicization)
3. Run Rust ONNX path on same fixtures.
4. Compare stage-by-stage with strict thresholds and fail fast on first mismatch layer.

## Work Breakdown

## Phase 0: Ground Truth + Harness Setup
### Deliverables
- `tools/augnet/` scripts to:
  - run official Python AugmentedNet inference,
  - collect canonical per-head outputs,
  - normalize outputs for parity comparison.
- Fixture corpus definitions.

### Requirements
- Include small deterministic fixtures first.
- Include larger corpus fixtures and When-in-Rome-derived evaluation set.

### Tests / Acceptance
- Script can produce reproducible baseline snapshots from Python AugmentedNet.
- Snapshot format stable and versioned.

### Finalized decisions
- CI smoke corpus uses a pinned manifest file: `tests/corpora/when_in_rome_ci_manifest.txt`.
- Initial CI manifest size: 40 pieces (stratified across major composers/periods present in When-in-Rome).
- Nightly/extended CI runs the full configured When-in-Rome corpus manifest.

## Phase 1: Model Conversion Pipeline
### Deliverables
- `tools/augnet/convert_to_onnx.py`
- Conversion README with pinned versions.
- Model manifest file in repo.

### Requirements
- Fixed opset and deterministic conversion flags.
- Preserve all output heads and expected names/order.
- Preserve fixed sequence length input shape (`T`) used by the source model.

### Tests / Acceptance
- ONNX loads in Python ONNX Runtime and Rust `ort`.
- Output head count matches source model.
- Conversion reproducible from clean environment.
- Fixed-shape session rejects non-conforming input shapes unless padded/chunked first.

### Finalized decisions
- ONNX export opset is fixed to **13** for this port.
- Any opset change requires explicit changelog entry and parity re-baseline.

## Phase 2: Rust Port of Required music21 Functionality
### Deliverables
- `cp_music21_compat` Rust module implementing the required subset used by AugmentedNet:
  - score timeline extraction from MusicXML inputs
  - chordified/vertical slice construction equivalent to AugmentedNet needs
  - interval/pitch/key transform helpers used in input representations
  - tonicization denominator helper equivalent to current baseline behavior
- Deterministic serialization format for intermediate artifacts:
  - timeline slices
  - onset/hold flags
  - note spellings and interval labels
- Explicit compatibility boundaries (what is intentionally out of scope vs baseline-complete).

### Requirements
- Port only the subset required by AugmentedNet preprocessing/postprocessing paths; avoid full `music21` clone scope.
- Preserve behavior for known critical cases:
  - ties and held-note semantics
  - anacrusis measure-number shift behavior
  - enharmonic spelling-sensitive interval labels
  - offset and duration normalization semantics used by fixed-grid slicing
- Keep module API isolated so future native replacements can evolve without touching ONNX adapter or UI layers.
- Every implemented function must map to explicit Python baseline counterpart in documentation/comments.

### Tests / Acceptance
- Unit tests for each compatibility primitive (pitch/key/interval transform, slice construction, tie handling).
- Integration tests on fixture files with expected serialized timeline outputs.
- Edge-case fixture suite passes for:
  - tied notes across barlines
  - pickup bars
  - dense polyphonic overlap scenarios
  - enharmonic edge spellings (double accidentals where applicable)

### Finalized decisions
- This phase targets functional parity for the required subset only; full `music21` parity is explicitly out of scope.
- Compatibility outputs from this phase become the authoritative inputs for later differential validation gates.

## Phase 3: Direct music21 Differential Validation Harness
### Deliverables
- `tools/augnet/diff_against_music21.py` differential runner.
- Stage artifact exporter for Python baseline (`music21` + official AugmentedNet):
  - chordified/sliced timeline
  - encoded input tensors
  - per-head logits/argmax decodes
  - final harmonic labels
- Pinned baseline artifacts in `tests/augnet_parity/music21_baseline/`.
- CI job that runs differential comparisons and fails on first mismatching stage.

### Requirements
- Baseline environment must be pinned (Python version, `music21`, AugmentedNet model artifact, dependency lock).
- Differential harness must support stage-by-stage comparison, not only final labels.
- Fail-fast behavior: earliest mismatch stage terminates run and emits actionable diff report.
- Fixtures must include deterministic edge cases (ties, anacrusis, enharmonic spellings, chunk boundaries).

### Tests / Acceptance
- Differential runner executes end-to-end on local and CI with deterministic outputs.
- Stage-level parity checks pass on pinned fixture corpus:
  - timeline parity
  - tensor parity
  - model-output parity
  - final-label parity
- CI artifact includes machine-readable diff report when failures occur.

### Finalized decisions
- Direct Python baseline comparison is a hard gate before Rust parity claims are accepted.
- No phase can be marked complete if the previous phase breaks differential parity.

## Phase 4: Rust ONNX Runtime Adapter
### Deliverables
- `AugmentedNetOnnxBackend` implementation.
- Session init with CPU execution provider.
- Typed output mapping for all heads + logits.
- Direct cross-check harness wiring so Phase 4 runs against Python baseline fixtures before later phases.
- Subcomponent implementation milestones:
  - 4A: session bootstrap and model artifact validation
  - 4B: input tensor contract binding
  - 4C: output head mapping and logits capture
  - 4D: ONNX-boundary parity gate
  - 4E: performance and determinism baseline

### Requirements
- Robust shape validation and error reporting.
- No JS fallback behavior; hard failure when backend is selected but unavailable.
- Phase-gate parity checks against Python baseline at the ONNX boundary:
  - output head names/order
  - tensor shapes for each head
  - logits tolerance checks on deterministic fixtures
- Each subcomponent (4A-4E) must be independently testable with its own end-to-end harness and acceptance criteria.

### Tests / Acceptance
- Unit tests for tensor IO mapping.
- Smoke test for one score end-to-end with non-empty outputs.
- Baseline comparison tests pass for ONNX outputs on fixed fixtures before preprocessing/postprocessing port layers are integrated.
- Any head-name or head-order mismatch fails CI immediately.
- 4A Session bootstrap acceptance:
  - End-to-end health test loads model and validates manifest fields (`model id`, `opset`, `sha256`) before inference.
  - Corrupt/mismatched artifact test fails with explicit fatal diagnostics.
- 4B Input tensor contract acceptance:
  - End-to-end fixture run verifies ONNX input name/order/shape contract and fixed-`T` chunk ingress.
  - Contract mismatch tests fail before model execution with actionable error output.
- 4C Output head mapping acceptance:
  - End-to-end fixture run verifies all expected heads are present, correctly named, and mapped to typed outputs.
  - Raw logits are captured for every head and serialized consistently.
- 4D ONNX-boundary parity acceptance:
  - Differential end-to-end check (Python ONNX vs Rust ONNX on same tensors) passes logits tolerance and argmax equality gates.
  - CI stores per-head diff artifacts on failure.
- 4E Performance/determinism acceptance:
  - End-to-end benchmark suite captures cold-start and warm-run latency on small/medium/large fixtures.
  - Determinism check (single-thread CI config) produces stable repeated outputs on identical inputs.

### Finalized decisions
- Default ONNX Runtime threading is deterministic: `intra_threads=1`, `inter_threads=1`.
- Local overrides are allowed via config/env, but CI must remain single-threaded.
- Phase 4 completion requires all subcomponent acceptance checks (4A-4E) to pass independently.

## Phase 5: Exact Preprocessing Port
### Deliverables
- Rust equivalents for AugmentedNet score slicing and input representations.
- Parity utilities to compare Rust tensors vs Python tensors.

### Requirements
- Match AugmentedNet fixed grid behavior exactly.
- Match pitch spelling/key-sensitive encodings exactly.
- Match onset/duration and measure indexing behavior exactly.
- Match AugmentedNet fixed-length chunking/padding behavior exactly.

### Tests / Acceptance
- Feature-level golden tests (exact equality for categorical and binary tensors).
- Corpus parity pass rate target: 100% on frozen fixture set.
- Chunk boundary parity tests pass (same output across chunk joins as Python baseline).

### Finalized decisions
- No preprocessing deviations are permitted in parity mode.
- If internal score model differs, add an adapter layer that maps to AugmentedNet canonical preprocessing semantics.

## Phase 6: Exact Postprocessing Port
### Deliverables
- Rust decode for all heads.
- RN/chord reconstruction parity implementation.
- Raw logits in output schema.

### Requirements
- Preserve inversion logic and RN formatting rules.
- Preserve tonicization/local key conventions.
- Preserve incomplete-chord/root-assumption fallback behavior.

### Tests / Acceptance
- Exact match against Python-generated decoded outputs on fixture corpus.
- Snapshot tests for representative edge cases (cadential, tonicization, incomplete chords).

### Finalized decisions
- Output includes raw logits for every head (required).
- Derived confidence fields per head:
  - `confidence_top1` (softmax max probability)
  - `confidence_margin` (top1 - top2 softmax probability)
- Confidence values are informational and do not change decoding decisions.

## Phase 7: Engine Integration + Backend Selection
### Deliverables
- Configurable backend selector:
  - `rule_based`
  - `augnet_onnx`
  - `hybrid`
- Unified analyzer response including source attribution and logits.

### Requirements
- User-selectable per request/preset.
- Hybrid mode merges rule diagnostics and harmonic predictions without conflict loss.

### Tests / Acceptance
- Integration tests for each backend mode.
- Existing rule-engine tests remain passing.

### Finalized decisions
- `hybrid` mode never silently resolves disagreements.
- Rule diagnostics remain authoritative for rule violations.
- Harmonic labels from AugmentedNet are shown with source attribution.
- On disagreement between rule-derived harmonic interpretation and AugmentedNet label, emit informational diagnostic `hybrid.harmony.disagreement`.

## Phase 8: Corpus-Level Validation
### Deliverables
- Automated parity/evaluation pipeline using:
  - Official Python AugmentedNet outputs as parity baseline.
  - When-in-Rome corpus as large-scale evaluation input.
- Summary report artifact (CI).

### Requirements
- Two validation layers:
  1. **Implementation parity**: Rust ONNX output must match Python AugmentedNet output.
  2. **Musical evaluation**: compare against When-in-Rome annotations for quality metrics.

### Tests / Acceptance
- Parity mismatch rate target: 0 on deterministic fixture set.
- Corpus regression thresholds defined and enforced in CI.

### Finalized decisions
- Musical evaluation metrics:
  - Roman numeral exact accuracy
  - Local key accuracy
  - Tonicized key accuracy
  - Chord quality accuracy
  - Inversion accuracy
  - Harmonic segment boundary F1
- Regression policy:
  - Parity layer (Rust vs Python AugmentedNet): exact match required on deterministic fixtures; corpus mismatch rate must be <= 0.1%.
  - Musical evaluation layer (vs When-in-Rome): Rust ONNX implementation must stay within 0.25 percentage points of Python AugmentedNet baseline on each metric.

## Phase 9: Web UI Integration for AugmentedNet Analysis
### Deliverables
- Web analysis method selector integrated with engine backend modes:
  - `rule_based`
  - `augnet_onnx`
  - `hybrid`
- Harmonic labeling UI updates for AugmentedNet-capable modes:
  - Roman numeral
  - local key
  - tonicized key
  - chord quality
  - inversion
  - confidence summary fields
- UI treatment for hybrid disagreements (`hybrid.harmony.disagreement`) with direct jump-to-location behavior.
- Updated web integration contract docs for method-specific label fields and diagnostics mapping.

### Requirements
- The web client must pass backend mode explicitly in analysis requests.
- Label rendering must be method-aware:
  - `rule_based`: existing rule-based labeling behavior only.
  - `augnet_onnx`: show full AugmentedNet harmonic labels and confidence summaries.
  - `hybrid`: show AugmentedNet harmonic labels plus rule diagnostics with disagreement indicators.
- Tonicization display must be clear and musically legible (e.g., secondary-function notation with local key context).
- Raw logits must be available in payload handling but hidden from default UI; expose behind an advanced/debug toggle.
- If `augnet_onnx` or `hybrid` is selected and backend is unavailable, UI must present a fatal analysis error state (no silent fallback).
- Preserve interactive editing performance and avoid UI reflow jitter when harmonic labels update.

### Tests / Acceptance
- Component tests:
  - method selector state and persistence behavior
  - label visibility toggles by backend mode
  - disagreement indicator rendering and interaction
- Integration tests:
  - request payload includes selected backend mode
  - response mapping correctly renders tonicization/local key/quality/inversion/confidence
  - fatal error state is shown when AugmentedNet backend is unavailable
- End-to-end tests:
  - switch among all three backend modes and verify expected UI changes
  - edit notes and confirm labels/diagnostics refresh correctly
  - hybrid disagreement click navigates/highlights corresponding musical location
- Visual regression checks:
  - harmonic label placement with long annotations
  - disagreement marker visibility and readability at multiple zoom levels

### Finalized decisions
- Default web analysis method remains `rule_based` to preserve existing behavior.
- Advanced AugmentedNet label fields are shown only in `augnet_onnx` and `hybrid` modes.
- Raw logits are never shown in default UI and require explicit advanced/debug toggle.
- No JavaScript analysis fallback is permitted when AugmentedNet mode is requested.

## Test Strategy (Detailed)

### A. Unit tests
- Input encoding primitives.
- Head decode and class index mapping.
- RN formatter and inversion resolver.

### B. Golden parity tests
- Rust encoder tensors vs Python encoder tensors.
- Rust ONNX logits vs Python ONNX logits (within tolerance).
- Rust decoded labels vs Python decoded labels (exact).
- Chordified timeline parity (event offsets, durations, note spellings, onset flags).
- Per-head class-index parity checks (vocabulary order and cardinality).
- Fixed-`T` chunk boundary parity (first/last frame of each chunk + stitched sequence parity).

### C. End-to-end integration tests
- Piece -> harmonic slices + logits with backend selection.
- Hybrid mode output composition.

### D. Corpus tests (When-in-Rome)
- Batch-run selected corpus partition.
- Store comparison metrics and fail on regression.

### E. Web integration tests
- Method selector and request contract tests.
- Mode-specific label rendering tests (including tonicization).
- Disagreement UX tests in `hybrid` mode.
- Fatal-state tests for unavailable AugmentedNet backend.

### F. Baseline differential tests (music21 reference)
- Differential test runner executes Python baseline and Rust implementation on identical fixtures.
- Stage-level diff reports are generated for:
  - timeline/chordify layer
  - feature tensor layer
  - model-output layer
  - final label layer
- CI must fail with actionable diff output at earliest mismatching stage.

## Recommended File Layout
- `docs/planning/augnet-onnx-port-plan.md`
- `tools/augnet/convert_to_onnx.py`
- `tools/augnet/run_python_baseline.py`
- `tools/augnet/export_golden.py`
- `tools/augnet/diff_against_music21.py`
- `crates/harmony_ml/src/augnet_onnx.rs`
- `crates/harmony_ml/src/augnet_features.rs`
- `crates/harmony_ml/src/augnet_decode.rs`
- `tests/augnet_parity/`
- `tests/augnet_parity/music21_baseline/` (pinned stage artifacts)
- `web/src/*` updates for backend selector, mode-aware harmonic labels, and disagreement indicators.

## Risks and Mitigations
1. Tensor shape or output-order mismatch
- Mitigation: explicit model signature tests and fixed-name mapping checks.

2. Silent preprocessing drift
- Mitigation: mandatory tensor-level parity tests before integration acceptance.

3. Corpus runtime too slow for CI
- Mitigation: split smoke subset in mandatory CI, full corpus in scheduled/nightly CI.

4. Dependency drift in conversion toolchain
- Mitigation: pinned lockfile/containerized conversion environment.

## Definition of Done
1. Rust ONNX backend produces all heads + logits.
2. Pre/post processing parity with official Python AugmentedNet is validated.
3. Backend selection (`rule-based`, `augnet-onnx`, `hybrid`) works end-to-end.
4. Corpus-level regression checks run in CI with defined thresholds.
5. Web UI supports AugmentedNet-capable modes with tonicization-aware labeling and hybrid disagreement indicators.
6. Documentation includes build/run/test steps and model artifact manifest.
