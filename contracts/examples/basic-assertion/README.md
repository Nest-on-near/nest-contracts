# Basic Assertion Example

The simplest example of integrating with the Nest Optimistic Oracle.

This contract demonstrates:
- Accepting user assertions with bond tokens via `ft_transfer_call`
- Forwarding assertions to the oracle
- Receiving callbacks when assertions are resolved

## Flow

```
User → ft_transfer_call (wNEAR + claim) → This Contract → ft_transfer_call → Oracle
```

1. User calls `ft_transfer_call` on `wrap.testnet` with their bond + claim message
2. This contract receives tokens via `ft_on_transfer`
3. Contract forwards tokens to oracle with assertion details
4. Oracle creates the assertion with user as the `asserter` (bond recipient)
5. After liveness period, anyone can settle the assertion
6. Oracle calls `assertion_resolved_callback` on this contract

## Build

```bash
cd contracts/basic-assertion && cargo near build
```

## Deploy and Initialize

```bash
# Create account
near create-account <your-account>.testnet --useFaucet

# Deploy
near deploy <your-account>.testnet ./target/near/basic_assertion/basic_assertion.wasm

# Initialize
near call <your-account>.testnet new '{
  "oracle": "<oracle-account>.testnet",
  "bond_token": "wrap.testnet",
  "min_bond": "2000000000000000000000000"
}' --accountId <your-account>.testnet

# Register storage with wrap.testnet
near call wrap.testnet storage_deposit '{"account_id": "<your-account>.testnet"}' --accountId <your-account>.testnet --deposit 0.00125
```

## Make an Assertion

Users send wNEAR with their claim:

```bash
near call wrap.testnet ft_transfer_call '{
  "receiver_id": "<your-account>.testnet",
  "amount": "2000000000000000000000000",
  "msg": "{\"claim\": \"Today is 18th January\"}"
}' --accountId <user-account>.testnet --depositYocto 1 --gas 100000000000000
```

## Check Results

```bash
near view <your-account>.testnet get_last_claim
near view <your-account>.testnet get_last_assertion_id
near view <your-account>.testnet get_last_assertion_result
```

## Contract Methods

| Method | Type | Description |
|--------|------|-------------|
| `new(oracle, bond_token, min_bond)` | init | Initialize the contract |
| `ft_on_transfer(sender_id, amount, msg)` | call | Receives user tokens and creates assertion |
| `assertion_resolved_callback(...)` | callback | Called by oracle when assertion resolves |
| `get_oracle()` | view | Get the oracle address |
| `get_bond_token()` | view | Get the bond token address |
| `get_min_bond()` | view | Get the minimum bond amount |
| `get_last_claim()` | view | Get the last claim string |
| `get_last_assertion_id()` | view | Get the last assertion ID (hex) |
| `get_last_assertion_result()` | view | Get the last assertion result |
