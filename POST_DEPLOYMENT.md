# Post-Deployment Configuration

After deploying all contracts, complete these configuration steps in order.

## Deployed Addresses

| Contract | Address |
|----------|---------|
| Voting Token | `nest-token-1.testnet` |
| Finder | `nest-finder-1.testnet` |
| Store | `nest-store-1.testnet` |
| Identifier Whitelist | `nest-identifiers-1.testnet` |
| Registry | `nest-registry-1.testnet` |
| Slashing Library | `nest-slashing-1.testnet` |
| Voting | `nest-voting-1.testnet` |
| Optimistic Oracle | `nest-oracle-2.testnet` |
| **Owner** | `nest-owner-1.testnet` |
| **Treasury** | `nest-treasury-1.testnet` |

---

## Step 1: Add Voting as Minter

Allow the Voting contract to mint reward tokens:

```bash
near contract call-function as-transaction nest-token-1.testnet add_minter json-args '{
  "account_id": "nest-voting-1.testnet"
}' prepaid-gas '30 Tgas' attached-deposit '0 NEAR' sign-as nest-owner-1.testnet network-config testnet sign-with-keychain send
```

**Verify:**
```bash
near contract call-function as-read-only nest-token-1.testnet is_minter json-args '{
  "account_id": "nest-voting-1.testnet"
}' network-config testnet now
```

---

## Step 2: Register Interfaces in Finder

Register all DVM contracts with the Finder:

```bash
# Register Store
near contract call-function as-transaction nest-finder-1.testnet change_implementation_address json-args '{
  "interface_name": "Store",
  "implementation_address": "nest-store-1.testnet"
}' prepaid-gas '30 Tgas' attached-deposit '0 NEAR' sign-as nest-owner-1.testnet network-config testnet sign-with-keychain send

# Register Registry
near contract call-function as-transaction nest-finder-1.testnet change_implementation_address json-args '{
  "interface_name": "Registry",
  "implementation_address": "nest-registry-1.testnet"
}' prepaid-gas '30 Tgas' attached-deposit '0 NEAR' sign-as nest-owner-1.testnet network-config testnet sign-with-keychain send

# Register IdentifierWhitelist
near contract call-function as-transaction nest-finder-1.testnet change_implementation_address json-args '{
  "interface_name": "IdentifierWhitelist",
  "implementation_address": "nest-identifiers-1.testnet"
}' prepaid-gas '30 Tgas' attached-deposit '0 NEAR' sign-as nest-owner-1.testnet network-config testnet sign-with-keychain send

# Register SlashingLibrary
near contract call-function as-transaction nest-finder-1.testnet change_implementation_address json-args '{
  "interface_name": "SlashingLibrary",
  "implementation_address": "nest-slashing-1.testnet"
}' prepaid-gas '30 Tgas' attached-deposit '0 NEAR' sign-as nest-owner-1.testnet network-config testnet sign-with-keychain send

# Register Voting
near contract call-function as-transaction nest-finder-1.testnet change_implementation_address json-args '{
  "interface_name": "Voting",
  "implementation_address": "nest-voting-1.testnet"
}' prepaid-gas '30 Tgas' attached-deposit '0 NEAR' sign-as nest-owner-1.testnet network-config testnet sign-with-keychain send
```

**Verify:**
```bash
near contract call-function as-read-only nest-finder-1.testnet get_implementation_address json-args '{
  "interface_name": "Store"
}' network-config testnet now
```

---

## Step 3: Whitelist Price Identifiers

Add approved identifiers to the whitelist:

```bash
# Whitelist ASSERT_TRUTH (default identifier)
near contract call-function as-transaction nest-identifiers-1.testnet add_supported_identifier json-args '{
  "identifier": [65,83,83,69,82,84,95,84,82,85,84,72,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]
}' prepaid-gas '30 Tgas' attached-deposit '0 NEAR' sign-as nest-owner-1.testnet network-config testnet sign-with-keychain send

# Whitelist YES_OR_NO_QUERY
near contract call-function as-transaction nest-identifiers-1.testnet add_supported_identifier json-args '{
  "identifier": [89,69,83,95,79,82,95,78,79,95,81,85,69,82,89,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]
}' prepaid-gas '30 Tgas' attached-deposit '0 NEAR' sign-as nest-owner-1.testnet network-config testnet sign-with-keychain send
```

