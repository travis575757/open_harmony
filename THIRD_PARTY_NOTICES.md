# Third-Party Notices

This repository contains first-party code and third-party components under
different licenses.

## First-Party Code

Unless otherwise stated, original source code in this repository is licensed
under the MIT License. See `LICENSE`.

## Third-Party Components

The following third-party components are included or referenced by this
repository and retain their own licenses.

1. `docs/demos/AiHarmony` (git submodule)
- Upstream: https://github.com/napulen/AiHarmony
- License: GNU Affero General Public License v3.0 (AGPL-3.0)
- Local license file: `docs/demos/AiHarmony/LICENSE`

2. `tests/corpora/When-in-Rome` (git submodule)
- Upstream: https://github.com/MarkGotham/When-in-Rome
- License: CC BY-SA 4.0 (corpus/data and related materials)
- Local reference: `tests/corpora/When-in-Rome/README.md`

3. AugmentedNet model and tooling references
- Upstream model/tooling reference: https://github.com/napulen/AugmentedNet
- Upstream license: MIT
- Note: local model artifacts may be generated/downloaded into `models/augnet/`
  and are distributed according to upstream terms.

4. Web dependencies
- `onnxruntime-web` (MIT)
- `abcjs` (MIT)
- See `web/editor/package.json` and lockfile for exact versions.

5. Python tooling dependencies (conversion/validation)
- Includes packages under permissive licenses such as Apache-2.0, MIT, and BSD.
- See:
  - `tools/augnet/requirements-conversion.txt`
  - `tools/augnet/requirements-diff.txt`
  - `tools/augnet/requirements-corpus-eval.txt`

6. Rust crate dependencies
- The Rust workspace depends on crates with their own SPDX licenses.
- See `Cargo.lock` and crate metadata for exact dependency license expressions.

## Distribution Notes

1. The root MIT license applies to first-party code only.
2. Third-party submodules and dependency artifacts keep their original licenses.
3. If you redistribute this repository (or bundles/artifacts derived from it),
   you are responsible for complying with all applicable third-party licenses,
   including attribution/share-alike or copyleft obligations where relevant.

## No Legal Advice

This file is for engineering documentation only and is not legal advice.
