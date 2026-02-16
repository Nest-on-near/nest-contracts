#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

# -----------------------------------------------------------------------------
# Config (env-driven)
# Set these as environment variables before running:
#   OWNER_ACCOUNT=... TREASURY_ACCOUNT=... TOKEN_ACCOUNT=... ./deploy-testnet.sh
# -----------------------------------------------------------------------------
NETWORK="${NETWORK:-testnet}"
CREATE_SLEEP_SECONDS="${CREATE_SLEEP_SECONDS:-15}"
CREATE_RETRIES="${CREATE_RETRIES:-3}"

DEFAULT_FINAL_FEE="100000000000000000000000" # 0.1 NEAR (24 decimals)
DEFAULT_ORACLE_LIVENESS_NS="7200000000000" # 2 hours
DEFAULT_BURNED_BOND_PERCENTAGE="500000000000000000" # 50% in 1e18 scale
DEFAULT_SLASHING_RATE_BPS="1000" # 10%
DEFAULT_SLASHING_TREASURY_BPS="5000" # 50%
DEFAULT_MIN_PARTICIPATION_BPS="500" # 5%
DEFAULT_COMMIT_DURATION_NS="86400000000000" # 24h
DEFAULT_REVEAL_DURATION_NS="86400000000000" # 24h

ASSERT_TRUTH_BYTES32='[65,83,83,69,82,84,95,84,82,85,84,72,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]'

OWNER_ACCOUNT="${OWNER_ACCOUNT:-}"
TREASURY_ACCOUNT="${TREASURY_ACCOUNT:-}"
TOKEN_ACCOUNT="${TOKEN_ACCOUNT:-}"
FINDER_ACCOUNT="${FINDER_ACCOUNT:-}"
STORE_ACCOUNT="${STORE_ACCOUNT:-}"
IDENTIFIER_WHITELIST_ACCOUNT="${IDENTIFIER_WHITELIST_ACCOUNT:-}"
REGISTRY_ACCOUNT="${REGISTRY_ACCOUNT:-}"
SLASHING_ACCOUNT="${SLASHING_ACCOUNT:-}"
VOTING_ACCOUNT="${VOTING_ACCOUNT:-}"
ORACLE_ACCOUNT="${ORACLE_ACCOUNT:-}"
MINT_OPERATOR_ACCOUNT="${MINT_OPERATOR_ACCOUNT:-}"
MINT_RECIPIENTS="${MINT_RECIPIENTS:-}"
MINT_AMOUNT="${MINT_AMOUNT:-0}"

COLLATERAL_TOKEN="${COLLATERAL_TOKEN:-wrap.testnet}"
FINAL_FEE="${FINAL_FEE:-$DEFAULT_FINAL_FEE}"
ORACLE_LIVENESS_NS="${ORACLE_LIVENESS_NS:-$DEFAULT_ORACLE_LIVENESS_NS}"
BURNED_BOND_PERCENTAGE="${BURNED_BOND_PERCENTAGE:-$DEFAULT_BURNED_BOND_PERCENTAGE}"
SLASHING_RATE_BPS="${SLASHING_RATE_BPS:-$DEFAULT_SLASHING_RATE_BPS}"
SLASHING_TREASURY_BPS="${SLASHING_TREASURY_BPS:-$DEFAULT_SLASHING_TREASURY_BPS}"
MIN_PARTICIPATION_BPS="${MIN_PARTICIPATION_BPS:-$DEFAULT_MIN_PARTICIPATION_BPS}"
COMMIT_DURATION_NS="${COMMIT_DURATION_NS:-$DEFAULT_COMMIT_DURATION_NS}"
REVEAL_DURATION_NS="${REVEAL_DURATION_NS:-$DEFAULT_REVEAL_DURATION_NS}"

BUILD_NOW="${BUILD_NOW:-y}"
CREATE_ACCOUNTS="${CREATE_ACCOUNTS:-y}"
DEPLOY_MOCK_COLLATERAL="${DEPLOY_MOCK_COLLATERAL:-n}"
MOCK_COLLATERAL_ACCOUNT="${MOCK_COLLATERAL_ACCOUNT:-}"
MOCK_COLLATERAL_OWNER="${MOCK_COLLATERAL_OWNER:-}"
MOCK_COLLATERAL_TOTAL_SUPPLY="${MOCK_COLLATERAL_TOTAL_SUPPLY:-1000000000000000000000000000}"
MOCK_COLLATERAL_TRANSFER_RESTRICTED="${MOCK_COLLATERAL_TRANSFER_RESTRICTED:-false}"

