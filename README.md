# Open Harmony

Open-source harmonic analysis and voice-leading toolchain focused on species counterpoint pedagogy.

Inspired by ArtInfuser / AIHarmony research and demos in this repository (`docs/demos/AiHarmony/`), this project builds a transparent, extensible analysis engine and editor workflow.

## Note

All code in this repository is AI-generated.

## Purpose

Open Harmony provides:

1. A modular Rust analysis engine for harmony, counterpoint, and voice-leading diagnostics.
2. A web-based educational editor for species counterpoint.
3. A planned RealmGUI integration for DAW workflows (coming soon; see `docs/planning/project.md`).

## Components

1. Generic Rust engine:
`crates/cp_core`, `crates/cp_rules`, `crates/cp_engine`, `crates/cp_harmony`, `crates/cp_wasm`
2. Web species counterpoint editor:
`web/editor`
3. RealmGUI integration (planned):
See `docs/planning/project.md`

## Prerequisites

1. Rust toolchain (`cargo`, `rustc`)
2. `wasm-pack` for building the web analyzer module
3. A static file server (`python3 -m http.server` works)
4. Node.js (only needed for web tests)

Install `wasm-pack` if needed:

```bash
cargo install wasm-pack
```

## Build And Test (Rust)

From repo root:

```bash
cargo test
cargo build --release
```

## Build WASM For The Web Editor

From repo root:

```bash
wasm-pack build crates/cp_wasm --target web --out-dir pkg
```

This writes output to `crates/cp_wasm/pkg/`.

## Run The Web App

From repo root:

```bash
python3 -m http.server 8000
```

Open:

```text
http://localhost:8000/web/editor/
```

## How To Use The Web GUI

1. Build WASM (command above), then start the static server and open `http://localhost:8000/web/editor/`.
2. In **Session**, choose a preset (`species1`..`species5`, `general_voice_leading`, or `custom`), voice count, key/mode, and supported time signature.
3. Enter or edit notes in the voice text boxes (ABC-like tokens), or drag notes directly in the rendered score.
4. Optionally apply a built-in Fux cantus (or custom cantus), and lock its voice for exercises.
5. Use **Rule Controls** to enable/disable rules, filter rules, and set severity overrides.
6. Review diagnostics and score overlays; click diagnostics to highlight corresponding markings in the score.
7. Use MusicXML import/export and custom profile save/load as needed.

Engine status check:
- `Analyzer: Rust/WASM active` means the Rust analyzer is loaded.
- `Fatal initialization error: ...` means WASM failed to load and analysis is unavailable until fixed.

## Web Tests

```bash
cd web/editor
npm test
```

## Rebuild Loop After Code Changes

1. `cargo test` (optional but recommended)
2. `wasm-pack build crates/cp_wasm --target web --out-dir pkg`
3. Refresh browser tab (hard refresh if needed)
