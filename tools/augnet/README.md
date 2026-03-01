# AugmentedNet ONNX Conversion (Phase 1)

This folder contains the Phase 1 conversion tooling for the AugmentedNet model.

## Scope
- Convert Keras `.h5/.hdf5` model to ONNX.
- Enforce fixed sequence length (`T`) by default.
- Validate ONNX loading/signature.
- Generate a model manifest with checksums and signatures.
- Enforce Phase 1 opset policy (`13` by default and guarded).
- Enforce functional reproducibility policy in tests.

## Environment
Use `uv` with pinned dependencies.

```bash
uv venv --python 3.10 .venv-augnet
source .venv-augnet/bin/activate
uv pip install -r tools/augnet/requirements-conversion.txt
```

If you prefer not to activate a venv, run one-shot commands with `uv run`:

```bash
uv run --python 3.10 --with-requirements tools/augnet/requirements-conversion.txt \
  python tools/augnet/convert_to_onnx.py --help
```

## Get the HDF5 model

Fetch the pinned model file into this repository:

```bash
tools/augnet/fetch_model.sh models/augnet/source/AugmentedNet.hdf5
```

If upstream changes the model file, set `AUGNET_MODEL_SHA256` to the new hash before running.

## Convert

```bash
uv run --python 3.10 --with-requirements tools/augnet/requirements-conversion.txt \
  python tools/augnet/convert_to_onnx.py \
  --input-h5 models/augnet/source/AugmentedNet.hdf5 \
  --output-onnx models/augnet/AugmentedNet.onnx \
  --manifest models/augnet/model-manifest.json \
  --model-id augmentednet-v1 \
  --opset 13 \
  --overwrite
```

## What the script validates
- Input model exists and loads in TensorFlow.
- Fixed time-axis shape (`dim[1]`) unless `--allow-dynamic-time-axis` is explicitly set.
- Opset is fixed to `13` unless `--allow-opset-override` is explicitly set.
- ONNX checker passes.
- ONNX Runtime can load model and the runtime input signature matches the converted signature (unless `--skip-runtime-check`).
- Output head names/order match Keras outputs (unless `--allow-output-head-mismatch`).

## Rust ORT smoke validation
Phase 1 requires ONNX loading in Rust via `ort` in addition to Python ONNX Runtime.

Smoke tool:
- `tools/augnet/rust_ort_smoke/` (standalone Rust binary)

Manual run:

```bash
ORT_CACHE_DIR=.cache/ort-cache CARGO_HOME=.cache/cargo-home cargo run --manifest-path tools/augnet/rust_ort_smoke/Cargo.toml -- \
  --model models/augnet/AugmentedNet.onnx
```

This prints JSON with input/output names and tensor shapes as seen by Rust `ort`.

## Manifest
The generated manifest includes:
- source model path + SHA256
- ONNX path + SHA256
- opset and conversion settings
- package/tool versions
- input signature
- fixed time-axis contract
- output head mapping and head-order match flag
- reproducibility policy metadata

## Reproducibility policy (Phase 1)
Phase 1 enforces **functional reproducibility**, not byte-identical ONNX binaries:
- repeated conversions must preserve input/output signatures,
- repeated conversions must preserve output-head mapping/order,
- repeated conversions must produce numerically identical inference outputs for deterministic fixtures.

This policy is enforced by `tools/augnet/tests/test_conversion_integration.py`.

## Run tests

```bash
uv run --python 3.10 --with-requirements tools/augnet/requirements-conversion.txt \
  pytest tools/augnet/tests -q
```

The integration suite includes:
- conversion + manifest contract checks,
- Python ORT runtime shape checks,
- reproducibility policy checks,
- Rust `ort` smoke load/signature checks.

## Notes
- Phase 1 uses `opset=13` by plan.
- Keep `--allow-opset-override` disabled in CI.
- Keep `--allow-output-head-mismatch` disabled in CI.
- Keep `--allow-dynamic-time-axis` disabled for Phase 1 parity mode.
- Python 3.10 is recommended for this pinned dependency set.

