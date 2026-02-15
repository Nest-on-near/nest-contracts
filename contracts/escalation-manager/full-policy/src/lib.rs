//! Full Policy Escalation Manager
//!
//! A fully configurable escalation manager that supports:
//! - Whitelisting asserting callers (contracts that can create assertions)
//! - Whitelisting asserters (accounts that can be asserters)
//! - Whitelisting disputers
//! - Custom arbitration (owner sets resolution instead of DVM)
//! - Discarding oracle resolution

use near_sdk::{env, near, require, AccountId, PanicOnDefault};
use oracle_types::interfaces::AssertionPolicy;
use oracle_types::types::Bytes32;
use std::collections::{HashMap, HashSet};

/// Stored resolution for a disputed assertion.
#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct ArbitrationResolution {
    /// Whether the resolution has been set.
    pub value_set: bool,
    /// The resolution (true = assertion correct, false = assertion incorrect).
    pub resolution: bool,
}

/// Numerical representation of "true" (1e18).
pub const NUMERICAL_TRUE: i128 = 1_000_000_000_000_000_000;

/// Full policy escalation manager contract.
///
/// Provides complete control over assertion policies:
/// - Block assertions by caller or asserter whitelist
/// - Restrict who can dispute
/// - Use custom arbitration instead of DVM
/// - Optionally discard oracle resolution
#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct FullPolicyEscalationManager {
    /// The oracle that this escalation manager works with.
    oracle: AccountId,
    /// The owner who can configure policies.
    owner: AccountId,

    // Policy flags
    /// If true, only whitelisted asserting callers can create assertions.
    block_by_asserting_caller: bool,
    /// If true, only whitelisted asserters can be asserters (requires block_by_asserting_caller).
    block_by_asserter: bool,
    /// If true, the escalation manager validates disputers.
    validate_disputers: bool,
    /// If true, disputes are arbitrated by this manager instead of DVM.
    arbitrate_via_escalation_manager: bool,
    /// If true, discard the oracle's resolution.
    discard_oracle: bool,

    // Whitelists
    /// Contracts allowed to create assertions.
    whitelisted_asserting_callers: HashSet<AccountId>,
    /// Accounts allowed to be asserters.
    whitelisted_asserters: HashSet<AccountId>,
    /// Accounts allowed to dispute.
    whitelisted_dispute_callers: HashSet<AccountId>,

    // Arbitration resolutions (request_id -> resolution)
    arbitration_resolutions: HashMap<String, ArbitrationResolution>,
}

#[near]
impl FullPolicyEscalationManager {
    /// Initialize the escalation manager.
    #[init]
    pub fn new(oracle: AccountId) -> Self {
        Self {
            oracle,
            owner: env::predecessor_account_id(),
            block_by_asserting_caller: false,
            block_by_asserter: false,
            validate_disputers: false,
            arbitrate_via_escalation_manager: false,
            discard_oracle: false,
            whitelisted_asserting_callers: HashSet::new(),
            whitelisted_asserters: HashSet::new(),
            whitelisted_dispute_callers: HashSet::new(),
            arbitration_resolutions: HashMap::new(),
        }
    }

    // ========== Owner Configuration ==========

    /// Configure all policy flags at once.
    ///
    /// # Arguments
    ///
    /// * `block_by_asserting_caller` - Require whitelisted asserting callers
    /// * `block_by_asserter` - Require whitelisted asserters (needs block_by_asserting_caller)
    /// * `validate_disputers` - Require whitelisted disputers
    /// * `arbitrate_via_escalation_manager` - Use custom arbitration instead of DVM
    /// * `discard_oracle` - Ignore oracle resolution
    pub fn configure(
        &mut self,
        block_by_asserting_caller: bool,
        block_by_asserter: bool,
        validate_disputers: bool,
        arbitrate_via_escalation_manager: bool,
        discard_oracle: bool,
    ) {
        self.assert_only_owner();

        // Can't block by asserter without blocking by asserting caller
        require!(
            !block_by_asserter || block_by_asserting_caller,
            "Cannot block only by asserter"
        );

        self.block_by_asserting_caller = block_by_asserting_caller;
        self.block_by_asserter = block_by_asserter;
        self.validate_disputers = validate_disputers;
        self.arbitrate_via_escalation_manager = arbitrate_via_escalation_manager;
        self.discard_oracle = discard_oracle;
    }

    /// Set an arbitration resolution for a disputed assertion.
    ///
    /// Call this when `arbitrate_via_escalation_manager` is true and a dispute needs resolution.
    pub fn set_arbitration_resolution(
        &mut self,
        identifier: Bytes32,
        time: u64,
        ancillary_data: Vec<u8>,
        resolution: bool,
    ) {
        self.assert_only_owner();

        let request_id = Self::get_request_id(&identifier, time, &ancillary_data);

        require!(
            !self
                .arbitration_resolutions
                .get(&request_id)
                .map(|r| r.value_set)
                .unwrap_or(false),
            "Arbitration already resolved"
        );

        self.arbitration_resolutions.insert(
            request_id,
            ArbitrationResolution {
                value_set: true,
                resolution,
            },
        );
    }

    /// Add/remove an asserting caller from the whitelist.
    pub fn set_whitelisted_asserting_caller(&mut self, caller: AccountId, whitelisted: bool) {
        self.assert_only_owner();
        if whitelisted {
            self.whitelisted_asserting_callers.insert(caller);
        } else {
            self.whitelisted_asserting_callers.remove(&caller);
        }
    }

