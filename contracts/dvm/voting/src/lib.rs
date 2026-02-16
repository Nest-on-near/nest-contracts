use near_sdk::json_types::U128;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::store::LookupMap;
use near_sdk::{
    env, near, require, AccountId, CryptoHash, Gas, NearToken, PanicOnDefault, Promise,
};

use oracle_types::events::VotingEvent;

/// Voting phases for commit-reveal mechanism
#[near(serializers = [json, borsh])]
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum VotingPhase {
    /// Voters submit encrypted vote commitments
    Commit,
    /// Voters reveal their votes
    Reveal,
    /// Vote has been resolved
    Resolved,
}

/// Status of a price request
#[near(serializers = [json, borsh])]
#[derive(Clone, PartialEq, Debug)]
pub enum RequestStatus {
    /// Request is pending, vote not yet started
    Pending,
    /// Vote is active (commit or reveal phase)
    Active,
    /// Vote has been resolved with a result
    Resolved,
}

/// Outcome of attempting to resolve a request.
#[near(serializers = [json, borsh])]
#[derive(Clone, PartialEq, Debug)]
pub enum ResolvePriceOutcome {
    /// Price successfully resolved for the request.
    Resolved { price: i128 },
    /// Participation was too low, reveal phase was extended.
    RevealExtended,
    /// Participation remained too low and manual emergency resolution is required.
    EmergencyRequired,
}

/// A price request that needs to be resolved by voting
#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct PriceRequest {
    /// Unique identifier for this request
    pub identifier: String,
    /// Timestamp when the price is needed
    pub timestamp: u64,
    /// Additional data for the request (e.g., ancillary data)
    pub ancillary_data: Vec<u8>,
    /// Account that requested the price
    pub requester: AccountId,
    /// Current status of the request
    pub status: RequestStatus,
    /// Current voting phase
    pub phase: VotingPhase,
    /// When the commit phase started (nanoseconds)
    pub commit_start_time: u64,
    /// When the reveal phase started (nanoseconds)
    pub reveal_start_time: u64,
    /// Resolved price (if resolved)
    pub resolved_price: Option<i128>,
    /// Revealed stake observed for this request
    pub revealed_stake: u128,
    /// Number of automatic reveal extensions due to low participation
    pub low_participation_extensions: u8,
    /// Whether this request is blocked pending emergency resolution
    pub emergency_required: bool,
}

/// A voter's commitment for a specific request
#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct VoteCommitment {
    /// Hash of (price, salt)
    pub commit_hash: CryptoHash,
    /// The voter's staked amount at time of commitment
    pub staked_amount: u128,
    /// Whether the vote has been revealed
    pub revealed: bool,
    /// The revealed price (only set after reveal)
    pub revealed_price: Option<i128>,
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
#[serde(tag = "action")]
pub enum FtOnTransferMsg {
    CommitVote {
        request_id: CryptoHash,
        commit_hash: CryptoHash,
    },
}

/// Voting - DVM commit-reveal voting contract for dispute resolution.
///
/// This contract implements a commit-reveal voting mechanism where:
/// 1. A price request is created (from escalated disputes)
/// 2. Voters commit encrypted votes during the commit phase
/// 3. Voters reveal their votes during the reveal phase
/// 4. The median vote (weighted by stake) determines the result
/// 5. Wrong voters can be slashed
///
/// Key features:
/// - Two-phase commit-reveal to prevent vote copying
/// - Stake-weighted voting
/// - Configurable phase durations
#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Voting {
    /// Contract owner
    owner: AccountId,

    /// Duration of commit phase in nanoseconds
    commit_phase_duration: u64,

    /// Duration of reveal phase in nanoseconds
    reveal_phase_duration: u64,

    /// Minimum participation required (basis points, e.g., 500 = 5%)
    min_participation_rate: u64,

    /// Price requests by request_id (hash of identifier + timestamp + ancillary_data)
    requests: LookupMap<CryptoHash, PriceRequest>,

    /// Vote commitments: request_id -> voter -> commitment
    commitments: LookupMap<CryptoHash, LookupMap<AccountId, VoteCommitment>>,

    /// Total committed stake per request
    total_committed_stake: LookupMap<CryptoHash, u128>,

    /// Ordered list of voters per request (required for deterministic resolution)
    request_voters: LookupMap<CryptoHash, Vec<AccountId>>,

    /// Voting token (NEP-141) used for stake locking
    voting_token: Option<AccountId>,

    /// Treasury account that receives slashed stake share
    treasury: Option<AccountId>,

    /// Portion of slashed stake routed to treasury (bps)
    slashing_treasury_bps: u16,

    /// Maximum automatic reveal extensions before emergency path
    max_low_participation_extensions: u8,

    /// Next request nonce for generating unique IDs
    request_nonce: u64,
}