## Phase 3 differential validation (music21 baseline vs Rust compat)

Install the pinned Phase 3 environment:

```bash
uv run --python 3.10 --with-requirements tools/augnet/requirements-diff.txt \
  python --version
```

Export/rebuild pinned music21 baseline artifacts:

```bash
uv run --python 3.10 --with-requirements tools/augnet/requirements-diff.txt \
  python tools/augnet/export_music21_baseline.py \
  --manifest tests/augnet_parity/fixtures_manifest.json \
  --model models/augnet/AugmentedNet.onnx \
  --output-dir tests/augnet_parity/music21_baseline \
  --overwrite
```

Run fail-fast stage differential checks (A: timeline, B: tensors, C: logits/argmax, D: final labels):

```bash
uv run --python 3.10 --with-requirements tools/augnet/requirements-diff.txt \
  python tools/augnet/diff_against_music21.py \
  --manifest tests/augnet_parity/fixtures_manifest.json \
  --baseline-dir tests/augnet_parity/music21_baseline \
  --model models/augnet/AugmentedNet.onnx \
  --report-path tests/augnet_parity/last_diff_report.json
```

If a mismatch occurs, the run stops at the first failing stage/fixture and writes a machine-readable report at `tests/augnet_parity/last_diff_report.json`.

Phase 3 JSON contracts are stable and schema-versioned (`schema_version: 1`):
- Baseline artifact: `fixture_id`, `stage_a`, `stage_b`, `stage_c`, `stage_d`.
- Candidate artifact (Rust helper): `fixture_id`, `stage_a`, `stage_b`.
- Diff report: `status`, `fixture_id`, `stage_id`, `field_path`, `expected_summary`, `actual_summary`.

Direct parity tests for the music21-port surface (Stage A + Stage B) live in `tests/augnet_parity/test_music21_port_parity.py`:

```bash
uv run --python 3.10 --with-requirements tools/augnet/requirements-diff.txt \
  pytest tests/augnet_parity/test_music21_port_parity.py -q
```

## Phase 4 Rust ONNX Runtime adapter acceptance

Phase 4 adapter and ONNX-boundary parity acceptance tests live in `cp_engine`:

```bash
CARGO_HOME=.cache/cargo-home ORT_CACHE_DIR=.cache/ort-cache cargo test -p cp_engine --test augnet_onnx_integration
```

This suite covers:
- session bootstrap + manifest artifact validation (`4A`)
- input tensor contract binding (`4B`)
- output head mapping + raw logits capture (`4C`)
- ONNX-boundary parity gate against pinned fixtures (`4D`)
- performance/determinism baseline capture (`4E`)

The performance/determinism report is written to:
- `target/augnet/augnet_performance_determinism.json`

## Phase 5 exact preprocessing parity acceptance

Phase 5 preprocessing adapter + tensor parity tests live in `cp_engine`:

```bash
CARGO_HOME=.cache/cargo-home ORT_CACHE_DIR=.cache/ort-cache cargo test -p cp_engine --test augnet_preprocess_integration
```

This suite covers:
- exact Stage A parity (event frames + fixed-grid frames),
- exact Stage B tensor parity (binary/categorical channels + dimensions/order),
- chunk-boundary parity across chunk joins (`chunk_boundary` fixture),
- deterministic repeated preprocessing output/serialization,
- integration path from preprocessing chunks into ONNX adapter boundary.

The Phase 5 preprocessing parity report is written to:
- `target/augnet/preprocessing_parity_report.json`

## Phase 6 exact postprocessing parity acceptance

Generate/update the shared decode asset (derived from official AugmentedNet vocab + music21 numerators):

```bash
uv run --python 3.10 --with-requirements tools/augnet/requirements-diff.txt \
  python tools/augnet/generate_decode_assets.py \
  --augnet-repo third_party/AugmentedNet \
  --output crates/cp_engine/src/augnet_decode_assets.json
```

