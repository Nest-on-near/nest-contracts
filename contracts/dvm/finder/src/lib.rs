use near_sdk::store::LookupMap;
use near_sdk::{env, near, require, AccountId, PanicOnDefault};


/// Well-known interface names used by the DVM system.
/// These are string constants that get hashed/used as keys.
pub mod interface_names {
    pub const ORACLE: &str = "Oracle";
    pub const STORE: &str = "Store";
    pub const IDENTIFIER_WHITELIST: &str = "IdentifierWhitelist";
    pub const REGISTRY: &str = "Registry";
    pub const VOTING_TOKEN: &str = "VotingToken";
    pub const SLASHING_LIBRARY: &str = "SlashingLibrary";
}

/// Finder - Service discovery registry for DVM contracts.
///
/// Maps interface names (strings) to contract account IDs.
/// Inspired by UMA's Finder contract.
///
/// This allows contracts to look up other DVM contracts dynamically,
/// enabling upgradability without changing dependent contracts.
#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Finder {
    /// Contract owner - can update interface implementations
    owner: AccountId,

    /// Mapping from interface name to implementation contract address
    interfaces: LookupMap<String, AccountId>,
}

/// Event emitted when an interface implementation is changed
#[near(serializers = [json])]
pub struct InterfaceImplementationChanged {
    pub interface_name: String,
    pub new_implementation: AccountId,
}

#[near]
impl Finder {
    /// Initialize the Finder contract.
    ///
    /// # Arguments
    /// * `owner` - Account that can update interface implementations
    #[init]
    pub fn new(owner: AccountId) -> Self {
        Self {
            owner,
            interfaces: LookupMap::new(b"i"),
        }
    }

    /// Change the implementation address for an interface.
    /// Only the owner can call this method.
    ///
    /// # Arguments
    /// * `interface_name` - Name of the interface (e.g., "Oracle", "Store")
    /// * `implementation_address` - Contract account ID implementing the interface
    pub fn change_implementation_address(
        &mut self,
        interface_name: String,
        implementation_address: AccountId,
    ) {
        self.assert_owner();

        self.interfaces
            .insert(interface_name.clone(), implementation_address.clone());

        // Emit event
        let event = InterfaceImplementationChanged {
            interface_name,
            new_implementation: implementation_address,
        };
        let event_json = near_sdk::serde_json::to_string(&event).unwrap();
        env::log_str(&format!("EVENT_JSON:{{\"standard\":\"finder\",\"version\":\"1.0.0\",\"event\":\"interface_changed\",\"data\":{}}}", event_json));
    }

    /// Remove an interface implementation.
    /// Only the owner can call this method.
    ///
    /// # Arguments
    /// * `interface_name` - Name of the interface to remove
    pub fn remove_implementation(&mut self, interface_name: String) {
        self.assert_owner();
        self.interfaces.remove(&interface_name);
        self.interfaces.flush();
    }

    /// Get the implementation address for an interface.
    /// Panics if the interface is not registered.
    ///
    /// # Arguments
    /// * `interface_name` - Name of the interface
    ///
    /// # Returns
    /// The contract account ID implementing the interface
    pub fn get_implementation_address(&self, interface_name: String) -> AccountId {
        self.interfaces
            .get(&interface_name)
            .expect("Implementation not found")
            .clone()
    }

    /// Check if an interface has a registered implementation.
    ///
    /// # Arguments
    /// * `interface_name` - Name of the interface
    ///
    /// # Returns
    /// True if the interface is registered
    pub fn has_implementation(&self, interface_name: String) -> bool {
        self.interfaces.contains_key(&interface_name)
    }

    /// Transfer ownership to a new account.
    /// Only the current owner can call this method.
    ///
    /// # Arguments
    /// * `new_owner` - The new owner account
    pub fn set_owner(&mut self, new_owner: AccountId) {
        self.assert_owner();
        self.owner = new_owner;
    }

    /// Get the current owner.
    pub fn get_owner(&self) -> AccountId {
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

    fn get_context(predecessor: AccountId) -> VMContextBuilder {
        let mut builder = VMContextBuilder::new();
        builder.predecessor_account_id(predecessor);
        builder
    }

    #[test]
    fn test_new() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let contract = Finder::new(accounts(0));
        assert_eq!(contract.get_owner(), accounts(0));
    }

    #[test]
    fn test_change_implementation_address() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = Finder::new(accounts(0));

        // Register Oracle interface
        contract.change_implementation_address("Oracle".to_string(), accounts(1));

        assert!(contract.has_implementation("Oracle".to_string()));
        assert_eq!(
            contract.get_implementation_address("Oracle".to_string()),
            accounts(1)
        );
    }

    #[test]
    fn test_update_implementation() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = Finder::new(accounts(0));

        // Register and then update
        contract.change_implementation_address("Oracle".to_string(), accounts(1));
        contract.change_implementation_address("Oracle".to_string(), accounts(2));

        assert_eq!(
            contract.get_implementation_address("Oracle".to_string()),
            accounts(2)
        );
    }

    #[test]
    fn test_remove_implementation() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = Finder::new(accounts(0));

        contract.change_implementation_address("Oracle".to_string(), accounts(1));
        assert!(contract.has_implementation("Oracle".to_string()));

        contract.remove_implementation("Oracle".to_string());
        assert!(!contract.has_implementation("Oracle".to_string()));
    }

    #[test]
    #[should_panic(expected = "Implementation not found")]
    fn test_get_unregistered_implementation() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let contract = Finder::new(accounts(0));
        contract.get_implementation_address("Oracle".to_string());
    }

    #[test]
    #[should_panic(expected = "Only owner can call this method")]
    fn test_change_implementation_unauthorized() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = Finder::new(accounts(0));

        // Try to change as non-owner
        testing_env!(get_context(accounts(1)).build());
        contract.change_implementation_address("Oracle".to_string(), accounts(2));
    }

    #[test]
    fn test_transfer_ownership() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = Finder::new(accounts(0));

        contract.set_owner(accounts(1));
        assert_eq!(contract.get_owner(), accounts(1));

        // New owner can now make changes
        testing_env!(get_context(accounts(1)).build());
        contract.change_implementation_address("Oracle".to_string(), accounts(2));
        assert_eq!(
            contract.get_implementation_address("Oracle".to_string()),
            accounts(2)
        );
    }

    #[test]
    fn test_multiple_interfaces() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = Finder::new(accounts(0));

        contract.change_implementation_address(interface_names::ORACLE.to_string(), accounts(1));
        contract.change_implementation_address(interface_names::STORE.to_string(), accounts(2));
        contract.change_implementation_address(
            interface_names::IDENTIFIER_WHITELIST.to_string(),
            accounts(3),
        );

        assert_eq!(
            contract.get_implementation_address(interface_names::ORACLE.to_string()),
            accounts(1)
        );
        assert_eq!(
            contract.get_implementation_address(interface_names::STORE.to_string()),
            accounts(2)
        );
        assert_eq!(
            contract.get_implementation_address(interface_names::IDENTIFIER_WHITELIST.to_string()),
            accounts(3)
        );
    }
}
