//! Callback recipient interface for contracts integrating with the oracle.
//!
//! Contracts that want to be notified when their assertions are resolved
//! or disputed should implement this trait.

/// Interface for contracts that receive callbacks from the Optimistic Oracle.
///
/// When creating an assertion, the asserter can specify a `callback_recipient`.
/// If set, the oracle will call these methods on that contract when the
/// assertion's state changes.
pub trait OptimisticOracleCallbackRecipientInterface {
    /// Called when an assertion is resolved (settled).
    ///
    /// This callback is invoked after `settle_assertion` completes, providing
    /// the final resolution of the assertion.
    ///
    /// # Arguments
    ///
    /// * `assertion_id` - Hex-encoded 32-byte assertion identifier
    /// * `asserted_truthfully` - `true` if the assertion was resolved as truthful,
    ///   `false` if it was resolved as false (disputer won)
    fn assertion_resolved_callback(
        &mut self,
        assertion_id: String,
        asserted_truthfully: bool,
    );

    /// Called when an assertion is disputed.
    ///
    /// This callback is invoked when someone successfully disputes an assertion,
    /// before the dispute is resolved.
    ///
    /// # Arguments
    ///
    /// * `assertion_id` - Hex-encoded 32-byte assertion identifier
    fn assertion_disputed_callback(
        &mut self,
        assertion_id: String,
    );
}
