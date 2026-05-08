#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EDGE_ROOT="${OPCUA_EDGE_ROOT:-${ROOT}/../opcua-edge}"
EDGE_BIN="${EDGE_ROOT}/build/opcua-edge"
EDGE_TEMPLATE="${EDGE_ROOT}/templates/desalination_plant.edge"
MODBUS_SIM="${EDGE_ROOT}/tools/modbus_sim/modbus_sim.py"
MODBUS_PORT="${EDGE_MODBUS_PORT:-1502}"
OPCUA_PORT="${EDGE_OPCUA_PORT:-4840}"
PROFILE="${ASSET_AGENT_LIVE_PROFILE:-low_suction_pressure}"
OUT_DIR="${ASSET_AGENT_PHASE9_OUT_DIR:-/tmp/asset-agent-phase9}"

mkdir -p "${OUT_DIR}"

require_file() {
  local path="$1"
  if [[ ! -e "${path}" ]]; then
    echo "missing required file: ${path}" >&2
    exit 1
  fi
}

require_command() {
  local command="$1"
  if ! command -v "${command}" >/dev/null 2>&1; then
    echo "missing required command: ${command}" >&2
    exit 1
  fi
}

port_is_listening() {
  local port="$1"
  ss -ltn 2>/dev/null | awk '{print $4}' | grep -Eq "(^|:)${port}$"
}

wait_port() {
  local port="$1"
  local label="$2"
  for _ in $(seq 1 100); do
    if (echo >"/dev/tcp/127.0.0.1/${port}") >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.1
  done
  echo "timed out waiting for ${label} on port ${port}" >&2
  return 1
}

pids=()
cleanup() {
  for pid in "${pids[@]}"; do
    kill "${pid}" >/dev/null 2>&1 || true
  done
  for pid in "${pids[@]}"; do
    wait "${pid}" >/dev/null 2>&1 || true
  done
}
trap cleanup EXIT

require_file "${EDGE_BIN}"
require_file "${EDGE_TEMPLATE}"
require_file "${MODBUS_SIM}"
require_command cargo
require_command jq
require_command python3
require_command ss

if port_is_listening "${MODBUS_PORT}" || port_is_listening "${OPCUA_PORT}"; then
  echo "ports ${MODBUS_PORT} or ${OPCUA_PORT} are already in use" >&2
  exit 1
fi

cd "${ROOT}"

echo "phase9: rust tests"
cargo test -q

echo "phase9: replay determinism"
for trace in traces/hp_pump_1_*.jsonl; do
  name="$(basename "${trace}" .jsonl)"
  first="${OUT_DIR}/${name}.first.jsonl"
  second="${OUT_DIR}/${name}.second.jsonl"
  cargo run -q -p asset-agent-cli -- replay \
    --trace "${trace}" \
    --output "${first}" \
    --as-of trace:last >/dev/null
  cargo run -q -p asset-agent-cli -- replay \
    --trace "${trace}" \
    --output "${second}" \
    --as-of trace:last >/dev/null
  cmp "${first}" "${second}" >/dev/null
done

echo "phase9: live opc ua smoke profile=${PROFILE}"
(
  cd "${EDGE_ROOT}"
  python3 "${MODBUS_SIM}" \
    --template "${EDGE_TEMPLATE}" \
    --host 127.0.0.1 \
    --port "${MODBUS_PORT}" \
    --profile "${PROFILE}" \
    --tick-ms 100 \
    --quiet
) >"${OUT_DIR}/modbus-sim.log" 2>&1 &
pids+=("$!")
wait_port "${MODBUS_PORT}" "modbus simulator"

(
  cd "${EDGE_ROOT}"
  EDGE_MODBUS_HOST=127.0.0.1 \
  EDGE_MODBUS_PORT="${MODBUS_PORT}" \
  EDGE_OPCUA_PORT="${OPCUA_PORT}" \
  EDGE_DB_PATH="${OUT_DIR}/opcua-edge.sqlite" \
    "${EDGE_BIN}" "${EDGE_TEMPLATE}"
) >"${OUT_DIR}/opcua-edge.log" 2>&1 &
pids+=("$!")
wait_port "${OPCUA_PORT}" "opcua-edge"

live_record="${OUT_DIR}/live-${PROFILE}.jsonl"
live_summary="${OUT_DIR}/live-${PROFILE}-summary.json"
cargo run -q -p asset-agent-cli -- live \
  --endpoint "opc.tcp://127.0.0.1:${OPCUA_PORT}" \
  --namespace-uri urn:twinedge:opcua-edge \
  --asset hp_pump_1 \
  --window-seconds 10 \
  --record "${live_record}" >"${live_summary}"

jq -e '.ok == true and .namespace_index != null and .nodes_read == 25' "${live_summary}" >/dev/null
jq -e '
  .ok == true
  and .namespace_resolution.resolved == true
  and (.nodes_read | length) == 25
  and ((.nodes_read | map(contains("command_")) | any) | not)
  and .snapshot.design_specs.npshr_m == 19
  and .snapshot.design_specs.specific_gravity == 1.03
  and (.analysis.risk_level != "UNKNOWN")
' "${live_record}" >/dev/null

jq -S '{
  ok,
  mode,
  namespace_index,
  nodes_read,
  risk_level,
  npsh_margin_m,
  record
}' "${live_summary}"
