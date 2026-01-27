# Nest - Optimistic Oracle for NEAR

An optimistic oracle implementation for NEAR Protocol, inspired by UMA's Optimistic Oracle V3.

## Prerequisites

- [Rust](https://rustup.rs/)
- [cargo-near](https://github.com/near/cargo-near) - `cargo install cargo-near`
- [NEAR CLI](https://near.cli.rs) - `cargo install near-cli-rs`

## How to Build

Build the optimistic oracle contract:

```bash
cd contracts/optimistic-oracle && cargo near build
```

## How to Test

```bash
cargo test
```

## Testnet Deployment

### 1. Create a testnet account

```bash
near create-account <your-account>.testnet --useFaucet
```

### 2. Deploy the contract

```bash
near deploy <your-account>.testnet ./target/near/optimistic_oracle/optimistic_oracle.wasm
```

### 3. Initialize the contract

```bash
near call <your-account>.testnet new '{"owner": "<your-account>.testnet", "default_currency": "wrap.testnet"}' --accountId <your-account>.testnet
```

### 4. Whitelist a currency

Before assertions can be made, you need to whitelist at least one currency:

```bash
near call <your-account>.testnet whitelist_currency '{"currency": "wrap.testnet", "final_fee": "1000000000000000000"}' --accountId <your-account>.testnet
```

## Contract Methods

### View Methods

- `default_identifier()` - Returns the default identifier (ASSERT_TRUTH)
- `default_currency()` - Returns the default currency account
- `default_liveness()` - Returns the default liveness period in nanoseconds
- `get_assertion(assertion_id)` - Get assertion details
- `get_minimum_bond(currency)` - Get minimum bond for a currency
- `get_assertion_result(assertion_id)` - Get the resolution of a settled assertion
- `is_identifier_supported(identifier)` - Check if an identifier is approved
- `is_currency_whitelisted(currency)` - Check if a currency is whitelisted

### Admin Methods (owner only)

- `set_admin_properties(...)` - Update default currency, liveness, and burn percentage
- `whitelist_currency(currency, final_fee)` - Whitelist a currency
- `whitelist_identifier(identifier)` - Approve an identifier
- `resolve_disputed_assertion(assertion_id, resolution)` - Manually resolve disputes (Phase 1)

### Core Methods

- `ft_on_transfer(...)` - NEP-141 receiver for creating assertions and disputes via `ft_transfer_call`
- `settle_assertion(assertion_id)` - Settle an undisputed assertion after expiry
- `settle_and_get_assertion_result(assertion_id)` - Settle and return the result

## Useful Links

- [cargo-near](https://github.com/near/cargo-near) - NEAR smart contract development toolkit for Rust
- [near CLI](https://near.cli.rs) - Interact with NEAR blockchain from command line
- [NEAR Rust SDK Documentation](https://docs.near.org/sdk/rust/introduction)
- [NEAR Documentation](https://docs.near.org)
- [NEAR StackOverflow](https://stackoverflow.com/questions/tagged/nearprotocol)
- [NEAR Discord](https://near.chat)
- [NEAR Telegram Developers Community Group](https://t.me/neardev)
- NEAR DevHub: [Telegram](https://t.me/neardevhub), [Twitter](https://twitter.com/neardevhub)
