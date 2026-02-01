# Nest - Optimistic Oracle for NEAR

An optimistic oracle implementation for NEAR Protocol, inspired by UMA's Optimistic Oracle V3.

## Architecture

```mermaid
flowchart TB
    subgraph Consumers
        PM[Prediction Market]
        DAPP[Other dApps]
    end

    subgraph Oracle Layer
        OO[Optimistic Oracle]
    end

    subgraph Escalation
        EM_BASE[Base Escalation Manager]
        EM_WL[Whitelist Disputer]
        EM_FULL[Full Policy Manager]
    end

    subgraph DVM[Data Verification Mechanism]
        VT[Voting Token]
        FINDER[Finder]
        STORE[Store]
        VOTING[Voting]
        ID_WL[Identifier Whitelist]
        REG[Registry]
        SLASH[Slashing Library]
    end

    PM --> OO
    DAPP --> OO
    OO -->|if disputed| EM_BASE
    EM_BASE --> EM_WL
    EM_BASE --> EM_FULL
    EM_WL --> VOTING
    EM_FULL --> VOTING
    VOTING --> VT
    VOTING --> SLASH
    FINDER --> STORE
    FINDER --> REG
    FINDER --> ID_WL
```

## Dispute Resolution Flow

```mermaid
sequenceDiagram
    participant A as Asserter
    participant O as Oracle
    participant D as Disputer
    participant E as Escalation Manager
    participant V as DVM Voting

    A->>O: Assert claim (with bond)
    O->>O: Start liveness period

    alt No dispute
        O->>A: Return bond + reward
    else Disputed
        D->>O: Dispute (with matching bond)
        O->>E: Escalate to DVM
        E->>V: Request price
        V->>V: Commit phase (24h)
        V->>V: Reveal phase (24h)
        V->>E: Return resolved price
        E->>O: Settlement callback
        O->>O: Distribute bonds to winner
    end
```

## Contracts

Each contract has its own README with detailed deployment commands.

### Core Oracle

| Contract | Description | Docs |
|----------|-------------|------|
| **Optimistic Oracle** | Main entry point. Accepts assertions with bonds, handles disputes and settlements. | [README](contracts/optimistic-oracle/README.md) |

### DVM (Data Verification Mechanism)

| Contract | Description | Docs |
|----------|-------------|------|
| **Voting Token** | NEP-141 token for governance voting. Minters/burners controlled by owner. | [README](contracts/dvm/voting-token/README.md) |
| **Finder** | Service discovery. Maps interface names to contract addresses. | [README](contracts/dvm/finder/README.md) |
| **Store** | Fee collection. Tracks final fees per currency. | [README](contracts/dvm/store/README.md) |
| **Identifier Whitelist** | Approved price identifiers for oracle requests. | [README](contracts/dvm/identifier-whitelist/README.md) |
| **Registry** | Authorized contracts that can interact with oracle. | [README](contracts/dvm/registry/README.md) |
| **Slashing Library** | Calculates penalties for incorrect votes. | [README](contracts/dvm/slashing-library/README.md) |
| **Voting** | Commit-reveal voting for dispute resolution. | [README](contracts/dvm/voting/README.md) |

### Escalation Managers

| Contract | Description | Docs |
|----------|-------------|------|
| **Base Escalation Manager** | Default implementation with permissive policies. | [README](contracts/escalation-manager/base/README.md) |
| **Whitelist Disputer** | Restricts disputes to whitelisted addresses. | [README](contracts/escalation-manager/whitelist-disputer/README.md) |
| **Full Policy Manager** | Configurable assertion/dispute policies with custom arbitration. | [README](contracts/escalation-manager/full-policy/README.md) |

### Examples

| Contract | Description |
|----------|-------------|
| **Basic Assertion** | Example integration with Optimistic Oracle. |

## Prerequisites

