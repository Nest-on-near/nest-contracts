# NEST Oracle Scripts

JavaScript scripts for testing the NEST Optimistic Oracle on NEAR testnet using `near-api-js`.

The Oracle now has **full DVM integration** - when assertions are disputed, they automatically escalate to the DVM voting contract for resolution.

## Setup

1. Install dependencies:
   ```bash
   cd scripts
   npm install
   ```

2. Configure your environment:
   ```bash
   cp .env.example .env
   ```

3. Edit `.env` with your NEAR testnet account details:
   - `NEAR_ACCOUNT_ID`: Your testnet account (e.g., `yourname.testnet`)
   - `NEAR_PRIVATE_KEY`: Your account's private key (ed25519:xxx...)

   Alternatively, if you've logged in with NEAR CLI, the script can read from `~/.near-credentials/testnet/`.

## Prerequisites

- NEAR testnet account with some NEAR for gas fees
- NEST tokens for bonding (get from faucet or mint if you're the owner)
- Node.js 18+

## Scripts

### test-market-dvm-flow.js

One-command E2E runner for the prediction-market dispute path:
1. Create market
2. Buy YES and NO
3. Submit resolution
4. Dispute at oracle
5. Commit + reveal DVM votes with stake (`ft_transfer_call`)
6. Resolve DVM price
7. Settle oracle assertion and verify market settlement

```bash
npm run test:market-dvm
```

Quick start:

```bash
cp .env.market-dvm.example .env
npm run test:market-dvm
```

Required env vars for this script:

```bash
NETWORK=testnet-fastnear
MARKET_CONTRACT=market5-260214.nest-creator-260214a.testnet
ORACLE_CONTRACT=oracle5-260214.nest-creator-260214a.testnet
VOTING_CONTRACT=nest-voting-3.testnet
OUTCOME_TOKEN_CONTRACT=outcome5-260214.nest-creator-260214a.testnet
USDC_CONTRACT=nusd-1.testnet
VOTING_TOKEN_CONTRACT=nest-token-1.testnet

CREATOR_ACCOUNT=...
TRADER_YES_ACCOUNT=...
TRADER_NO_ACCOUNT=...
DISPUTER_ACCOUNT=...
VOTER1_ACCOUNT=...
VOTER2_ACCOUNT=...
```

Optional toggles:

```bash
# speed up voting phases (owner-only on voting contract)
SET_FAST_VOTING=1
VOTING_OWNER_ACCOUNT=oracle5-260214.nest-creator-260214a.testnet
COMMIT_DURATION_NS=120000000000
REVEAL_DURATION_NS=120000000000

# also run undisputed branch (waits full oracle liveness from market flow)
RUN_UNDISPUTED_BRANCH=1
UNDISPUTED_SETTLER_ACCOUNT=...
```

### test-oracle-flow.js

Full end-to-end test that:
1. Makes an assertion with a bond
2. Disputes the assertion
3. Attempts to settle (requires owner privileges)

```bash
# Run disputed flow (default)
npm run test:oracle
# or
node test-oracle-flow.js dispute

# Run undisputed flow (just make assertion)
node test-oracle-flow.js undisputed
```

### view-assertion.js

View oracle status or a specific assertion's details.

```bash
# View oracle configuration
node view-assertion.js

# View specific assertion
node view-assertion.js '[1,2,3,4,...,32]'
```

### settle-assertion.js

Settle an undisputed assertion after liveness expires.

```bash
node settle-assertion.js '[1,2,3,4,...,32]'
```

## Test Flow

### Disputed Assertion Flow (with DVM)

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│  Asserter   │     │   Oracle    │     │  Disputer   │     │ DVM Voting  │
└──────┬──────┘     └──────┬──────┘     └──────┬──────┘     └──────┬──────┘
       │                   │                   │                   │
       │ ft_transfer_call  │                   │                   │
       │ (AssertTruth)     │                   │                   │
       │──────────────────>│                   │                   │
       │                   │                   │                   │
       │  AssertionMade    │                   │                   │
       │<──────────────────│                   │                   │
       │                   │                   │                   │
       │                   │ ft_transfer_call  │                   │
       │                   │ (DisputeAssertion)│                   │
       │                   │<──────────────────│                   │
       │                   │                   │                   │
       │                   │ request_price()   │                   │
       │                   │───────────────────│──────────────────>│
       │                   │                   │                   │
       │                   │ AssertionDisputed │  PriceRequested   │
       │                   │──────────────────>│<──────────────────│
       │                   │                   │                   │
       │                   │   [DVM Voting: Commit -> Reveal -> Resolve]
       │                   │                   │                   │
       │ settle_assertion  │                   │                   │
       │──────────────────>│                   │                   │
       │                   │                   │                   │
       │                   │ get_price()       │                   │
       │                   │───────────────────│──────────────────>│
       │                   │                   │                   │
       │                   │<──────────────────│───────────────────│
       │                   │   resolution      │                   │
       │                   │                   │                   │
       │ AssertionSettled  │                   │                   │
       │<──────────────────│──────────────────>│                   │
```

### Undisputed Assertion Flow

```
┌─────────────┐     ┌─────────────┐
│  Asserter   │     │   Oracle    │
└──────┬──────┘     └──────┬──────┘
       │                   │
       │ ft_transfer_call  │
       │ (AssertTruth)     │
       │──────────────────>│
       │                   │
       │  AssertionMade    │
       │<──────────────────│
       │                   │
       │ [Wait for liveness period]
       │                   │
       │ settle_assertion  │
       │──────────────────>│
       │                   │
       │ AssertionSettled  │
       │ (bond returned)   │
       │<──────────────────│
```

## Contract Addresses (Testnet)

| Contract | Address |
|----------|---------|
| Oracle | `oracle5-260214.nest-creator-260214a.testnet` |
| Token | `nest-token-1.testnet` |
| Voting | `nest-voting-3.testnet` |
| Market | `market5-260214.nest-creator-260214a.testnet` |
| Outcome Token | `outcome5-260214.nest-creator-260214a.testnet` |
| USDC | `nusd-1.testnet` |

## Notes

- The test script uses a 5-minute liveness period for faster testing
- When disputed, the Oracle **automatically escalates to DVM voting**
- Settlement via `settle_assertion` queries DVM for resolution
- Owner can still manually resolve via `resolve_disputed_assertion` as fallback
- Bond amounts are in the smallest token unit (1 token = 1e24 units for 24 decimals)

## DVM Voting Flow

For disputed assertions, the resolution goes through DVM voting:

1. **Dispute** - Oracle calls `voting.request_price()` automatically
2. **Commit Phase** (24h default) - Voters commit encrypted votes
3. **Reveal Phase** (24h default) - Voters reveal votes
4. **Resolution** - Anyone calls `voting.resolve_price()`
5. **Settlement** - Anyone calls `oracle.settle_assertion()` to distribute bonds
