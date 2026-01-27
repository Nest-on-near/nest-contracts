use near_sdk::{
    env, near, require, AccountId, PanicOnDefault, Promise, Gas, NearToken,
    store::LookupMap,
    json_types::{U64, U128},
    serde::{Deserialize, Serialize},
};

/// Gas for cross-contract calls
const GAS_FOR_FT_TRANSFER: Gas = Gas::from_tgas(10);
const GAS_FOR_CALLBACK: Gas = Gas::from_tgas(5);

use oracle_types::{
    types::Bytes32,
    interfaces::{Assertion, EscalationManagerSettings, WhitelistedCurrency},
    events::Event,
};

// ============================================================================
// Constants
// ============================================================================

/// Default identifier for assertions (equivalent to "ASSERT_TRUTH" in UMA)
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
    /// Identifier for the assertion (if None, uses default)
    pub identifier: Option<Bytes32>,
    /// Optional domain ID for grouping assertions
    pub domain_id: Option<Bytes32>,
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
    /// Equivalent to: mapping(address => WhitelistedCurrency) cachedCurrencies
    cached_currencies: LookupMap<AccountId, WhitelistedCurrency>,

    /// Cached identifiers that are approved for use
    /// Equivalent to: mapping(bytes32 => bool) cachedIdentifiers
    cached_identifiers: LookupMap<Bytes32, bool>,

    /// All assertions made by the Optimistic Oracle
    /// Equivalent to: mapping(bytes32 => Assertion) assertions
    assertions: LookupMap<Bytes32, Assertion>,
}

// ============================================================================
// Contract Implementation
// ============================================================================

#[near]
impl NestOptimisticOracle {
    /// Initialize the contract
    /// Equivalent to constructor in OptimisticOracleV3.sol
    #[init]
    pub fn new(
        owner: AccountId,
        default_currency: AccountId,
        default_liveness_ns: Option<U64>,
        burned_bond_percentage: Option<U128>,
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
        };

        // Cache the default identifier as approved
        contract.cached_identifiers.insert(DEFAULT_IDENTIFIER, true);

        // Emit admin properties set event
        Event::AdminPropertiesSet {
            default_currency: &default_currency,
            default_liveness_ns: liveness,
            burned_bond_percentage: burn_pct,
        }.emit();

