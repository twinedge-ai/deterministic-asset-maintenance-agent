#!/usr/bin/env bash
set -euo pipefail

required_version="${MISTRALRS_CLI_VERSION:-0.8.1}"

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo is required; install Rust first." >&2
  exit 1
fi

if ! command -v mistralrs >/dev/null 2>&1 || ! mistralrs --version | grep -q "$required_version"; then
  cargo install mistralrs-cli --version "$required_version" --locked
fi

"$(dirname "$0")/download_mistral_model.sh"

cat <<EOF
mistral.rs setup complete.

Runner:
  $(command -v mistralrs)
  $(mistralrs --version)

Model:
  ${MISTRAL_MODEL_DIR:-models/mistral}/${MISTRAL_MODEL_FILE:-model.gguf}

To enable the API LLM path:
  ASSET_AGENT_LLM_ENABLED=1 MISTRALRS_TIMEOUT_MS=180000 cargo run -p asset-agent-api
EOF
