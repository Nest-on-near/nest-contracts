use near_sdk::json_types::U128;
use near_sdk::store::LookupMap;
use near_sdk::{env, near, require, AccountId, NearToken, PanicOnDefault, Promise};

/// Store - Oracle fee collection contract.
///
/// Manages final fees per currency (NEP-141 token).
/// Inspired by UMA's Store contract (simplified - no time-based fees).
///
/// Final fees are one-time fees paid when a dispute is resolved.
/// The oracle takes this fee from the loser's bond as payment for resolution.
#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Store {
    /// Contract owner - can set fees
    owner: AccountId,

    /// Account that can withdraw collected fees
    withdrawer: AccountId,

    /// Final fee per currency (token_id → fee amount in that token's smallest unit)
    final_fees: LookupMap<AccountId, u128>,
}

/// Event emitted when a final fee is set
#[near(serializers = [json])]
pub struct FinalFeeSet {
    pub currency: AccountId,
    pub fee: U128,
}

#[near]
impl Store {
    /// Initialize the Store contract.
    ///
    /// # Arguments
    /// * `owner` - Account that can set fees and manage withdrawer
    /// * `withdrawer` - Account that can withdraw collected fees
    #[init]
    pub fn new(owner: AccountId, withdrawer: AccountId) -> Self {
        Self {
            owner,
            withdrawer,
            final_fees: LookupMap::new(b"f"),
        }
    }

    // ==================== Fee Management ====================

    /// Set the final fee for a currency.
    /// Only the owner can call this method.
    ///
    /// # Arguments
    /// * `currency` - Token contract account ID
    /// * `fee` - Fee amount in the token's smallest unit
    pub fn set_final_fee(&mut self, currency: AccountId, fee: U128) {
        self.assert_owner();

        self.final_fees.insert(currency.clone(), fee.0);

        // Emit event
        let event = FinalFeeSet { currency, fee };
        let event_json = near_sdk::serde_json::to_string(&event).unwrap();
        env::log_str(&format!(
            "EVENT_JSON:{{\"standard\":\"store\",\"version\":\"1.0.0\",\"event\":\"final_fee_set\",\"data\":{}}}",
            event_json
        ));
    }

    /// Remove the final fee for a currency.
    /// Only the owner can call this method.
    ///
    /// # Arguments
    /// * `currency` - Token contract account ID
    pub fn remove_final_fee(&mut self, currency: AccountId) {
        self.assert_owner();
        self.final_fees.remove(&currency);
        self.final_fees.flush();
    }

    /// Get the final fee for a currency.
    /// Returns 0 if no fee is set for the currency.
    ///
    /// # Arguments
    /// * `currency` - Token contract account ID
    ///
    /// # Returns
    /// The final fee amount
    pub fn get_final_fee(&self, currency: AccountId) -> U128 {
        U128(self.final_fees.get(&currency).copied().unwrap_or(0))
    }

    /// Check if a final fee is set for a currency.
    ///
    /// # Arguments
    /// * `currency` - Token contract account ID
    ///
    /// # Returns
    /// True if a fee is set
    pub fn has_final_fee(&self, currency: AccountId) -> bool {
        self.final_fees.contains_key(&currency)
    }

    // ==================== Withdrawal ====================

    /// Withdraw NEAR from the contract.
    /// Only the withdrawer can call this method.
    ///
    /// # Arguments
    /// * `amount` - Amount of NEAR to withdraw (in yoctoNEAR)
    pub fn withdraw_near(&mut self, amount: U128) -> Promise {
        self.assert_withdrawer();
        require!(amount.0 > 0, "Amount must be positive");

        let balance = env::account_balance();
        require!(
            balance >= NearToken::from_yoctonear(amount.0),
            "Insufficient balance"
        );

        Promise::new(self.withdrawer.clone()).transfer(NearToken::from_yoctonear(amount.0))
    }

    /// Withdraw NEP-141 tokens from the contract.
    /// Only the withdrawer can call this method.
    ///
    /// # Arguments
    /// * `token` - Token contract account ID
    /// * `amount` - Amount to withdraw
    pub fn withdraw_token(&mut self, token: AccountId, amount: U128) -> Promise {
        self.assert_withdrawer();
        require!(amount.0 > 0, "Amount must be positive");

        // Call ft_transfer on the token contract
        Promise::new(token).function_call(
            "ft_transfer".to_string(),
            near_sdk::serde_json::json!({
                "receiver_id": self.withdrawer,
                "amount": amount,
            })
            .to_string()
            .into_bytes(),
            NearToken::from_yoctonear(1), // 1 yoctoNEAR for ft_transfer
            near_sdk::Gas::from_tgas(10),
        )
    }

