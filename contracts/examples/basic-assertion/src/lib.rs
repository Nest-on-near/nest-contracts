use near_sdk::{
    env,
    json_types::U128,
    near, require,
    serde::{Deserialize, Serialize},
    AccountId, Gas, NearToken, PanicOnDefault, Promise,
};
use oracle_types::types::Bytes32;

const GAS_FOR_FT_TRANSFER_CALL: Gas = Gas::from_tgas(50);

/// Message format for asserting truth via ft_transfer_call to the oracle
#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct AssertTruthArgs {
    pub claim: Bytes32,
    pub asserter: AccountId,
    pub callback_recipient: Option<AccountId>,
    pub escalation_manager: Option<AccountId>,
    pub liveness_ns: Option<u64>,
    pub identifier: Option<Bytes32>,
    pub domain_id: Option<Bytes32>,
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
#[serde(tag = "action")]
pub enum OracleMsg {
    AssertTruth(AssertTruthArgs),
}

/// Message that users send to this contract via ft_transfer_call
#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct UserAssertionMsg {
    /// The claim string to assert
    pub claim: String,
}

/// Example contract that demonstrates making assertions to the Nest Optimistic Oracle
///
/// Users send wNEAR to this contract via ft_transfer_call with their claim.
/// The contract forwards the tokens to the oracle to create the assertion.
#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct AssertionExample {
    /// The optimistic oracle contract address
    oracle: AccountId,
    /// The NEP-141 token used for bonds
    bond_token: AccountId,
    /// Minimum bond amount required
    min_bond: U128,
    /// Stores the last assertion ID made via this contract
    last_assertion_id: Option<Bytes32>,
    /// Stores the last claim string
    last_claim: Option<String>,
    /// Stores the result of the last resolved assertion
    last_assertion_result: Option<bool>,
}

#[near]
impl AssertionExample {
    /// Initialize the contract
    #[init]
    pub fn new(oracle: AccountId, bond_token: AccountId, min_bond: U128) -> Self {
        Self {
            oracle,
            bond_token,
            min_bond,
            last_assertion_id: None,
            last_claim: None,
            last_assertion_result: None,
        }
    }

    /// NEP-141 receiver - users send tokens here with their claim
    ///
    /// Usage: User calls ft_transfer_call on wrap.testnet with:
    /// - receiver_id: this contract
    /// - amount: bond amount (must be >= min_bond)
    /// - msg: JSON with claim string, e.g. {"claim": "Today is 18th January"}
    ///
    /// Returns "0" to indicate all tokens were used (no refund)
    pub fn ft_on_transfer(&mut self, sender_id: AccountId, amount: U128, msg: String) -> Promise {
        let token = env::predecessor_account_id();

        // Verify it's the correct bond token
        require!(
            token == self.bond_token,
            "Only accepts the configured bond token"
        );

        // Verify minimum bond
        require!(amount.0 >= self.min_bond.0, "Bond amount too low");

        // Parse the user's message
        let user_msg: UserAssertionMsg = serde_json::from_str(&msg)
            .expect("Invalid message format. Expected: {\"claim\": \"your claim\"}");

        // Hash the claim string to get 32-byte claim
        let claim_bytes: Bytes32 = env::keccak256(user_msg.claim.as_bytes())
            .try_into()
            .expect("keccak256 should produce 32 bytes");

        // Store for reference
        self.last_claim = Some(user_msg.claim.clone());

        env::log_str(&format!(
            "User {} asserting claim: {}",
            sender_id, user_msg.claim
        ));

        // Build the message for the oracle
        let oracle_msg = OracleMsg::AssertTruth(AssertTruthArgs {
            claim: claim_bytes,
            asserter: sender_id.clone(), // User gets the bond back on settlement
            callback_recipient: Some(env::current_account_id()), // This contract gets notified
            escalation_manager: None,
            liveness_ns: None,
            identifier: None,
            domain_id: None,
        });

        // Forward the tokens to the oracle
        Promise::new(self.bond_token.clone()).function_call(
            "ft_transfer_call".to_string(),
            serde_json::json!({
                "receiver_id": self.oracle,
                "amount": amount,
                "msg": serde_json::to_string(&oracle_msg).unwrap(),
            })
            .to_string()
            .into_bytes(),
            NearToken::from_yoctonear(1),
            GAS_FOR_FT_TRANSFER_CALL,
        )
    }

    /// Callback function called by the oracle when an assertion is resolved
    pub fn assertion_resolved_callback(&mut self, assertion_id: String, asserted_truthfully: bool) {
        require!(
            env::predecessor_account_id() == self.oracle,
            "Only oracle can call this callback"
        );

        let assertion_id_bytes: Bytes32 = hex::decode(&assertion_id)
            .expect("Invalid assertion_id hex")
            .try_into()
            .expect("assertion_id must be 32 bytes");

        self.last_assertion_id = Some(assertion_id_bytes);
        self.last_assertion_result = Some(asserted_truthfully);

        env::log_str(&format!(
            "Assertion {} resolved: {}",
            assertion_id,
            if asserted_truthfully { "TRUE" } else { "FALSE" }
        ));
    }

    // ========================================================================
    // View Methods
    // ========================================================================

    pub fn get_oracle(&self) -> AccountId {
        self.oracle.clone()
    }

    pub fn get_bond_token(&self) -> AccountId {
        self.bond_token.clone()
    }

    pub fn get_min_bond(&self) -> U128 {
        self.min_bond
    }

    pub fn get_last_assertion_id(&self) -> Option<String> {
        self.last_assertion_id.map(|id| hex::encode(id))
    }

    pub fn get_last_claim(&self) -> Option<String> {
        self.last_claim.clone()
    }

    pub fn get_last_assertion_result(&self) -> Option<bool> {
        self.last_assertion_result
    }
}
