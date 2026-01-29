# Store

Oracle fee collection and management contract. Tracks final fees per currency.

## Overview

- Manages final fees per currency (NEP-141 tokens)
- Final fees are paid when disputes are resolved
- Withdrawer can collect accumulated fees
- Owner can configure fee amounts

## Building

```bash
cd contracts/dvm/store
cargo near build non-reproducible-wasm
```

## Deployment

### 1. Create account and deploy

```bash
near account create-account sponsor-by-faucet-service nest-store.testnet autogenerate-new-keypair save-to-keychain network-config testnet create

near deploy nest-store.testnet ../../../target/near/store/store.wasm
```

### 2. Initialize the contract

```bash
near contract call-function as-transaction nest-store.testnet new json-args '{
  "owner": "YOUR_OWNER_ACCOUNT.testnet",
  "withdrawer": "YOUR_TREASURY_ACCOUNT.testnet"
}' prepaid-gas '30 Tgas' attached-deposit '0 NEAR' sign-as nest-store.testnet network-config testnet sign-with-keychain send
```

**Parameters:**
- `owner`: Account that can set fees and manage withdrawer
- `withdrawer`: Account that can withdraw collected fees

### 3. Set final fees for currencies

```bash
# Set final fee for wNEAR (24 decimals, 1 NEAR = 1000000000000000000000000)
near contract call-function as-transaction nest-store.testnet set_final_fee json-args '{
  "currency": "wrap.testnet",
  "fee": "1000000000000000000000000"
}' prepaid-gas '30 Tgas' attached-deposit '0 NEAR' sign-as YOUR_OWNER_ACCOUNT.testnet network-config testnet sign-with-keychain send
```

## View Methods

```bash
# Get final fee for a currency
near contract call-function as-read-only nest-store.testnet get_final_fee json-args '{"currency": "wrap.testnet"}' network-config testnet now

# Check if final fee is set
near contract call-function as-read-only nest-store.testnet has_final_fee json-args '{"currency": "wrap.testnet"}' network-config testnet now
```

## Testing

```bash
cargo test -p store
```