/// Default phase durations
const DEFAULT_COMMIT_DURATION: u64 = 24 * 60 * 60 * 1_000_000_000; // 24 hours in nanoseconds
const DEFAULT_REVEAL_DURATION: u64 = 24 * 60 * 60 * 1_000_000_000; // 24 hours in nanoseconds
const BASIS_POINTS_DENOMINATOR: u64 = 10_000;
const GAS_FOR_FT_TRANSFER: Gas = Gas::from_tgas(10);

#[near]
impl Voting {
    /// Initialize the Voting contract.
    ///
    /// # Arguments
    /// * `owner` - Account that can configure voting parameters
    #[init]
    pub fn new(owner: AccountId) -> Self {
        Self {
            owner,
            commit_phase_duration: DEFAULT_COMMIT_DURATION,
            reveal_phase_duration: DEFAULT_REVEAL_DURATION,
            min_participation_rate: 500, // 5% default
            requests: LookupMap::new(b"r"),
            commitments: LookupMap::new(b"c"),
            total_committed_stake: LookupMap::new(b"s"),
            request_voters: LookupMap::new(b"v"),
            voting_token: None,
            treasury: None,
            slashing_treasury_bps: 5_000, // 50%
            max_low_participation_extensions: 1,
            request_nonce: 0,
        }
    }

    // ==================== Price Request Management ====================

    /// Request a price vote for a disputed assertion.
    /// This would typically be called by the OptimisticOracle when a dispute escalates to DVM.
    ///
    /// # Arguments
    /// * `identifier` - The price identifier (e.g., "YES_OR_NO_QUERY")
    /// * `timestamp` - The timestamp for the price
    /// * `ancillary_data` - Additional data (e.g., the assertion claim)
    ///
    /// # Returns
    /// The request_id for tracking this vote
    pub fn request_price(
        &mut self,
        identifier: String,
        timestamp: u64,
        ancillary_data: Vec<u8>,
    ) -> CryptoHash {
        let requester = env::predecessor_account_id();

        // Generate request ID
        let request_id = self.generate_request_id(&identifier, timestamp, &ancillary_data);

        // Ensure request doesn't already exist
        require!(
            self.requests.get(&request_id).is_none(),
            "Price request already exists"
        );

        let request = PriceRequest {
            identifier: identifier.clone(),
            timestamp,
            ancillary_data: ancillary_data.clone(),
            requester: requester.clone(),
            status: RequestStatus::Active,
            phase: VotingPhase::Commit,
            commit_start_time: env::block_timestamp(),
            reveal_start_time: 0,
            resolved_price: None,
            revealed_stake: 0,
            low_participation_extensions: 0,
            emergency_required: false,
        };

        self.requests.insert(request_id, request);

        // Initialize commitments map for this request
        self.commitments
            .insert(request_id, LookupMap::new(request_id.as_ref()));
        self.total_committed_stake.insert(request_id, 0);
        self.request_voters.insert(request_id, Vec::new());

        self.request_nonce += 1;

        VotingEvent::PriceRequested {
            request_id: &request_id,
            identifier: &identifier,
            timestamp,
            ancillary_data: &ancillary_data,
            requester: &requester,
        }
        .emit();

        request_id
    }

    /// Commit a vote for a price request.
    /// The vote is encrypted as hash(price, salt).
    ///
    /// # Arguments
    /// * `request_id` - The price request ID
    /// * `commit_hash` - Hash of (price, salt)
    /// * `staked_amount` - Amount of voting tokens staked for this vote
    pub fn commit_vote(
        &mut self,
        _request_id: CryptoHash,
        _commit_hash: CryptoHash,
        _staked_amount: U128,
    ) {
        env::panic_str(
            "Direct commit disabled. Use ft_transfer_call on voting token with CommitVote action.",
        );
    }

    pub fn ft_on_transfer(&mut self, sender_id: AccountId, amount: U128, msg: String) -> U128 {
        let token = env::predecessor_account_id();
        require!(
            self.voting_token.as_ref() == Some(&token),
            "Only voting token can call ft_on_transfer"
        );
        require!(amount.0 > 0, "Stake amount must be positive");

        let parsed: FtOnTransferMsg =
            near_sdk::serde_json::from_str(&msg).expect("Invalid ft_on_transfer message format");

        match parsed {
            FtOnTransferMsg::CommitVote {
                request_id,
                commit_hash,
            } => {
                self.internal_commit_vote(request_id, sender_id, commit_hash, amount.0);
            }
        }

        U128(0)
    }

