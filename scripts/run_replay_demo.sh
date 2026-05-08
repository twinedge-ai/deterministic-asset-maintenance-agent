#!/usr/bin/env bash
set -euo pipefail

cargo run -p asset-agent-cli -- replay \
  --trace traces/hp_pump_1_low_suction_pressure.jsonl \
  --as-of trace:last \
  --output traces/replay_decisions.jsonl \
  --disable-wall-clock

