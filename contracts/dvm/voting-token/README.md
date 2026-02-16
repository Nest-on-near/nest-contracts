# voting-token (NEST)

NEST governance/staking token (NEP-141) with protocol-safe issuance and transfer controls.

## Key Behavior

- Permissioned mint/burn via owner-managed roles.
- Dedicated `set_vault_account` helper grants vault mint+burn authority.
- Restricted transfer mode is enabled by default:
  - wallet-to-wallet transfers are blocked,
  - transfer succeeds only if sender or receiver is allowlisted (`add_transfer_router`).
- This keeps staking/reward routes working while preventing unrestricted governance token circulation.

## Core Methods

- `set_vault_account(vault_account: Option<AccountId>)`
- `add_transfer_router(account_id)` / `remove_transfer_router(account_id)`
- `set_transfer_restricted(restricted)`
- `mint(account_id, amount)` (requires minter + pre-registered receiver)
- `burn(amount)` / `burn_from(account_id, amount)` (requires burner)

## Required Wiring

- Set vault authority:
  - `set_vault_account("<vault-account>")`
- Allow voting payouts/staking routes:
  - `add_transfer_router("<voting-account>")`

## Build

```bash
cargo near build non-reproducible-wasm
```

## Test

```bash
cargo test -p voting-token
```
