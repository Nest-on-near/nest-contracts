# Optimistic Oracle

The main entry point for the Nest oracle system. Accepts assertions with bonded stakes and handles dispute resolution.

## Overview

- Accepts truth assertions with bond tokens via `ft_transfer_call`
- Enforces liveness periods during which assertions can be disputed
- Settles assertions by distributing bonds to the correct party
- Supports callback notifications on resolution

## How It Works

1. **Assert**: User sends bond tokens with assertion claim
2. **Liveness Period**: Anyone can dispute by matching the bond
3. **Settlement**: 
   - If undisputed: Asserter gets bond back after liveness
   - If disputed: Winner gets both bonds minus oracle fee

## Building

```bash
cd contracts/optimistic-oracle
cargo near build non-reproducible-wasm
```

## Deployment

### 1. Create account and deploy

```bash
near account create-account sponsor-by-faucet-service nest-oracle.testnet autogenerate-new-keypair save-to-keychain network-config testnet create

near deploy nest-oracle.testnet ../../target/near/optimistic_oracle/optimistic_oracle.wasm
```

### 2. Initialize the contract

```bash
near contract call-function as-transaction nest-oracle.testnet new json-args '{
  "owner": "YOUR_OWNER_ACCOUNT.testnet",
  "default_currency": "wrap.testnet",
  "default_liveness_ns": "7200000000000",
  "burned_bond_percentage": "500000000000000000"
}' prepaid-gas '30 Tgas' attached-deposit '0 NEAR' sign-as nest-oracle.testnet network-config testnet sign-with-keychain send
```

**Parameters:**
- `owner`: Account that can configure oracle settings
- `default_currency`: Default NEP-141 token for bonds
- `default_liveness_ns`: Default liveness period in nanoseconds (optional, default 2 hours = 7200000000000)
- `burned_bond_percentage`: Fee percentage scaled by 1e18 (optional, default 50% = 500000000000000000)

### 3. Whitelist currencies

```bash
# Whitelist wNEAR with final fee of 0.1 NEAR
near contract call-function as-transaction nest-oracle.testnet whitelist_currency json-args '{
  "currency": "wrap.testnet",
  "final_fee": "100000000000000000000000"
}' prepaid-gas '30 Tgas' attached-deposit '0 NEAR' sign-as YOUR_OWNER_ACCOUNT.testnet network-config testnet sign-with-keychain send
```

## Making an Assertion

Assertions are made via `ft_transfer_call` on the bond token:

```bash
# Transfer tokens to oracle with assertion message
near contract call-function as-transaction wrap.testnet ft_transfer_call json-args '{
  "receiver_id": "nest-oracle.testnet",
  "amount": "1000000000000000000000000",
  "msg": "{\"action\":\"AssertTruth\",\"claim\":[1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32],\"asserter\":\"alice.testnet\",\"callback_recipient\":null,\"escalation_manager\":null,\"liveness_ns\":null,\"identifier\":null,\"domain_id\":null}"
}' prepaid-gas '100 Tgas' attached-deposit '1 yoctoNEAR' sign-as alice.testnet network-config testnet sign-with-keychain send
```

## View Methods

```bash
# Get minimum bond for a currency
near contract call-function as-read-only nest-oracle.testnet get_minimum_bond json-args '{"currency": "wrap.testnet"}' network-config testnet now

# Get assertion details
near contract call-function as-read-only nest-oracle.testnet get_assertion json-args '{"assertion_id": [1,2,3,...,32]}' network-config testnet now

# Check if currency is whitelisted
near contract call-function as-read-only nest-oracle.testnet is_currency_whitelisted json-args '{"currency": "wrap.testnet"}' network-config testnet now
```

## Testing

```bash
cargo test -p optimistic-oracle
```
