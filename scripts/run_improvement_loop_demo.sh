#!/usr/bin/env bash
set -euo pipefail

db_path="${ASSET_AGENT_DEMO_DB:-/tmp/asset-agent-phase5-demo.sqlite3}"
rm -f "$db_path"

cargo run -p asset-agent-cli -- improvement-demo \
  --db "$db_path" \
  --trace traces/hp_pump_1_high_vibration.jsonl \
  --as-of trace:last \
  --feedback-count 3 \
  --solved-item-id check_suction_strainer
