//! Optimistic Oracle interface and data structures.
//!
//! This module defines the core data structures used by the oracle and the
//! trait interface that the oracle contract implements.

use near_sdk::json_types::{U128, U64};
use near_sdk::{near, AccountId};

use crate::types::Bytes32;

/// Settings that control how an assertion interacts with its escalation manager.
///
/// These settings are stored with each assertion and determine the dispute
/// resolution behavior.
#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct EscalationManagerSettings {
    /// If true, disputes are resolved by the escalation manager instead of the DVM.
    pub arbitrate_via_escalation_manager: bool,

    /// If true, ignore the oracle/DVM result and use the escalation manager's decision.
    pub discard_oracle: bool,

    /// If true, the escalation manager validates who can dispute via `is_dispute_allowed`.
    pub validate_disputers: bool,

    /// The account that originally called the assertion function.
    pub asserting_caller: AccountId,

    /// Optional escalation manager contract for custom dispute handling.
    /// If None, default dispute resolution is used.
    pub escalation_manager: Option<AccountId>,
}

/// Represents a truth assertion in the oracle.
///
/// An assertion is a claim about the world that can be disputed within a
/// liveness period. If undisputed, it resolves as true. If disputed, it
/// goes through a resolution process.
#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct Assertion {
    /// Settings for escalation manager interaction.
    pub escalation_manager_settings: EscalationManagerSettings,

    /// Account that receives the bond back if the assertion is truthful.
    pub asserter: AccountId,

    /// Timestamp (in nanoseconds) when the assertion was created.
    pub assertion_time_ns: u64,

    /// Whether the assertion has been settled.
    pub settled: bool,

    /// Whether settlement is pending async payout completion.
    pub settlement_pending: bool,

    /// Whether a settlement payout attempt is currently in-flight.
    pub settlement_in_flight: bool,

    /// NEP-141 token contract used for the bond.
    pub currency: AccountId,

    /// Timestamp (in nanoseconds) after which the assertion can be settled if undisputed.
    pub expiration_time_ns: u64,

    /// The final resolution: true if assertion was truthful, false otherwise.
    /// Only valid after `settled` is true.
    pub settlement_resolution: bool,

    /// Pending resolution being finalized after payout callback succeeds.
    /// Only meaningful while `settlement_pending` is true.
    pub pending_settlement_resolution: bool,

    /// Domain identifier for grouping related assertions.
    pub domain_id: Bytes32,

    /// Identifier type for this assertion (e.g., ASSERT_TRUTH).
    pub identifier: Bytes32,

    /// Bond amount locked for this assertion.
    pub bond: U128,

    /// Optional contract to notify when the assertion is resolved.
    pub callback_recipient: Option<AccountId>,

    /// Account that disputed the assertion, if any.
    /// If Some, the assertion has been disputed and awaits resolution.
    pub disputer: Option<AccountId>,
}

/// Information about a whitelisted currency.
///
/// Only whitelisted currencies can be used for assertion bonds.
#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct WhitelistedCurrency {
    /// Whether this currency is currently whitelisted.
    pub is_whitelisted: bool,

    /// The fee charged when disputes are resolved.
    /// Used to calculate minimum bond: `min_bond = final_fee * 1e18 / burned_bond_percentage`
    pub final_fee: U128,
}

/// The main Optimistic Oracle interface.
///
/// This trait defines all the methods that the oracle contract exposes.
/// Contracts can use this trait to interact with the oracle in a type-safe manner.
pub trait OptimisticOracle {
    /// Dispute an existing assertion.
    ///
    /// The disputer must post a bond equal to the assertion's bond.
    /// Must be called before the assertion expires.
    ///
    /// # Arguments
    ///
    /// * `assertion_id` - The assertion to dispute
    /// * `disputer` - Account to credit if the dispute succeeds
    fn dispute_assertion(&mut self, assertion_id: Bytes32, disputer: AccountId);

