#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
anchor_cli="${ADSPACES_ANCHOR_CLI:-$HOME/.avm/bin/anchor-1.2.1}"

if [[ ! -x "$anchor_cli" ]]; then
  echo "Anchor 1.2.1 was not found at $anchor_cli" >&2
  exit 1
fi
if [[ "$("$anchor_cli" --version)" != "anchor-cli 1.2.1" ]]; then
  echo "ADSPACES_ANCHOR_CLI must point to Anchor 1.2.1" >&2
  exit 1
fi

cd "$repo_root"
mkdir -p target/idl target/types
# --ignore-keys: the program ID comes from declare_id! (lib.rs); the local program keypair file is
# not needed to build or to upgrade the existing devnet program.
"$anchor_cli" build --no-idl --arch v3 --ignore-keys
CARGO_BUILD_JOBS=1 "$anchor_cli" idl build \
  --out target/idl/adspaces.json \
  --out-ts target/types/adspaces.ts \
  -- --lib
