#!/usr/bin/env bash
set -euo pipefail

readonly program_id="6ft49ExLRbHUcY6crReW7qibZLrS5rdCudKWQRp2LMSW"
readonly devnet_url="https://api.devnet.solana.com"
readonly script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly repo_root="$(cd "$script_dir/.." && pwd)"
readonly wallet="${ADSPACES_DEPLOY_WALLET:-$HOME/.config/solana/id.json}"
readonly program_keypair="${ADSPACES_PROGRAM_KEYPAIR:-$repo_root/target/deploy/adspaces-keypair.json}"
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
for file in "$wallet" "$program_keypair"; do
  [[ -f "$file" ]] || {
    echo "Required keypair was not found: $file" >&2
    exit 1
  }
done

actual_program_id="$(solana address -k "$program_keypair")"
[[ "$actual_program_id" == "$program_id" ]] || {
  echo "Program keypair resolves to $actual_program_id, expected $program_id" >&2
  exit 1
}

deployer="$(solana address -k "$wallet")"
balance="$(solana balance "$deployer" --url "$devnet_url")"
echo "Deployer: $deployer"
echo "Devnet balance: $balance"

if program_info="$(solana program show "$program_id" --url "$devnet_url" 2>/dev/null)"; then
  authority="$(awk -F': ' '/^(Upgrade )?Authority:/{print $2}' <<<"$program_info")"
  [[ "$authority" == "$deployer" ]] || {
    echo "Upgrade authority is $authority, not $deployer" >&2
    exit 1
  }
fi

bash "$script_dir/build-program.sh"
[[ -f "$program_binary" ]] || {
  echo "Program binary was not produced" >&2
  exit 1
}

echo "Binary SHA-256: $(sha256sum "$program_binary" | awk '{print $1}')"
solana program deploy "$program_binary" \
  --url "$devnet_url" \
  --keypair "$wallet" \
  --program-id "$program_keypair" \
  --upgrade-authority "$wallet"

solana program show "$program_id" --url "$devnet_url"
