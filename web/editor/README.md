# Web Viewer/Editor

This module implements the educational web editor:
- abcjs score rendering with multi-voice editing (up to 4 voices)
- text ABC-token editing and drag-to-change pitch
- rest tokens (`zN`) and end-of-score measure add/remove controls
- preset + per-rule configuration (`species1..species5`, `general_voice_leading`, `custom`)
- diagnostics overlays mapped from analyzer note IDs
- Fux cantus presets (above/below by mode) and custom cantus input
- MusicXML import/export
- local persistence for custom rule profiles and session settings

## Phase 9 Analysis Modes (AugmentedNet Web Integration)

The editor now exposes explicit analysis backend selection:

- `rule_based` (default)
- `augnet_onnx`

### Request contract

Every analysis request now sends:

- `config.analysis_backend` (explicit mode selector value)
- `config.harmonic_rhythm` (mode-aware)

Mode-specific harmonic rhythm behavior:

- `rule_based`: uses UI-selected fixed-per-bar rhythm (`chords_per_bar`).
- `augnet_onnx`: UI harmonic-rhythm input is hidden/ignored and request uses `note_onset` (AugmentedNet determines segmentation internally).

### Harmonic label rendering contract

- `rule_based`:
  - renders existing rule-based `harmonic_slices` labels.
- `augnet_onnx`:
  - voice-leading/counterpoint diagnostics still come from the rule engine
  - renders AugmentedNet `harmonic_outputs` (`source=augnet_onnx`) with:
    - Roman numeral
    - local key
    - tonicized key (with local-key context text)
    - chord quality
    - inversion
    - confidence summary

### Raw logits / debug toggle

- Raw logits are carried in `harmonic_outputs[*].logits`.
- Hidden by default.
- Visible only when `Show AugNet debug logits` toggle is enabled.

### Fatal behavior (no fallback)

- If `augnet_onnx` is selected and backend initialization/inference is unavailable, the UI enters a fatal analysis state.
- No JavaScript analysis fallback is used.

## Duration Range

- Voice editor token durations support fractional eighth units from **32nd** (`/4`) through **double whole** (`16`).
- Score insert palettes include note/rest buttons with abcjs previews.
- Examples:
  - `C/4` = 32nd note
  - `C/2` = 16th note
  - `C1` = eighth note
  - `C8` = whole note
  - `C16` = double whole note
  - `z3/2` = dotted eighth rest

## Prerequisites

1. Rust toolchain installed
2. `wasm-pack` available
3. Python 3 (or another static server)
4. Node.js (for tests)

Install `wasm-pack` if missing:

```bash
cargo install wasm-pack
```

## Build WASM Analyzer

From repo root:

```bash
wasm-pack build crates/cp_wasm --target web --out-dir pkg
```

Expected output path:
- `crates/cp_wasm/pkg/cp_wasm.js`
- `crates/cp_wasm/pkg/cp_wasm_bg.wasm`

## Run

1. Serve repository root with a static server:
```bash
python3 -m http.server 8000
```
2. Open: `http://localhost:8000/web/editor/`

## Confirm WASM Is Active

In the sidebar Engine panel:
- `Analyzer: Rust/WASM active` = wasm loaded and in use
- `Fatal initialization error: ...` = wasm failed to load; app analysis is unavailable until fixed

## Tests

From `web/editor`:

```bash
npm test
```

## Rebuild After Changes

If you change Rust analysis code:

1. From repo root: `cargo test`
2. From repo root: `wasm-pack build crates/cp_wasm --target web --out-dir pkg`
3. Hard refresh the browser (`Ctrl+Shift+R`)
