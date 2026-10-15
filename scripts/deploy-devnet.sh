#!/usr/bin/env bash
set -euo pipefail

# Deploys or upgrades the devnet program. A first deploy needs the program keypair (kept OUTSIDE the
# repo so builds can never delete it) and checks it matches program_id. Once the program exists,
# upgrades need only its address plus the upgrade authority.
readonly program_id="8Vg24teSaNrY4AmksQ8hZw2z2RRPuobeCtir7jVTYGw5"
readonly devnet_url="https://api.devnet.solana.com"
readonly script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly repo_root="$(cd "$script_dir/.." && pwd)"
readonly wallet="${ADSPACES_DEPLOY_WALLET:-$HOME/.config/solana/adspaces-devnet.json}"
readonly program_keypair="${ADSPACES_PROGRAM_KEYPAIR:-$HOME/.config/solana/programs/adspaces-devnet-v3-program.json}"
readonly program_binary="$repo_root/target/deploy/adspaces.so"

if [[ "${1:-}" != "--confirm" ]]; then
  echo "Usage: $0 --confirm" >&2
  exit 1
fi
for command in solana sha256sum; do
  command -v "$command" >/dev/null || {
    echo "$command is required" >&2
    exit 1
  }
done
[[ -f "$wallet" ]] || {
  echo "Upgrade authority keypair was not found: $wallet" >&2
  exit 1
}
grep -q "declare_id!(\"$program_id\")" "$repo_root/programs/adspaces/src/lib.rs" || {
  echo "declare_id! in lib.rs is not $program_id" >&2
  exit 1
}

deployer="$(solana address -k "$wallet")"
balance="$(solana balance "$deployer" --url "$devnet_url")"
echo "Deployer: $deployer"
echo "Devnet balance: $balance"

if program_info="$(solana program show "$program_id" --url "$devnet_url" 2>/dev/null)"; then
  mode="upgrade"
  program_arg="$program_id"
  authority="$(awk -F': ' '/^(Upgrade )?Authority:/{print $2}' <<<"$program_info")"
  [[ "$authority" == "$deployer" ]] || {
    echo "Upgrade authority is $authority, not $deployer" >&2
    exit 1
  }
else
  mode="first deploy"
  [[ -f "$program_keypair" ]] || {
    echo "First deploy needs the program keypair: $program_keypair" >&2
    exit 1
  }
  [[ "$(solana address -k "$program_keypair")" == "$program_id" ]] || {
    echo "Program keypair does not resolve to $program_id" >&2
    exit 1
  }
  program_arg="$program_keypair"
fi
echo "Mode: $mode of $program_id"

bash "$script_dir/build-program.sh"
[[ -f "$program_binary" ]] || {
  echo "Program binary was not produced" >&2
  exit 1
}

echo "Binary SHA-256: $(sha256sum "$program_binary" | awk '{print $1}')"
solana program deploy "$program_binary" \
  --url "$devnet_url" \
  --keypair "$wallet" \
  --program-id "$program_arg" \
  --upgrade-authority "$wallet"

solana program show "$program_id" --url "$devnet_url"
