# Project Plan: Counterpoint Analyzer Platform (Rules Corpus -> Rust Engine -> Web Editor -> MIDI Decomposer -> ReaImGui)

## Summary
Build the project in five dependency-ordered tracks:

1. Canonical rule corpus extraction and normalization from current docs.
2. Rust analysis engine (Phase 1) with rule toggles, presets, and robust diagnostics.
3. Educational web editor/viewer (abcjs-based) backed by Rust/WASM engine.
4. Phase 2 MIDI decomposer and multistage analysis pipeline.
5. ReaImGui integration for Reaper (overlay + diagnostics panel).

Chosen defaults:
- Rule authority: AiHarmony-first (`docs/demos/AiHarmony/*`) with research docs as commentary/cross-check.
- Web stack: static SPA + TypeScript + abcjs + Rust/WASM.
- Delivery order: Rules doc -> Engine -> Web -> MIDI -> ReaImGui.

## Public APIs / Interfaces / Types (Planned Additions)
- `RuleId`: stable string ID (example: `sp1.parallel_perfects`, `gen.voice_crossing`).
- `RuleConfig`: `{ enabled: bool, severity: Error|Warning, params?: object }`.
- `PresetId`: `species1..species5`, `general_voice_leading`, `custom`.
- `AnalysisRequest`:
  - metadata: key, mode, time signature, species, voices count, cantus assignment
  - score: normalized note events per voice
  - ruleset: selected preset + overrides
- `AnalysisDiagnostic`:
  - `rule_id`, `message`, `severity`
  - location: measure/beat/tick + note ids + voice indices
  - optional relation: second location (for parallels/directs)
- `AnalysisResponse`:
  - diagnostics[]
  - derived tags (harmonic slices, NCT tags, optional roman numerals)
  - summary stats
- `DecomposeRequest` / `DecomposeResponse` for MIDI-to-voices stage.
- WASM boundary: JSON in/out with same request/response schema as native Rust.
- Reaper bridge contract: Lua <-> Rust shared lib over JSON payloads.

## Task 1: Rules Extraction and Formatted Specification
### Deliverables
- `docs/planning/rules-canonical.md`: normalized rules catalog.
- `docs/planning/rules-mapping.csv`: source mapping (`RuleId` <-> source paragraph/page/file).
- `docs/planning/rules-decision-log.md`: conflict resolutions and rationale.
- `docs/planning/rules-test-fixtures.md`: per-rule positive/negative fixture references.

### Requirements (In-depth)
- Extract all general + species-specific rules (1-5), including rhythm constraints, melodic constraints, vertical constraints, cadence constraints, suspension/NCT handling, voice-spacing/crossing, multi-voice constraints.
- Normalize each rule into:
  - intent
  - formal condition
  - scope (species, voices, metric positions)
  - exception list
  - severity default
- Include rule provenance with direct references to AiHarmony rule tables and research docs.
- Separate strict enforcement rules vs style-guidance warnings.

### Tests / Acceptance Criteria
- 100% rule entries include stable `RuleId`, formalized trigger condition, and source mapping.
- No duplicate semantic rules without merge note.
- Validation pass: each engine rule planned in Task 2 maps back to at least one canonical rule row.

### Open Questions
- Whether to ship one canonical corpus only or keep alternate strict Fux and common-practice profiles as first-class branches.
- Whether harmony/roman analysis is mandatory in Phase 1 or optional diagnostics.

## Task 2: Rust Phase 1 Analysis Engine
### Deliverables
- New Rust workspace crates (planned):
  - `cp_core` (music types, timeline model, parsing-normalization)
  - `cp_rules` (rule trait system + implementations)
  - `cp_engine` (orchestration, presets, diagnostics)
  - `cp_io` (MusicXML/ABC adapters, JSON API DTOs)
  - `cp_wasm` (WASM bindings)
- Preset packs: species 1-5 strict + general voice-leading.
- Test suite:
  - unit tests by rule
  - scenario/integration tests
  - regression corpus tests using AiHarmony sample XML files.

### Requirements (In-depth)
- Internal canonical time grid with explicit bar/beat offsets.
- Rule execution model:
  - stateless checks (single event/slice)
  - contextual checks (across events, cross-voice, cadence windows)
- Rule toggling:
  - enable/disable by `RuleId`
  - per-rule severity override
  - preset inheritance with overrides.
- Diagnostics must include exact involved notes and dual-location links for relation rules.
- Multi-voice support target for Phase 1 analyzer: up to 4 voices for product UI (engine type design should remain extensible past 4).
- MusicXML and ABC normalization must preserve measure boundaries, ties, and note durations relevant to species constraints.

### Tests / Acceptance Criteria
- Rule unit tests for each canonical rule family (positive/negative examples).
- Integration tests for species 1-5 known-good and known-bad exercises.
- Deterministic results across native + WASM for same payload.
- Performance baseline: interactive analysis response suitable for editor feedback on typical educational exercises.
- Snapshot tests for diagnostics schema stability.

