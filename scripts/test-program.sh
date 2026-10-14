#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
wallet="${ADSPACES_TEST_WALLET:-$HOME/.config/solana/id.json}"
validator_url="http://127.0.0.1:8899"

cd "$repo_root"
if [[ ! -f "$wallet" ]]; then
  echo "Test wallet was not found at $wallet" >&2
  exit 1
fi
if solana -u "$validator_url" cluster-version >/dev/null 2>&1; then
  echo "Port 8899 already has a validator; stop it before running this script" >&2
  exit 1
fi

bash "$script_dir/build-program.sh"
ledger="$(mktemp -d /tmp/adspaces-validator.XXXXXX)"
readonly ledger
solana-test-validator --quiet --ledger "$ledger" \
  --upgradeable-program 8Vg24teSaNrY4AmksQ8hZw2z2RRPuobeCtir7jVTYGw5 \
  target/deploy/adspaces.so "$wallet" &
validator_pid=$!
cleanup() {
  kill "$validator_pid" 2>/dev/null || true
  wait "$validator_pid" 2>/dev/null || true
  rm -rf -- "$ledger"
}
trap cleanup EXIT

ready=0
for ((attempt = 0; attempt < 60; attempt++)); do
  if ! kill -0 "$validator_pid" 2>/dev/null; then
    echo "Local validator stopped before becoming ready" >&2
    exit 1
  fi
  if solana -u "$validator_url" cluster-version >/dev/null 2>&1; then
    ready=1
    break
  fi
  sleep 1
done
if [[ "$ready" -ne 1 ]]; then
  echo "Local validator did not become ready" >&2
  exit 1
fi

source "$HOME/.nvm/nvm.sh"
nvm use
ANCHOR_PROVIDER_URL="$validator_url" ANCHOR_WALLET="$wallet" npm run test:anchor
