//! Escalation Manager interface for custom assertion policies.
//!
//! Escalation managers allow customization of assertion behavior on a per-assertion
//! basis. They can control who can dispute, how disputes are resolved, and whether
//! to use the DVM or custom arbitration.
//!
//! This trait extends [`OptimisticOracleCallbackRecipientInterface`] since escalation
//! managers also receive callbacks when assertions are resolved or disputed.

use near_sdk::{near, AccountId};

use crate::types::Bytes32;

use super::OptimisticOracleCallbackRecipientInterface;

/// Policy flags that control assertion behavior.
///
/// These flags are returned by the escalation manager when an assertion is created
/// and determine how the oracle handles that assertion.
#[near(serializers = [json, borsh])]
#[derive(Clone, Default)]
pub struct AssertionPolicy {
    /// If true, the assertion should be blocked/rejected.
    pub block_assertion: bool,

    /// If true, the escalation manager will arbitrate disputes instead of the DVM.
    /// The oracle will call `get_price` on the escalation manager to resolve disputes.
    pub arbitrate_via_escalation_manager: bool,

    /// If true, the oracle should discard the DVM price and use the escalation
    /// manager's decision instead.
    pub discard_oracle: bool,

    /// If true, the escalation manager validates who can dispute via `is_dispute_allowed`.
    pub validate_disputers: bool,
}

/// Interface for contracts that manage escalation policies for assertions.
///
/// Escalation managers are optional contracts that can customize how assertions
/// behave. When an assertion specifies an escalation manager, the oracle will
/// call these methods at various points in the assertion lifecycle.
///
/// This trait extends [`OptimisticOracleCallbackRecipientInterface`], inheriting
/// the `assertion_resolved_callback` and `assertion_disputed_callback` methods.
///
/// # Lifecycle
///
/// 1. **Assertion Creation**: Oracle calls `get_assertion_policy` to get policy flags
/// 2. **Dispute**: If `validate_disputers` is true, oracle calls `is_dispute_allowed`
/// 3. **Dispute Resolution**: If `arbitrate_via_escalation_manager` is true:
///    - Oracle calls `request_price` when dispute occurs
///    - Oracle calls `get_price` when settling to get the resolution
/// 4. **Callbacks**: Oracle calls inherited `assertion_resolved_callback` and
///    `assertion_disputed_callback` methods
pub trait EscalationManagerInterface: OptimisticOracleCallbackRecipientInterface {
    /// Returns the assertion policy for a given assertion.
    ///
    /// Called by the oracle when an assertion is created to determine how
    /// to handle that assertion.
    ///
    /// # Arguments
    ///
    /// * `assertion_id` - The unique identifier of the assertion
    ///
    /// # Returns
    ///
    /// The policy flags for this assertion.
    fn get_assertion_policy(&self, assertion_id: Bytes32) -> AssertionPolicy;

    /// Validates whether a dispute should be allowed.
    ///
    /// Only called if `validate_disputers` is true in the assertion policy.
    /// Use this to implement disputer whitelists or other access control.
    ///
    /// # Arguments
    ///
    /// * `assertion_id` - The assertion being disputed
    /// * `dispute_caller` - The account attempting to dispute
    ///
    /// # Returns
    ///
    /// `true` if the dispute should be allowed, `false` to reject it.
    fn is_dispute_allowed(&self, assertion_id: Bytes32, dispute_caller: AccountId) -> bool;

    /// Requests a price/resolution for a disputed assertion.
    ///
    /// Called by the oracle when a dispute occurs and `arbitrate_via_escalation_manager`
    /// is true. The escalation manager should record this request and prepare to
    /// provide a resolution via `get_price`.
    ///
    /// This mimics the UMA DVM interface where disputes trigger price requests.
    ///
    /// # Arguments
    ///
    /// * `identifier` - The assertion identifier type (e.g., ASSERT_TRUTH)
    /// * `time` - The timestamp of the assertion (in nanoseconds)
    /// * `ancillary_data` - Additional data about the assertion (typically the claim)
    fn request_price(&mut self, identifier: Bytes32, time: u64, ancillary_data: Vec<u8>);

    /// Returns the resolution for a disputed assertion.
    ///
    /// Called by the oracle when settling a disputed assertion that uses
    /// escalation manager arbitration.
    ///
    /// # Arguments
    ///
    /// * `identifier` - The assertion identifier type
    /// * `time` - The timestamp of the assertion (in nanoseconds)
    /// * `ancillary_data` - Additional data about the assertion
    ///
    /// # Returns
    ///
    /// The resolution as an i128. Convention:
    /// - `1e18` (1_000_000_000_000_000_000) = assertion is true
    /// - `0` = assertion is false
    fn get_price(&self, identifier: Bytes32, time: u64, ancillary_data: Vec<u8>) -> i128;
}
