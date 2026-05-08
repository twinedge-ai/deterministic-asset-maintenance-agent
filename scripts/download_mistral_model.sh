#!/usr/bin/env bash
set -euo pipefail

model_dir="${MISTRAL_MODEL_DIR:-models/mistral}"
model_url="${MISTRAL_MODEL_URL:-https://huggingface.co/TheBloke/Mistral-7B-Instruct-v0.2-GGUF/resolve/main/mistral-7b-instruct-v0.2.Q4_K_M.gguf}"
model_file="${MISTRAL_MODEL_FILE:-model.gguf}"
expected_sha256="${MISTRAL_MODEL_SHA256:-3e0039fd0273fcbebb49228943b17831aadd55cbcbf56f0af00499be2040ccf9}"

mkdir -p "$model_dir"

target="$model_dir/$model_file"

if [[ -f "$target" ]]; then
  actual_sha256="$(sha256sum "$target" | awk '{print $1}')"
  if [[ "$actual_sha256" == "$expected_sha256" ]]; then
    echo "Model already present and verified: $target"
    exit 0
  fi
  echo "Existing model checksum does not match; resuming/replacing $target" >&2
fi

curl -L --fail --continue-at - "$model_url" -o "$target"

actual_sha256="$(sha256sum "$target" | awk '{print $1}')"
if [[ "$actual_sha256" != "$expected_sha256" ]]; then
  echo "sha256 mismatch for $target" >&2
  echo "expected: $expected_sha256" >&2
  echo "actual:   $actual_sha256" >&2
  exit 1
fi

echo "Model ready: $target"
