#!/usr/bin/env bash
set -euo pipefail

OUT_PATH="${1:-models/augnet/source/AugmentedNet.hdf5}"
URL="${AUGNET_MODEL_URL:-https://github.com/napulen/AugmentedNet/raw/main/AugmentedNet.hdf5}"
EXPECTED_SHA256="${AUGNET_MODEL_SHA256:-a4e410e75d72fa270163c3585e5a9651668dbcec9b459f803e0e14ba51aed78b}"

mkdir -p "$(dirname "$OUT_PATH")"

echo "Downloading AugmentedNet model..."
echo "  url:  $URL"
echo "  out:  $OUT_PATH"

curl -fL "$URL" -o "$OUT_PATH"

ACTUAL_SHA256="$(sha256sum "$OUT_PATH" | awk '{print $1}')"
echo "  sha256: $ACTUAL_SHA256"

if [[ -n "$EXPECTED_SHA256" && "$ACTUAL_SHA256" != "$EXPECTED_SHA256" ]]; then
  echo "ERROR: SHA256 mismatch" >&2
  echo "  expected: $EXPECTED_SHA256" >&2
  echo "  actual:   $ACTUAL_SHA256" >&2
  echo "If upstream model changed intentionally, set AUGNET_MODEL_SHA256 to the new value." >&2
  exit 1
fi

echo "Model fetched successfully."