usage() {
  cat <<EOF
Usage:
  OWNER_ACCOUNT=... TREASURY_ACCOUNT=... TOKEN_ACCOUNT=... \\
  FINDER_ACCOUNT=... STORE_ACCOUNT=... IDENTIFIER_WHITELIST_ACCOUNT=... \\
  REGISTRY_ACCOUNT=... SLASHING_ACCOUNT=... VOTING_ACCOUNT=... ORACLE_ACCOUNT=... \\
  ./scripts/deploy-testnet.sh

Optional env vars:
  NETWORK=${NETWORK}
  CREATE_SLEEP_SECONDS=${CREATE_SLEEP_SECONDS}
  CREATE_RETRIES=${CREATE_RETRIES}
  COLLATERAL_TOKEN=${COLLATERAL_TOKEN}
  BUILD_NOW=${BUILD_NOW}
  CREATE_ACCOUNTS=${CREATE_ACCOUNTS}
  DEPLOY_MOCK_COLLATERAL=${DEPLOY_MOCK_COLLATERAL}
  MOCK_COLLATERAL_ACCOUNT=<required when DEPLOY_MOCK_COLLATERAL=y>
  MOCK_COLLATERAL_OWNER=${MOCK_COLLATERAL_OWNER:-OWNER_ACCOUNT}
  MOCK_COLLATERAL_TOTAL_SUPPLY=${MOCK_COLLATERAL_TOTAL_SUPPLY}
  MOCK_COLLATERAL_TRANSFER_RESTRICTED=${MOCK_COLLATERAL_TRANSFER_RESTRICTED}
  MINT_OPERATOR_ACCOUNT=${MINT_OPERATOR_ACCOUNT:-OWNER_ACCOUNT}
  MINT_RECIPIENTS=<comma-separated account list for initial NEST mint>
  MINT_AMOUNT=${MINT_AMOUNT} (raw yocto units, applied to each MINT_RECIPIENTS account)
EOF
}

require_env() {
  local name="$1"
  local value="$2"
  if [[ -z "$value" ]]; then
    echo "Missing required environment variable: $name" >&2
    usage
    exit 1
  fi
}

run_cmd() {
  echo "+ $*"
  "$@"
}

near_tx() {
  local contract="$1"
  local method="$2"
  local json_args="$3"
  local signer="$4"
  local gas="${5:-80 Tgas}"
  local deposit="${6:-0 NEAR}"

  run_cmd near contract call-function as-transaction "$contract" "$method" \
    json-args "$json_args" \
    prepaid-gas "$gas" \
    attached-deposit "$deposit" \
    sign-as "$signer" \
    network-config "$NETWORK" \
    sign-with-keychain send
}

account_exists() {
  local account_id="$1"
  near account view-account-summary "$account_id" network-config "$NETWORK" now >/dev/null 2>&1
}

create_account_if_missing() {
  local account_id="$1"

  if account_exists "$account_id"; then
    echo "Account already exists, skipping create: $account_id"
    return 0
  fi

  local attempt=1
  while (( attempt <= CREATE_RETRIES )); do
    echo "Creating account ($attempt/$CREATE_RETRIES): $account_id"
    if near account create-account sponsor-by-faucet-service "$account_id" \
      autogenerate-new-keypair save-to-keychain \
      network-config "$NETWORK" create; then
      echo "Created: $account_id"
      echo "Sleeping ${CREATE_SLEEP_SECONDS}s to reduce faucet rate-limit risk..."
      sleep "$CREATE_SLEEP_SECONDS"
      return 0
    fi

    echo "Create failed for $account_id; retrying after ${CREATE_SLEEP_SECONDS}s..."
    sleep "$CREATE_SLEEP_SECONDS"
    attempt=$((attempt + 1))
  done

  echo "Failed to create account after ${CREATE_RETRIES} attempts: $account_id" >&2
  return 1
}

deploy_contract() {
  local account_id="$1"
  local wasm_path="$2"

  if [[ ! -f "$wasm_path" ]]; then
    echo "Missing wasm artifact: $wasm_path" >&2
    echo "Build first, or run this script with build enabled." >&2
    return 1
  fi

  run_cmd near contract deploy "$account_id" use-file "$wasm_path" without-init-call \
    network-config "$NETWORK" sign-with-keychain send
}

