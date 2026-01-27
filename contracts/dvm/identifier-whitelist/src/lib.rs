use near_sdk::store::LookupSet;
use near_sdk::{env, near, require, PanicOnDefault};

/// IdentifierWhitelist - Manages approved price identifiers for the oracle.
///
/// Price identifiers are strings that describe what kind of data is being requested.
/// Examples: "YES_OR_NO_QUERY", "NUMERICAL", "ETH/USD", "BTC/USD"
///
/// Only whitelisted identifiers can be used to create price requests or assertions.
/// This prevents spam and ensures the oracle only handles supported query types.
#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct IdentifierWhitelist {
    /// Contract owner - can add/remove identifiers
    owner: near_sdk::AccountId,

    /// Set of approved identifiers
    supported_identifiers: LookupSet<String>,
}

/// Event emitted when an identifier is added to the whitelist
#[near(serializers = [json])]
pub struct SupportedIdentifierAdded {
    pub identifier: String,
}

/// Event emitted when an identifier is removed from the whitelist
#[near(serializers = [json])]
pub struct SupportedIdentifierRemoved {
    pub identifier: String,
}

#[near]
impl IdentifierWhitelist {
    /// Initialize the IdentifierWhitelist contract.
    ///
    /// # Arguments
    /// * `owner` - Account that can add/remove identifiers
    #[init]
    pub fn new(owner: near_sdk::AccountId) -> Self {
        Self {
            owner,
            supported_identifiers: LookupSet::new(b"i"),
        }
    }

    // ==================== Identifier Management ====================

    /// Add an identifier to the whitelist.
    /// Only the owner can call this method.
    ///
    /// # Arguments
    /// * `identifier` - The identifier string to whitelist
    pub fn add_supported_identifier(&mut self, identifier: String) {
        self.assert_owner();
        require!(!identifier.is_empty(), "Identifier cannot be empty");

        if self.supported_identifiers.insert(identifier.clone()) {
            // Emit event only if it was newly added
            let event = SupportedIdentifierAdded { identifier };
            let event_json = near_sdk::serde_json::to_string(&event).unwrap();
            env::log_str(&format!(
                "EVENT_JSON:{{\"standard\":\"identifier_whitelist\",\"version\":\"1.0.0\",\"event\":\"supported_identifier_added\",\"data\":{}}}",
                event_json
            ));
        }
    }

    /// Remove an identifier from the whitelist.
    /// Only the owner can call this method.
    ///
    /// # Arguments
    /// * `identifier` - The identifier string to remove
    pub fn remove_supported_identifier(&mut self, identifier: String) {
        self.assert_owner();

        if self.supported_identifiers.remove(&identifier) {
            // Emit event only if it was actually removed
            let event = SupportedIdentifierRemoved { identifier };
            let event_json = near_sdk::serde_json::to_string(&event).unwrap();
            env::log_str(&format!(
                "EVENT_JSON:{{\"standard\":\"identifier_whitelist\",\"version\":\"1.0.0\",\"event\":\"supported_identifier_removed\",\"data\":{}}}",
                event_json
            ));
        }
    }

    /// Check if an identifier is whitelisted.
    ///
    /// # Arguments
    /// * `identifier` - The identifier string to check
    ///
    /// # Returns
    /// True if the identifier is supported
    pub fn is_identifier_supported(&self, identifier: String) -> bool {
        self.supported_identifiers.contains(&identifier)
    }

    // ==================== Role Management ====================

    /// Transfer ownership to a new account.
    /// Only the current owner can call this method.
    ///
    /// # Arguments
    /// * `new_owner` - The new owner account
    pub fn set_owner(&mut self, new_owner: near_sdk::AccountId) {
        self.assert_owner();
        self.owner = new_owner;
    }

    /// Get the current owner.
    pub fn get_owner(&self) -> near_sdk::AccountId {
        self.owner.clone()
    }

    // ==================== Internal ====================

