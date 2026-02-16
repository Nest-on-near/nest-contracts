use near_sdk::{
    env,
    json_types::{U128, U64},
    near, require,
    serde::{Deserialize, Serialize},
    store::LookupMap,
    AccountId, CryptoHash, Gas, NearToken, PanicOnDefault, Promise, PromiseError,
};

/// Gas for cross-contract calls
const GAS_FOR_FT_TRANSFER: Gas = Gas::from_tgas(10);
const GAS_FOR_CALLBACK: Gas = Gas::from_tgas(5);
const GAS_FOR_DVM_REQUEST: Gas = Gas::from_tgas(30);
const GAS_FOR_DVM_CALLBACK: Gas = Gas::from_tgas(50);
const GAS_FOR_DVM_GET_PRICE: Gas = Gas::from_tgas(10);
const GAS_FOR_SETTLE_CALLBACK: Gas = Gas::from_tgas(80);

use oracle_types::{
    events::Event,
    interfaces::{Assertion, EscalationManagerSettings, WhitelistedCurrency},
    types::Bytes32,
};

// ============================================================================
// Constants
// ============================================================================

/// Default identifier for assertions
/// Padded to 32 bytes
const DEFAULT_IDENTIFIER: Bytes32 = *b"ASSERT_TRUTH\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";

/// Default liveness period: 2 hours in nanoseconds
const DEFAULT_LIVENESS_NS: u64 = 2 * 60 * 60 * 1_000_000_000;

/// Burned bond percentage: 50% represented as 0.5e18 (same as UMA)
const BURNED_BOND_PERCENTAGE: u128 = 500_000_000_000_000_000; // 0.5e18

/// Numerical representation of "true" for oracle responses
const NUMERICAL_TRUE: i128 = 1_000_000_000_000_000_000; // 1e18

/// 1e18 for percentage calculations
const SCALE: u128 = 1_000_000_000_000_000_000;

// ============================================================================
// NEP-141 ft_on_transfer Message Types
// ============================================================================

/// Message passed via ft_transfer_call to create an assertion
/// The bond amount comes from the ft_transfer_call amount
#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct AssertTruthArgs {
    /// The truth claim being asserted (32 bytes)
    pub claim: Bytes32,
    /// Account that receives bonds back at settlement
    pub asserter: AccountId,
    /// Optional callback recipient for assertion resolution
    pub callback_recipient: Option<AccountId>,
    /// Optional escalation manager address
    pub escalation_manager: Option<AccountId>,
    /// Liveness period in nanoseconds (if None, uses default)
    pub liveness_ns: Option<U64>,
    /// Optional assertion timestamp in nanoseconds used for deterministic assertion IDs.
    /// If omitted, oracle uses current block timestamp (legacy behavior).
    pub assertion_time_ns: Option<U64>,
    /// Identifier for the assertion (if None, uses default)
    pub identifier: Option<Bytes32>,
    /// Optional domain ID for grouping assertions
    pub domain_id: Option<Bytes32>,
    /// Optional deterministic assertion id supplied by an upstream integrator.
    /// If provided, the oracle uses it directly instead of recomputing from inputs.
    pub assertion_id_override: Option<Bytes32>,
}

/// Message types for ft_on_transfer
#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
#[serde(tag = "action")]
pub enum FtOnTransferMsg {
    /// Create a new assertion with the transferred tokens as bond
    AssertTruth(AssertTruthArgs),
    /// Dispute an existing assertion
    DisputeAssertion {
        assertion_id: Bytes32,
        disputer: AccountId,
    },
}

// ============================================================================
// Contract State
// ============================================================================

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct NestOptimisticOracle {
    /// Contract owner (equivalent to Ownable in Solidity)
    owner: AccountId,

    /// Default currency for assertions (NEP-141 token account ID)
    default_currency: AccountId,

    /// Default liveness period in nanoseconds
    default_liveness_ns: u64,

    /// Percentage of the bond that is burned on disputes (scaled by 1e18)
    burned_bond_percentage: u128,

    /// Whitelisted currencies with their final fees
    cached_currencies: LookupMap<AccountId, WhitelistedCurrency>,

    /// Cached identifiers that are approved for use
    cached_identifiers: LookupMap<Bytes32, bool>,

    /// All assertions made by the Optimistic Oracle
    assertions: LookupMap<Bytes32, Assertion>,

    /// DVM Voting contract for dispute resolution
    voting_contract: Option<AccountId>,

    /// Mapping from assertion_id to DVM request_id for disputed assertions
    /// Used to track which DVM vote corresponds to which assertion
    dispute_requests: LookupMap<Bytes32, CryptoHash>,

    /// Reverse mapping from DVM request_id to assertion_id
    request_to_assertion: LookupMap<CryptoHash, Bytes32>,
}

// ============================================================================
// Contract Implementation
// ============================================================================

