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
