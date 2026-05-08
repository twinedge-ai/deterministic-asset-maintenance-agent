#!/usr/bin/env bash
set -euo pipefail

api_addr="${ASSET_AGENT_API_ADDR:-127.0.0.1:18083}"
db_path="${ASSET_AGENT_DB_PATH:-/tmp/asset-agent-mistral-smoke.sqlite3}"
snapshot_path="/tmp/asset-agent-mistral-smoke-snapshot.json"
response_path="/tmp/asset-agent-mistral-smoke-response.json"

rm -f "$db_path" "$snapshot_path" "$response_path"

ASSET_AGENT_API_ADDR="$api_addr" \
ASSET_AGENT_DB_PATH="$db_path" \
ASSET_AGENT_LLM_ENABLED=1 \
MISTRALRS_TIMEOUT_MS="${MISTRALRS_TIMEOUT_MS:-180000}" \
cargo run -p asset-agent-api >/tmp/asset-agent-mistral-smoke-api.log 2>&1 &
api_pid="$!"

cleanup() {
  kill "$api_pid" >/dev/null 2>&1 || true
}
trap cleanup EXIT

for _ in $(seq 1 60); do
  if curl -fsS "http://$api_addr/api/health" >/dev/null 2>&1; then
    break
  fi
  sleep 1
done

curl -fsS "http://$api_addr/api/llm/status" | jq -e '.llm_available == true' >/dev/null
curl -fsS "http://$api_addr/api/opc/snapshot?asset_id=hp_pump_1" | jq '.snapshot' > "$snapshot_path"
curl -fsS -X POST "http://$api_addr/api/llm/explain" \
  -H 'content-type: application/json' \
  --data-binary @"$snapshot_path" > "$response_path"

jq -e '.llm.source == "mistralrs" and .llm.fallback_used == false and .llm.guard_passed == true' \
  "$response_path" >/dev/null

jq '{ok, case_id, llm_run_id, source: .llm.source, fallback_used: .llm.fallback_used, guard_passed: .llm.guard_passed, guarded_output: .llm.guarded_output}' \
  "$response_path"