#[near]
impl NestOptimisticOracle {
    /// Initialize the contract
    #[init]
    pub fn new(
        owner: AccountId,
        default_currency: AccountId,
        default_liveness_ns: Option<U64>,
        burned_bond_percentage: Option<U128>,
        voting_contract: Option<AccountId>,
    ) -> Self {
        let liveness = default_liveness_ns
            .map(|l| l.0)
            .unwrap_or(DEFAULT_LIVENESS_NS);

        let burn_pct = burned_bond_percentage
            .map(|b| b.0)
            .unwrap_or(BURNED_BOND_PERCENTAGE);

        require!(burn_pct <= SCALE, "Burned bond percentage > 100%");
        require!(burn_pct > 0, "Burned bond percentage is 0");

        let mut contract = Self {
            owner,
            default_currency: default_currency.clone(),
            default_liveness_ns: liveness,
            burned_bond_percentage: burn_pct,
            cached_currencies: LookupMap::new(b"c"),
            cached_identifiers: LookupMap::new(b"i"),
            assertions: LookupMap::new(b"a"),
            voting_contract,
            dispute_requests: LookupMap::new(b"d"),
            request_to_assertion: LookupMap::new(b"r"),
        };

        // Cache the default identifier as approved
        contract.cached_identifiers.insert(DEFAULT_IDENTIFIER, true);

        // Emit admin properties set event
        Event::AdminPropertiesSet {
            default_currency: &default_currency,
            default_liveness_ns: liveness,
            burned_bond_percentage: burn_pct,
        }
        .emit();

        contract
    }

    // ========================================================================
    // View Methods
    // ========================================================================

    /// Returns the default identifier used by the Optimistic Oracle
    pub fn default_identifier(&self) -> Bytes32 {
        DEFAULT_IDENTIFIER
    }

    /// Returns the default currency
    pub fn default_currency(&self) -> AccountId {
        self.default_currency.clone()
    }

    /// Returns the default liveness in nanoseconds
    pub fn default_liveness(&self) -> U64 {
        U64(self.default_liveness_ns)
    }

    /// Fetches information about a specific assertion
    pub fn get_assertion(&self, assertion_id: Bytes32) -> Option<Assertion> {
        self.assertions.get(&assertion_id).cloned()
    }

    /// Returns the minimum bond amount required to make an assertion
    /// min_bond = final_fee * 1e18 / burned_bond_percentage
    pub fn get_minimum_bond(&self, currency: AccountId) -> U128 {
        match self.cached_currencies.get(&currency) {
            Some(cached) if cached.is_whitelisted => {
                let final_fee = cached.final_fee.0;
                let min_bond = final_fee
                    .saturating_mul(SCALE)
                    .saturating_div(self.burned_bond_percentage);
                U128(min_bond)
            }
            _ => U128(0),
        }
    }

    /// Fetches the resolution of a specific assertion
    pub fn get_assertion_result(&self, assertion_id: Bytes32) -> bool {
        let assertion = self
            .assertions
            .get(&assertion_id)
            .expect("Assertion does not exist");

        // Return early if not using answer from resolved dispute (discardOracle = true)
        if assertion.disputer.is_some() && assertion.escalation_manager_settings.discard_oracle {
            return false;
        }

        require!(assertion.settled, "Assertion not settled");
        assertion.settlement_resolution
    }

    /// Check if an identifier is cached/approved
    pub fn is_identifier_supported(&self, identifier: Bytes32) -> bool {
        self.cached_identifiers
            .get(&identifier)
            .copied()
            .unwrap_or(false)
    }

    /// Check if a currency is whitelisted
    pub fn is_currency_whitelisted(&self, currency: AccountId) -> bool {
        self.cached_currencies
            .get(&currency)
            .map(|c| c.is_whitelisted)
            .unwrap_or(false)
    }

    /// Get the voting contract address
    pub fn get_voting_contract(&self) -> Option<AccountId> {
        self.voting_contract.clone()
    }

    /// Get the DVM request ID for a disputed assertion
    pub fn get_dispute_request(&self, assertion_id: Bytes32) -> Option<CryptoHash> {
        self.dispute_requests.get(&assertion_id).copied()
    }

    /// Check if a disputed assertion has been resolved by DVM
    pub fn is_dispute_resolved(&self, assertion_id: Bytes32) -> bool {
        self.dispute_requests.get(&assertion_id).is_some()
    }

    // ========================================================================
    // Admin Methods (onlyOwner)
    // ========================================================================

    /// Sets the default currency, liveness, and burned bond percentage
    /// Equivalent to: function setAdminProperties(...) public onlyOwner
    pub fn set_admin_properties(
        &mut self,
        default_currency: AccountId,
        default_liveness_ns: U64,
        burned_bond_percentage: U128,
    ) {
        self.assert_owner();

        require!(
            burned_bond_percentage.0 <= SCALE,
            "Burned bond percentage > 100%"
        );
        require!(burned_bond_percentage.0 > 0, "Burned bond percentage is 0");

        self.default_currency = default_currency.clone();
        self.default_liveness_ns = default_liveness_ns.0;
        self.burned_bond_percentage = burned_bond_percentage.0;

        Event::AdminPropertiesSet {
            default_currency: &default_currency,
            default_liveness_ns: default_liveness_ns.0,
            burned_bond_percentage: burned_bond_percentage.0,
        }
        .emit();
    }

    /// Whitelist a currency with its final fee (Phase 1 simplified)
    /// In UMA this is done via syncUmaParams, but we simplify for Phase 1
    pub fn whitelist_currency(&mut self, currency: AccountId, final_fee: U128) {
        self.assert_owner();
        self.cached_currencies.insert(
            currency,
            WhitelistedCurrency {
                is_whitelisted: true,
                final_fee,
            },
        );
    }

