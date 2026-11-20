#!/usr/bin/env bash
# Build + pack the hApp, start a throwaway two-agent conductor, run the A/B
# lifecycle suite (owner vs contributor), and stop the conductor.
#
# Usage:
#   ./tests/conductor/run-ab.sh
#
# Uses admin port 34818 and app port 9001 by default (override with ADMIN_PORT /
# APP_PORT) so it does not collide with a conductor already on 34817/9000.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SANDBOX_NAME="ha-ab-$$"
SANDBOX_DIR="${SANDBOX_DIR:-/tmp/harvest-advance-ab-$$}"
ADMIN_PORT="${ADMIN_PORT:-34818}"
APP_PORT="${APP_PORT:-9001}"
APP_ID="${APP_ID:-harvest-advance}"
HAPP="$ROOT/dist/harvest-advance.happ"

cd "$ROOT"

echo "building zomes (wasm) ..."
cargo build --release --target wasm32-unknown-unknown \
  -p harvest_advance_integrity -p harvest_advance

echo "packing dna + happ ..."
hc dna pack dna -o dna/harvest-advance.dna
hc app pack happ -o "$HAPP"

rm -rf "$SANDBOX_DIR" "/tmp/$SANDBOX_NAME"
mkdir -p "$SANDBOX_DIR"
cd "$SANDBOX_DIR"

printf 'test-passphrase\n' | hc sandbox -f="$ADMIN_PORT" --piped generate \
  --in-process-lair -r="$APP_PORT" --app-id="$APP_ID" -d="$SANDBOX_NAME" \
  "$HAPP" &
CONDUCTOR_PID=$!
cleanup() {
  pkill -P "$CONDUCTOR_PID" 2>/dev/null || true
  kill "$CONDUCTOR_PID" 2>/dev/null || true
  rm -rf "/tmp/$SANDBOX_NAME" "$SANDBOX_DIR"
}
trap cleanup EXIT

echo "waiting for app interface on port $APP_PORT ..."
for _ in $(seq 1 120); do
  if (echo > "/dev/tcp/127.0.0.1/$APP_PORT") >/dev/null 2>&1; then
    break
  fi
  sleep 0.5
done

cd "$ROOT/tests/conductor"
if [[ ! -d node_modules ]]; then
  npm install --no-audit --no-fund
fi

ADMIN_URL="ws://127.0.0.1:$ADMIN_PORT" \
APP_URL="ws://127.0.0.1:$APP_PORT" \
APP_ID="$APP_ID" \
HAPP_PATH="$HAPP" \
node ab-test.mjs