**Verify:**
```bash
near contract call-function as-read-only nest-identifiers-1.testnet is_identifier_supported json-args '{
  "identifier": [65,83,83,69,82,84,95,84,82,85,84,72,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]
}' network-config testnet now
```

---

## Step 4: Set Final Fees in Store

Configure final fees for bond currencies:

```bash
# Set final fee for wNEAR (0.1 NEAR = 100000000000000000000000)
near contract call-function as-transaction nest-store-1.testnet set_final_fee json-args '{
  "currency": "wrap.testnet",
  "fee": "100000000000000000000000"
}' prepaid-gas '30 Tgas' attached-deposit '0 NEAR' sign-as nest-owner-1.testnet network-config testnet sign-with-keychain send
```

**Verify:**
```bash
near contract call-function as-read-only nest-store-1.testnet get_final_fee json-args '{
  "currency": "wrap.testnet"
}' network-config testnet now
```

---

## Step 5: Whitelist Currencies in Oracle

Whitelist bond tokens in the Optimistic Oracle:

```bash
# Whitelist wNEAR with final fee of 0.1 NEAR
near contract call-function as-transaction nest-oracle-2.testnet whitelist_currency json-args '{
  "currency": "wrap.testnet",
  "final_fee": "100000000000000000000000"
}' prepaid-gas '30 Tgas' attached-deposit '0 NEAR' sign-as nest-owner-1.testnet network-config testnet sign-with-keychain send
```

**Verify:**
```bash
near contract call-function as-read-only nest-oracle-2.testnet is_currency_whitelisted json-args '{
  "currency": "wrap.testnet"
}' network-config testnet now

# Check minimum bond required
near contract call-function as-read-only nest-oracle-2.testnet get_minimum_bond json-args '{
  "currency": "wrap.testnet"
}' network-config testnet now
```

---

## Step 6: Whitelist ASSERT_TRUTH Identifier in Oracle

Whitelist the default identifier (should be done during deployment, but verify):

```bash
near contract call-function as-transaction nest-oracle-2.testnet whitelist_identifier json-args '{
  "identifier": [65,83,83,69,82,84,95,84,82,85,84,72,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]
}' prepaid-gas '30 Tgas' attached-deposit '0 NEAR' sign-as nest-owner-1.testnet network-config testnet sign-with-keychain send
```

**Verify:**
```bash
near contract call-function as-read-only nest-oracle-2.testnet is_identifier_supported json-args '{
  "identifier": [65,83,83,69,82,84,95,84,82,85,84,72,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]
}' network-config testnet now
```

---

## Step 7: Register Oracle in Registry

Allow the Oracle to make price requests to the DVM:

```bash
near contract call-function as-transaction nest-registry-1.testnet register_contract json-args '{
  "contract_address": "nest-oracle-2.testnet"
}' prepaid-gas '30 Tgas' attached-deposit '0 NEAR' sign-as nest-owner-1.testnet network-config testnet sign-with-keychain send
```

**Verify:**
```bash
near contract call-function as-read-only nest-registry-1.testnet is_contract_registered json-args '{
  "contract_address": "nest-oracle-2.testnet"
}' network-config testnet now
```

---

## Configuration Complete! 🎉

Your Nest oracle system is now fully configured and ready to accept assertions.

### Quick Health Check

Run all verification commands to ensure everything is configured correctly:

```bash
# Check Voting is a minter
near contract call-function as-read-only nest-token-1.testnet is_minter json-args '{"account_id": "nest-voting-1.testnet"}' network-config testnet now

# Check Finder has Store registered
near contract call-function as-read-only nest-finder-1.testnet get_implementation_address json-args '{"interface_name": "Store"}' network-config testnet now

# Check identifier is whitelisted
near contract call-function as-read-only nest-identifiers-1.testnet is_identifier_supported json-args '{"identifier": [65,83,83,69,82,84,95,84,82,85,84,72,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]}' network-config testnet now

# Check currency is whitelisted in Oracle
near contract call-function as-read-only nest-oracle-2.testnet is_currency_whitelisted json-args '{"currency": "wrap.testnet"}' network-config testnet now

# Check Oracle is registered
near contract call-function as-read-only nest-registry-1.testnet is_contract_registered json-args '{"contract_address": "nest-oracle-2.testnet"}' network-config testnet now
```

All commands should return `true` or the expected address.

### Next Steps

Now you can:
1. **Make test assertions** - See the Oracle README for examples
2. **Build a UI** - Integrate with nest-ui
3. **Write JS tests** - Create test scripts with near-api-js