- [Rust](https://rustup.rs/)
- [cargo-near](https://github.com/near/cargo-near) - `cargo install cargo-near`
- [NEAR CLI](https://near.cli.rs) - `cargo install near-cli-rs`

## Building

```bash
# Build a single contract
cd contracts/optimistic-oracle && cargo near build non-reproducible-wasm

# Build all DVM contracts
for contract in voting-token finder store identifier-whitelist registry slashing-library voting; do
  (cd contracts/dvm/$contract && cargo near build non-reproducible-wasm)
done

# Build escalation managers (optional)
for contract in base whitelist-disputer full-policy; do
  (cd contracts/escalation-manager/$contract && cargo near build non-reproducible-wasm)
done

# Build optimistic oracle
(cd contracts/optimistic-oracle && cargo near build non-reproducible-wasm)
```

Output WASM files are in `target/near/<contract_name>/<contract_name>.wasm`

## Testing

```bash
# Run all unit tests
cargo test --workspace

# Run integration tests (requires built WASM files)
cargo test -p integration-tests

# Run a specific contract's tests
cargo test -p voting-token
cargo test -p optimistic-oracle
```

## Deployment Guide

### Current Testnet Deployment

The following contracts are currently deployed on NEAR testnet:

| Contract | Address |
|----------|---------|
| Voting Token | `nest-token-1.testnet` |
| Finder | `nest-finder-1.testnet` |
| Store | `nest-store-1.testnet` |
| Identifier Whitelist | `nest-identifiers-1.testnet` |
| Registry | `nest-registry-1.testnet` |
| Slashing Library | `nest-slashing-1.testnet` |
| Voting | `nest-voting-1.testnet` |
| Optimistic Oracle | `nest-oracle-3.testnet` |
| **Owner** | `nest-owner-1.testnet` |
| **Treasury** | `nest-treasury-1.testnet` |

### Quick Start (Testnet)

#### 1. Build all contracts

See [Building](#building) section above.

#### 2. Deploy in order

Deploy contracts in this order (each depends on previous ones):

| Order | Contract | Suggested Account | Docs |
|-------|----------|------------------|------|
| 1 | Voting Token | `nest-token.testnet` | [README](contracts/dvm/voting-token/README.md) |
| 2 | Finder | `nest-finder.testnet` | [README](contracts/dvm/finder/README.md) |
| 3 | Store | `nest-store.testnet` | [README](contracts/dvm/store/README.md) |
| 4 | Identifier Whitelist | `nest-identifiers.testnet` | [README](contracts/dvm/identifier-whitelist/README.md) |
| 5 | Registry | `nest-registry.testnet` | [README](contracts/dvm/registry/README.md) |
| 6 | Slashing Library | `nest-slashing.testnet` | [README](contracts/dvm/slashing-library/README.md) |
| 7 | Voting | `nest-voting.testnet` | [README](contracts/dvm/voting/README.md) |
| 8* | Base Escalation Manager | `nest-escalation-base.testnet` | [README](contracts/escalation-manager/base/README.md) |
| 9* | Whitelist Disputer | `nest-escalation-whitelist.testnet` | [README](contracts/escalation-manager/whitelist-disputer/README.md) |
| 10* | Full Policy Manager | `nest-escalation-full.testnet` | [README](contracts/escalation-manager/full-policy/README.md) |
| 11 | Optimistic Oracle | `nest-oracle-3.testnet` | [README](contracts/optimistic-oracle/README.md) |

**\*Optional:** Escalation managers are only needed if you want to customize assertion/dispute behavior.

#### 3. Post-deployment configuration

After deploying all contracts, complete the configuration steps in **[POST_DEPLOYMENT.md](POST_DEPLOYMENT.md)**.

**Quick summary:**
1. Add Voting as minter on VotingToken
2. Register all interfaces in Finder
3. Whitelist price identifiers (ASSERT_TRUTH, YES_OR_NO_QUERY)
4. Set final fees in Store
5. Whitelist currencies in Oracle
6. Register Oracle in Registry

See [POST_DEPLOYMENT.md](POST_DEPLOYMENT.md) for complete step-by-step commands with verification.

## Links

- [NEAR Rust SDK](https://docs.near.org/sdk/rust/introduction)
- [UMA Protocol](https://docs.uma.xyz/)