    /// Returns the default identifier used for assertions (ASSERT_TRUTH).
    fn default_identifier(&self) -> Bytes32;

    /// Get the full details of an assertion.
    ///
    /// # Arguments
    ///
    /// * `assertion_id` - The assertion to query
    ///
    /// # Returns
    ///
    /// The assertion if it exists, None otherwise.
    fn get_assertion(&self, assertion_id: Bytes32) -> Option<Assertion>;

    /// Create an assertion with default parameters.
    ///
    /// Uses default liveness, identifier, and the contract's default currency.
    ///
    /// # Arguments
    ///
    /// * `claim` - The 32-byte encoded claim being asserted
    /// * `asserter` - Account to receive the bond if assertion is truthful
    ///
    /// # Returns
    ///
    /// The unique identifier for the created assertion.
    fn assert_truth_with_defaults(&mut self, claim: Bytes32, asserter: AccountId) -> Bytes32;

    /// Create an assertion with full parameter control.
    ///
    /// # Arguments
    ///
    /// * `claim` - The 32-byte encoded claim being asserted
    /// * `asserter` - Account to receive the bond if assertion is truthful
    /// * `callback_recipient` - Optional contract to notify on resolution
    /// * `escalation_manager` - Optional custom escalation manager
    /// * `liveness` - Time in nanoseconds before assertion can be settled
    /// * `currency` - NEP-141 token for the bond
    /// * `bond` - Amount of tokens to lock as bond
    /// * `identifier` - Assertion type identifier
    /// * `domain_id` - Domain for grouping assertions
    ///
    /// # Returns
    ///
    /// The unique identifier for the created assertion.
    fn assert_truth(
        &mut self,
        claim: Bytes32,
        asserter: AccountId,
        callback_recipient: Option<AccountId>,
        escalation_manager: Option<AccountId>,
        liveness: U64,
        currency: AccountId,
        bond: U128,
        identifier: Bytes32,
        domain_id: Bytes32,
    ) -> Bytes32;

    /// Sync parameters from the Nest registry.
    ///
    /// Updates cached identifier and currency whitelist status.
    ///
    /// # Arguments
    ///
    /// * `identifier` - Identifier to sync
    /// * `currency` - Currency to sync
    fn sync_nest_params(&mut self, identifier: Bytes32, currency: AccountId);

    /// Settle an assertion after its liveness period has expired.
    ///
    /// For undisputed assertions, resolves as true and returns bond to asserter.
    /// For disputed assertions, resolves based on dispute resolution outcome.
    ///
    /// # Arguments
    ///
    /// * `assertion_id` - The assertion to settle
    fn settle_assertion(&mut self, assertion_id: Bytes32);

    /// Settle an assertion and return its resolution.
    ///
    /// Convenience method that combines `settle_assertion` and `get_assertion_result`.
    ///
    /// # Arguments
    ///
    /// * `assertion_id` - The assertion to settle
    ///
    /// # Returns
    ///
    /// `true` if the assertion was truthful, `false` otherwise.
    fn settle_and_get_assertion_result(&mut self, assertion_id: Bytes32) -> bool;

    /// Get the resolution of a settled assertion.
    ///
    /// # Arguments
    ///
    /// * `assertion_id` - The assertion to query
    ///
    /// # Returns
    ///
    /// The resolution if the assertion is settled, None otherwise.
    fn get_assertion_result(&self, assertion_id: Bytes32) -> Option<bool>;

    /// Get the minimum bond required for a currency.
    ///
    /// Calculated as: `final_fee * 1e18 / burned_bond_percentage`
    ///
    /// # Arguments
    ///
    /// * `currency` - The NEP-141 token to query
    ///
    /// # Returns
    ///
    /// The minimum bond amount, or 0 if the currency is not whitelisted.
    fn get_minimum_bond(&self, currency: AccountId) -> U128;
}