    fn assert_owner(&self) {
        require!(
            env::predecessor_account_id() == self.owner,
            "Only owner can call this method"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::testing_env;

    fn get_context(predecessor: near_sdk::AccountId) -> VMContextBuilder {
        let mut builder = VMContextBuilder::new();
        builder.predecessor_account_id(predecessor);
        builder
    }

    #[test]
    fn test_new() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let contract = IdentifierWhitelist::new(accounts(0));
        assert_eq!(contract.get_owner(), accounts(0));
    }

    #[test]
    fn test_add_supported_identifier() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = IdentifierWhitelist::new(accounts(0));

        assert!(!contract.is_identifier_supported("YES_OR_NO_QUERY".to_string()));

        contract.add_supported_identifier("YES_OR_NO_QUERY".to_string());

        assert!(contract.is_identifier_supported("YES_OR_NO_QUERY".to_string()));
    }

    #[test]
    fn test_remove_supported_identifier() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = IdentifierWhitelist::new(accounts(0));

        contract.add_supported_identifier("YES_OR_NO_QUERY".to_string());
        assert!(contract.is_identifier_supported("YES_OR_NO_QUERY".to_string()));

        contract.remove_supported_identifier("YES_OR_NO_QUERY".to_string());
        assert!(!contract.is_identifier_supported("YES_OR_NO_QUERY".to_string()));
    }

    #[test]
    fn test_multiple_identifiers() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = IdentifierWhitelist::new(accounts(0));

        contract.add_supported_identifier("YES_OR_NO_QUERY".to_string());
        contract.add_supported_identifier("NUMERICAL".to_string());
        contract.add_supported_identifier("ETH/USD".to_string());

        assert!(contract.is_identifier_supported("YES_OR_NO_QUERY".to_string()));
        assert!(contract.is_identifier_supported("NUMERICAL".to_string()));
        assert!(contract.is_identifier_supported("ETH/USD".to_string()));
        assert!(!contract.is_identifier_supported("UNKNOWN".to_string()));
    }

    #[test]
    fn test_add_duplicate_identifier() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = IdentifierWhitelist::new(accounts(0));

        contract.add_supported_identifier("YES_OR_NO_QUERY".to_string());
        // Adding again should not panic
        contract.add_supported_identifier("YES_OR_NO_QUERY".to_string());

        assert!(contract.is_identifier_supported("YES_OR_NO_QUERY".to_string()));
    }

    #[test]
    fn test_remove_nonexistent_identifier() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = IdentifierWhitelist::new(accounts(0));

        // Removing non-existent identifier should not panic
        contract.remove_supported_identifier("NONEXISTENT".to_string());
    }

    #[test]
    #[should_panic(expected = "Only owner can call this method")]
    fn test_add_identifier_unauthorized() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = IdentifierWhitelist::new(accounts(0));

        // Try to add as non-owner
        testing_env!(get_context(accounts(1)).build());
        contract.add_supported_identifier("YES_OR_NO_QUERY".to_string());
    }

    #[test]
    #[should_panic(expected = "Only owner can call this method")]
    fn test_remove_identifier_unauthorized() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = IdentifierWhitelist::new(accounts(0));
        contract.add_supported_identifier("YES_OR_NO_QUERY".to_string());

        // Try to remove as non-owner
        testing_env!(get_context(accounts(1)).build());
        contract.remove_supported_identifier("YES_OR_NO_QUERY".to_string());
    }

    #[test]
    #[should_panic(expected = "Identifier cannot be empty")]
    fn test_add_empty_identifier() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = IdentifierWhitelist::new(accounts(0));
        contract.add_supported_identifier("".to_string());
    }

    #[test]
    fn test_transfer_ownership() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = IdentifierWhitelist::new(accounts(0));

        contract.set_owner(accounts(1));
        assert_eq!(contract.get_owner(), accounts(1));

        // New owner can add identifiers
        testing_env!(get_context(accounts(1)).build());
        contract.add_supported_identifier("YES_OR_NO_QUERY".to_string());
        assert!(contract.is_identifier_supported("YES_OR_NO_QUERY".to_string()));
    }
}
