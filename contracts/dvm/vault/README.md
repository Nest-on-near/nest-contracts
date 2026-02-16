# vault

Collateral vault for NEST governance token issuance.

## Design

- Accepts collateral via `ft_transfer_call` from a configured collateral token (mockNEAR/wNEAR-compatible).
- Mints NEST 1:1 to depositor after collateral arrives.
- Redeems collateral 1:1 by burning NEST and transferring collateral back.
- Tracks locked collateral and minted liability for backing diagnostics.
- Owner can pause/resume redemptions and update emergency receiver.

## Core Methods

- `ft_on_transfer(sender_id, amount, msg)`:
  - callable only by collateral token.
  - `msg = {"action":"DepositCollateral"}`.
- `redeem_collateral(amount)`:
  - burns caller NEST then transfers collateral back.
- `get_total_locked_collateral()`
- `get_total_minted_liability()`
- `get_backing_ratio_bps()`
- `get_invariant_diagnostics()`

## Build

```bash
cargo near build non-reproducible-wasm
```

## Test

```bash
cargo test
```