    /// Approve an identifier for use
    pub fn whitelist_identifier(&mut self, identifier: Bytes32) {
        self.assert_owner();
        self.cached_identifiers.insert(identifier, true);
    }

    /// Set the DVM voting contract address
    pub fn set_voting_contract(&mut self, voting_contract: AccountId) {
        self.assert_owner();
        self.voting_contract = Some(voting_contract);
    }

    /// Emergency token withdrawal for stuck funds recovery.
    /// Owner-only: can move bonded funds, so use only for controlled recovery.
    pub fn emergency_withdraw_token(
        &mut self,
        token: AccountId,
        receiver_id: AccountId,
        amount: U128,
    ) -> Promise {
        self.assert_owner();
        require!(amount.0 > 0, "Amount must be positive");

        Promise::new(token).function_call(
            "ft_transfer".to_string(),
            near_sdk::serde_json::json!({
                "receiver_id": receiver_id,
                "amount": amount,
            })
            .to_string()
            .into_bytes(),
            NearToken::from_yoctonear(1),
            GAS_FOR_FT_TRANSFER,
        )
    }

    /// Emergency native NEAR withdrawal for stuck balance recovery.
    /// Owner-only.
    pub fn emergency_withdraw_near(&mut self, receiver_id: AccountId, amount: U128) -> Promise {
        self.assert_owner();
        require!(amount.0 > 0, "Amount must be positive");
        require!(
            env::account_balance() >= NearToken::from_yoctonear(amount.0),
            "Insufficient balance"
        );

        Promise::new(receiver_id).transfer(NearToken::from_yoctonear(amount.0))
    }

    // ========================================================================
    // NEP-141 Receiver (for bonding)
    // ========================================================================

    /// Called by NEP-141 token contract when tokens are transferred via ft_transfer_call
    /// Returns the amount of tokens to refund (0 if all tokens are used)
    pub fn ft_on_transfer(&mut self, sender_id: AccountId, amount: U128, msg: String) -> U128 {
        let currency = env::predecessor_account_id();

        // Parse the message to determine the action
        let parsed_msg: FtOnTransferMsg =
            near_sdk::serde_json::from_str(&msg).expect("Invalid ft_on_transfer message format");

        match parsed_msg {
            FtOnTransferMsg::AssertTruth(args) => {
                let _assertion_id = self.internal_assert_truth(
                    args.claim,
                    args.asserter,
                    args.callback_recipient,
                    args.escalation_manager,
                    args.liveness_ns.map(|l| l.0),
                    args.assertion_time_ns.map(|t| t.0),
                    currency,
                    amount.0,
                    args.identifier,
                    args.domain_id,
                    args.assertion_id_override,
                    sender_id,
                );
                // All tokens used for bond, no refund
                U128(0)
            }
            FtOnTransferMsg::DisputeAssertion {
                assertion_id,
                disputer,
            } => {
                self.internal_dispute_assertion(
                    assertion_id,
                    disputer,
                    currency,
                    amount.0,
                    sender_id,
                );
                // All tokens used for dispute bond, no refund
                U128(0)
            }
        }
    }

    // ========================================================================
    // Core Assertion Methods
    // ========================================================================

    /// Internal implementation of assert_truth
    /// Called by ft_on_transfer when receiving bond tokens
    fn internal_assert_truth(
        &mut self,
        claim: Bytes32,
        asserter: AccountId,
        callback_recipient: Option<AccountId>,
        escalation_manager: Option<AccountId>,
        liveness_ns: Option<u64>,
        assertion_time_ns: Option<u64>,
        currency: AccountId,
        bond: u128,
        identifier: Option<Bytes32>,
        domain_id: Option<Bytes32>,
        assertion_id_override: Option<Bytes32>,
        caller: AccountId,
    ) -> Bytes32 {
        let time = assertion_time_ns.unwrap_or_else(|| self.get_current_time());
        let liveness = liveness_ns.unwrap_or(self.default_liveness_ns);
        let identifier = identifier.unwrap_or(DEFAULT_IDENTIFIER);
        let domain_id = domain_id.unwrap_or([0u8; 32]);

        // Generate unique assertion ID (or accept integrator-provided deterministic override)
        let assertion_id = assertion_id_override.unwrap_or_else(|| {
            self.get_assertion_id(
                &claim,
                bond,
                time,
                liveness,
                &currency,
                &callback_recipient,
                &escalation_manager,
                &identifier,
                &caller,
            )
        });

        // Validations (equivalent to Solidity requires)
        require!(
            self.assertions.get(&assertion_id).is_none(),
            "Assertion already exists"
        );
        require!(
            self.cached_identifiers
                .get(&identifier)
                .copied()
                .unwrap_or(false),
            "Unsupported identifier"
        );
        require!(
            self.cached_currencies
                .get(&currency)
                .map(|c| c.is_whitelisted)
                .unwrap_or(false),
            "Unsupported currency"
        );
        let min_bond = self.get_minimum_bond(currency.clone()).0;
        require!(bond >= min_bond, "Bond amount too low");

        // Create the assertion
        let assertion = Assertion {
            escalation_manager_settings: EscalationManagerSettings {
                arbitrate_via_escalation_manager: false,
                discard_oracle: false,
                validate_disputers: false,
                asserting_caller: caller.clone(),
                escalation_manager: escalation_manager.clone(),
            },
            asserter: asserter.clone(),
            assertion_time_ns: time,
            settled: false,
            settlement_pending: false,
            settlement_in_flight: false,
            currency: currency.clone(),
            expiration_time_ns: time + liveness,
            settlement_resolution: false,
            pending_settlement_resolution: false,
            domain_id,
            identifier,
            bond: U128(bond),
            callback_recipient: callback_recipient.clone(),
            disputer: None,
        };

        self.assertions.insert(assertion_id, assertion);

        // Emit event
        Event::AssertionMade {
            assertion_id: &assertion_id,
            domain_id: &domain_id,
            claim: &claim,
            asserter: &asserter,
            callback_recipient: &callback_recipient,
            escalation_manager: &escalation_manager,
            caller: &caller,
            expiration_time_ns: time + liveness,
            currency: &currency,
            bond: &U128(bond),
            identifier: &identifier,
        }
        .emit();

        assertion_id
    }

