# Full Policy Escalation Manager

Comprehensive escalation manager with configurable assertion/dispute policies.

## Overview

- **Assertion Control**: Whitelist contracts and asserters
- **Dispute Control**: Whitelist disputers  
- **Custom Arbitration**: Owner can manually resolve disputes instead of DVM
- **Oracle Override**: Optionally discard oracle resolution
- Fully configurable for complex governance scenarios

## Building

```bash
cd contracts/escalation-manager/full-policy
cargo near build non-reproducible-wasm
```

## Deployment

### 1. Create account and deploy

```bash
near account create-account sponsor-by-faucet-service nest-escalation-full.testnet autogenerate-new-keypair save-to-keychain network-config testnet create

near deploy nest-escalation-full.testnet ../../../target/near/full_policy_escalation_manager/full_policy_escalation_manager.wasm
```

### 2. Initialize the contract

```bash
near contract call-function as-transaction nest-escalation-full.testnet new json-args '{
  "oracle": "nest-oracle-3.testnet"
}' prepaid-gas '30 Tgas' attached-deposit '0 NEAR' sign-as nest-escalation-full.testnet network-config testnet sign-with-keychain send
```

**Parameters:**
- `oracle`: The Optimistic Oracle contract

**Note:** The deployer automatically becomes the owner.

### 3. Configure policies

```bash
# Enable full policy controls
near contract call-function as-transaction nest-escalation-full.testnet configure json-args '{
  "block_by_asserting_caller": true,
  "block_by_asserter": true,
  "validate_disputers": true,
  "arbitrate_via_escalation_manager": true,
  "discard_oracle": false
}' prepaid-gas '30 Tgas' attached-deposit '0 NEAR' sign-as YOUR_OWNER_ACCOUNT.testnet network-config testnet sign-with-keychain send
```

**Policy Flags:**
- `block_by_asserting_caller`: Only whitelisted contracts can create assertions
- `block_by_asserter`: Only whitelisted accounts can be asserters (requires `block_by_asserting_caller`)
- `validate_disputers`: Only whitelisted accounts can dispute
- `arbitrate_via_escalation_manager`: Owner manually resolves disputes (no DVM)
- `discard_oracle`: Ignore oracle resolution in callbacks

### 4. Manage whitelists

```bash
# Whitelist a contract that can create assertions
near contract call-function as-transaction nest-escalation-full.testnet set_asserting_caller_in_whitelist json-args '{
  "asserting_caller": "prediction-market.testnet",
  "whitelisted": true
}' prepaid-gas '30 Tgas' attached-deposit '0 NEAR' sign-as YOUR_OWNER_ACCOUNT.testnet network-config testnet sign-with-keychain send

# Whitelist an asserter
near contract call-function as-transaction nest-escalation-full.testnet set_asserter_in_whitelist json-args '{
  "asserter": "alice.testnet",
  "whitelisted": true
}' prepaid-gas '30 Tgas' attached-deposit '0 NEAR' sign-as YOUR_OWNER_ACCOUNT.testnet network-config testnet sign-with-keychain send

# Whitelist a disputer
near contract call-function as-transaction nest-escalation-full.testnet set_dispute_caller_in_whitelist json-args '{
  "dispute_caller": "trusted-disputer.testnet",
  "whitelisted": true
}' prepaid-gas '30 Tgas' attached-deposit '0 NEAR' sign-as YOUR_OWNER_ACCOUNT.testnet network-config testnet sign-with-keychain send
```

### 5. Manual arbitration (if enabled)

```bash
# Set resolution for a disputed assertion
near contract call-function as-transaction nest-escalation-full.testnet set_resolution json-args '{
  "identifier": [65,83,83,69,82,84,95,84,82,85,84,72,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
  "time": 1234567890000000000,
  "ancillary_data": [],
  "resolution": true
}' prepaid-gas '30 Tgas' attached-deposit '0 NEAR' sign-as YOUR_OWNER_ACCOUNT.testnet network-config testnet sign-with-keychain send
```

## View Methods

```bash
# Get policy configuration
near contract call-function as-read-only nest-escalation-full.testnet get_config json-args '{}' network-config testnet now

# Check if account is whitelisted as asserter
near contract call-function as-read-only nest-escalation-full.testnet is_on_asserter_whitelist json-args '{"asserter": "alice.testnet"}' network-config testnet now

# Get assertion policy for an assertion
near contract call-function as-read-only nest-escalation-full.testnet get_assertion_policy json-args '{
  "assertion_id": [1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32]
}' network-config testnet now
```

## Testing

```bash
cargo test -p full-policy-escalation-manager
```
