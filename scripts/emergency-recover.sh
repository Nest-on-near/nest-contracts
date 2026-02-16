#!/usr/bin/env bash
set -euo pipefail

# Emergency recovery helper for oracle stack contracts.
# Uses owner/withdrawer-only methods already implemented on-chain.

NETWORK="${NETWORK:-mainnet}"

OWNER_ACCOUNT="${OWNER_ACCOUNT:-}"
TREASURY_ACCOUNT="${TREASURY_ACCOUNT:-}"

ORACLE_ACCOUNT="${ORACLE_ACCOUNT:-}"
STORE_ACCOUNT="${STORE_ACCOUNT:-}"
STORE_WITHDRAWER="${STORE_WITHDRAWER:-}"
VAULT_ACCOUNT="${VAULT_ACCOUNT:-}"

COLLATERAL_TOKEN="${COLLATERAL_TOKEN:-}"

ORACLE_WITHDRAW_TOKEN_AMOUNT="${ORACLE_WITHDRAW_TOKEN_AMOUNT:-0}"
ORACLE_WITHDRAW_NEAR_YOCTO="${ORACLE_WITHDRAW_NEAR_YOCTO:-0}"

STORE_WITHDRAW_TOKEN_AMOUNT="${STORE_WITHDRAW_TOKEN_AMOUNT:-0}"
STORE_WITHDRAW_NEAR_YOCTO="${STORE_WITHDRAW_NEAR_YOCTO:-0}"

VAULT_EMERGENCY_COLLATERAL_AMOUNT="${VAULT_EMERGENCY_COLLATERAL_AMOUNT:-0}"

require_env() {
  local name="$1"
  local value="$2"
  if [[ -z "$value" ]]; then
    echo "Missing required env var: $name" >&2
    exit 1
  fi
}

near_tx() {
  local contract="$1"
  local method="$2"
  local json_args="$3"
  local signer="$4"
  local gas="${5:-80 Tgas}"
  local deposit="${6:-0 NEAR}"

  echo "+ near contract call-function as-transaction $contract $method ..."
  near contract call-function as-transaction "$contract" "$method" \
    json-args "$json_args" \
    prepaid-gas "$gas" \
    attached-deposit "$deposit" \
    sign-as "$signer" \
    network-config "$NETWORK" \
    sign-with-keychain send
}

if [[ -n "$ORACLE_ACCOUNT" ]]; then
  require_env "OWNER_ACCOUNT" "$OWNER_ACCOUNT"
  require_env "TREASURY_ACCOUNT" "$TREASURY_ACCOUNT"

  if [[ "$ORACLE_WITHDRAW_TOKEN_AMOUNT" != "0" ]]; then
    require_env "COLLATERAL_TOKEN" "$COLLATERAL_TOKEN"
    near_tx "$ORACLE_ACCOUNT" "emergency_withdraw_token" \
      "{\"token\":\"$COLLATERAL_TOKEN\",\"receiver_id\":\"$TREASURY_ACCOUNT\",\"amount\":\"$ORACLE_WITHDRAW_TOKEN_AMOUNT\"}" \
      "$OWNER_ACCOUNT" "120 Tgas" "1 yoctoNEAR"
  fi

  if [[ "$ORACLE_WITHDRAW_NEAR_YOCTO" != "0" ]]; then
    near_tx "$ORACLE_ACCOUNT" "emergency_withdraw_near" \
      "{\"receiver_id\":\"$TREASURY_ACCOUNT\",\"amount\":\"$ORACLE_WITHDRAW_NEAR_YOCTO\"}" \
      "$OWNER_ACCOUNT" "80 Tgas" "0 NEAR"
  fi
fi

if [[ -n "$STORE_ACCOUNT" ]]; then
  require_env "STORE_WITHDRAWER" "$STORE_WITHDRAWER"
  require_env "TREASURY_ACCOUNT" "$TREASURY_ACCOUNT"

  if [[ "$STORE_WITHDRAWER" != "$TREASURY_ACCOUNT" ]]; then
    echo "Warning: STORE_WITHDRAWER != TREASURY_ACCOUNT. Proceeds go to STORE_WITHDRAWER."
  fi

  if [[ "$STORE_WITHDRAW_TOKEN_AMOUNT" != "0" ]]; then
    require_env "COLLATERAL_TOKEN" "$COLLATERAL_TOKEN"
    near_tx "$STORE_ACCOUNT" "withdraw_token" \
      "{\"token\":\"$COLLATERAL_TOKEN\",\"amount\":\"$STORE_WITHDRAW_TOKEN_AMOUNT\"}" \
      "$STORE_WITHDRAWER" "120 Tgas" "1 yoctoNEAR"
  fi

  if [[ "$STORE_WITHDRAW_NEAR_YOCTO" != "0" ]]; then
    near_tx "$STORE_ACCOUNT" "withdraw_near" \
      "{\"amount\":\"$STORE_WITHDRAW_NEAR_YOCTO\"}" \
      "$STORE_WITHDRAWER" "80 Tgas" "0 NEAR"
  fi
fi

if [[ -n "$VAULT_ACCOUNT" && "$VAULT_EMERGENCY_COLLATERAL_AMOUNT" != "0" ]]; then
  require_env "OWNER_ACCOUNT" "$OWNER_ACCOUNT"
  # Contract requires paused redemptions before emergency collateral withdrawal.
  near_tx "$VAULT_ACCOUNT" "pause_redemptions" "{}" "$OWNER_ACCOUNT" "30 Tgas" "0 NEAR"
  near_tx "$VAULT_ACCOUNT" "emergency_withdraw_collateral" \
    "{\"amount\":\"$VAULT_EMERGENCY_COLLATERAL_AMOUNT\"}" \
    "$OWNER_ACCOUNT" "120 Tgas" "1 yoctoNEAR"
fi

echo "Emergency recovery calls complete."
