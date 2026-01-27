//! Base Escalation Manager
//!
//! A base implementation of the escalation manager interface that provides
//! default (permissive) behavior. Other escalation managers can use this
//! as a reference or extend it.

use near_sdk::{env, near, require, AccountId, PanicOnDefault};
use oracle_types::interfaces::AssertionPolicy;
use oracle_types::types::Bytes32;

/// Event emitted when a price request is added.
#[near(serializers = [json])]
pub struct PriceRequestAdded {
    pub identifier: String,
    pub time: u64,
    pub ancillary_data: String,
}

/// Base escalation manager contract.
///
/// Provides default implementations for all escalation manager methods:
/// - `get_assertion_policy`: Returns all-false policy (no special handling)
/// - `is_dispute_allowed`: Returns true (all disputes allowed)
/// - `request_price`: Emits an event (for off-chain tracking)
/// - `get_price`: Panics (not implemented by default)
/// - Callbacks: No-op
#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct BaseEscalationManager {
    /// The oracle that this escalation manager works with.
    oracle: AccountId,
}

#[near]
impl BaseEscalationManager {
    /// Initialize the escalation manager.
    ///
    /// # Arguments
    ///
    /// * `oracle` - The optimistic oracle contract address
    #[init]
    pub fn new(oracle: AccountId) -> Self {
        Self { oracle }
    }

    /// Returns the assertion policy for a given assertion.
    ///
    /// Default implementation returns a permissive policy with all flags false.
    pub fn get_assertion_policy(&self, _assertion_id: Bytes32) -> AssertionPolicy {
        AssertionPolicy::default()
    }

    /// Validates whether a dispute should be allowed.
    ///
    /// Default implementation allows all disputes.
    pub fn is_dispute_allowed(&self, _assertion_id: Bytes32, _dispute_caller: AccountId) -> bool {
        true
    }

    /// Called when a price is requested for dispute resolution.
    ///
    /// Default implementation emits an event for off-chain tracking.
    /// Only callable by the oracle.
    pub fn request_price(&mut self, identifier: Bytes32, time: u64, ancillary_data: Vec<u8>) {
        self.assert_only_oracle();

        let event = PriceRequestAdded {
            identifier: hex::encode(identifier),
            time,
            ancillary_data: hex::encode(&ancillary_data),
        };

        env::log_str(&format!(
            "EVENT_JSON:{}",
            near_sdk::serde_json::to_string(&event).unwrap()
        ));
    }

    /// Returns the price/resolution for a disputed assertion.
    ///
    /// Default implementation panics - subclasses should override this
    /// if they set `arbitrate_via_escalation_manager` to true.
    pub fn get_price(&self, _identifier: Bytes32, _time: u64, _ancillary_data: Vec<u8>) -> i128 {
        env::panic_str("get_price not implemented in base escalation manager")
    }

    /// Callback when an assertion is resolved.
    ///
    /// Default implementation does nothing. Only callable by the oracle.
    pub fn assertion_resolved_callback(
        &mut self,
        _assertion_id: String,
        _asserted_truthfully: bool,
    ) {
        self.assert_only_oracle();
    }

    /// Callback when an assertion is disputed.
    ///
    /// Default implementation does nothing. Only callable by the oracle.
    pub fn assertion_disputed_callback(&mut self, _assertion_id: String) {
        self.assert_only_oracle();
    }

    // ========== View Methods ==========

    /// Get the oracle address.
    pub fn get_oracle(&self) -> &AccountId {
        &self.oracle
    }

    // ========== Internal Methods ==========

    /// Asserts that the caller is the oracle.
    fn assert_only_oracle(&self) {
        require!(
            env::predecessor_account_id() == self.oracle,
            "Only the oracle can call this method"
        );
    }
}