mint_initial_nest_if_requested() {
  if [[ -z "$MINT_RECIPIENTS" ]]; then
    echo "No initial NEST recipients configured (MINT_RECIPIENTS empty); skipping mint."
    return 0
  fi

  if [[ "$MINT_AMOUNT" == "0" ]]; then
    echo "MINT_AMOUNT is 0 while MINT_RECIPIENTS is set; skipping initial mint."
    return 0
  fi

  IFS=',' read -r -a recipients <<< "$MINT_RECIPIENTS"
  local recipient
  for recipient in "${recipients[@]}"; do
    local account
    account="$(echo "$recipient" | tr -d '[:space:]')"
    if [[ -z "$account" ]]; then
      continue
    fi
    near_tx "$TOKEN_ACCOUNT" "storage_deposit" "{\"account_id\":\"$account\",\"registration_only\":true}" "$OWNER_ACCOUNT" "30 Tgas" "0.01 NEAR"
    near_tx "$TOKEN_ACCOUNT" "mint" "{\"account_id\":\"$account\",\"amount\":\"$MINT_AMOUNT\"}" "$MINT_OPERATOR_ACCOUNT"
  done
}

build_contracts() {
  echo "Building contracts..."
  local build_dirs=(
    "contracts/dvm/voting-token"
    "contracts/dvm/finder"
    "contracts/dvm/store"
    "contracts/dvm/identifier-whitelist"
    "contracts/dvm/registry"
    "contracts/dvm/slashing-library"
    "contracts/dvm/voting"
    "contracts/optimistic-oracle"
  )

  local dir
  for dir in "${build_dirs[@]}"; do
    echo "Building $dir"
    (
      cd "${ROOT_DIR}/${dir}"
      run_cmd cargo near build non-reproducible-wasm
    )
  done
}

