//! Oracle event definitions following the NEP-297 standard.
//!
//! This module defines all events emitted by the Nest Optimistic Oracle.
//! Events are logged in JSON format and can be indexed by off-chain services.
//!
//! Reference: https://nomicon.io/Standards/EventsFormat

use near_sdk::{
    AccountId, log,
    serde::Serialize,
    serde_json::json,
    json_types::U128,
};

use crate::types::Bytes32;

/// Event standard identifier for Nest oracle events.
const EVENT_STANDARD: &str = "nest-events";

/// Current version of the event standard.
const EVENT_STANDARD_VERSION: &str = "1.0.0";

/// All events emitted by the Nest Optimistic Oracle.
///
/// Each variant represents a distinct event type with its associated data.
/// Events are serialized to JSON with snake_case field names.
#[derive(Clone, Serialize)]
#[serde(crate = "near_sdk::serde")]
#[serde(tag = "event", content = "data")]
#[serde(rename_all = "snake_case")]
pub enum Event<'a> {
    /// Emitted when a new assertion is created.
    ///
    /// This event is logged after an asserter successfully submits a claim
    /// with the required bond through `ft_transfer_call`.
    AssertionMade {
        /// Unique identifier for this assertion (keccak256 hash of parameters).
        assertion_id: &'a Bytes32,
        /// Domain identifier for grouping related assertions.
        domain_id: &'a Bytes32,
        /// The claim being asserted (32-byte encoded value).
        claim: &'a Bytes32,
        /// Account that will receive the bond back if the assertion is truthful.
        asserter: &'a AccountId,
        /// Optional contract to notify when the assertion is resolved.
        callback_recipient: &'a Option<AccountId>,
        /// Optional escalation manager contract for custom dispute handling.
        escalation_manager: &'a Option<AccountId>,
        /// Account that initiated the assertion (may differ from asserter).
        caller: &'a AccountId,
        /// Timestamp (in nanoseconds) when the assertion can be settled if undisputed.
        expiration_time_ns: u64,
        /// NEP-141 token used for the bond.
        currency: &'a AccountId,
        /// Bond amount locked for this assertion.
        bond: &'a U128,
        /// Identifier type for this assertion (e.g., ASSERT_TRUTH).
        identifier: &'a Bytes32,
    },

    /// Emitted when an assertion is disputed.
    ///
    /// A dispute occurs when someone challenges an assertion by posting
    /// a matching bond before the assertion expires.
    AssertionDisputed {
        /// The assertion being disputed.
        assertion_id: &'a Bytes32,
        /// Account that called the dispute function.
        caller: &'a AccountId,
        /// Account designated as the disputer (receives bond if dispute succeeds).
        disputer: &'a AccountId,
    },

    /// Emitted when an assertion is settled.
    ///
    /// Settlement occurs either after the liveness period expires (for undisputed
    /// assertions) or after dispute resolution (for disputed assertions).
    AssertionSettled {
        /// The assertion being settled.
        assertion_id: &'a Bytes32,
        /// Account receiving the bond(s) after settlement.
        bond_recipient: &'a AccountId,
        /// True if the assertion was disputed before settlement.
        disputed: bool,
        /// True if the assertion was resolved as truthful, false otherwise.
        settlement_resolution: bool,
        /// Account that triggered the settlement.
        settle_caller: &'a AccountId,
    },

    /// Emitted when the contract owner updates administrative properties.
    ///
    /// These properties affect default values for new assertions.
    AdminPropertiesSet {
        /// Default NEP-141 token for bonds.
        default_currency: &'a AccountId,
        /// Default liveness period in nanoseconds.
        default_liveness_ns: u64,
        /// Percentage of bond burned on dispute (scaled by 1e18, e.g., 0.5e18 = 50%).
        burned_bond_percentage: u128,
    },
}

impl Event<'_> {
    /// Emit this event to the NEAR logs.
    ///
    /// The event is formatted as JSON following NEP-297 and prefixed with "EVENT_JSON:".
    pub fn emit(&self) {
        emit_event(&self);
    }
}

/// Formats and logs an event following the NEP-297 standard.
///
/// NEP-297 defines a standard format for indexable events on NEAR:
/// - `standard`: Name of the event standard (e.g., "nest-events")
/// - `version`: Version of the standard (e.g., "1.0.0")
/// - `event`: Event type name (e.g., "assertion_made")
/// - `data`: Array of event data objects
///
/// The output is logged with the "EVENT_JSON:" prefix for indexer detection.
pub(crate) fn emit_event<T: ?Sized + Serialize>(data: &T) {
    let result = json!(data);
    let event_json = json!({
        "standard": EVENT_STANDARD,
        "version": EVENT_STANDARD_VERSION,
        "event": result["event"],
        "data": [result["data"]]
    })
    .to_string();
    log!("{}", format!("EVENT_JSON:{}", event_json));
}
