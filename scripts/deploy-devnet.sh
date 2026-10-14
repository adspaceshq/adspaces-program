#!/usr/bin/env bash
set -euo pipefail

# Upgrades the existing devnet program in place. The program ID never changes, and upgrading needs
# only its address plus the upgrade authority (the program keypair is only needed for a first deploy),
# so this script refuses to run unless the program already exists on devnet.
readonly program_id="6ft49ExLRbHUcY6crReW7qibZLrS5rdCudKWQRp2LMSW"
readonly devnet_url="https://api.devnet.solana.com"
readonly script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly repo_root="$(cd "$script_dir/.." && pwd)"
readonly wallet="${ADSPACES_DEPLOY_WALLET:-$HOME/.config/solana/id.json}"
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

program_info="$(solana program show "$program_id" --url "$devnet_url" 2>/dev/null)" || {
  echo "Program $program_id is not on devnet; this script only upgrades it" >&2
  exit 1
}
authority="$(awk -F': ' '/^(Upgrade )?Authority:/{print $2}' <<<"$program_info")"
[[ "$authority" == "$deployer" ]] || {
  echo "Upgrade authority is $authority, not $deployer" >&2
  exit 1
}

bash "$script_dir/build-program.sh"
[[ -f "$program_binary" ]] || {
  echo "Program binary was not produced" >&2
  exit 1
}

echo "Binary SHA-256: $(sha256sum "$program_binary" | awk '{print $1}')"
solana program deploy "$program_binary" \
  --url "$devnet_url" \
  --keypair "$wallet" \
  --program-id "$program_id" \
  --upgrade-authority "$wallet"

solana program show "$program_id" --url "$devnet_url"