    fn internal_commit_vote(
        &mut self,
        request_id: CryptoHash,
        voter: AccountId,
        commit_hash: CryptoHash,
        staked_amount: u128,
    ) {
        let request = self.requests.get(&request_id).expect("Request not found");
        require!(request.phase == VotingPhase::Commit, "Not in commit phase");

        // Check commit phase hasn't expired
        let now = env::block_timestamp();
        require!(
            now < request.commit_start_time + self.commit_phase_duration,
            "Commit phase has ended"
        );

        // Get or create commitments map for this request
        let commitments = self
            .commitments
            .get_mut(&request_id)
            .expect("Commitments not initialized");

        // Check voter hasn't already committed
        require!(
            commitments.get(&voter).is_none(),
            "Already committed a vote"
        );

        let commitment = VoteCommitment {
            commit_hash,
            staked_amount,
            revealed: false,
            revealed_price: None,
        };

        commitments.insert(voter.clone(), commitment);
        let voters = self
            .request_voters
            .get_mut(&request_id)
            .expect("Voter list not initialized");
        voters.push(voter.clone());

        // Update total stake
        let total = self
            .total_committed_stake
            .get(&request_id)
            .copied()
            .unwrap_or(0);
        self.total_committed_stake
            .insert(request_id, total + staked_amount);

        VotingEvent::VoteCommitted {
            request_id: &request_id,
            voter: &voter,
            stake: &U128(staked_amount),
        }
        .emit();
    }

    /// Advance a request from commit phase to reveal phase.
    /// Can be called by anyone after commit phase duration has passed.
    ///
    /// # Arguments
    /// * `request_id` - The price request ID
    pub fn advance_to_reveal(&mut self, request_id: CryptoHash) {
        let mut request = self
            .requests
            .get(&request_id)
            .expect("Request not found")
            .clone();

        require!(request.phase == VotingPhase::Commit, "Not in commit phase");

        let now = env::block_timestamp();
        require!(
            now >= request.commit_start_time + self.commit_phase_duration,
            "Commit phase not yet ended"
        );

        request.phase = VotingPhase::Reveal;
        request.reveal_start_time = now;
        self.requests.insert(request_id, request);

        VotingEvent::RevealPhaseStarted {
            request_id: &request_id,
            reveal_start_time: now,
        }
        .emit();
    }

    /// Reveal a previously committed vote.
    ///
    /// # Arguments
    /// * `request_id` - The price request ID
    /// * `price` - The actual price voted for
    /// * `salt` - The salt used in the commitment
    pub fn reveal_vote(&mut self, request_id: CryptoHash, price: i128, salt: CryptoHash) {
        let voter = env::predecessor_account_id();

        // Verify request exists and is in reveal phase
        let request = self.requests.get(&request_id).expect("Request not found");
        require!(request.phase == VotingPhase::Reveal, "Not in reveal phase");

        // Check reveal phase hasn't expired
        let now = env::block_timestamp();
        require!(
            now < request.reveal_start_time + self.reveal_phase_duration,
            "Reveal phase has ended"
        );

        // Compute the expected hash first (before borrowing commitments mutably)
        let computed_hash = Self::compute_vote_hash_static(price, salt);

        // Get commitment
        let commitments = self
            .commitments
            .get_mut(&request_id)
            .expect("Commitments not initialized");

        let mut commitment = commitments
            .get(&voter)
            .expect("No commitment found")
            .clone();

        require!(!commitment.revealed, "Already revealed");

        // Verify the commitment hash
        require!(
            computed_hash == commitment.commit_hash,
            "Hash doesn't match commitment"
        );

        commitment.revealed = true;
        commitment.revealed_price = Some(price);
        let stake = U128(commitment.staked_amount);
        commitments.insert(voter.clone(), commitment);
        let mut mutable_request = request.clone();
        mutable_request.revealed_stake = mutable_request.revealed_stake.saturating_add(stake.0);
        self.requests.insert(request_id, mutable_request);

        VotingEvent::VoteRevealed {
            request_id: &request_id,
            voter: &voter,
            price,
            stake: &stake,
        }
        .emit();
    }

