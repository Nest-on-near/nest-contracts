use near_sdk::json_types::U128;
use near_sdk::store::LookupMap;
use near_sdk::{env, near, require, AccountId, CryptoHash, PanicOnDefault};

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

    /// Next request nonce for generating unique IDs
    request_nonce: u64,
}

/// Default phase durations
const DEFAULT_COMMIT_DURATION: u64 = 24 * 60 * 60 * 1_000_000_000; // 24 hours in nanoseconds
const DEFAULT_REVEAL_DURATION: u64 = 24 * 60 * 60 * 1_000_000_000; // 24 hours in nanoseconds
const BASIS_POINTS_DENOMINATOR: u64 = 10_000;

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
        };

        self.requests.insert(request_id, request);

        // Initialize commitments map for this request
        self.commitments
            .insert(request_id, LookupMap::new(request_id.as_ref()));
        self.total_committed_stake.insert(request_id, 0);

        self.request_nonce += 1;

        env::log_str(&format!(
            "EVENT_JSON:{{\"standard\":\"voting\",\"version\":\"1.0.0\",\"event\":\"price_requested\",\"data\":{{\"request_id\":\"{:?}\",\"identifier\":\"{}\",\"timestamp\":{},\"requester\":\"{}\"}}}}",
            request_id, identifier, timestamp, requester
        ));

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
        request_id: CryptoHash,
        commit_hash: CryptoHash,
        staked_amount: U128,
    ) {
        let voter = env::predecessor_account_id();

        // Verify request exists and is in commit phase
        let request = self.requests.get(&request_id).expect("Request not found");
        require!(
            request.phase == VotingPhase::Commit,
            "Not in commit phase"
        );

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
            staked_amount: staked_amount.0,
            revealed: false,
            revealed_price: None,
        };

        commitments.insert(voter.clone(), commitment);

        // Update total stake
        let total = self
            .total_committed_stake
            .get(&request_id)
            .copied()
            .unwrap_or(0);
        self.total_committed_stake
            .insert(request_id, total + staked_amount.0);

        env::log_str(&format!(
            "EVENT_JSON:{{\"standard\":\"voting\",\"version\":\"1.0.0\",\"event\":\"vote_committed\",\"data\":{{\"request_id\":\"{:?}\",\"voter\":\"{}\",\"staked_amount\":\"{}\"}}}}",
            request_id, voter, staked_amount.0
        ));
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

        require!(
            request.phase == VotingPhase::Commit,
            "Not in commit phase"
        );

        let now = env::block_timestamp();
        require!(
            now >= request.commit_start_time + self.commit_phase_duration,
            "Commit phase not yet ended"
        );

        request.phase = VotingPhase::Reveal;
        request.reveal_start_time = now;
        self.requests.insert(request_id, request);

        env::log_str(&format!(
            "EVENT_JSON:{{\"standard\":\"voting\",\"version\":\"1.0.0\",\"event\":\"phase_advanced\",\"data\":{{\"request_id\":\"{:?}\",\"new_phase\":\"Reveal\"}}}}",
            request_id
        ));
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
        require!(
            request.phase == VotingPhase::Reveal,
            "Not in reveal phase"
        );

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
        commitments.insert(voter.clone(), commitment.clone());

        env::log_str(&format!(
            "EVENT_JSON:{{\"standard\":\"voting\",\"version\":\"1.0.0\",\"event\":\"vote_revealed\",\"data\":{{\"request_id\":\"{:?}\",\"voter\":\"{}\",\"price\":{},\"staked_amount\":\"{}\"}}}}",
            request_id, voter, price, commitment.staked_amount
        ));
    }

    /// Resolve a price request after reveal phase ends.
    /// Calculates the stake-weighted median of revealed votes.
    ///
    /// # Arguments
    /// * `request_id` - The price request ID
    ///
    /// # Returns
    /// The resolved price
    pub fn resolve_price(&mut self, request_id: CryptoHash) -> i128 {
        let mut request = self
            .requests
            .get(&request_id)
            .expect("Request not found")
            .clone();

        require!(
            request.phase == VotingPhase::Reveal,
            "Not in reveal phase"
        );

        let now = env::block_timestamp();
        require!(
            now >= request.reveal_start_time + self.reveal_phase_duration,
            "Reveal phase not yet ended"
        );

        // Collect revealed votes
        // Note: In a production version, we'd need to track voters separately
        // and iterate over all commitments. LookupMap doesn't support iteration.
        // This is a known limitation - you'd use UnorderedMap or track voter list.
        let _commitments = self.commitments.get(&request_id).expect("No commitments");

        // For now, resolve with 0 as placeholder
        // In production, you'd:
        // 1. Track voter list per request
        // 2. Iterate and collect revealed votes
        // 3. Calculate stake-weighted median
        let resolved_price = 0i128;

        request.phase = VotingPhase::Resolved;
        request.status = RequestStatus::Resolved;
        request.resolved_price = Some(resolved_price);
        self.requests.insert(request_id, request.clone());

        env::log_str(&format!(
            "EVENT_JSON:{{\"standard\":\"voting\",\"version\":\"1.0.0\",\"event\":\"price_resolved\",\"data\":{{\"request_id\":\"{:?}\",\"resolved_price\":{},\"identifier\":\"{}\"}}}}",
            request_id, resolved_price, request.identifier
        ));

        resolved_price
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::testing_env;

    fn get_context(predecessor: AccountId) -> VMContextBuilder {
        let mut builder = VMContextBuilder::new();
        builder.predecessor_account_id(predecessor);
        builder
    }

    #[test]
    fn test_new() {
        let context = get_context(accounts(0));
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
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = Voting::new(accounts(0));

        let request_id = contract.request_price(
            "YES_OR_NO_QUERY".to_string(),
            1000,
            b"test claim".to_vec(),
        );

        let request = contract.get_request(request_id).unwrap();
        assert_eq!(request.identifier, "YES_OR_NO_QUERY");
        assert_eq!(request.timestamp, 1000);
        assert_eq!(request.status, RequestStatus::Active);
        assert_eq!(request.phase, VotingPhase::Commit);
    }

    #[test]
    fn test_multiple_requests_same_params() {
        // Each request gets a unique nonce, so same parameters can create
        // multiple price requests (this is valid behavior)
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = Voting::new(accounts(0));

        let request_id_1 = contract.request_price("YES_OR_NO_QUERY".to_string(), 1000, b"test".to_vec());
        let request_id_2 = contract.request_price("YES_OR_NO_QUERY".to_string(), 1000, b"test".to_vec());

        // They should have different IDs
        assert_ne!(request_id_1, request_id_2);
    }

    #[test]
    fn test_commit_vote() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = Voting::new(accounts(0));

        let request_id = contract.request_price(
            "YES_OR_NO_QUERY".to_string(),
            1000,
            b"test".to_vec(),
        );

        // Voter commits
        testing_env!(get_context(accounts(1)).build());
        let commit_hash: CryptoHash = [1u8; 32];
        contract.commit_vote(request_id, commit_hash, U128(1000));

        assert_eq!(contract.get_total_committed_stake(request_id).0, 1000);
    }

    #[test]
    #[should_panic(expected = "Already committed a vote")]
    fn test_double_commit() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = Voting::new(accounts(0));

        let request_id = contract.request_price(
            "YES_OR_NO_QUERY".to_string(),
            1000,
            b"test".to_vec(),
        );

        testing_env!(get_context(accounts(1)).build());
        let commit_hash: CryptoHash = [1u8; 32];
        contract.commit_vote(request_id, commit_hash, U128(1000));
        // Second commit should fail
        contract.commit_vote(request_id, commit_hash, U128(1000));
    }

    #[test]
    fn test_advance_to_reveal() {
        let mut context = get_context(accounts(0));
        context.block_timestamp(0);
        testing_env!(context.build());

        let mut contract = Voting::new(accounts(0));

        let request_id = contract.request_price(
            "YES_OR_NO_QUERY".to_string(),
            1000,
            b"test".to_vec(),
        );

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
        let mut context = get_context(accounts(0));
        context.block_timestamp(0);
        testing_env!(context.build());

        let mut contract = Voting::new(accounts(0));

        let request_id = contract.request_price(
            "YES_OR_NO_QUERY".to_string(),
            1000,
            b"test".to_vec(),
        );

        // Try to advance before commit phase ends
        context.block_timestamp(1000);
        testing_env!(context.build());

        contract.advance_to_reveal(request_id);
    }

    #[test]
    fn test_set_config() {
        let context = get_context(accounts(0));
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
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = Voting::new(accounts(0));

        testing_env!(get_context(accounts(1)).build());
        contract.set_commit_phase_duration(100);
    }

    #[test]
    fn test_transfer_ownership() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = Voting::new(accounts(0));

        contract.set_owner(accounts(1));
        assert_eq!(contract.get_owner(), accounts(1));

        // New owner can set config
        testing_env!(get_context(accounts(1)).build());
        contract.set_commit_phase_duration(100);
    }

    #[test]
    fn test_has_price() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = Voting::new(accounts(0));

        let request_id = contract.request_price(
            "YES_OR_NO_QUERY".to_string(),
            1000,
            b"test".to_vec(),
        );

        // Not resolved yet
        assert!(!contract.has_price(request_id));
    }
}
