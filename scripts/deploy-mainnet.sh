#!/usr/bin/env bash
set -euo pipefail

# Mainnet wrapper for the oracle/DVM deployment flow.
# Reuses deploy-testnet.sh logic but enforces mainnet-safe defaults.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

export NETWORK="${NETWORK_PROFILE:-mainnet-fastnear}"
export CREATE_ACCOUNTS=n
export BUILD_NOW="${BUILD_NOW:-y}"
export DEPLOY_MOCK_COLLATERAL=n

# Prevent accidental .testnet usage on mainnet
check_mainnet_id() {
  local name="$1"
  local value="$2"
  if [[ "$value" == *.testnet ]]; then
    echo "Error: $name cannot end with .testnet for mainnet deploy: $value" >&2
    exit 1
  fi
  if [[ "$value" == *.mainnet ]]; then
    echo "Error: $name cannot end with .mainnet (invalid NEAR account suffix): $value" >&2
    exit 1
  fi
}

required=(
  OWNER_ACCOUNT
  TREASURY_ACCOUNT
  TOKEN_ACCOUNT
  FINDER_ACCOUNT
  STORE_ACCOUNT
  IDENTIFIER_WHITELIST_ACCOUNT
  REGISTRY_ACCOUNT
  SLASHING_ACCOUNT
  VOTING_ACCOUNT
  ORACLE_ACCOUNT
  COLLATERAL_TOKEN
  FINAL_FEE
)

for var in "${required[@]}"; do
  if [[ -z "${!var:-}" ]]; then
    echo "Missing required env var: $var" >&2
    exit 1
  fi
done

check_mainnet_id OWNER_ACCOUNT "$OWNER_ACCOUNT"
check_mainnet_id TREASURY_ACCOUNT "$TREASURY_ACCOUNT"
check_mainnet_id TOKEN_ACCOUNT "$TOKEN_ACCOUNT"
check_mainnet_id FINDER_ACCOUNT "$FINDER_ACCOUNT"
check_mainnet_id STORE_ACCOUNT "$STORE_ACCOUNT"
check_mainnet_id IDENTIFIER_WHITELIST_ACCOUNT "$IDENTIFIER_WHITELIST_ACCOUNT"
check_mainnet_id REGISTRY_ACCOUNT "$REGISTRY_ACCOUNT"
check_mainnet_id SLASHING_ACCOUNT "$SLASHING_ACCOUNT"
check_mainnet_id VOTING_ACCOUNT "$VOTING_ACCOUNT"
check_mainnet_id ORACLE_ACCOUNT "$ORACLE_ACCOUNT"
check_mainnet_id COLLATERAL_TOKEN "$COLLATERAL_TOKEN"

echo "Running mainnet oracle/DVM deployment with:"
echo "  OWNER_ACCOUNT=$OWNER_ACCOUNT"
echo "  TREASURY_ACCOUNT=$TREASURY_ACCOUNT"
echo "  ORACLE_ACCOUNT=$ORACLE_ACCOUNT"
echo "  COLLATERAL_TOKEN=$COLLATERAL_TOKEN"
echo "  FINAL_FEE=$FINAL_FEE"
echo

"$SCRIPT_DIR/deploy-testnet.sh"