    /// Resolve a price request after reveal phase ends.
    /// Calculates the stake-weighted median of revealed votes.
    ///
    /// # Arguments
    /// * `request_id` - The price request ID
    ///
    /// # Returns
    /// Outcome describing whether the request resolved or needs additional action.
    pub fn resolve_price(&mut self, request_id: CryptoHash) -> ResolvePriceOutcome {
        let mut request = self
            .requests
            .get(&request_id)
            .expect("Request not found")
            .clone();

        require!(request.phase == VotingPhase::Reveal, "Not in reveal phase");

        let now = env::block_timestamp();
        require!(
            now >= request.reveal_start_time + self.reveal_phase_duration,
            "Reveal phase not yet ended"
        );

        let total_committed = self
            .total_committed_stake
            .get(&request_id)
            .copied()
            .unwrap_or(0);
        require!(total_committed > 0, "No committed stake");

        let required_participation = total_committed
            .saturating_mul(self.min_participation_rate as u128)
            / BASIS_POINTS_DENOMINATOR as u128;

        if request.revealed_stake < required_participation {
            let committed_u128 = U128(total_committed);
            let revealed_u128 = U128(request.revealed_stake);
            let required_u128 = U128(required_participation);
            if request.low_participation_extensions < self.max_low_participation_extensions {
                request.low_participation_extensions += 1;
                request.reveal_start_time = now;
                self.requests.insert(request_id, request);
                VotingEvent::LowParticipationTriggered {
                    request_id: &request_id,
                    committed_stake: &committed_u128,
                    revealed_stake: &revealed_u128,
                    required_stake: &required_u128,
                    emergency_required: false,
                }
                .emit();
                return ResolvePriceOutcome::RevealExtended;
            }
            request.emergency_required = true;
            self.requests.insert(request_id, request);
            VotingEvent::LowParticipationTriggered {
                request_id: &request_id,
                committed_stake: &committed_u128,
                revealed_stake: &revealed_u128,
                required_stake: &required_u128,
                emergency_required: true,
            }
            .emit();
            return ResolvePriceOutcome::EmergencyRequired;
        }

        let commitments = self
            .commitments
            .get(&request_id)
            .expect("Commitments not initialized");
        let voters = self
            .request_voters
            .get(&request_id)
            .expect("Voter list not initialized")
            .clone();

        let mut revealed_votes: Vec<(i128, u128, AccountId)> = Vec::new();
        for voter in voters {
            if let Some(commitment) = commitments.get(&voter) {
                if commitment.revealed {
                    if let Some(price) = commitment.revealed_price {
                        revealed_votes.push((price, commitment.staked_amount, voter.clone()));
                    }
                }
            }
        }

        require!(!revealed_votes.is_empty(), "No revealed votes");
        let resolved_price = Self::stake_weighted_median(&mut revealed_votes);
        self.distribute_rewards_and_slashing(&request_id, resolved_price, &revealed_votes);

        request.phase = VotingPhase::Resolved;
        request.status = RequestStatus::Resolved;
        request.resolved_price = Some(resolved_price);
        request.emergency_required = false;
        self.requests.insert(request_id, request);

        let total_stake = self.get_total_committed_stake(request_id);
        VotingEvent::PriceResolved {
            request_id: &request_id,
            resolved_price,
            total_stake: &total_stake,
        }
        .emit();

        ResolvePriceOutcome::Resolved {
            price: resolved_price,
        }
    }

    // ==================== View Functions ====================

    /// Get a price request by ID.
    pub fn get_request(&self, request_id: CryptoHash) -> Option<PriceRequest> {
        self.requests.get(&request_id).cloned()
    }

    /// Get the resolved price for a request.
    pub fn get_price(&self, request_id: CryptoHash) -> Option<i128> {
        self.requests
            .get(&request_id)
            .and_then(|r| r.resolved_price)
    }

    /// Check if a price has been resolved.
    pub fn has_price(&self, request_id: CryptoHash) -> bool {
        self.requests
            .get(&request_id)
            .map(|r| r.status == RequestStatus::Resolved)
            .unwrap_or(false)
    }

    /// Get the current phase for a request.
    pub fn get_phase(&self, request_id: CryptoHash) -> Option<VotingPhase> {
        self.requests.get(&request_id).map(|r| r.phase)
    }

    /// Get total committed stake for a request.
    pub fn get_total_committed_stake(&self, request_id: CryptoHash) -> U128 {
        U128(
            self.total_committed_stake
                .get(&request_id)
                .copied()
                .unwrap_or(0),
        )
    }

    // ==================== Configuration ====================

    /// Set the commit phase duration.
    /// Only owner can call.
    pub fn set_commit_phase_duration(&mut self, duration_ns: u64) {
        self.assert_owner();
        self.commit_phase_duration = duration_ns;
    }

    /// Set the reveal phase duration.
    /// Only owner can call.
    pub fn set_reveal_phase_duration(&mut self, duration_ns: u64) {
        self.assert_owner();
        self.reveal_phase_duration = duration_ns;
    }

    /// Set minimum participation rate.
    /// Only owner can call.
    pub fn set_min_participation_rate(&mut self, rate_bps: u64) {
        self.assert_owner();
        require!(
            rate_bps <= BASIS_POINTS_DENOMINATOR,
            "Rate cannot exceed 100%"
        );
        self.min_participation_rate = rate_bps;
    }