    /// Add/remove an asserter from the whitelist.
    pub fn set_whitelisted_asserter(&mut self, asserter: AccountId, whitelisted: bool) {
        self.assert_only_owner();
        if whitelisted {
            self.whitelisted_asserters.insert(asserter);
        } else {
            self.whitelisted_asserters.remove(&asserter);
        }
    }

    /// Add/remove a dispute caller from the whitelist.
    pub fn set_whitelisted_dispute_caller(&mut self, caller: AccountId, whitelisted: bool) {
        self.assert_only_owner();
        if whitelisted {
            self.whitelisted_dispute_callers.insert(caller);
        } else {
            self.whitelisted_dispute_callers.remove(&caller);
        }
    }

    /// Transfer ownership.
    pub fn set_owner(&mut self, new_owner: AccountId) {
        self.assert_only_owner();
        self.owner = new_owner;
    }

    // ========== Escalation Manager Interface ==========

    /// Returns the assertion policy based on current configuration.
    ///
    /// Note: `block_assertion` is always false here because we can't check
    /// the asserting caller/asserter without querying the oracle. The oracle
    /// should call a separate method to check blocking if needed.
    pub fn get_assertion_policy(&self, _assertion_id: Bytes32) -> AssertionPolicy {
        // Note: In UMA's Solidity version, they query the oracle to get assertion
        // details and check whitelists. For NEAR, we'd need cross-contract calls.
        // For now, return the policy flags - blocking logic would need oracle integration.
        AssertionPolicy {
            block_assertion: false, // Would need oracle query to determine
            arbitrate_via_escalation_manager: self.arbitrate_via_escalation_manager,
            discard_oracle: self.discard_oracle,
            validate_disputers: self.validate_disputers,
        }
    }

    /// Check if an asserting caller is allowed (for oracle to call).
    pub fn is_asserting_caller_allowed(&self, asserting_caller: AccountId) -> bool {
        if !self.block_by_asserting_caller {
            return true;
        }
        self.whitelisted_asserting_callers
            .contains(&asserting_caller)
    }

    /// Check if an asserter is allowed (for oracle to call).
    pub fn is_asserter_allowed(&self, asserter: AccountId) -> bool {
        if !self.block_by_asserter {
            return true;
        }
        self.whitelisted_asserters.contains(&asserter)
    }

    /// Check if a dispute is allowed.
    pub fn is_dispute_allowed(&self, _assertion_id: Bytes32, dispute_caller: AccountId) -> bool {
        if !self.validate_disputers {
            return true;
        }
        self.whitelisted_dispute_callers.contains(&dispute_caller)
    }

    /// Called when a price is requested for arbitration.
    pub fn request_price(&mut self, _identifier: Bytes32, _time: u64, _ancillary_data: Vec<u8>) {
        self.assert_only_oracle();
        // Event emitted by base - owner should watch for this and call set_arbitration_resolution
    }

    /// Get the arbitration resolution.
    pub fn get_price(&self, identifier: Bytes32, time: u64, ancillary_data: Vec<u8>) -> i128 {
        let request_id = Self::get_request_id(&identifier, time, &ancillary_data);

        let resolution = self
            .arbitration_resolutions
            .get(&request_id)
            .expect("Arbitration resolution not set");

        require!(resolution.value_set, "Arbitration resolution not set");

        if resolution.resolution {
            NUMERICAL_TRUE
        } else {
            0
        }
    }

    /// Callback when an assertion is resolved.
    pub fn assertion_resolved_callback(
        &mut self,
        _assertion_id: String,
        _asserted_truthfully: bool,
    ) {
        self.assert_only_oracle();
    }

    /// Callback when an assertion is disputed.
    pub fn assertion_disputed_callback(&mut self, _assertion_id: String) {
        self.assert_only_oracle();
    }

    // ========== View Methods ==========

    pub fn get_oracle(&self) -> &AccountId {
        &self.oracle
    }

    pub fn get_owner(&self) -> &AccountId {
        &self.owner
    }

    pub fn get_config(
        &self,
    ) -> (
        bool, // block_by_asserting_caller
        bool, // block_by_asserter
        bool, // validate_disputers
        bool, // arbitrate_via_escalation_manager
        bool, // discard_oracle
    ) {
        (
            self.block_by_asserting_caller,
            self.block_by_asserter,
            self.validate_disputers,
            self.arbitrate_via_escalation_manager,
            self.discard_oracle,
        )
    }

    pub fn is_asserting_caller_whitelisted(&self, caller: AccountId) -> bool {
        self.whitelisted_asserting_callers.contains(&caller)
    }

    pub fn is_asserter_whitelisted(&self, asserter: AccountId) -> bool {
        self.whitelisted_asserters.contains(&asserter)
    }

    pub fn is_dispute_caller_whitelisted(&self, caller: AccountId) -> bool {
        self.whitelisted_dispute_callers.contains(&caller)
    }

    /// Get the request ID for a price request.
    pub fn get_request_id(identifier: &Bytes32, time: u64, ancillary_data: &[u8]) -> String {
        use near_sdk::env::keccak256;

        let mut data = Vec::new();
        data.extend_from_slice(identifier);
        data.extend_from_slice(&time.to_le_bytes());
        data.extend_from_slice(ancillary_data);

        hex::encode(keccak256(&data))
    }

    // ========== Internal ==========

    fn assert_only_oracle(&self) {
        require!(
            env::predecessor_account_id() == self.oracle,
            "Only the oracle can call this method"
        );
    }

    fn assert_only_owner(&self) {
        require!(
            env::predecessor_account_id() == self.owner,
            "Only the owner can call this method"
        );
    }
}