### Open Questions
- Exact parser behavior for ambiguous imports (voice assignment conflicts, tied-note interpretation).
- Threshold policy for warning vs error for style rules.

## Task 3: Educational Web Viewer/Editor (abcjs)
### Deliverables
- New web app module:
  - abc input/edit panel
  - staff rendering and drag-note editing
  - rule/preset controls
  - diagnostics list + red overlay markers/lines
  - cantus library selector (Fux variants above/below by mode)
  - MusicXML import/export.
- UX parity baseline with `docs/demos/cp_demo.html` plus species 1-5 and general rules support.
- Web integration docs for request/response schema and rule config UI mapping.

### Requirements (In-depth)
- Two-way binding: text notation <-> visual note movement.
- Strict species enforcement in editor constraints:
  - example: species 1 whole notes only.
- Voice count configurable up to 4.
- Custom rule set builder UI and saved preset profiles.
- Cantus options:
  - built-in Fux library by mode/position
  - custom cantus upload/edit.
- Real-time diagnostics overlays:
  - note highlights
  - relation lines (parallel/direct)
  - clear per-rule messages.

### Tests / Acceptance Criteria
- UI integration tests: input edit, drag edit, rule toggle, preset apply.
- Golden visual tests for overlay placement on representative examples.
- Round-trip import/export tests for MusicXML.
- Accessibility/basic responsiveness verified on desktop and mobile widths.

### Open Questions
- Exact UX for conflicting edits (drag action that violates duration/species constraints).
- Whether harmonic annotations (Roman numerals) appear by default or toggle-only in initial release.

## Task 4: Phase 2 MIDI Decomposer + Multistage Analyzer
### Deliverables
- `cp_decompose` module with staged pipeline:
  1. quantization/alignment
  2. voice separation/clustering
  3. line-continuity optimization
  4. confidence scoring + ambiguity reporting
- Pipeline orchestrator integrating decomposition output into Phase 1 analyzer schema.
- Evaluation dataset + scoring script for decomposition quality.

### Requirements (In-depth)
- Accept raw MIDI and produce analyzer-ready voice streams.
- Handle overlapping notes, crossings, and polyphonic ambiguity.
- Keep decomposition auditable:
  - confidence per assignment
  - explainable decisions for ambiguous events.
- Provide fallback/manual correction hooks for UI/DAW consumers.

### Tests / Acceptance Criteria
- Synthetic test cases for known edge patterns (crossing voices, shared register, syncopation).
- Benchmark against curated MIDI examples (including converted AiHarmony examples where applicable).
- Accuracy targets defined for voice assignment and rhythm quantization before production rollout.

### Open Questions
- Ground-truth corpus source and labeling workflow for decomposition evaluation.
- Whether Phase 2 ships with hard auto-assignment or assisted decomposition mode first.

## Task 5: ReaImGui + Reaper Integration
### Deliverables
- ReaScript (Lua) plugin entrypoint for selected MIDI take analysis.
- Rust shared library wrapper target for Reaper-supported OSes.
- ReaImGui panel:
  - diagnostics table
  - click-to-navigate timeline
  - simplified staff/overlay renderer.

### Requirements (In-depth)
- Stable Lua<->Rust JSON contract.
- Efficient transfer from `MIDI_GetAllEvts` into engine schema.
- Non-blocking analysis flow suitable for DAW usage.
- Visual parity of core error markers with web overlay vocabulary.

### Tests / Acceptance Criteria
- Integration tests on supported Reaper setups (at least one OS baseline first, then expansion).
- Navigation accuracy tests (error click seeks exact position).
- Large-take latency checks and graceful timeout/error handling.

### Open Questions
- First officially supported OS target for ReaImGui release.
- Distribution model (script-only vs packaged binary per platform).

## Cross-Task Dependencies and Decision Gates
- Gate A (after Task 1): canonical rule schema signed off; no engine implementation before this.
- Gate B (after Task 2 core): schema freeze for diagnostics before full web UI integration.
- Gate C (after Task 3): finalize user-driven correction flows before Phase 2 decomposition integration.
- Gate D (before Task 5): lock shared-library ABI and JSON contract.

## Investigation Work Required Before Execution (Explicit)
- Parse and map AiHarmony rule IDs/paragraph links into stable `RuleId` taxonomy.
- Audit AiHarmony sample corpus for test fixture importability and licensing reuse.
- Validate abcjs interaction model for drag editing across multi-voice staff layouts.
- Confirm ReaImGui API capability for required overlay rendering and interaction patterns.
- Define decomposition evaluation dataset and metrics prior to Phase 2 implementation.

## Assumptions and Defaults
- Repo currently doc/demo-heavy with no active Rust/web app scaffold; scaffolding is in-scope.
- Canonical rules prioritize AiHarmony docs/tables; research docs are secondary references.
- Educational product scope for initial UI is max 4 voices, even if legacy analyzer supported more.
- Species 1-5 strict mode is mandatory; broader common-practice checks are configurable.
- Implementation should proceed in delivery order above with gate reviews.
