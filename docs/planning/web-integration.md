# Web Integration Contract

## Purpose
Define the request/response and UI mapping contract used by `web/editor` against the Task 2 engine (`cp_wasm` / `cp_engine`).

## Request Mapping
`web/editor/src/app.js` builds `AnalysisRequest` using:
- `score.meta.key_signature` from key + mode controls.
- `score.meta.time_signature` from session controls.
- `score.meta.ticks_per_quarter = 480`.
- `score.voices[*].notes[*]` from parsed voice text/drag edits.
- `config.preset_id` from preset selector.
- `config.enabled_rule_ids` / `disabled_rule_ids` / `severity_overrides` from rules panel.

## Rule Mapping
Source of truth for shipped presets/rule groups:
- `docs/planning/rules-presets.json`

UI mapping behavior:
- Non-custom presets: UI computes delta overrides relative to preset baseline.
- Custom preset: UI computes full active rule set and sends in `enabled_rule_ids` with `preset_id = custom`.
- Severity dropdown sets `config.severity_overrides[rule_id]`.

## Diagnostic Mapping
Each `AnalysisDiagnostic` is rendered in 3 channels:
1. Diagnostics list (`rule_id`, message, measure/beat/voice).
2. Note overlay circle at `primary.note_id`.
3. Relation line when `related.note_id` is present.

Note-id projection:
- Voice note ids are deterministic (`v{voice}_n{index}`) and used for both request and SVG mapping.

## Harmonic Mapping
When `Show Roman Numerals` is enabled, UI displays:
- `harmonic_slices[*].roman_numeral`
- plus quality/inversion context.

Default is `off` (user-selected toggle).

## MusicXML Mapping
Import:
- Converts MusicXML parts/voices into UI voices (max 4).
- Imports key/time/mode into session metadata.
- Preserves rest events as UI rest tokens.

Export:
- Emits one part per UI voice.
- Preserves pitch and duration values from current state.
- Emits `<rest/>` notes for UI rest tokens.

## Measure Controls
- `Add Measure`: appends one full-measure rest block to each voice.
- `Remove Measure`: removes one full measure of duration from the end of each voice.
- Cantus apply flow rebalances all voices to cantus length by trimming trailing rests and extending shorter voices with rests, without deleting existing non-rest counterpoint notes.

## Persistence Mapping
Local storage keys:
- `oh.cp.custom_presets.v1`: array of custom rule profile objects.
- `oh.cp.editor_settings.v1`: last editor state and rule override session data.

## Known MVP limits
- Strict species behavior is analyzer-driven (editor never blocks edits).
- MusicXML parser handles common linear note content; complex notations are reduced to analyzer-relevant pitch/duration streams.
- WASM load failure falls back to JS diagnostics with explicit warning text.
