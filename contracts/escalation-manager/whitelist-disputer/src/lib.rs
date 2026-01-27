//! Whitelist Disputer Escalation Manager
//!
//! An escalation manager that restricts who can dispute assertions.
//! Only accounts on the whitelist are allowed to file disputes.

use near_sdk::{env, near, require, AccountId, PanicOnDefault};
use oracle_types::interfaces::AssertionPolicy;
use oracle_types::types::Bytes32;
use std::collections::HashSet;

/// Whitelist disputer escalation manager contract.
///
/// Only whitelisted accounts can dispute assertions managed by this escalation manager.
/// The owner can add/remove accounts from the whitelist.
#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct WhitelistDisputerEscalationManager {
    /// The oracle that this escalation manager works with.
    oracle: AccountId,
    /// The owner who can manage the whitelist.
    owner: AccountId,
    /// Accounts that are allowed to dispute.
    whitelisted_dispute_callers: HashSet<AccountId>,
}

#[near]
impl WhitelistDisputerEscalationManager {
    /// Initialize the escalation manager.
    ///
    /// # Arguments
    ///
    /// * `oracle` - The optimistic oracle contract address
    #[init]
    pub fn new(oracle: AccountId) -> Self {
        Self {
            oracle,
            owner: env::predecessor_account_id(),
            whitelisted_dispute_callers: HashSet::new(),
        }
    }

    // ========== Owner Methods ==========

    /// Add or remove an account from the disputer whitelist.
    ///
    /// # Arguments
    ///
    /// * `dispute_caller` - The account to add/remove
    /// * `whitelisted` - True to add, false to remove
    pub fn set_dispute_caller_in_whitelist(&mut self, dispute_caller: AccountId, whitelisted: bool) {
        self.assert_only_owner();

        if whitelisted {
            self.whitelisted_dispute_callers.insert(dispute_caller);
        } else {
            self.whitelisted_dispute_callers.remove(&dispute_caller);
        }
    }

    /// Transfer ownership to a new account.
    pub fn set_owner(&mut self, new_owner: AccountId) {
        self.assert_only_owner();
        self.owner = new_owner;
    }

    // ========== Escalation Manager Interface ==========

    /// Returns the assertion policy.
    ///
    /// Always returns `validate_disputers: true` so the oracle calls `is_dispute_allowed`.
    pub fn get_assertion_policy(&self, _assertion_id: Bytes32) -> AssertionPolicy {
        AssertionPolicy {
            block_assertion: false,
            arbitrate_via_escalation_manager: false,
            discard_oracle: false,
            validate_disputers: true,
        }
    }

    /// Check if a dispute is allowed.
    ///
    /// Returns true only if the caller is on the whitelist.
    pub fn is_dispute_allowed(&self, _assertion_id: Bytes32, dispute_caller: AccountId) -> bool {
        self.whitelisted_dispute_callers.contains(&dispute_caller)
    }

    /// Called when a price is requested (not used by this manager).
    pub fn request_price(&mut self, _identifier: Bytes32, _time: u64, _ancillary_data: Vec<u8>) {
        self.assert_only_oracle();
    }

    /// Get price (not implemented - this manager doesn't do custom arbitration).
    pub fn get_price(&self, _identifier: Bytes32, _time: u64, _ancillary_data: Vec<u8>) -> i128 {
        env::panic_str("This escalation manager does not support custom arbitration")
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

    /// Get the oracle address.
    pub fn get_oracle(&self) -> &AccountId {
        &self.oracle
    }

    /// Get the owner address.
    pub fn get_owner(&self) -> &AccountId {
        &self.owner
    }

    /// Check if an account is whitelisted.
    pub fn is_whitelisted(&self, account: AccountId) -> bool {
        self.whitelisted_dispute_callers.contains(&account)
    }

    // ========== Internal Methods ==========

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