    /// Internal implementation of dispute_assertion
    /// Called by ft_on_transfer when receiving dispute bond tokens
    fn internal_dispute_assertion(
        &mut self,
        assertion_id: Bytes32,
        disputer: AccountId,
        currency: AccountId,
        bond_amount: u128,
        _caller: AccountId,
    ) {
        let current_time = self.get_current_time();

        let assertion = self
            .assertions
            .get_mut(&assertion_id)
            .expect("Assertion does not exist");

        require!(assertion.disputer.is_none(), "Assertion already disputed");
        require!(
            assertion.expiration_time_ns > current_time,
            "Assertion is expired"
        );
        require!(assertion.currency == currency, "Wrong currency for dispute");
        require!(
            bond_amount == assertion.bond.0,
            "Dispute bond must match assertion bond"
        );

        // Store the identifier before we release the borrow
        let identifier = assertion.identifier;

        // Set the disputer
        assertion.disputer = Some(disputer.clone());

        // Emit event
        Event::AssertionDisputed {
            assertion_id: &assertion_id,
            caller: &env::predecessor_account_id(),
            disputer: &disputer,
        }
        .emit();

        // Escalate to DVM if voting contract is configured
        if let Some(ref voting_contract) = self.voting_contract {
            // Convert identifier to string for DVM
            let identifier_str = String::from_utf8_lossy(&identifier)
                .trim_end_matches('\0')
                .to_string();

            // Use assertion_id as ancillary data so DVM can identify the dispute
            let ancillary_data = assertion_id.to_vec();

            // Call voting.request_price() to create a DVM vote
            let _ = Promise::new(voting_contract.clone())
                .function_call(
                    "request_price".to_string(),
                    near_sdk::serde_json::json!({
                        "identifier": identifier_str,
                        "timestamp": current_time,
                        "ancillary_data": ancillary_data,
                    })
                    .to_string()
                    .into_bytes(),
                    NearToken::from_yoctonear(0),
                    GAS_FOR_DVM_REQUEST,
                )
                .then(
                    Promise::new(env::current_account_id()).function_call(
                        "on_dvm_request_complete".to_string(),
                        near_sdk::serde_json::json!({
                            "assertion_id": assertion_id,
                        })
                        .to_string()
                        .into_bytes(),
                        NearToken::from_yoctonear(0),
                        GAS_FOR_DVM_CALLBACK,
                    ),
                );
        }
    }

    /// Callback after DVM request_price completes
    /// Stores the request_id for later settlement
    #[private]
    pub fn on_dvm_request_complete(
        &mut self,
        assertion_id: Bytes32,
        #[callback_result] request_id_result: Result<CryptoHash, PromiseError>,
    ) {
        match request_id_result {
            Ok(request_id) => {
                // Store the mapping between assertion and DVM request
                self.dispute_requests.insert(assertion_id, request_id);
                self.request_to_assertion.insert(request_id, assertion_id);

                env::log_str(&format!(
                    "DVM request created for assertion. request_id: {:?}",
                    hex::encode(request_id)
                ));
            }
            Err(_) => {
                env::log_str("Failed to create DVM request - dispute will need manual resolution");
            }
        }
    }

    // ========================================================================
    // Settlement Methods
    // ========================================================================

    /// Resolves an assertion. If the assertion has not been disputed, the assertion is resolved
    /// as true and the asserter receives the bond. If disputed, resolution is fetched from DVM.
    pub fn settle_assertion(&mut self, assertion_id: Bytes32) {
        let current_time = self.get_current_time();

        // Get assertion and validate
        let assertion = self
            .assertions
            .get(&assertion_id)
            .expect("Assertion does not exist")
            .clone();

        require!(!assertion.settled, "Assertion already settled");
        require!(
            !assertion.settlement_pending,
            "Settlement already pending payout callback"
        );

        if assertion.disputer.is_none() {
            // No dispute - settle in favor of asserter
            require!(
                assertion.expiration_time_ns <= current_time,
                "Assertion not expired"
            );

            let _ = self.start_settlement_payout(assertion_id, true);
        } else {
            // Disputed - check if DVM has resolved this
            let request_id = self.dispute_requests.get(&assertion_id)
                .expect("Dispute not escalated to DVM - use resolve_disputed_assertion for manual resolution");

            let voting_contract = self
                .voting_contract
                .clone()
                .expect("Voting contract not configured");

            // Query DVM for resolution and settle in callback
            let _ = Promise::new(voting_contract)
                .function_call(
                    "get_price".to_string(),
                    near_sdk::serde_json::json!({
                        "request_id": request_id,
                    })
                    .to_string()
                    .into_bytes(),
                    NearToken::from_yoctonear(0),
                    GAS_FOR_DVM_GET_PRICE,
                )
                .then(
                    Promise::new(env::current_account_id()).function_call(
                        "on_dvm_price_received".to_string(),
                        near_sdk::serde_json::json!({
                            "assertion_id": assertion_id,
                        })
                        .to_string()
                        .into_bytes(),
                        NearToken::from_yoctonear(0),
                        GAS_FOR_SETTLE_CALLBACK,
                    ),
                );
        }
    }