    /// Get current configuration.
    pub fn get_config(&self) -> (u64, u64, u64) {
        (
            self.commit_phase_duration,
            self.reveal_phase_duration,
            self.min_participation_rate,
        )
    }

    pub fn set_voting_token(&mut self, voting_token: AccountId) {
        self.assert_owner();
        self.voting_token = Some(voting_token);
    }

    pub fn set_treasury(&mut self, treasury: AccountId) {
        self.assert_owner();
        self.treasury = Some(treasury);
    }

    pub fn set_slashing_treasury_bps(&mut self, bps: u16) {
        self.assert_owner();
        require!(
            bps <= BASIS_POINTS_DENOMINATOR as u16,
            "BPS cannot exceed 100%"
        );
        self.slashing_treasury_bps = bps;
    }

    pub fn set_max_low_participation_extensions(&mut self, max_extensions: u8) {
        self.assert_owner();
        self.max_low_participation_extensions = max_extensions;
    }

    pub fn emergency_resolve_price(
        &mut self,
        request_id: CryptoHash,
        resolved_price: i128,
        reason: String,
    ) -> i128 {
        self.assert_owner();
        let mut request = self
            .requests
            .get(&request_id)
            .expect("Request not found")
            .clone();
        require!(
            request.phase == VotingPhase::Reveal,
            "Emergency resolve only from reveal phase"
        );
        require!(
            request.emergency_required,
            "Emergency resolution not enabled for this request"
        );

        request.phase = VotingPhase::Resolved;
        request.status = RequestStatus::Resolved;
        request.resolved_price = Some(resolved_price);
        request.emergency_required = false;
        self.requests.insert(request_id, request);

        env::log_str(&format!(
            "EMERGENCY_RESOLUTION request_id={} resolved_price={} reason={}",
            hex::encode(request_id),
            resolved_price,
            reason
        ));

        VotingEvent::PriceResolved {
            request_id: &request_id,
            resolved_price,
            total_stake: &self.get_total_committed_stake(request_id),
        }
        .emit();
        VotingEvent::EmergencyPriceResolved {
            request_id: &request_id,
            resolved_price,
            reason: &reason,
        }
        .emit();

        resolved_price
    }

    // ==================== Role Management ====================

    /// Transfer ownership.
    pub fn set_owner(&mut self, new_owner: AccountId) {
        self.assert_owner();
        self.owner = new_owner;
    }

    /// Get current owner.
    pub fn get_owner(&self) -> AccountId {
        self.owner.clone()
    }

    // ==================== Internal ====================

    fn assert_owner(&self) {
        require!(
            env::predecessor_account_id() == self.owner,
            "Only owner can call this method"
        );
    }

    /// Generate a unique request ID from the request parameters.
    fn generate_request_id(
        &self,
        identifier: &str,
        timestamp: u64,
        ancillary_data: &[u8],
    ) -> CryptoHash {
        let mut data = Vec::new();
        data.extend_from_slice(identifier.as_bytes());
        data.extend_from_slice(&timestamp.to_le_bytes());
        data.extend_from_slice(ancillary_data);
        data.extend_from_slice(&self.request_nonce.to_le_bytes());
        env::sha256(&data)
            .try_into()
            .expect("Hash should be 32 bytes")
    }

    /// Compute vote hash for commitment verification.
    #[allow(dead_code)]
    fn compute_vote_hash(&self, price: i128, salt: CryptoHash) -> CryptoHash {
        Self::compute_vote_hash_static(price, salt)
    }

    /// Static version of compute_vote_hash to avoid borrow issues.
    fn compute_vote_hash_static(price: i128, salt: CryptoHash) -> CryptoHash {
        let mut data = Vec::new();
        data.extend_from_slice(&price.to_le_bytes());
        data.extend_from_slice(&salt);
        env::sha256(&data)
            .try_into()
            .expect("Hash should be 32 bytes")
    }

    fn stake_weighted_median(votes: &mut [(i128, u128, AccountId)]) -> i128 {
        votes.sort_by(|a, b| a.0.cmp(&b.0));
        let total: u128 = votes.iter().map(|(_, stake, _)| *stake).sum();
        let midpoint = total / 2 + total % 2;
        let mut running = 0u128;
        for (price, stake, _) in votes.iter() {
            running = running.saturating_add(*stake);
            if running >= midpoint {
                return *price;
            }
        }
        votes.last().map(|(price, _, _)| *price).unwrap_or(0)
    }

