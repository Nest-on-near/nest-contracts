# voting

Nest DVM commit-reveal voting contract for disputed assertion resolution.

## Current Design

1. Oracle calls `request_price`.
2. Voters lock stake by calling `ft_transfer_call` on the configured voting token with:
   - `receiver_id = voting contract`
   - `amount = stake`
   - `msg = {"action":"CommitVote","request_id":..., "commit_hash":...}`
3. Anyone can advance to reveal with `advance_to_reveal` after commit duration.
4. Voters reveal with `reveal_vote(request_id, price, salt)`.
5. `resolve_price` computes stake-weighted median from revealed votes.

## Security / Policy

- Stake is locked in-contract until resolution.
- Incorrect or unrevealed votes are slashed at settlement.
- Slashed stake is split between treasury and winning voters (`slashing_treasury_bps`).
- Minimum participation is enforced (`min_participation_rate`).
- Low participation fallback:
  - automatic reveal extension up to `max_low_participation_extensions`
  - then emergency-only resolution path (`emergency_resolve_price`) by owner
- Emergency actions emit explicit audit events.

## How to Build Locally?

Install [`cargo-near`](https://github.com/near/cargo-near) and run:

```bash
cargo near build
```

## How to Test Locally?

```bash
cargo test
```

## How to Deploy?

Deployment is automated with GitHub Actions CI/CD pipeline.
To deploy manually, install [`cargo-near`](https://github.com/near/cargo-near) and run:

If you deploy for debugging purposes:
```bash
cargo near deploy build-non-reproducible-wasm <account-id>
```

If you deploy production ready smart contract:
```bash
cargo near deploy build-reproducible-wasm <account-id>
```

## Useful Links

- [cargo-near](https://github.com/near/cargo-near) - NEAR smart contract development toolkit for Rust
- [near CLI](https://near.cli.rs) - Interact with NEAR blockchain from command line
- [NEAR Rust SDK Documentation](https://docs.near.org/sdk/rust/introduction)
- [NEAR Documentation](https://docs.near.org)
- [NEAR StackOverflow](https://stackoverflow.com/questions/tagged/nearprotocol)
- [NEAR Discord](https://near.chat)
- [NEAR Telegram Developers Community Group](https://t.me/neardev)
- NEAR DevHub: [Telegram](https://t.me/neardevhub), [Twitter](https://twitter.com/neardevhub)
