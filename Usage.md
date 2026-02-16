# Nest Oracle Integration Guide (`Usage.md`)

This guide explains how any dApp can integrate with Nest Optimistic Oracle, similar to how `nest-markets` uses it.

## Who This Is For

- Teams building NEAR dApps that need verifiable outcome resolution.
- Protocols that want optimistic-by-default resolution with dispute fallback.
- Apps that need callbacks when an assertion is finalized.

## Integration Pattern

Most integrations follow this pattern:

1. Your app decides the claim and bond token/amount.
2. Your app submits an assertion by sending `ft_transfer_call` to the oracle (`AssertTruth` message).
3. Optional: your contract receives oracle callbacks to update app state.
4. If disputed, a disputer posts matching bond (`DisputeAssertion` message).
5. Anyone calls `settle_assertion` after liveness/dispute resolution.
6. Your app reads final state via `get_assertion` and/or callback output.

`nest-markets` uses this exact model in `contracts/market/src/resolution.rs`.

## Pre-Integration Checklist

- Oracle contract deployed and initialized.
- Bond token (NEP-141) is whitelisted in oracle (`whitelist_currency`).
- Final fee configured for that token.
- If your contract receives payouts or forwards tokens, required storage is registered on the bond token contract.
- If you use custom identifiers, they are approved (`whitelist_identifier`).

## Core Calls You Will Use

### 1) Create Assertion (`AssertTruth`)

Assertions are created by calling `ft_transfer_call` on the bond token contract with `receiver_id = <oracle>`.

```json
{
  "action": "AssertTruth",
  "claim": [/* 32-byte array */],
  "asserter": "alice.testnet",
  "callback_recipient": "your-app.testnet",
  "escalation_manager": null,
  "liveness_ns": "7200000000000",
  "assertion_time_ns": "1739400000000000000",
  "identifier": [/* optional 32-byte identifier */],
  "domain_id": [/* optional 32-byte domain id */],
  "assertion_id_override": [/* optional 32-byte id */]
}
```

Notes:

- `claim` is bytes32. A common pattern is `keccak256(claim_string)`.
- `asserter` is the economic owner of the assertion side.
- `callback_recipient` is optional but recommended for contract integrations.
- `assertion_time_ns` + `assertion_id_override` are useful for deterministic mapping (used in `nest-markets`).

### 2) Dispute Assertion (`DisputeAssertion`)

Disputes are also sent through `ft_transfer_call` on the same bond token, matching the bond amount.

```json
{
  "action": "DisputeAssertion",
  "assertion_id": [/* 32-byte array */],
  "disputer": "bob.testnet"
}
```

### 3) Settle Assertion

After liveness / dispute resolution, call:

- `settle_assertion(assertion_id)`
- if payout callback failed and assertion is pending, call `retry_settlement_payout(assertion_id)`

## Recommended Callback Interface (For Contract Integrations)

If your dApp contract needs push-based updates, implement:

```rust
pub fn assertion_resolved_callback(&mut self, assertion_id: String, asserted_truthfully: bool);
pub fn assertion_disputed_callback(&mut self, assertion_id: String);
```

Implementation tips:

- Enforce caller check: only accept callback from oracle account.
- Maintain your own mapping (e.g., `assertion_id -> domain object id`).
- On `assertion_resolved_callback`, finalize app state deterministically.
- Treat callbacks as state transitions, not UI hints.

See:

- `contracts/examples/basic-assertion` for minimal callback usage.
- `nest-markets/contracts/market/src/resolution.rs` for a production-style callback + mapping flow.

## Example: `nest-markets` Integration Pattern

`nest-markets` resolution module demonstrates a robust pattern:

1. Build deterministic claim (`keccak256("market:{id}:outcome:{yes/no}")`).
2. Compute/store assertion ID locally before submitting.
3. Forward bond to oracle with `AssertTruth` via `ft_transfer_call`.
4. Map `assertion_id -> market_id` in contract state.
5. On callback, move market state:
   - truthful -> `Settled`
   - false -> `Closed` (re-resolvable)
   - disputed callback -> `Disputed`

This pattern is reusable for lending liquidations, insurance outcomes, governance challenges, and any dApp that needs delayed truth finalization.

## Minimal Read Methods

Useful read methods from oracle:

- `get_assertion(assertion_id)`
- `get_assertion_result(assertion_id)`
- `get_dispute_request(assertion_id)`
- `get_minimum_bond(currency)`
- `is_currency_whitelisted(currency)`

## Integration Safety Checklist

- Validate callback caller (`predecessor == oracle`).
- Handle duplicate/replayed callback attempts safely.
- Keep idempotent state transitions where possible.
- Register storage for every account that may receive FT transfers.
- Guard against unresolved assertions in your app logic.

## References

- Core oracle contract docs: `contracts/optimistic-oracle/README.md`
- Minimal integration example: `contracts/examples/basic-assertion/README.md`
- Real integration example: `https://github.com/Nest-on-near/nest-markets`
