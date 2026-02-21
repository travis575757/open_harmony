# Rules Decision Log (Task 1)

## Purpose
This file records non-trivial decisions made while normalizing source rules into engine-ready canonical checks.

## D-001: Canonical Source Priority
- Problem: Sources differ in detail and strictness.
- Decision: Use AiHarmony artifacts as canonical (`Artinfuser_Counterpoint_rules.pdf`, `rules2.xlsm`, `rules_paragraphs.js`), use research docs as secondary clarification.
- Rationale: Project direction explicitly chose AiHarmony-first.
- Impact: Engine behavior matches existing analyzer expectations first.

## D-002: Strict vs Style Guidance
- Problem: Some constraints are hard prohibitions in strict species but softer in broader practice.
- Decision: Keep one formal condition per rule; allow severity/profile overrides (`strict`, `pedagogical`, `lenient`).
- Rationale: Avoid duplicate logic while preserving pedagogical flexibility.
- Impact: Same rule implementation can serve multiple modes.

## D-003: Direct/Hidden Perfect Intervals
- Problem: Source interpretations vary by context (outer vs inner voices; strict vs historical).
- Decision: Canonical default marks these as restricted, with strict profile = `error`, non-strict = `warning`.
- Rationale: Aligns with education-first feedback without losing strict mode.
- Impact: Requires profile-aware severity at evaluation/reporting layer.

## D-004: Voice Count Scope
- Problem: Legacy analyzer supports up to 9 voices; current product scope targets up to 4 in editor.
- Decision: Keep canonical rules voice-agnostic, with product profile limit <=4 voices.
- Rationale: Prevents rewriting rule corpus when voice count expands later.
- Impact: Engine schema should remain extensible for >4 voices.

## D-005: Species 3 Cambiata Variants
- Problem: Cambiata appears in multiple theoretical variants.
- Decision: Normalize one canonical strict pattern as active rule; log variant forms as deferred subrules.
- Rationale: Avoid ambiguous first implementation.
- Impact: Adds clear future extension point (`sp3.dissonance.cambiata_limited_exception.<variant>`).

## D-006: Species 5 Eighth-Note Policy
- Problem: Eighth-note strictness is style-dependent.
- Decision: Strict profile enforces weak-position paired usage; other profiles downgrade to warning.
- Rationale: Matches pedagogical strict-counterpoint expectations while permitting later flexibility.
- Impact: Requires profile-sensitive gating in rhythm checker.

## D-007: Spacing and Crossing in Multi-Voice Context
- Problem: Historical corpora show exceptions to textbook spacing norms.
- Decision: Keep crossing as hard error in strict profile; spacing limits as warnings by default unless SATB strict mode is selected.
- Rationale: Crossing damages line identity more consistently than moderate spacing deviation.
- Impact: Reduces false-positive severity for educational composition drafts.

## D-008: Paragraph-Level Traceability
- Problem: Canonical source includes broad paragraph taxonomy that may exceed active engine checks.
- Decision: Include paragraph anchor rows (`aih.paragraph.*`) in mapping CSV in addition to normalized rules.
- Rationale: Guarantees full provenance and future rule expansion path.
- Impact: Mapping file includes both active rules and source anchors.

## D-009: Input Constraints vs Musical Rules
- Problem: Parser/import constraints (key placement, tuplets, note-length floor) are not contrapuntal checks but affect analysis validity.
- Decision: Keep them in canonical corpus under `gen.input.*` for deterministic pre-validation.
- Rationale: Prevents undefined analysis behavior on unsupported inputs.
- Impact: Engine requires validation phase before rule evaluation.

## D-010: Fixture Assignment Strategy
- Problem: Not every rule has an explicitly labeled failing sample in current corpus.
- Decision: Use known good/bad files first, then assign edge-case fixtures from species examples; mark gaps where synthetic fixtures are needed.
- Rationale: Enables immediate test harness bootstrap without blocking on new composition.
- Impact: Task 2 should add synthetic fixture generation for underrepresented edge cases.

## D-011: Research-Only Advanced Rules
- Problem: `docs/research/*` includes advanced tonal/invertible constraints beyond immediate Phase 1 strict-species editor scope.
- Decision: Normalize and track these as explicit canonical entries under `gen.*` and `adv.*`, but mark advanced invertible constraints deferred by default.
- Rationale: Ensures no research rules are lost while keeping implementation order aligned with product goals.
- Impact: Coverage is complete; Task 2 can activate deferred rules via profile gates when the corresponding feature area is implemented.

## D-012: Preset Membership Must Be Explicit
- Problem: Preset membership was implicit via rule-id prefixes (`sp1.*`, `gen.*`) and not encoded as a consumable artifact.
- Decision: Add `rules-presets.json` as the machine-readable source and `rules-presets.md` as human-readable documentation.
- Rationale: Engine and UI require deterministic, versioned preset definitions with no inference.
- Impact: Task 2 can load presets directly and support custom presets by diff/override operations.