main() {
  if ! command -v near >/dev/null 2>&1; then
    echo "'near' CLI not found in PATH" >&2
    exit 1
  fi

  require_env "OWNER_ACCOUNT" "$OWNER_ACCOUNT"
  require_env "TREASURY_ACCOUNT" "$TREASURY_ACCOUNT"
  require_env "TOKEN_ACCOUNT" "$TOKEN_ACCOUNT"
  require_env "FINDER_ACCOUNT" "$FINDER_ACCOUNT"
  require_env "STORE_ACCOUNT" "$STORE_ACCOUNT"
  require_env "IDENTIFIER_WHITELIST_ACCOUNT" "$IDENTIFIER_WHITELIST_ACCOUNT"
  require_env "REGISTRY_ACCOUNT" "$REGISTRY_ACCOUNT"
  require_env "SLASHING_ACCOUNT" "$SLASHING_ACCOUNT"
  require_env "VOTING_ACCOUNT" "$VOTING_ACCOUNT"
  require_env "ORACLE_ACCOUNT" "$ORACLE_ACCOUNT"
  if [[ -z "$MINT_OPERATOR_ACCOUNT" ]]; then
    MINT_OPERATOR_ACCOUNT="$OWNER_ACCOUNT"
  fi

  if [[ "${DEPLOY_MOCK_COLLATERAL,,}" == "y" ]]; then
    require_env "MOCK_COLLATERAL_ACCOUNT" "$MOCK_COLLATERAL_ACCOUNT"
    if [[ -z "$MOCK_COLLATERAL_OWNER" ]]; then
      MOCK_COLLATERAL_OWNER="$OWNER_ACCOUNT"
    fi
    if [[ "$MOCK_COLLATERAL_ACCOUNT" == "$TOKEN_ACCOUNT" ]]; then
      echo "MOCK_COLLATERAL_ACCOUNT must be different from TOKEN_ACCOUNT" >&2
      exit 1
    fi
    COLLATERAL_TOKEN="$MOCK_COLLATERAL_ACCOUNT"
  fi

  echo "=== NEST Testnet Deploy (DVM + direct-mint NEST) ==="
  echo

  cat <<EOF

Deployment plan:
  network: $NETWORK
  owner: $OWNER_ACCOUNT
  treasury: $TREASURY_ACCOUNT
  collateral token: $COLLATERAL_TOKEN
  contracts:
    token: $TOKEN_ACCOUNT
    finder: $FINDER_ACCOUNT
    store: $STORE_ACCOUNT
    identifier whitelist: $IDENTIFIER_WHITELIST_ACCOUNT
    registry: $REGISTRY_ACCOUNT
    slashing library: $SLASHING_ACCOUNT
    voting: $VOTING_ACCOUNT
    oracle: $ORACLE_ACCOUNT
  build now: $BUILD_NOW
  create accounts: $CREATE_ACCOUNTS
  deploy mock collateral: $DEPLOY_MOCK_COLLATERAL
  mint operator: $MINT_OPERATOR_ACCOUNT
  mint recipients: ${MINT_RECIPIENTS:-<none>}
  mint amount each: $MINT_AMOUNT
EOF

  if [[ "${BUILD_NOW,,}" == "y" ]]; then
    build_contracts
  fi

  local accounts=(
    "$OWNER_ACCOUNT"
    "$TREASURY_ACCOUNT"
    "$MINT_OPERATOR_ACCOUNT"
    "$TOKEN_ACCOUNT"
    "$FINDER_ACCOUNT"
    "$STORE_ACCOUNT"
    "$IDENTIFIER_WHITELIST_ACCOUNT"
    "$REGISTRY_ACCOUNT"
    "$SLASHING_ACCOUNT"
    "$VOTING_ACCOUNT"
    "$ORACLE_ACCOUNT"
  )

  if [[ "${DEPLOY_MOCK_COLLATERAL,,}" == "y" ]]; then
    accounts+=("$MOCK_COLLATERAL_ACCOUNT")
  fi

  if [[ "${CREATE_ACCOUNTS,,}" == "y" ]]; then
    echo "Creating accounts (if missing)..."
    local acc
    for acc in "${accounts[@]}"; do
      create_account_if_missing "$acc"
    done
  fi

  local wasm_paths=(
    "${ROOT_DIR}/target/near/voting_token/voting_token.wasm"
    "${ROOT_DIR}/target/near/finder/finder.wasm"
    "${ROOT_DIR}/target/near/store/store.wasm"
    "${ROOT_DIR}/target/near/identifier_whitelist/identifier_whitelist.wasm"
    "${ROOT_DIR}/target/near/registry/registry.wasm"
    "${ROOT_DIR}/target/near/slashing_library/slashing_library.wasm"
    "${ROOT_DIR}/target/near/voting/voting.wasm"
    "${ROOT_DIR}/target/near/optimistic_oracle/optimistic_oracle.wasm"
  )

  local deploy_accounts=(
    "$TOKEN_ACCOUNT"
    "$FINDER_ACCOUNT"
    "$STORE_ACCOUNT"
    "$IDENTIFIER_WHITELIST_ACCOUNT"
    "$REGISTRY_ACCOUNT"
    "$SLASHING_ACCOUNT"
    "$VOTING_ACCOUNT"
    "$ORACLE_ACCOUNT"
  )

  echo "Deploying contracts..."
  local i
  for i in "${!deploy_accounts[@]}"; do
    deploy_contract "${deploy_accounts[$i]}" "${wasm_paths[$i]}"
  done

  if [[ "${DEPLOY_MOCK_COLLATERAL,,}" == "y" ]]; then
    echo "Deploying mock collateral token..."
    deploy_contract "$MOCK_COLLATERAL_ACCOUNT" "${ROOT_DIR}/target/near/voting_token/voting_token.wasm"
    near_tx "$MOCK_COLLATERAL_ACCOUNT" "new" "{\"owner\":\"$MOCK_COLLATERAL_OWNER\",\"total_supply\":\"$MOCK_COLLATERAL_TOTAL_SUPPLY\"}" "$MOCK_COLLATERAL_OWNER"
    near_tx "$MOCK_COLLATERAL_ACCOUNT" "set_transfer_restricted" "{\"restricted\":$MOCK_COLLATERAL_TRANSFER_RESTRICTED}" "$MOCK_COLLATERAL_OWNER"
  fi

  echo "Initializing contracts..."
  near_tx "$TOKEN_ACCOUNT" "new" "{\"owner\":\"$OWNER_ACCOUNT\",\"total_supply\":\"0\"}" "$OWNER_ACCOUNT"
  near_tx "$FINDER_ACCOUNT" "new" "{\"owner\":\"$OWNER_ACCOUNT\"}" "$OWNER_ACCOUNT"
  near_tx "$STORE_ACCOUNT" "new" "{\"owner\":\"$OWNER_ACCOUNT\",\"withdrawer\":\"$TREASURY_ACCOUNT\"}" "$OWNER_ACCOUNT"
  near_tx "$IDENTIFIER_WHITELIST_ACCOUNT" "new" "{\"owner\":\"$OWNER_ACCOUNT\"}" "$OWNER_ACCOUNT"
  near_tx "$REGISTRY_ACCOUNT" "new" "{\"owner\":\"$OWNER_ACCOUNT\"}" "$OWNER_ACCOUNT"
  near_tx "$SLASHING_ACCOUNT" "new" "{\"owner\":\"$OWNER_ACCOUNT\",\"base_slashing_rate\":$SLASHING_RATE_BPS}" "$OWNER_ACCOUNT"
  near_tx "$VOTING_ACCOUNT" "new" "{\"owner\":\"$OWNER_ACCOUNT\"}" "$OWNER_ACCOUNT"
  near_tx "$ORACLE_ACCOUNT" "new" "{\"owner\":\"$OWNER_ACCOUNT\",\"default_currency\":\"$COLLATERAL_TOKEN\",\"default_liveness_ns\":\"$ORACLE_LIVENESS_NS\",\"burned_bond_percentage\":\"$BURNED_BOND_PERCENTAGE\",\"voting_contract\":\"$VOTING_ACCOUNT\"}" "$OWNER_ACCOUNT"

  echo "Running post-deploy wiring..."
  near_tx "$TOKEN_ACCOUNT" "add_minter" "{\"account_id\":\"$MINT_OPERATOR_ACCOUNT\"}" "$OWNER_ACCOUNT"
  near_tx "$TOKEN_ACCOUNT" "set_transfer_restricted" "{\"restricted\":true}" "$OWNER_ACCOUNT"
  near_tx "$TOKEN_ACCOUNT" "add_transfer_router" "{\"account_id\":\"$VOTING_ACCOUNT\"}" "$OWNER_ACCOUNT"
  near_tx "$VOTING_ACCOUNT" "set_voting_token" "{\"voting_token\":\"$TOKEN_ACCOUNT\"}" "$OWNER_ACCOUNT"
  near_tx "$VOTING_ACCOUNT" "set_treasury" "{\"treasury\":\"$TREASURY_ACCOUNT\"}" "$OWNER_ACCOUNT"
  near_tx "$VOTING_ACCOUNT" "set_slashing_treasury_bps" "{\"bps\":$SLASHING_TREASURY_BPS}" "$OWNER_ACCOUNT"
  near_tx "$VOTING_ACCOUNT" "set_min_participation_rate" "{\"rate_bps\":$MIN_PARTICIPATION_BPS}" "$OWNER_ACCOUNT"
  near_tx "$VOTING_ACCOUNT" "set_commit_phase_duration" "{\"duration_ns\":$COMMIT_DURATION_NS}" "$OWNER_ACCOUNT"
  near_tx "$VOTING_ACCOUNT" "set_reveal_phase_duration" "{\"duration_ns\":$REVEAL_DURATION_NS}" "$OWNER_ACCOUNT"

  near_tx "$STORE_ACCOUNT" "set_final_fee" "{\"currency\":\"$COLLATERAL_TOKEN\",\"fee\":\"$FINAL_FEE\"}" "$OWNER_ACCOUNT"
  near_tx "$ORACLE_ACCOUNT" "whitelist_currency" "{\"currency\":\"$COLLATERAL_TOKEN\",\"final_fee\":\"$FINAL_FEE\"}" "$OWNER_ACCOUNT"
  near_tx "$ORACLE_ACCOUNT" "set_voting_contract" "{\"voting_contract\":\"$VOTING_ACCOUNT\"}" "$OWNER_ACCOUNT"
  near_tx "$ORACLE_ACCOUNT" "whitelist_identifier" "{\"identifier\":$ASSERT_TRUTH_BYTES32}" "$OWNER_ACCOUNT"

  near_tx "$IDENTIFIER_WHITELIST_ACCOUNT" "add_supported_identifier" "{\"identifier\":\"ASSERT_TRUTH\"}" "$OWNER_ACCOUNT"
  near_tx "$IDENTIFIER_WHITELIST_ACCOUNT" "add_supported_identifier" "{\"identifier\":\"YES_OR_NO_QUERY\"}" "$OWNER_ACCOUNT"

  near_tx "$FINDER_ACCOUNT" "change_implementation_address" "{\"interface_name\":\"Store\",\"implementation_address\":\"$STORE_ACCOUNT\"}" "$OWNER_ACCOUNT"
  near_tx "$FINDER_ACCOUNT" "change_implementation_address" "{\"interface_name\":\"Registry\",\"implementation_address\":\"$REGISTRY_ACCOUNT\"}" "$OWNER_ACCOUNT"
  near_tx "$FINDER_ACCOUNT" "change_implementation_address" "{\"interface_name\":\"IdentifierWhitelist\",\"implementation_address\":\"$IDENTIFIER_WHITELIST_ACCOUNT\"}" "$OWNER_ACCOUNT"
  near_tx "$FINDER_ACCOUNT" "change_implementation_address" "{\"interface_name\":\"SlashingLibrary\",\"implementation_address\":\"$SLASHING_ACCOUNT\"}" "$OWNER_ACCOUNT"
  near_tx "$FINDER_ACCOUNT" "change_implementation_address" "{\"interface_name\":\"Voting\",\"implementation_address\":\"$VOTING_ACCOUNT\"}" "$OWNER_ACCOUNT"
  near_tx "$FINDER_ACCOUNT" "change_implementation_address" "{\"interface_name\":\"VotingToken\",\"implementation_address\":\"$TOKEN_ACCOUNT\"}" "$OWNER_ACCOUNT"
  near_tx "$FINDER_ACCOUNT" "change_implementation_address" "{\"interface_name\":\"Oracle\",\"implementation_address\":\"$ORACLE_ACCOUNT\"}" "$OWNER_ACCOUNT"

  near_tx "$REGISTRY_ACCOUNT" "register_contract" "{\"contract_address\":\"$ORACLE_ACCOUNT\"}" "$OWNER_ACCOUNT"

  echo "Running storage registrations..."
  near_tx "$TOKEN_ACCOUNT" "storage_deposit" "{\"account_id\":\"$OWNER_ACCOUNT\",\"registration_only\":true}" "$OWNER_ACCOUNT" "30 Tgas" "0.01 NEAR"
  near_tx "$TOKEN_ACCOUNT" "storage_deposit" "{\"account_id\":\"$MINT_OPERATOR_ACCOUNT\",\"registration_only\":true}" "$OWNER_ACCOUNT" "30 Tgas" "0.01 NEAR"
  near_tx "$TOKEN_ACCOUNT" "storage_deposit" "{\"account_id\":\"$VOTING_ACCOUNT\",\"registration_only\":true}" "$OWNER_ACCOUNT" "30 Tgas" "0.01 NEAR"
  near_tx "$TOKEN_ACCOUNT" "storage_deposit" "{\"account_id\":\"$TREASURY_ACCOUNT\",\"registration_only\":true}" "$OWNER_ACCOUNT" "30 Tgas" "0.01 NEAR"
  near_tx "$TOKEN_ACCOUNT" "storage_deposit" "{\"account_id\":\"$ORACLE_ACCOUNT\",\"registration_only\":true}" "$OWNER_ACCOUNT" "30 Tgas" "0.01 NEAR"
  near_tx "$COLLATERAL_TOKEN" "storage_deposit" "{\"account_id\":\"$ORACLE_ACCOUNT\",\"registration_only\":true}" "$OWNER_ACCOUNT" "30 Tgas" "0.01 NEAR"
  mint_initial_nest_if_requested

  echo
  echo "Deployment complete."
  echo "Recommended checks:"
  echo "  near contract call-function as-read-only $TOKEN_ACCOUNT is_minter json-args '{\"account_id\":\"$MINT_OPERATOR_ACCOUNT\"}' network-config $NETWORK now"
  echo "  near contract call-function as-read-only $TOKEN_ACCOUNT get_transfer_restricted json-args '{}' network-config $NETWORK now"
  echo "  near contract call-function as-read-only $ORACLE_ACCOUNT is_currency_whitelisted json-args '{\"currency\":\"$COLLATERAL_TOKEN\"}' network-config $NETWORK now"
}

main "$@"