Re-export pinned baseline artifacts (Stage D now includes decoded labels, RN/chord reconstruction, and head confidence metadata):

```bash
uv run --python 3.10 --with-requirements tools/augnet/requirements-diff.txt \
  python tools/augnet/export_music21_baseline.py \
  --manifest tests/augnet_parity/fixtures_manifest.json \
  --model models/augnet/AugmentedNet.onnx \
  --output-dir tests/augnet_parity/music21_baseline \
  --overwrite
```

Run Phase 6 Rust acceptance tests:

```bash
CARGO_HOME=.cache/cargo-home ORT_CACHE_DIR=.cache/ort-cache cargo test -p cp_engine --test augnet_postprocess_integration
```

## Phase 7 engine backend integration

Phase 7 backend-selection and hybrid integration acceptance tests live in:

- `crates/cp_engine/tests/engine_backend_modes.rs`

Run the Phase 7 suite:

```bash
cargo test -p cp_engine --test engine_backend_modes
```

This suite covers:
- exact decoded fixture parity against Python baseline artifacts,
- edge-case snapshots (cadential, tonicization, root-assumption fallback),
- confidence derivation checks and non-decision behavior,
- output schema/raw-logit contract checks,
- fail-fast Stage D parity gate with per-field diff artifact emission.

## Phase 8 corpus-level validation

Phase 8 uses a single deterministic pipeline entrypoint:

- `tools/augnet/evaluate_corpus.py`

It runs three gates:

1. deterministic fixture exact-match gate (Phase 3 differential runner)
2. corpus implementation parity gate (Rust ONNX vs Python baseline)
3. musical evaluation gate (both systems vs When-in-Rome annotations)

Install dependencies:

```bash
uv run --python 3.10 --with-requirements tools/augnet/requirements-corpus-eval.txt python --version
```

Initialize corpus submodule (one-time per clone):

```bash
git submodule update --init --recursive tests/corpora/When-in-Rome
```

Run CI manifest validation:

```bash
uv run --python 3.10 --with-requirements tools/augnet/requirements-corpus-eval.txt \
  python tools/augnet/evaluate_corpus.py \
  --corpus-manifest tests/corpora/when_in_rome_ci_manifest.txt \
  --report-dir target/augnet/corpus_validation
```

Optional full-corpus run: pass a larger manifest with `--corpus-manifest <path>`.

Phase 8 thresholds:

- deterministic fixture parity: exact (must pass)
- corpus parity mismatch rate: `<= 0.1%`
- per-metric musical delta (Rust vs Python): `<= 0.25` percentage points for each:
  - `roman_numeral_exact_accuracy`
  - `local_key_accuracy`
  - `tonicized_key_accuracy`
  - `chord_quality_accuracy`
  - `inversion_accuracy`
  - `harmonic_segment_boundary_f1`

Artifacts:

- machine-readable summary: `target/augnet/corpus_validation/summary.json`
- per-piece CSV: `target/augnet/corpus_validation/piece_metrics.csv`
- human-readable summary: `target/augnet/corpus_validation/summary.md`

Exit codes:

- `0`: all gates passed
- `2`: deterministic fixture gate failed
- `3`: corpus parity threshold failed
- `4`: musical metric delta threshold failed

## Phase 9 web integration checks

Phase 9 web tests live under `web/editor/test` and validate:

- backend mode selector/request contract (`rule_based`, `augnet_onnx`, `hybrid`)
- mode-aware harmonic label mapping (RN/local key/tonicized key/quality/inversion/confidence)
- hybrid disagreement indicators and jump mapping (`hybrid.harmony.disagreement`)
- fatal UI model when AugmentedNet backend is unavailable
- visual regression guards for long labels and disagreement marker readability

Run:

```bash
cd web/editor
npm test
```