    // ==================== Role Management ====================

    /// Set a new owner.
    /// Only the current owner can call this method.
    ///
    /// # Arguments
    /// * `new_owner` - The new owner account
    pub fn set_owner(&mut self, new_owner: AccountId) {
        self.assert_owner();
        self.owner = new_owner;
    }

    /// Set a new withdrawer.
    /// Only the owner can call this method.
    ///
    /// # Arguments
    /// * `new_withdrawer` - The new withdrawer account
    pub fn set_withdrawer(&mut self, new_withdrawer: AccountId) {
        self.assert_owner();
        self.withdrawer = new_withdrawer;
    }

    /// Get the current owner.
    pub fn get_owner(&self) -> AccountId {
        self.owner.clone()
    }

    /// Get the current withdrawer.
    pub fn get_withdrawer(&self) -> AccountId {
        self.withdrawer.clone()
    }

    // ==================== Internal ====================

    fn assert_owner(&self) {
        require!(
            env::predecessor_account_id() == self.owner,
            "Only owner can call this method"
        );
    }

    fn assert_withdrawer(&self) {
        require!(
            env::predecessor_account_id() == self.withdrawer,
            "Only withdrawer can call this method"
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

        let contract = Store::new(accounts(0), accounts(1));
        assert_eq!(contract.get_owner(), accounts(0));
        assert_eq!(contract.get_withdrawer(), accounts(1));
    }

    #[test]
    fn test_set_final_fee() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = Store::new(accounts(0), accounts(1));
        let token = accounts(2);

        // Set fee
        contract.set_final_fee(token.clone(), U128(1000));

        assert!(contract.has_final_fee(token.clone()));
        assert_eq!(contract.get_final_fee(token).0, 1000);
    }

    #[test]
    fn test_get_unset_fee_returns_zero() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let contract = Store::new(accounts(0), accounts(1));

        // Unset fee should return 0
        assert_eq!(contract.get_final_fee(accounts(2)).0, 0);
        assert!(!contract.has_final_fee(accounts(2)));
    }

    #[test]
    fn test_remove_final_fee() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = Store::new(accounts(0), accounts(1));
        let token = accounts(2);

        contract.set_final_fee(token.clone(), U128(1000));
        assert!(contract.has_final_fee(token.clone()));

        contract.remove_final_fee(token.clone());
        assert!(!contract.has_final_fee(token.clone()));
        assert_eq!(contract.get_final_fee(token).0, 0);
    }

    #[test]
    fn test_update_final_fee() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = Store::new(accounts(0), accounts(1));
        let token = accounts(2);

        contract.set_final_fee(token.clone(), U128(1000));
        assert_eq!(contract.get_final_fee(token.clone()).0, 1000);

        contract.set_final_fee(token.clone(), U128(2000));
        assert_eq!(contract.get_final_fee(token).0, 2000);
    }

    #[test]
    #[should_panic(expected = "Only owner can call this method")]
    fn test_set_final_fee_unauthorized() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = Store::new(accounts(0), accounts(1));

        // Try to set fee as non-owner
        testing_env!(get_context(accounts(2)).build());
        contract.set_final_fee(accounts(3), U128(1000));
    }

    #[test]
    fn test_multiple_currencies() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = Store::new(accounts(0), accounts(1));

        let usdc = accounts(2);
        let dai = accounts(3);
        let near_token = accounts(4);

        contract.set_final_fee(usdc.clone(), U128(100_000_000)); // 100 USDC (6 decimals)
        contract.set_final_fee(dai.clone(), U128(100_000_000_000_000_000_000)); // 100 DAI (18 decimals)
        contract.set_final_fee(near_token.clone(), U128(5_000_000_000_000_000_000_000_000)); // 5 NEAR (24 decimals)

        assert_eq!(contract.get_final_fee(usdc).0, 100_000_000);
        assert_eq!(contract.get_final_fee(dai).0, 100_000_000_000_000_000_000);
        assert_eq!(
            contract.get_final_fee(near_token).0,
            5_000_000_000_000_000_000_000_000
        );
    }

    #[test]
    fn test_change_withdrawer() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = Store::new(accounts(0), accounts(1));

        contract.set_withdrawer(accounts(2));
        assert_eq!(contract.get_withdrawer(), accounts(2));
    }

    #[test]
    fn test_transfer_ownership() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = Store::new(accounts(0), accounts(1));

        contract.set_owner(accounts(2));
        assert_eq!(contract.get_owner(), accounts(2));

        // New owner can set fees
        testing_env!(get_context(accounts(2)).build());
        contract.set_final_fee(accounts(3), U128(500));
        assert_eq!(contract.get_final_fee(accounts(3)).0, 500);
    }
}
