use near_sdk::store::LookupSet;
use near_sdk::{env, near, require, AccountId, PanicOnDefault};

/// Registry - Manages contracts allowed to interact with the oracle.
///
/// In UMA's architecture, the Registry keeps track of which contracts
/// (like synthetic tokens, prediction markets, etc.) are registered
/// and allowed to request prices from the DVM.
///
/// This is a simplified version that maintains a whitelist of contract addresses.
#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Registry {
    /// Contract owner - can register/unregister contracts
    owner: AccountId,

    /// Set of registered contract addresses
    registered_contracts: LookupSet<AccountId>,
}

/// Event emitted when a contract is registered
#[near(serializers = [json])]
pub struct ContractRegistered {
    pub contract_address: AccountId,
    pub creator: AccountId,
}

/// Event emitted when a contract is unregistered
#[near(serializers = [json])]
pub struct ContractUnregistered {
    pub contract_address: AccountId,
}

#[near]
impl Registry {
    /// Initialize the Registry contract.
    ///
    /// # Arguments
    /// * `owner` - Account that can register/unregister contracts
    #[init]
    pub fn new(owner: AccountId) -> Self {
        Self {
            owner,
            registered_contracts: LookupSet::new(b"r"),
        }
    }

    // ==================== Contract Registration ====================

    /// Register a contract with the oracle.
    /// Only the owner can call this method.
    ///
    /// # Arguments
    /// * `contract_address` - The contract address to register
    pub fn register_contract(&mut self, contract_address: AccountId) {
        self.assert_owner();

        if self.registered_contracts.insert(contract_address.clone()) {
            // Emit event only if it was newly registered
            let event = ContractRegistered {
                contract_address,
                creator: env::predecessor_account_id(),
            };
            let event_json = near_sdk::serde_json::to_string(&event).unwrap();
            env::log_str(&format!(
                "EVENT_JSON:{{\"standard\":\"registry\",\"version\":\"1.0.0\",\"event\":\"contract_registered\",\"data\":{}}}",
                event_json
            ));
        }
    }

    /// Unregister a contract from the oracle.
    /// Only the owner can call this method.
    ///
    /// # Arguments
    /// * `contract_address` - The contract address to unregister
    pub fn unregister_contract(&mut self, contract_address: AccountId) {
        self.assert_owner();

        if self.registered_contracts.remove(&contract_address) {
            // Emit event only if it was actually removed
            let event = ContractUnregistered { contract_address };
            let event_json = near_sdk::serde_json::to_string(&event).unwrap();
            env::log_str(&format!(
                "EVENT_JSON:{{\"standard\":\"registry\",\"version\":\"1.0.0\",\"event\":\"contract_unregistered\",\"data\":{}}}",
                event_json
            ));
        }
    }

    /// Check if a contract is registered.
    ///
    /// # Arguments
    /// * `contract_address` - The contract address to check
    ///
    /// # Returns
    /// True if the contract is registered
    pub fn is_contract_registered(&self, contract_address: AccountId) -> bool {
        self.registered_contracts.contains(&contract_address)
    }

    // ==================== Role Management ====================

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

        let contract = Registry::new(accounts(0));
        assert_eq!(contract.get_owner(), accounts(0));
    }

    #[test]
    fn test_register_contract() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = Registry::new(accounts(0));
        let contract_addr = accounts(1);

        assert!(!contract.is_contract_registered(contract_addr.clone()));

        contract.register_contract(contract_addr.clone());

        assert!(contract.is_contract_registered(contract_addr));
    }

    #[test]
    fn test_unregister_contract() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = Registry::new(accounts(0));
        let contract_addr = accounts(1);

        contract.register_contract(contract_addr.clone());
        assert!(contract.is_contract_registered(contract_addr.clone()));

        contract.unregister_contract(contract_addr.clone());
        assert!(!contract.is_contract_registered(contract_addr));
    }

    #[test]
    fn test_multiple_contracts() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = Registry::new(accounts(0));

        contract.register_contract(accounts(1));
        contract.register_contract(accounts(2));
        contract.register_contract(accounts(3));

        assert!(contract.is_contract_registered(accounts(1)));
        assert!(contract.is_contract_registered(accounts(2)));
        assert!(contract.is_contract_registered(accounts(3)));
        assert!(!contract.is_contract_registered(accounts(4)));
    }

    #[test]
    fn test_register_duplicate_contract() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = Registry::new(accounts(0));
        let contract_addr = accounts(1);

        contract.register_contract(contract_addr.clone());
        // Registering again should not panic
        contract.register_contract(contract_addr.clone());

        assert!(contract.is_contract_registered(contract_addr));
    }

    #[test]
    fn test_unregister_nonexistent_contract() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = Registry::new(accounts(0));

        // Unregistering non-existent contract should not panic
        contract.unregister_contract(accounts(1));
    }

    #[test]
    #[should_panic(expected = "Only owner can call this method")]
    fn test_register_unauthorized() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = Registry::new(accounts(0));

        // Try to register as non-owner
        testing_env!(get_context(accounts(1)).build());
        contract.register_contract(accounts(2));
    }

    #[test]
    #[should_panic(expected = "Only owner can call this method")]
    fn test_unregister_unauthorized() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = Registry::new(accounts(0));
        contract.register_contract(accounts(1));

        // Try to unregister as non-owner
        testing_env!(get_context(accounts(2)).build());
        contract.unregister_contract(accounts(1));
    }

    #[test]
    fn test_transfer_ownership() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = Registry::new(accounts(0));

        contract.set_owner(accounts(1));
        assert_eq!(contract.get_owner(), accounts(1));

        // New owner can register contracts
        testing_env!(get_context(accounts(1)).build());
        contract.register_contract(accounts(2));
        assert!(contract.is_contract_registered(accounts(2)));
    }

    #[test]
    fn test_re_register_after_unregister() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = Registry::new(accounts(0));
        let contract_addr = accounts(1);

        contract.register_contract(contract_addr.clone());
        assert!(contract.is_contract_registered(contract_addr.clone()));

        contract.unregister_contract(contract_addr.clone());
        assert!(!contract.is_contract_registered(contract_addr.clone()));

        contract.register_contract(contract_addr.clone());
        assert!(contract.is_contract_registered(contract_addr));
    }
}