    /// Callback after DVM get_price completes
    /// Settles the disputed assertion based on DVM resolution
    #[private]
    pub fn on_dvm_price_received(
        &mut self,
        assertion_id: Bytes32,
        #[callback_result] price_result: Result<Option<i128>, PromiseError>,
    ) {
        match price_result {
            Ok(Some(price)) => {
                // DVM has resolved - price >= NUMERICAL_TRUE means asserter wins
                let resolution = price >= NUMERICAL_TRUE;
                let _ = self.start_settlement_payout(assertion_id, resolution);
            }
            Ok(None) => {
                env::panic_str("DVM has not resolved this dispute yet");
            }
            Err(_) => {
                env::panic_str("Failed to get DVM resolution");
            }
        }
    }

    /// Settles an assertion and returns the resolution
    /// Equivalent to: function settleAndGetAssertionResult(bytes32 assertionId) external returns (bool)
    pub fn settle_and_get_assertion_result(&mut self, assertion_id: Bytes32) -> bool {
        let assertion = self
            .assertions
            .get(&assertion_id)
            .expect("Assertion does not exist");

        if !assertion.settled {
            self.settle_assertion(assertion_id);
        }

        self.get_assertion_result(assertion_id)
    }

    /// Admin method to manually resolve a disputed assertion
    /// This is a fallback for when DVM escalation fails or is not configured
    /// In normal operation, use settle_assertion which queries DVM automatically
    pub fn resolve_disputed_assertion(
        &mut self,
        assertion_id: Bytes32,
        resolution: bool, // true = asserter wins, false = disputer wins
    ) {
        self.assert_owner();

        let assertion = self
            .assertions
            .get(&assertion_id)
            .expect("Assertion does not exist");

        require!(!assertion.settled, "Assertion already settled");
        require!(
            !assertion.settlement_pending,
            "Settlement already pending payout callback"
        );
        require!(assertion.disputer.is_some(), "Assertion not disputed");

        // Check if DVM has been used - if so, should use settle_assertion instead
        if self.dispute_requests.get(&assertion_id).is_some() {
            env::log_str("Warning: This dispute was escalated to DVM. Consider using settle_assertion instead.");
        }

        let _ = self.start_settlement_payout(assertion_id, resolution);
    }

    /// Retry a failed settlement payout callback.
    /// Can be called after a payout failure to re-attempt token transfer finalization.
    pub fn retry_settlement_payout(&mut self, assertion_id: Bytes32) {
        let assertion = self
            .assertions
            .get(&assertion_id)
            .expect("Assertion does not exist")
            .clone();

        require!(!assertion.settled, "Assertion already settled");
        require!(assertion.settlement_pending, "Settlement is not pending");
        require!(
            !assertion.settlement_in_flight,
            "Settlement payout attempt already in-flight"
        );

        let assertion_mut = self.assertions.get_mut(&assertion_id).unwrap();
        assertion_mut.settlement_in_flight = true;

        Event::AssertionSettlementRetryRequested {
            assertion_id: &assertion_id,
            settlement_resolution: assertion.pending_settlement_resolution,
            caller: &env::predecessor_account_id(),
        }
        .emit();

        let _ = self.dispatch_settlement_payout(assertion_id, assertion.pending_settlement_resolution);
    }

    /// Internal helper to begin async settlement payout flow.
    fn start_settlement_payout(
        &mut self,
        assertion_id: Bytes32,
        resolution: bool, // true = asserter wins, false = disputer wins
    ) -> Promise {
        let assertion = self
            .assertions
            .get(&assertion_id)
            .expect("Assertion does not exist")
            .clone();

        require!(!assertion.settled, "Assertion already settled");
        require!(
            !assertion.settlement_pending,
            "Settlement already pending payout callback"
        );

        let assertion_mut = self.assertions.get_mut(&assertion_id).unwrap();
        assertion_mut.settlement_pending = true;
        assertion_mut.settlement_in_flight = true;
        assertion_mut.pending_settlement_resolution = resolution;

        let (payout_recipient, payout_amount, disputed, _) =
            self.compute_settlement_payout(&assertion, resolution);
        Event::AssertionSettlementPending {
            assertion_id: &assertion_id,
            disputed,
            settlement_resolution: resolution,
            payout_recipient: &payout_recipient,
            payout_amount: &U128(payout_amount),
            settle_caller: &env::predecessor_account_id(),
        }
        .emit();

        self.dispatch_settlement_payout(assertion_id, resolution)
    }

