# Whitelist Disputer Escalation Manager

Restricts who can dispute assertions to a whitelisted set of accounts.

## Overview

- Only whitelisted accounts can file disputes
- Owner can add/remove accounts from the whitelist
- Useful for controlled environments with trusted disputers
- Inherits other behaviors from base escalation manager

## Building

```bash
cd contracts/escalation-manager/whitelist-disputer
cargo near build non-reproducible-wasm
```

## Deployment

### 1. Create account and deploy

```bash
near account create-account sponsor-by-faucet-service nest-escalation-whitelist.testnet autogenerate-new-keypair save-to-folder ~/.near-credentials/testnet network-config testnet create

near contract deploy nest-escalation-whitelist.testnet use-file ../../../target/near/whitelist_disputer_escalation_manager/whitelist_disputer_escalation_manager.wasm without-init-call network-config testnet sign-with-keychain send
```

### 2. Initialize the contract

```bash
near contract call-function as-transaction nest-escalation-whitelist.testnet new json-args '{
  "oracle": "nest-oracle.testnet"
}' prepaid-gas '30 Tgas' attached-deposit '0 NEAR' sign-as nest-escalation-whitelist.testnet network-config testnet sign-with-keychain send
```

**Parameters:**
- `oracle`: The Optimistic Oracle contract

**Note:** The deployer automatically becomes the owner.

### 3. Add whitelisted disputers

```bash
# Add a disputer to the whitelist
near contract call-function as-transaction nest-escalation-whitelist.testnet set_dispute_caller_in_whitelist json-args '{
  "dispute_caller": "trusted-disputer.testnet",
  "whitelisted": true
}' prepaid-gas '30 Tgas' attached-deposit '0 NEAR' sign-as YOUR_OWNER_ACCOUNT.testnet network-config testnet sign-with-keychain send

# Remove a disputer from the whitelist
near contract call-function as-transaction nest-escalation-whitelist.testnet set_dispute_caller_in_whitelist json-args '{
  "dispute_caller": "untrusted.testnet",
  "whitelisted": false
}' prepaid-gas '30 Tgas' attached-deposit '0 NEAR' sign-as YOUR_OWNER_ACCOUNT.testnet network-config testnet sign-with-keychain send
```

## View Methods

```bash
# Check if an account is whitelisted
near contract call-function as-read-only nest-escalation-whitelist.testnet is_dispute_allowed json-args '{
  "assertion_id": [1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32],
  "dispute_caller": "alice.testnet"
}' network-config testnet now

# Get all whitelisted disputers
near contract call-function as-read-only nest-escalation-whitelist.testnet get_whitelisted_dispute_callers json-args '{}' network-config testnet now
```

## Testing

```bash
cargo test -p whitelist-disputer-escalation-manager
```
