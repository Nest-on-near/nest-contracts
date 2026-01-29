# Slashing Library

Calculates slashing penalties for incorrect votes in DVM voting.

## Overview

- Calculates penalties for voters who voted against the resolved outcome
- Configurable slashing rate in basis points (10000 = 100%)
- Used by Voting contract to determine slashing amounts

## Slashing Formula

```
slashing_amount = wrong_vote_stake * slashing_rate / 10000
```

## Building

```bash
cd contracts/dvm/slashing-library
cargo near build non-reproducible-wasm
```

## Deployment

### 1. Create account and deploy

```bash
near account create-account sponsor-by-faucet-service nest-slashing.testnet autogenerate-new-keypair save-to-keychain network-config testnet create

near deploy nest-slashing.testnet ../../../target/near/slashing_library/slashing_library.wasm
```

### 2. Initialize the contract

```bash
near contract call-function as-transaction nest-slashing.testnet new json-args '{
  "owner": "YOUR_OWNER_ACCOUNT.testnet",
  "base_slashing_rate": 1000
}' prepaid-gas '30 Tgas' attached-deposit '0 NEAR' sign-as nest-slashing.testnet network-config testnet sign-with-keychain send
```

**Parameters:**
- `owner`: Account that can configure slashing parameters
- `base_slashing_rate`: Slashing rate in basis points (1000 = 10%, max 10000 = 100%)

## View Methods

```bash
# Get slashing rate
near contract call-function as-read-only nest-slashing.testnet get_base_slashing_rate json-args '{}' network-config testnet now

# Calculate slashing amount
near contract call-function as-read-only nest-slashing.testnet calculate_slashing json-args '{"wrong_vote_total_stake": "1000000000000000000000000"}' network-config testnet now
```

## Testing

```bash
cargo test -p slashing-library
```
