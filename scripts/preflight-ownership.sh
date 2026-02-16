#!/usr/bin/env bash
set -euo pipefail

# Read-only ownership/admin preflight checks before mainnet actions.

NETWORK="${NETWORK:-mainnet}"
ORACLE_ACCOUNT="${ORACLE_ACCOUNT:-}"
MARKET_ACCOUNT="${MARKET_ACCOUNT:-}"
STORE_ACCOUNT="${STORE_ACCOUNT:-}"
VAULT_ACCOUNT="${VAULT_ACCOUNT:-}"

if [[ -n "$ORACLE_ACCOUNT" ]]; then
  echo "== Oracle owner =="
  near contract call-function as-read-only "$ORACLE_ACCOUNT" get_owner \
    json-args '{}' network-config "$NETWORK" now
fi

if [[ -n "$MARKET_ACCOUNT" ]]; then
  echo "== Market config (includes owner) =="
  near contract call-function as-read-only "$MARKET_ACCOUNT" get_config \
    json-args '{}' network-config "$NETWORK" now
fi

if [[ -n "$STORE_ACCOUNT" ]]; then
  echo "== Store owner =="
  near contract call-function as-read-only "$STORE_ACCOUNT" get_owner \
    json-args '{}' network-config "$NETWORK" now
  echo "== Store withdrawer =="
  near contract call-function as-read-only "$STORE_ACCOUNT" get_withdrawer \
    json-args '{}' network-config "$NETWORK" now
fi

if [[ -n "$VAULT_ACCOUNT" ]]; then
  echo "== Vault owner =="
  near contract call-function as-read-only "$VAULT_ACCOUNT" get_owner \
    json-args '{}' network-config "$NETWORK" now
fi

echo "Preflight checks complete."
