#!/usr/bin/env bash
# Start a throwaway conductor, install the packed hApp, run the zome-call smoke
# suite, and stop the conductor.
#
# Usage:
#   ./tests/conductor/run.sh
#
# Requirements: dist/harvest-advance.happ exists (run `hc app pack`), `hc`,
# node 20+, and network access for the one-time `npm install`.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SANDBOX_DIR="${SANDBOX_DIR:-/tmp/harvest-advance-sandbox}"
SANDBOX_NAME="ha-test-$$"
ADMIN_PORT="${ADMIN_PORT:-34817}"
APP_PORT="${APP_PORT:-9000}"
APP_ID="${APP_ID:-harvest-advance}"

if [[ ! -f "$ROOT/dist/harvest-advance.happ" ]]; then
  echo "missing $ROOT/dist/harvest-advance.happ; run 'hc app pack happ -o dist/harvest-advance.happ' first" >&2
  exit 1
fi

rm -rf "$SANDBOX_DIR" "/tmp/$SANDBOX_NAME"
mkdir -p "$SANDBOX_DIR"
cd "$SANDBOX_DIR"

printf 'test-passphrase\n' | hc sandbox -f="$ADMIN_PORT" --piped generate \
  --in-process-lair -r="$APP_PORT" --app-id="$APP_ID" -d="$SANDBOX_NAME" \
  "$ROOT/dist/harvest-advance.happ" &
CONDUCTOR_PID=$!
cleanup() {
  pkill -P "$CONDUCTOR_PID" 2>/dev/null || true
  kill "$CONDUCTOR_PID" 2>/dev/null || true
  rm -rf "/tmp/$SANDBOX_NAME"
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
node smoke.mjs