    fn distribute_rewards_and_slashing(
        &self,
        request_id: &CryptoHash,
        resolved_price: i128,
        revealed_votes: &[(i128, u128, AccountId)],
    ) {
        let Some(voting_token) = self.voting_token.clone() else {
            return;
        };
        let Some(treasury) = self.treasury.clone() else {
            return;
        };

        let commitments = self
            .commitments
            .get(request_id)
            .expect("Commitments not initialized");
        let voters = self
            .request_voters
            .get(request_id)
            .expect("Voter list not initialized")
            .clone();

        let winner_stake: u128 = revealed_votes
            .iter()
            .filter(|(price, _, _)| *price == resolved_price)
            .map(|(_, stake, _)| *stake)
            .sum();
        let mut total_slashed = 0u128;
        for voter in &voters {
            if let Some(commitment) = commitments.get(voter) {
                let is_winner =
                    commitment.revealed && commitment.revealed_price == Some(resolved_price);
                if !is_winner {
                    total_slashed = total_slashed.saturating_add(commitment.staked_amount);
                }
            }
        }
        if total_slashed > 0 {
            let treasury_cut = total_slashed.saturating_mul(self.slashing_treasury_bps as u128)
                / BASIS_POINTS_DENOMINATOR as u128;
            let reward_pool = total_slashed.saturating_sub(treasury_cut);
            self.transfer_ft(voting_token.clone(), treasury, treasury_cut);

            for (price, stake, voter) in revealed_votes {
                if *price == resolved_price {
                    let reward = if winner_stake > 0 {
                        reward_pool.saturating_mul(*stake) / winner_stake
                    } else {
                        0
                    };
                    self.transfer_ft(
                        voting_token.clone(),
                        voter.clone(),
                        stake.saturating_add(reward),
                    );
                }
            }
        } else {
            for (price, stake, voter) in revealed_votes {
                if *price == resolved_price {
                    self.transfer_ft(voting_token.clone(), voter.clone(), *stake);
                }
            }
        }
    }