    fn dispatch_settlement_payout(&self, assertion_id: Bytes32, resolution: bool) -> Promise {
        let assertion = self
            .assertions
            .get(&assertion_id)
            .expect("Assertion does not exist")
            .clone();

        let (bond_recipient, bond_recipient_amount, disputed, oracle_fee) =
            self.compute_settlement_payout(&assertion, resolution);

        // Best-effort owner fee transfer; final settlement is gated on recipient payout callback.
        if disputed && oracle_fee > 0 {
            let _ = self.transfer_tokens(assertion.currency.clone(), self.owner.clone(), oracle_fee);
        }

        self.transfer_tokens(
            assertion.currency.clone(),
            bond_recipient,
            bond_recipient_amount,
        )
        .then(
            Promise::new(env::current_account_id()).function_call(
                "on_settlement_payout_complete".to_string(),
                near_sdk::serde_json::json!({
                    "assertion_id": assertion_id,
                })
                .to_string()
                .into_bytes(),
                NearToken::from_yoctonear(0),
                GAS_FOR_SETTLE_CALLBACK,
            ),
        )
    }

    fn compute_settlement_payout(
        &self,
        assertion: &Assertion,
        resolution: bool,
    ) -> (AccountId, u128, bool, u128) {
        if let Some(disputer) = &assertion.disputer {
            let oracle_fee = (self.burned_bond_percentage * assertion.bond.0) / SCALE;
            let bond_recipient_amount = assertion.bond.0 * 2 - oracle_fee;
            let bond_recipient = if resolution {
                assertion.asserter.clone()
            } else {
                disputer.clone()
            };
            (bond_recipient, bond_recipient_amount, true, oracle_fee)
        } else {
            // Undisputed assertions always settle to the asserter.
            (assertion.asserter.clone(), assertion.bond.0, false, 0)
        }
    }

    #[private]
    pub fn on_settlement_payout_complete(
        &mut self,
        assertion_id: Bytes32,
        #[callback_result] payout_result: Result<(), PromiseError>,
    ) {
        let assertion = self
            .assertions
            .get(&assertion_id)
            .expect("Assertion does not exist")
            .clone();

        require!(assertion.settlement_pending, "Settlement is not pending");
        require!(
            assertion.settlement_in_flight,
            "Settlement payout not in-flight"
        );

        match payout_result {
            Ok(()) => {
                let resolution = assertion.pending_settlement_resolution;
                let (bond_recipient, _, disputed, _) =
                    self.compute_settlement_payout(&assertion, resolution);

                let assertion_mut = self.assertions.get_mut(&assertion_id).unwrap();
                assertion_mut.settlement_in_flight = false;
                assertion_mut.settlement_pending = false;
                assertion_mut.settled = true;
                assertion_mut.settlement_resolution = resolution;

                if !assertion.escalation_manager_settings.discard_oracle {
                    if let Some(ref callback_recipient) = assertion.callback_recipient {
                        let _ = self.call_assertion_resolved_callback(
                            callback_recipient.clone(),
                            assertion_id,
                            resolution,
                        );
                    }
                }

                Event::AssertionSettled {
                    assertion_id: &assertion_id,
                    bond_recipient: &bond_recipient,
                    disputed,
                    settlement_resolution: resolution,
                    settle_caller: &env::predecessor_account_id(),
                }
                .emit();
            }
            Err(_) => {
                let resolution = assertion.pending_settlement_resolution;
                let (payout_recipient, payout_amount, disputed, _) =
                    self.compute_settlement_payout(&assertion, resolution);
                let assertion_mut = self.assertions.get_mut(&assertion_id).unwrap();
                assertion_mut.settlement_in_flight = false;

                Event::AssertionSettlementPayoutFailed {
                    assertion_id: &assertion_id,
                    disputed,
                    settlement_resolution: resolution,
                    payout_recipient: &payout_recipient,
                    payout_amount: &U128(payout_amount),
                }
                .emit();

                env::log_str(&format!(
                    "Settlement payout failed for assertion {:?}; remains pending for retry",
                    hex::encode(assertion_id)
                ));
            }
        }
    }

    // ========================================================================
    // Token Transfer Helpers
    // ========================================================================

    /// Transfer NEP-141 tokens
    fn transfer_tokens(&self, token: AccountId, recipient: AccountId, amount: u128) -> Promise {
        Promise::new(token).function_call(
            "ft_transfer".to_string(),
            near_sdk::serde_json::json!({
                "receiver_id": recipient,
                "amount": U128(amount),
            })
            .to_string()
            .into_bytes(),
            NearToken::from_yoctonear(1), // 1 yoctoNEAR for ft_transfer
            GAS_FOR_FT_TRANSFER,
        )
    }

    /// Call assertion resolved callback on recipient contract
    fn call_assertion_resolved_callback(
        &self,
        recipient: AccountId,
        assertion_id: Bytes32,
        asserted_truthfully: bool,
    ) -> Promise {
        // Convert assertion_id to hex string for callback
        let assertion_id_hex = hex::encode(assertion_id);

        Promise::new(recipient).function_call(
            "assertion_resolved_callback".to_string(),
            near_sdk::serde_json::json!({
                "assertion_id": assertion_id_hex,
                "asserted_truthfully": asserted_truthfully,
            })
            .to_string()
            .into_bytes(),
            NearToken::from_yoctonear(0),
            GAS_FOR_CALLBACK,
        )
    }

    // ========================================================================
    // Internal Helpers
    // ========================================================================