        contract
    }

    // ========================================================================
    // View Methods
    // ========================================================================

    /// Returns the default identifier used by the Optimistic Oracle
    /// Equivalent to: function defaultIdentifier() external view returns (bytes32)
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
    /// Equivalent to: function getAssertion(bytes32 assertionId) external view returns (Assertion memory)
    pub fn get_assertion(&self, assertion_id: Bytes32) -> Option<Assertion> {
        self.assertions.get(&assertion_id).cloned()
    }

    /// Returns the minimum bond amount required to make an assertion
    /// min_bond = final_fee * 1e18 / burned_bond_percentage
    /// Equivalent to: function getMinimumBond(address currency) public view returns (uint256)
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
    /// Equivalent to: function getAssertionResult(bytes32 assertionId) public view returns (bool)
    pub fn get_assertion_result(&self, assertion_id: Bytes32) -> bool {
        let assertion = self.assertions.get(&assertion_id)
            .expect("Assertion does not exist");

        // Return early if not using answer from resolved dispute (discardOracle = true)
        if assertion.disputer.is_some()
            && assertion.escalation_manager_settings.discard_oracle
        {
            return false;
        }

        require!(assertion.settled, "Assertion not settled");
        assertion.settlement_resolution
    }

    /// Check if an identifier is cached/approved
    pub fn is_identifier_supported(&self, identifier: Bytes32) -> bool {
        self.cached_identifiers.get(&identifier).copied().unwrap_or(false)
    }

    /// Check if a currency is whitelisted
    pub fn is_currency_whitelisted(&self, currency: AccountId) -> bool {
        self.cached_currencies
            .get(&currency)
            .map(|c| c.is_whitelisted)
            .unwrap_or(false)
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

        require!(burned_bond_percentage.0 <= SCALE, "Burned bond percentage > 100%");
        require!(burned_bond_percentage.0 > 0, "Burned bond percentage is 0");

        self.default_currency = default_currency.clone();
        self.default_liveness_ns = default_liveness_ns.0;
        self.burned_bond_percentage = burned_bond_percentage.0;

        Event::AdminPropertiesSet {
            default_currency: &default_currency,
            default_liveness_ns: default_liveness_ns.0,
            burned_bond_percentage: burned_bond_percentage.0,
        }.emit();
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

    // ========================================================================
    // NEP-141 Receiver (for bonding)
    // ========================================================================

    /// Called by NEP-141 token contract when tokens are transferred via ft_transfer_call
    /// Returns the amount of tokens to refund (0 if all tokens are used)
    pub fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> U128 {
        let currency = env::predecessor_account_id();

        // Parse the message to determine the action
        let parsed_msg: FtOnTransferMsg = near_sdk::serde_json::from_str(&msg)
            .expect("Invalid ft_on_transfer message format");

        match parsed_msg {
            FtOnTransferMsg::AssertTruth(args) => {
                let assertion_id = self.internal_assert_truth(
                    args.claim,
                    args.asserter,
                    args.callback_recipient,
                    args.escalation_manager,
                    args.liveness_ns.map(|l| l.0),
                    currency,
                    amount.0,
                    args.identifier,
                    args.domain_id,
                    sender_id,
                );
                // All tokens used for bond, no refund
                U128(0)
            }
            FtOnTransferMsg::DisputeAssertion { assertion_id, disputer } => {
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
        currency: AccountId,
        bond: u128,
        identifier: Option<Bytes32>,
        domain_id: Option<Bytes32>,
        caller: AccountId,
    ) -> Bytes32 {
        let time = self.get_current_time();
        let liveness = liveness_ns.unwrap_or(self.default_liveness_ns);
        let identifier = identifier.unwrap_or(DEFAULT_IDENTIFIER);
        let domain_id = domain_id.unwrap_or([0u8; 32]);

        // Generate unique assertion ID
        let assertion_id = self.get_assertion_id(
            &claim,
            bond,
            time,
            liveness,
            &currency,
            &callback_recipient,
            &escalation_manager,
            &identifier,
            &caller,
        );

        // Validations (equivalent to Solidity requires)
        require!(
            self.assertions.get(&assertion_id).is_none(),
            "Assertion already exists"
        );
        require!(
            self.cached_identifiers.get(&identifier).copied().unwrap_or(false),
            "Unsupported identifier"
        );
        require!(
            self.cached_currencies.get(&currency).map(|c| c.is_whitelisted).unwrap_or(false),
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
            currency: currency.clone(),
            expiration_time_ns: time + liveness,
            settlement_resolution: false,
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
        }.emit();

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

        let assertion = self.assertions.get_mut(&assertion_id)
            .expect("Assertion does not exist");

        require!(assertion.disputer.is_none(), "Assertion already disputed");
        require!(
            assertion.expiration_time_ns > current_time,
            "Assertion is expired"
        );
        require!(
            assertion.currency == currency,
            "Wrong currency for dispute"
        );
        require!(
            bond_amount >= assertion.bond.0,
            "Dispute bond too low"
        );

        // Set the disputer
        assertion.disputer = Some(disputer.clone());

        // Emit event
        Event::AssertionDisputed {
            assertion_id: &assertion_id,
            caller: &env::predecessor_account_id(),
            disputer: &disputer,
        }.emit();

        // Note: In Phase 1, we don't have DVM/escalation manager for dispute resolution
        // The dispute will need to be resolved manually or via a future oracle integration
    }

    // ========================================================================
    // Settlement Methods
    // ========================================================================

    /// Resolves an assertion. If the assertion has not been disputed, the assertion is resolved
    /// as true and the asserter receives the bond. If disputed, resolution depends on oracle result.
    /// Equivalent to: function settleAssertion(bytes32 assertionId) public
    pub fn settle_assertion(&mut self, assertion_id: Bytes32) {
        let current_time = self.get_current_time();

        // Get assertion and validate
        let assertion = self.assertions.get(&assertion_id)
            .expect("Assertion does not exist")
            .clone();

        require!(!assertion.settled, "Assertion already settled");

        if assertion.disputer.is_none() {
            // No dispute - settle in favor of asserter
            require!(
                assertion.expiration_time_ns <= current_time,
                "Assertion not expired"
            );

            // Update assertion state
            let assertion_mut = self.assertions.get_mut(&assertion_id).unwrap();
            assertion_mut.settled = true;
            assertion_mut.settlement_resolution = true;

            // Transfer bond back to asserter
            let _ = self.transfer_tokens(
                assertion.currency.clone(),
                assertion.asserter.clone(),
                assertion.bond.0,
            );

            // Callback if configured
            if let Some(ref callback_recipient) = assertion.callback_recipient {
                let _ = self.call_assertion_resolved_callback(
                    callback_recipient.clone(),
                    assertion_id,
                    true,
                );
            }

            // Emit event
            Event::AssertionSettled {
                assertion_id: &assertion_id,
                bond_recipient: &assertion.asserter,
                disputed: false,
                settlement_resolution: true,
                settle_caller: &env::predecessor_account_id(),
            }.emit();
        } else {
            // Disputed - Phase 1: For now, we need manual resolution by owner
            // In a full implementation, this would query the DVM/escalation manager
            env::panic_str("Disputed assertions require manual resolution in Phase 1");
        }
    }

    /// Settles an assertion and returns the resolution
    /// Equivalent to: function settleAndGetAssertionResult(bytes32 assertionId) external returns (bool)
    pub fn settle_and_get_assertion_result(&mut self, assertion_id: Bytes32) -> bool {
        let assertion = self.assertions.get(&assertion_id)
            .expect("Assertion does not exist");

        if !assertion.settled {
            self.settle_assertion(assertion_id);
        }

        self.get_assertion_result(assertion_id)
    }

    /// Admin method to manually resolve a disputed assertion (Phase 1 only)
    /// In production, this would be replaced by DVM/escalation manager integration
    pub fn resolve_disputed_assertion(
        &mut self,
        assertion_id: Bytes32,
        resolution: bool, // true = asserter wins, false = disputer wins
    ) {
        self.assert_owner();

        let assertion = self.assertions.get(&assertion_id)
            .expect("Assertion does not exist")
            .clone();

        require!(!assertion.settled, "Assertion already settled");
        require!(assertion.disputer.is_some(), "Assertion not disputed");

        let disputer = assertion.disputer.clone().unwrap();

        // Update assertion state
        let assertion_mut = self.assertions.get_mut(&assertion_id).unwrap();
        assertion_mut.settled = true;
        assertion_mut.settlement_resolution = resolution;

        // Calculate fee and bond distribution
        let oracle_fee = (self.burned_bond_percentage * assertion.bond.0) / SCALE;
        let bond_recipient_amount = assertion.bond.0 * 2 - oracle_fee;

        let bond_recipient = if resolution {
            assertion.asserter.clone()
        } else {
            disputer.clone()
        };

        // Transfer oracle fee to owner (in production, this goes to Store contract)
        let _ = self.transfer_tokens(
            assertion.currency.clone(),
            self.owner.clone(),
            oracle_fee,
        );

        // Transfer remaining bonds to winner
        let _ = self.transfer_tokens(
            assertion.currency.clone(),
            bond_recipient.clone(),
            bond_recipient_amount,
        );

        // Callback if configured and not discarding oracle
        if !assertion.escalation_manager_settings.discard_oracle {
            if let Some(ref callback_recipient) = assertion.callback_recipient {
                let _ = self.call_assertion_resolved_callback(
                    callback_recipient.clone(),
                    assertion_id,
                    resolution,
                );
            }
        }

        // Emit event
        Event::AssertionSettled {
            assertion_id: &assertion_id,
            bond_recipient: &bond_recipient,
            disputed: true,
            settlement_resolution: resolution,
            settle_caller: &env::predecessor_account_id(),
        }.emit();
    }

    // ========================================================================
    // Token Transfer Helpers
    // ========================================================================

    /// Transfer NEP-141 tokens
    fn transfer_tokens(&self, token: AccountId, recipient: AccountId, amount: u128) -> Promise {
        Promise::new(token)
            .function_call(
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

        Promise::new(recipient)
            .function_call(
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

        env::keccak256(&data).try_into().expect("Hash should be 32 bytes")
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
    use near_sdk::test_utils::VMContextBuilder;
    use near_sdk::testing_env;

    fn get_context(predecessor: AccountId) -> VMContextBuilder {
        let mut builder = VMContextBuilder::new();
        builder.predecessor_account_id(predecessor);
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
        );

        assert_eq!(contract.default_identifier(), DEFAULT_IDENTIFIER);
        assert_eq!(contract.default_liveness().0, DEFAULT_LIVENESS_NS);
        assert_eq!(contract.default_currency(), currency);
        assert!(contract.is_identifier_supported(DEFAULT_IDENTIFIER));
    }

    #[test]
    fn test_get_minimum_bond() {
        let owner: AccountId = "owner.near".parse().unwrap();
        let currency: AccountId = "usdc.near".parse().unwrap();

        let context = get_context(owner.clone());
        testing_env!(context.build());

        let mut contract = NestOptimisticOracle::new(
            owner.clone(),
            currency.clone(),
            None,
            None,
        );

        // Currency not whitelisted yet
        assert_eq!(contract.get_minimum_bond(currency.clone()).0, 0);

        // Whitelist with final_fee = 1e18 (1 token)
        contract.whitelist_currency(currency.clone(), U128(SCALE));

        // min_bond = final_fee * 1e18 / burned_bond_percentage
        // = 1e18 * 1e18 / 0.5e18 = 2e18
        let expected_min_bond = 2 * SCALE;
        assert_eq!(contract.get_minimum_bond(currency).0, expected_min_bond);
    }
}
