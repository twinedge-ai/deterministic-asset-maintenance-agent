#!/usr/bin/env bash
set -euo pipefail

cargo run -p asset-agent-cli -- live \
  --endpoint opc.tcp://127.0.0.1:4840 \
  --namespace-uri urn:twinedge:opcua-edge \
  --asset hp_pump_1 \
  --window-seconds 10 \
  --record /tmp/asset-agent-live-hp-pump-1.jsonl