    /// Generate unique assertion ID (equivalent to _getId in Solidity)
    fn get_assertion_id(
        &self,
        claim: &Bytes32,
        bond: u128,
        time: u64,
        liveness: u64,
        currency: &AccountId,
        callback_recipient: &Option<AccountId>,
        escalation_manager: &Option<AccountId>,
        identifier: &Bytes32,
        caller: &AccountId,
    ) -> Bytes32 {
        // Create a deterministic hash from all parameters
        let mut data = Vec::new();
        data.extend_from_slice(claim);
        data.extend_from_slice(&bond.to_le_bytes());
        data.extend_from_slice(&time.to_le_bytes());
        data.extend_from_slice(&liveness.to_le_bytes());
        data.extend_from_slice(currency.as_bytes());
        if let Some(cr) = callback_recipient {
            data.extend_from_slice(cr.as_bytes());
        }
        if let Some(em) = escalation_manager {
            data.extend_from_slice(em.as_bytes());
        }
        data.extend_from_slice(identifier);
        data.extend_from_slice(caller.as_bytes());

        env::keccak256(&data)
            .try_into()
            .expect("Hash should be 32 bytes")
    }

    fn assert_owner(&self) {
        require!(
            env::predecessor_account_id() == self.owner,
            "Only owner can call this method"
        );
    }