    fn transfer_ft(&self, token: AccountId, receiver_id: AccountId, amount: u128) {
        if amount == 0 {
            return;
        }
        let _ = Promise::new(token).function_call(
            "ft_transfer".to_string(),
            near_sdk::serde_json::json!({
                "receiver_id": receiver_id,
                "amount": U128(amount),
            })
            .to_string()
            .into_bytes(),
            NearToken::from_yoctonear(1),
            GAS_FOR_FT_TRANSFER,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::testing_env;
    const TOKEN_ACCOUNT: &str = "token.testnet";
    const TREASURY_ACCOUNT: &str = "treasury.testnet";

    fn account(id: &str) -> AccountId {
        id.parse().unwrap()
    }

    fn get_context(predecessor: AccountId, block_timestamp: u64) -> VMContextBuilder {
        let mut builder = VMContextBuilder::new();
        builder
            .predecessor_account_id(predecessor)
            .block_timestamp(block_timestamp);
        builder
    }

    fn setup_contract() -> Voting {
        let mut contract = Voting::new(accounts(0));
        contract.set_voting_token(account(TOKEN_ACCOUNT));
        contract.set_treasury(account(TREASURY_ACCOUNT));
        contract
    }

    #[test]
    fn test_new() {
        let context = get_context(accounts(0), 0);
        testing_env!(context.build());

        let contract = Voting::new(accounts(0));
        assert_eq!(contract.get_owner(), accounts(0));

        let (commit_dur, reveal_dur, min_part) = contract.get_config();
        assert_eq!(commit_dur, DEFAULT_COMMIT_DURATION);
        assert_eq!(reveal_dur, DEFAULT_REVEAL_DURATION);
        assert_eq!(min_part, 500);
    }

    #[test]
    fn test_request_price() {
        let context = get_context(accounts(0), 0);
        testing_env!(context.build());

        let mut contract = Voting::new(accounts(0));

        let request_id =
            contract.request_price("YES_OR_NO_QUERY".to_string(), 1000, b"test claim".to_vec());

        let request = contract.get_request(request_id).unwrap();
        assert_eq!(request.identifier, "YES_OR_NO_QUERY");
        assert_eq!(request.timestamp, 1000);
        assert_eq!(request.status, RequestStatus::Active);
        assert_eq!(request.phase, VotingPhase::Commit);
        assert_eq!(request.revealed_stake, 0);
    }

    #[test]
    fn test_multiple_requests_same_params() {
        // Each request gets a unique nonce, so same parameters can create
        // multiple price requests (this is valid behavior)
        let context = get_context(accounts(0), 0);
        testing_env!(context.build());

        let mut contract = Voting::new(accounts(0));

        let request_id_1 =
            contract.request_price("YES_OR_NO_QUERY".to_string(), 1000, b"test".to_vec());
        let request_id_2 =
            contract.request_price("YES_OR_NO_QUERY".to_string(), 1000, b"test".to_vec());

        // They should have different IDs
        assert_ne!(request_id_1, request_id_2);
    }

    #[test]
    fn test_commit_vote_via_ft_transfer_call() {
        let context = get_context(accounts(0), 0);
        testing_env!(context.build());

        let mut contract = setup_contract();

        let request_id =
            contract.request_price("YES_OR_NO_QUERY".to_string(), 1000, b"test".to_vec());

        let salt = [7u8; 32];
        let commit_hash = Voting::compute_vote_hash_static(1_000, salt);
        testing_env!(get_context(account(TOKEN_ACCOUNT), 1).build());
        let msg = near_sdk::serde_json::to_string(&FtOnTransferMsg::CommitVote {
            request_id,
            commit_hash,
        })
        .unwrap();
        contract.ft_on_transfer(accounts(1), U128(1_000), msg);

        assert_eq!(contract.get_total_committed_stake(request_id).0, 1000);
    }

    #[test]
    #[should_panic(expected = "Only voting token can call ft_on_transfer")]
    fn test_commit_wrong_token_rejected() {
        let context = get_context(accounts(0), 0);
        testing_env!(context.build());

        let mut contract = setup_contract();

        let request_id =
            contract.request_price("YES_OR_NO_QUERY".to_string(), 1000, b"test".to_vec());

        let salt = [9u8; 32];
        let commit_hash = Voting::compute_vote_hash_static(1_000, salt);
        testing_env!(get_context(accounts(1), 1).build());
        let msg = near_sdk::serde_json::to_string(&FtOnTransferMsg::CommitVote {
            request_id,
            commit_hash,
        })
        .unwrap();
        contract.ft_on_transfer(accounts(1), U128(1_000), msg);
    }

    #[test]
    fn test_advance_to_reveal() {
        let mut context = get_context(accounts(0), 0);
        testing_env!(context.build());

        let mut contract = Voting::new(accounts(0));

        let request_id =
            contract.request_price("YES_OR_NO_QUERY".to_string(), 1000, b"test".to_vec());

        // Fast forward past commit phase
        context.block_timestamp(DEFAULT_COMMIT_DURATION + 1);
        testing_env!(context.build());

        contract.advance_to_reveal(request_id);

        let phase = contract.get_phase(request_id).unwrap();
        assert_eq!(phase, VotingPhase::Reveal);
    }

    #[test]
    #[should_panic(expected = "Commit phase not yet ended")]
    fn test_advance_too_early() {
        let mut context = get_context(accounts(0), 0);
        testing_env!(context.build());

        let mut contract = Voting::new(accounts(0));

        let request_id =
            contract.request_price("YES_OR_NO_QUERY".to_string(), 1000, b"test".to_vec());

        // Try to advance before commit phase ends
        context.block_timestamp(1000);
        testing_env!(context.build());

        contract.advance_to_reveal(request_id);
    }

    #[test]
    fn test_set_config() {
        let context = get_context(accounts(0), 0);
        testing_env!(context.build());

        let mut contract = Voting::new(accounts(0));

        contract.set_commit_phase_duration(100);
        contract.set_reveal_phase_duration(200);
        contract.set_min_participation_rate(1000);

        let (commit_dur, reveal_dur, min_part) = contract.get_config();
        assert_eq!(commit_dur, 100);
        assert_eq!(reveal_dur, 200);
        assert_eq!(min_part, 1000);
    }

    #[test]
    #[should_panic(expected = "Only owner can call this method")]
    fn test_set_config_unauthorized() {
        let context = get_context(accounts(0), 0);
        testing_env!(context.build());

        let mut contract = Voting::new(accounts(0));

        testing_env!(get_context(accounts(1), 0).build());
        contract.set_commit_phase_duration(100);
    }

    #[test]
    fn test_transfer_ownership() {
        let context = get_context(accounts(0), 0);
        testing_env!(context.build());

        let mut contract = Voting::new(accounts(0));

        contract.set_owner(accounts(1));
        assert_eq!(contract.get_owner(), accounts(1));

        // New owner can set config
        testing_env!(get_context(accounts(1), 0).build());
        contract.set_commit_phase_duration(100);
    }

    #[test]
    fn test_has_price() {
        let context = get_context(accounts(0), 0);
        testing_env!(context.build());

        let mut contract = Voting::new(accounts(0));

        let request_id =
            contract.request_price("YES_OR_NO_QUERY".to_string(), 1000, b"test".to_vec());

        // Not resolved yet
        assert!(!contract.has_price(request_id));
    }

    #[test]
    fn test_resolve_price_weighted_median() {
        testing_env!(get_context(accounts(0), 0).build());
        let mut contract = setup_contract();
        contract.set_min_participation_rate(0);

        let request_id =
            contract.request_price("YES_OR_NO_QUERY".to_string(), 1000, b"test".to_vec());

        let v1_salt = [1u8; 32];
        let v2_salt = [2u8; 32];
        let v3_salt = [3u8; 32];
        let v1_hash = Voting::compute_vote_hash_static(0, v1_salt);
        let v2_hash = Voting::compute_vote_hash_static(1, v2_salt);
        let v3_hash = Voting::compute_vote_hash_static(1, v3_salt);

        testing_env!(get_context(account(TOKEN_ACCOUNT), 1).build());
        contract.ft_on_transfer(
            accounts(1),
            U128(100),
            near_sdk::serde_json::to_string(&FtOnTransferMsg::CommitVote {
                request_id,
                commit_hash: v1_hash,
            })
            .unwrap(),
        );
        testing_env!(get_context(account(TOKEN_ACCOUNT), 1).build());
        contract.ft_on_transfer(
            accounts(2),
            U128(400),
            near_sdk::serde_json::to_string(&FtOnTransferMsg::CommitVote {
                request_id,
                commit_hash: v2_hash,
            })
            .unwrap(),
        );
        testing_env!(get_context(account(TOKEN_ACCOUNT), 1).build());
        contract.ft_on_transfer(
            accounts(3),
            U128(500),
            near_sdk::serde_json::to_string(&FtOnTransferMsg::CommitVote {
                request_id,
                commit_hash: v3_hash,
            })
            .unwrap(),
        );

        testing_env!(get_context(accounts(0), DEFAULT_COMMIT_DURATION + 2).build());
        contract.advance_to_reveal(request_id);

        testing_env!(get_context(accounts(1), DEFAULT_COMMIT_DURATION + 3).build());
        contract.reveal_vote(request_id, 0, v1_salt);
        testing_env!(get_context(accounts(2), DEFAULT_COMMIT_DURATION + 4).build());
        contract.reveal_vote(request_id, 1, v2_salt);
        testing_env!(get_context(accounts(3), DEFAULT_COMMIT_DURATION + 5).build());
        contract.reveal_vote(request_id, 1, v3_salt);

        testing_env!(get_context(
            accounts(0),
            DEFAULT_COMMIT_DURATION + DEFAULT_REVEAL_DURATION + 10
        )
        .build());
        let outcome = contract.resolve_price(request_id);
        assert_eq!(outcome, ResolvePriceOutcome::Resolved { price: 1 });
        assert!(contract.has_price(request_id));
    }

    #[test]
    fn test_low_participation_requires_emergency() {
        testing_env!(get_context(accounts(0), 0).build());
        let mut contract = setup_contract();
        contract.set_min_participation_rate(9_000);
        contract.set_max_low_participation_extensions(0);

        let request_id =
            contract.request_price("YES_OR_NO_QUERY".to_string(), 1000, b"test".to_vec());
        let salt = [1u8; 32];
        let hash = Voting::compute_vote_hash_static(1, salt);

        testing_env!(get_context(account(TOKEN_ACCOUNT), 1).build());
        contract.ft_on_transfer(
            accounts(1),
            U128(100),
            near_sdk::serde_json::to_string(&FtOnTransferMsg::CommitVote {
                request_id,
                commit_hash: hash,
            })
            .unwrap(),
        );
        testing_env!(get_context(account(TOKEN_ACCOUNT), 1).build());
        contract.ft_on_transfer(
            accounts(2),
            U128(900),
            near_sdk::serde_json::to_string(&FtOnTransferMsg::CommitVote {
                request_id,
                commit_hash: Voting::compute_vote_hash_static(0, [2u8; 32]),
            })
            .unwrap(),
        );

        testing_env!(get_context(accounts(0), DEFAULT_COMMIT_DURATION + 2).build());
        contract.advance_to_reveal(request_id);
        testing_env!(get_context(accounts(1), DEFAULT_COMMIT_DURATION + 3).build());
        contract.reveal_vote(request_id, 1, salt);

        testing_env!(get_context(
            accounts(0),
            DEFAULT_COMMIT_DURATION + DEFAULT_REVEAL_DURATION + 10
        )
        .build());
        let outcome = contract.resolve_price(request_id);
        assert_eq!(outcome, ResolvePriceOutcome::EmergencyRequired);
        let req = contract.get_request(request_id).unwrap();
        assert!(req.emergency_required);

        testing_env!(get_context(
            accounts(0),
            DEFAULT_COMMIT_DURATION + DEFAULT_REVEAL_DURATION + 11
        )
        .build());
        let emergency =
            contract.emergency_resolve_price(request_id, 0, "Low participation".to_string());
        assert_eq!(emergency, 0);
        assert!(contract.has_price(request_id));
    }
}
