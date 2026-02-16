#!/usr/bin/env bash
set -euo pipefail

# Reclaim NEAR storage deposit from an FT contract using storage_unregister.
# Requires the target account to have zero token balance on that FT.

NETWORK="${NETWORK:-mainnet}"
TOKEN_CONTRACT="${TOKEN_CONTRACT:-}"
TARGET_ACCOUNT="${TARGET_ACCOUNT:-}"
SIGNER_ACCOUNT="${SIGNER_ACCOUNT:-}"
FORCE="${FORCE:-true}"

if [[ -z "$TOKEN_CONTRACT" || -z "$TARGET_ACCOUNT" ]]; then
  echo "Usage:"
  echo "  NETWORK=mainnet TOKEN_CONTRACT=<ft> TARGET_ACCOUNT=<account> [SIGNER_ACCOUNT=<same>] ./scripts/reclaim-storage-deposit.sh"
  exit 1
fi

if [[ -z "$SIGNER_ACCOUNT" ]]; then
  SIGNER_ACCOUNT="$TARGET_ACCOUNT"
fi

echo "Checking storage balance on $TOKEN_CONTRACT for $TARGET_ACCOUNT ..."
near contract call-function as-read-only "$TOKEN_CONTRACT" storage_balance_of \
  json-args "{\"account_id\":\"$TARGET_ACCOUNT\"}" \
  network-config "$NETWORK" now || true

echo "Attempting storage_unregister on $TOKEN_CONTRACT for signer $SIGNER_ACCOUNT ..."
near contract call-function as-transaction "$TOKEN_CONTRACT" storage_unregister \
  json-args "{\"force\":$FORCE}" \
  prepaid-gas "30 Tgas" \
  attached-deposit "1 yoctoNEAR" \
  sign-as "$SIGNER_ACCOUNT" \
  network-config "$NETWORK" \
  sign-with-keychain send

echo "Done."