    /// Get current time in nanoseconds
    fn get_current_time(&self) -> u64 {
        env::block_timestamp()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::AccountId;
    use near_sdk::test_utils::VMContextBuilder;
    use near_sdk::testing_env;

    fn get_context(predecessor: AccountId) -> VMContextBuilder {
        let mut builder = VMContextBuilder::new();
        builder.predecessor_account_id(predecessor);
        builder
    }

    fn get_context_with_time(
        predecessor: AccountId,
        current_account: AccountId,
        block_timestamp: u64,
    ) -> VMContextBuilder {
        let mut builder = VMContextBuilder::new();
        builder.predecessor_account_id(predecessor);
        builder.current_account_id(current_account);
        builder.block_timestamp(block_timestamp);
        builder
    }

    #[test]
    fn test_new() {
        let owner: AccountId = "owner.near".parse().unwrap();
        let currency: AccountId = "usdc.near".parse().unwrap();

        let context = get_context(owner.clone());
        testing_env!(context.build());

        let contract = NestOptimisticOracle::new(
            owner.clone(),
            currency.clone(),
            None,
            None,
            None, // No voting contract for basic test
        );

        assert_eq!(contract.default_identifier(), DEFAULT_IDENTIFIER);
        assert_eq!(contract.default_liveness().0, DEFAULT_LIVENESS_NS);
        assert_eq!(contract.default_currency(), currency);
        assert!(contract.is_identifier_supported(DEFAULT_IDENTIFIER));
    }

    #[test]
    fn test_new_with_voting_contract() {
        let owner: AccountId = "owner.near".parse().unwrap();
        let currency: AccountId = "usdc.near".parse().unwrap();
        let voting: AccountId = "voting.near".parse().unwrap();

        let context = get_context(owner.clone());
        testing_env!(context.build());

        let contract = NestOptimisticOracle::new(
            owner.clone(),
            currency.clone(),
            None,
            None,
            Some(voting.clone()),
        );

        assert_eq!(contract.get_voting_contract(), Some(voting));
    }

    #[test]
    fn test_get_minimum_bond() {
        let owner: AccountId = "owner.near".parse().unwrap();
        let currency: AccountId = "usdc.near".parse().unwrap();

        let context = get_context(owner.clone());
        testing_env!(context.build());

        let mut contract =
            NestOptimisticOracle::new(owner.clone(), currency.clone(), None, None, None);

        // Currency not whitelisted yet
        assert_eq!(contract.get_minimum_bond(currency.clone()).0, 0);

        // Whitelist with final_fee = 1e18 (1 token)
        contract.whitelist_currency(currency.clone(), U128(SCALE));

        // min_bond = final_fee * 1e18 / burned_bond_percentage
        // = 1e18 * 1e18 / 0.5e18 = 2e18
        let expected_min_bond = 2 * SCALE;
        assert_eq!(contract.get_minimum_bond(currency).0, expected_min_bond);
    }

    #[test]
    fn test_set_voting_contract() {
        let owner: AccountId = "owner.near".parse().unwrap();
        let currency: AccountId = "usdc.near".parse().unwrap();
        let voting: AccountId = "voting.near".parse().unwrap();

        let context = get_context(owner.clone());
        testing_env!(context.build());

        let mut contract =
            NestOptimisticOracle::new(owner.clone(), currency.clone(), None, None, None);

        assert_eq!(contract.get_voting_contract(), None);

        contract.set_voting_contract(voting.clone());

        assert_eq!(contract.get_voting_contract(), Some(voting));
    }

    #[test]
    fn test_settlement_payout_success_finalizes_assertion() {
        let owner: AccountId = "owner.near".parse().unwrap();
        let oracle: AccountId = "oracle.near".parse().unwrap();
        let asserter: AccountId = "asserter.near".parse().unwrap();
        let caller: AccountId = "caller.near".parse().unwrap();
        let currency: AccountId = "usdc.near".parse().unwrap();

        testing_env!(get_context_with_time(owner.clone(), oracle.clone(), 1).build());
        let mut contract =
            NestOptimisticOracle::new(owner.clone(), currency.clone(), None, None, None);
        contract.whitelist_currency(currency.clone(), U128(1));

        let assertion_id = contract.internal_assert_truth(
            [1u8; 32],
            asserter.clone(),
            None,
            None,
            Some(1),
            Some(0),
            currency.clone(),
            10,
            None,
            None,
            None,
            caller,
        );

        testing_env!(get_context_with_time(asserter.clone(), oracle.clone(), 5).build());
        contract.settle_assertion(assertion_id);

        let pending = contract.get_assertion(assertion_id).unwrap();
        assert!(!pending.settled);
        assert!(pending.settlement_pending);
        assert!(pending.settlement_in_flight);

        testing_env!(get_context_with_time(oracle.clone(), oracle.clone(), 6).build());
        contract.on_settlement_payout_complete(assertion_id, Ok(()));

        let finalized = contract.get_assertion(assertion_id).unwrap();
        assert!(finalized.settled);
        assert!(!finalized.settlement_pending);
        assert!(!finalized.settlement_in_flight);
        assert!(finalized.settlement_resolution);
    }

    #[test]
    fn test_settlement_payout_failure_stays_pending_and_retryable() {
        let owner: AccountId = "owner.near".parse().unwrap();
        let oracle: AccountId = "oracle.near".parse().unwrap();
        let asserter: AccountId = "asserter.near".parse().unwrap();
        let caller: AccountId = "caller.near".parse().unwrap();
        let currency: AccountId = "usdc.near".parse().unwrap();

        testing_env!(get_context_with_time(owner.clone(), oracle.clone(), 1).build());
        let mut contract =
            NestOptimisticOracle::new(owner.clone(), currency.clone(), None, None, None);
        contract.whitelist_currency(currency.clone(), U128(1));

        let assertion_id = contract.internal_assert_truth(
            [2u8; 32],
            asserter.clone(),
            None,
            None,
            Some(1),
            Some(0),
            currency.clone(),
            10,
            None,
            None,
            None,
            caller,
        );

        testing_env!(get_context_with_time(asserter.clone(), oracle.clone(), 5).build());
        contract.settle_assertion(assertion_id);

        testing_env!(get_context_with_time(oracle.clone(), oracle.clone(), 6).build());
        contract.on_settlement_payout_complete(assertion_id, Err(PromiseError::Failed));

        let failed = contract.get_assertion(assertion_id).unwrap();
        assert!(!failed.settled);
        assert!(failed.settlement_pending);
        assert!(!failed.settlement_in_flight);

        testing_env!(get_context_with_time(asserter, oracle, 7).build());
        contract.retry_settlement_payout(assertion_id);

        let retried = contract.get_assertion(assertion_id).unwrap();
        assert!(retried.settlement_pending);
        assert!(retried.settlement_in_flight);
    }

    #[test]
    fn test_dispute_requires_exact_bond_amount() {
        let owner: AccountId = "owner.near".parse().unwrap();
        let oracle: AccountId = "oracle.near".parse().unwrap();
        let asserter: AccountId = "asserter.near".parse().unwrap();
        let disputer: AccountId = "disputer.near".parse().unwrap();
        let caller: AccountId = "caller.near".parse().unwrap();
        let currency: AccountId = "usdc.near".parse().unwrap();

        testing_env!(get_context_with_time(owner.clone(), oracle.clone(), 1).build());
        let mut contract =
            NestOptimisticOracle::new(owner.clone(), currency.clone(), None, None, None);
        contract.whitelist_currency(currency.clone(), U128(1));

        let assertion_id = contract.internal_assert_truth(
            [3u8; 32],
            asserter,
            None,
            None,
            Some(100),
            Some(0),
            currency.clone(),
            10,
            None,
            None,
            None,
            caller.clone(),
        );

        testing_env!(get_context_with_time(caller, oracle, 10).build());
        contract.internal_dispute_assertion(
            assertion_id,
            disputer.clone(),
            currency,
            10,
            disputer.clone(),
        );

        let assertion = contract.get_assertion(assertion_id).unwrap();
        assert_eq!(assertion.disputer, Some(disputer));
    }

    #[test]
    #[should_panic(expected = "Dispute bond must match assertion bond")]
    fn test_dispute_rejects_overpayment_bond_amount() {
        let owner: AccountId = "owner.near".parse().unwrap();
        let oracle: AccountId = "oracle.near".parse().unwrap();
        let asserter: AccountId = "asserter.near".parse().unwrap();
        let disputer: AccountId = "disputer.near".parse().unwrap();
        let caller: AccountId = "caller.near".parse().unwrap();
        let currency: AccountId = "usdc.near".parse().unwrap();

        testing_env!(get_context_with_time(owner.clone(), oracle.clone(), 1).build());
        let mut contract =
            NestOptimisticOracle::new(owner.clone(), currency.clone(), None, None, None);
        contract.whitelist_currency(currency.clone(), U128(1));

        let assertion_id = contract.internal_assert_truth(
            [4u8; 32],
            asserter,
            None,
            None,
            Some(100),
            Some(0),
            currency.clone(),
            10,
            None,
            None,
            None,
            caller.clone(),
        );

        testing_env!(get_context_with_time(caller, oracle, 10).build());
        contract.internal_dispute_assertion(assertion_id, disputer.clone(), currency, 11, disputer);
    }
}
