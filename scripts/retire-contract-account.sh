#!/usr/bin/env bash
set -euo pipefail

# Permanently deletes an account and transfers all remaining NEAR to beneficiary.
# This is destructive and cannot be undone.

NETWORK="${NETWORK:-mainnet}"
CONTRACT_ACCOUNT="${CONTRACT_ACCOUNT:-}"
BENEFICIARY_ACCOUNT="${BENEFICIARY_ACCOUNT:-}"

if [[ -z "$CONTRACT_ACCOUNT" || -z "$BENEFICIARY_ACCOUNT" ]]; then
  echo "Usage:"
  echo "  NETWORK=mainnet CONTRACT_ACCOUNT=<contract> BENEFICIARY_ACCOUNT=<receiver> ./scripts/retire-contract-account.sh"
  exit 1
fi

echo "About to delete $CONTRACT_ACCOUNT on $NETWORK and transfer remaining NEAR to $BENEFICIARY_ACCOUNT"
echo "Type DELETE to continue:"
read -r confirm

if [[ "$confirm" != "DELETE" ]]; then
  echo "Aborted."
  exit 1
fi

near account delete-account "$CONTRACT_ACCOUNT" beneficiary "$BENEFICIARY_ACCOUNT" \
  network-config "$NETWORK" now

echo "Account deleted."
