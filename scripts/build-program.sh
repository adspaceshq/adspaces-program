#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
anchor_cli="${ADSPACES_ANCHOR_CLI:-$HOME/.avm/bin/anchor-1.1.2}"

if [[ ! -x "$anchor_cli" ]]; then
  echo "Anchor 1.1.2 was not found at $anchor_cli" >&2
  exit 1
fi
if [[ "$("$anchor_cli" --version)" != "anchor-cli 1.1.2" ]]; then
  echo "ADSPACES_ANCHOR_CLI must point to Anchor 1.1.2" >&2
  exit 1
fi

cd "$repo_root"
mkdir -p target/idl target/types
"$anchor_cli" build --no-idl
CARGO_BUILD_JOBS=1 "$anchor_cli" idl build \
  --out target/idl/adspaces.json \
  --out-ts target/types/adspaces.ts \
  -- --lib
