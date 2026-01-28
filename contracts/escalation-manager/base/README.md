# Base Escalation Manager

Default escalation manager implementation with permissive policies.

## Overview

- Provides default (permissive) behavior for all escalation hooks
- Allows all disputes
- Returns default assertion policy (no special handling)
- Can be extended or used as-is for simple use cases

## Building

```bash
cd contracts/escalation-manager/base
cargo near build non-reproducible-wasm
```

## Deployment

### 1. Create account and deploy

```bash
near account create-account sponsor-by-faucet-service nest-escalation-base.testnet autogenerate-new-keypair save-to-folder ~/.near-credentials/testnet network-config testnet create

near contract deploy nest-escalation-base.testnet use-file ../../../target/near/base_escalation_manager/base_escalation_manager.wasm without-init-call network-config testnet sign-with-keychain send
```

### 2. Initialize the contract

```bash
near contract call-function as-transaction nest-escalation-base.testnet new json-args '{
  "oracle": "nest-oracle.testnet"
}' prepaid-gas '30 Tgas' attached-deposit '0 NEAR' sign-as nest-escalation-base.testnet network-config testnet sign-with-keychain send
```

**Parameters:**
- `oracle`: The Optimistic Oracle contract that will call this escalation manager

## Usage

This escalation manager can be used when creating assertions by setting the `escalation_manager` field:

```json
{
  "action": "AssertTruth",
  "escalation_manager": "nest-escalation-base.testnet",
  ...
}
```

## View Methods

```bash
# Get the oracle address
near contract call-function as-read-only nest-escalation-base.testnet get_oracle json-args '{}' network-config testnet now

# Check if dispute is allowed (always returns true in base implementation)
near contract call-function as-read-only nest-escalation-base.testnet is_dispute_allowed json-args '{
  "assertion_id": [1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32],
  "dispute_caller": "alice.testnet"
}' network-config testnet now
```

## Testing

```bash
cargo test -p base-escalation-manager
```
