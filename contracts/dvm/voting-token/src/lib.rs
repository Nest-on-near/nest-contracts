use near_contract_standards::fungible_token::core::FungibleTokenCore;
use near_contract_standards::fungible_token::metadata::{
    FungibleTokenMetadata, FungibleTokenMetadataProvider, FT_METADATA_SPEC,
};
use near_contract_standards::fungible_token::resolver::FungibleTokenResolver;
use near_contract_standards::fungible_token::FungibleToken;
use near_contract_standards::storage_management::{
    StorageBalance, StorageBalanceBounds, StorageManagement,
};
use near_sdk::borsh::BorshSerialize;
use near_sdk::collections::{LazyOption, LookupSet};
use near_sdk::json_types::U128;
use near_sdk::{
    env, near, require, AccountId, BorshStorageKey, NearToken, PanicOnDefault, PromiseOrValue,
};

/// Storage keys for collections
#[derive(BorshStorageKey, BorshSerialize)]
#[borsh(crate = "near_sdk::borsh")]
enum StorageKey {
    FungibleToken,
    Metadata,
    Minters,
    Burners,
}

/// VotingToken - NEP-141 fungible token with permissioned minting and burning.
///
/// Inspired by UMA's VotingToken (ERC20 with roles).
/// - Owner can add/remove minters and burners
/// - Minters can mint new tokens (used by Voting contract for rewards)
/// - Burners can burn tokens
#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct VotingToken {
    /// NEP-141 fungible token implementation
    token: FungibleToken,

    /// Token metadata
    metadata: LazyOption<FungibleTokenMetadata>,

    /// Contract owner - can manage roles
    owner: AccountId,

    /// Accounts with minting permission
    minters: LookupSet<AccountId>,

    /// Accounts with burning permission
    burners: LookupSet<AccountId>,
}

#[near]
impl VotingToken {
    /// Initialize the VotingToken contract.
    ///
    /// # Arguments
    /// * `owner` - Account that can manage minter/burner roles
    /// * `total_supply` - Initial token supply (minted to owner)
    #[init]
    pub fn new(owner: AccountId, total_supply: U128) -> Self {
        let mut this = Self {
            token: FungibleToken::new(StorageKey::FungibleToken),
            metadata: LazyOption::new(
                StorageKey::Metadata,
                Some(&FungibleTokenMetadata {
                    spec: FT_METADATA_SPEC.to_string(),
                    name: "Nest Voting Token".to_string(),
                    symbol: "NEST".to_string(),
                    icon: None,
                    reference: None,
                    reference_hash: None,
                    decimals: 24, // Same as NEAR
                }),
            ),
            owner: owner.clone(),
            minters: LookupSet::new(StorageKey::Minters),
            burners: LookupSet::new(StorageKey::Burners),
        };

        // Register owner and mint initial supply
        this.token.internal_register_account(&owner);
        if total_supply.0 > 0 {
            this.token.internal_deposit(&owner, total_supply.0);
            near_contract_standards::fungible_token::events::FtMint {
                owner_id: &owner,
                amount: total_supply,
                memo: Some("Initial supply"),
            }
            .emit();
        }

        this
    }

    // ==================== Role Management ====================

    /// Add a minter. Only owner can call.
    pub fn add_minter(&mut self, account_id: AccountId) {
        self.assert_owner();
        self.minters.insert(&account_id);
    }

    /// Remove a minter. Only owner can call.
    pub fn remove_minter(&mut self, account_id: AccountId) {
        self.assert_owner();
        self.minters.remove(&account_id);
    }

    /// Add a burner. Only owner can call.
    pub fn add_burner(&mut self, account_id: AccountId) {
        self.assert_owner();
        self.burners.insert(&account_id);
    }

    /// Remove a burner. Only owner can call.
    pub fn remove_burner(&mut self, account_id: AccountId) {
        self.assert_owner();
        self.burners.remove(&account_id);
    }

    /// Transfer ownership. Only owner can call.
    pub fn set_owner(&mut self, new_owner: AccountId) {
        self.assert_owner();
        self.owner = new_owner;
    }

    // ==================== Minting & Burning ====================

    /// Mint tokens to an account. Only minters can call.
    ///
    /// # Arguments
    /// * `account_id` - Account to mint tokens to
    /// * `amount` - Amount of tokens to mint
    pub fn mint(&mut self, account_id: AccountId, amount: U128) {
        self.assert_minter();
        require!(amount.0 > 0, "Amount must be positive");

        // Register account if not registered
        if !self.token.accounts.contains_key(&account_id) {
            self.token.internal_register_account(&account_id);
        }

        self.token.internal_deposit(&account_id, amount.0);

        near_contract_standards::fungible_token::events::FtMint {
            owner_id: &account_id,
            amount,
            memo: Some("Minted by minter"),
        }
        .emit();
    }

    /// Burn tokens from caller's account. Only burners can call.
    ///
    /// # Arguments
    /// * `amount` - Amount of tokens to burn
    pub fn burn(&mut self, amount: U128) {
        self.assert_burner();
        let account_id = env::predecessor_account_id();
        self.token.internal_withdraw(&account_id, amount.0);

        near_contract_standards::fungible_token::events::FtBurn {
            owner_id: &account_id,
            amount,
            memo: Some("Burned by burner"),
        }
        .emit();
    }

    /// Burn tokens from a specific account. Only burners can call.
    ///
    /// # Arguments
    /// * `account_id` - Account to burn tokens from
    /// * `amount` - Amount of tokens to burn
    pub fn burn_from(&mut self, account_id: AccountId, amount: U128) {
        self.assert_burner();
        self.token.internal_withdraw(&account_id, amount.0);

        near_contract_standards::fungible_token::events::FtBurn {
            owner_id: &account_id,
            amount,
            memo: Some("Burned by burner"),
        }
        .emit();
    }

    // ==================== View Methods ====================

    /// Get the contract owner.
    pub fn get_owner(&self) -> AccountId {
        self.owner.clone()
    }

    /// Check if an account is a minter.
    pub fn is_minter(&self, account_id: AccountId) -> bool {
        self.minters.contains(&account_id)
    }

    /// Check if an account is a burner.
    pub fn is_burner(&self, account_id: AccountId) -> bool {
        self.burners.contains(&account_id)
    }

    // ==================== Internal ====================

    fn assert_owner(&self) {
        require!(
            env::predecessor_account_id() == self.owner,
            "Only owner can call this method"
        );
    }

    fn assert_minter(&self) {
        require!(
            self.minters.contains(&env::predecessor_account_id()),
            "Only minters can call this method"
        );
    }

    fn assert_burner(&self) {
        require!(
            self.burners.contains(&env::predecessor_account_id()),
            "Only burners can call this method"
        );
    }
}

// ==================== NEP-141 Implementation ====================

#[near]
impl FungibleTokenCore for VotingToken {
    #[payable]
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>) {
        self.token.ft_transfer(receiver_id, amount, memo)
    }

    #[payable]
    fn ft_transfer_call(
        &mut self,
        receiver_id: AccountId,
        amount: U128,
        memo: Option<String>,
        msg: String,
    ) -> PromiseOrValue<U128> {
        self.token.ft_transfer_call(receiver_id, amount, memo, msg)
    }

    fn ft_total_supply(&self) -> U128 {
        self.token.ft_total_supply()
    }

    fn ft_balance_of(&self, account_id: AccountId) -> U128 {
        self.token.ft_balance_of(account_id)
    }
}

#[near]
impl FungibleTokenResolver for VotingToken {
    #[private]
    fn ft_resolve_transfer(
        &mut self,
        sender_id: AccountId,
        receiver_id: AccountId,
        amount: U128,
    ) -> U128 {
        let (used_amount, burned_amount) =
            self.token.internal_ft_resolve_transfer(&sender_id, receiver_id, amount);
        if burned_amount > 0 {
            near_contract_standards::fungible_token::events::FtBurn {
                owner_id: &sender_id,
                amount: burned_amount.into(),
                memo: Some("Refund burned"),
            }
            .emit();
        }
        used_amount.into()
    }
}

#[near]
impl FungibleTokenMetadataProvider for VotingToken {
    fn ft_metadata(&self) -> FungibleTokenMetadata {
        self.metadata.get().unwrap()
    }
}

// ==================== Storage Management ====================

#[near]
impl StorageManagement for VotingToken {
    #[payable]
    fn storage_deposit(
        &mut self,
        account_id: Option<AccountId>,
        registration_only: Option<bool>,
    ) -> StorageBalance {
        self.token.storage_deposit(account_id, registration_only)
    }

    #[payable]
    fn storage_withdraw(&mut self, amount: Option<NearToken>) -> StorageBalance {
        self.token.storage_withdraw(amount)
    }

    #[payable]
    fn storage_unregister(&mut self, force: Option<bool>) -> bool {
        #[allow(unused_variables)]
        if let Some((account_id, balance)) = self.token.internal_storage_unregister(force) {
            true
        } else {
            false
        }
    }

    fn storage_balance_bounds(&self) -> StorageBalanceBounds {
        self.token.storage_balance_bounds()
    }

    fn storage_balance_of(&self, account_id: AccountId) -> Option<StorageBalance> {
        self.token.storage_balance_of(account_id)
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

        let contract = VotingToken::new(accounts(0), U128(1_000_000));
        assert_eq!(contract.ft_total_supply().0, 1_000_000);
        assert_eq!(contract.ft_balance_of(accounts(0)).0, 1_000_000);
        assert_eq!(contract.get_owner(), accounts(0));
    }

    #[test]
    fn test_add_minter_and_mint() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = VotingToken::new(accounts(0), U128(0));

        // Add minter
        contract.add_minter(accounts(1));
        assert!(contract.is_minter(accounts(1)));

        // Mint as minter
        testing_env!(get_context(accounts(1)).build());
        contract.mint(accounts(2), U128(500));

        assert_eq!(contract.ft_balance_of(accounts(2)).0, 500);
        assert_eq!(contract.ft_total_supply().0, 500);
    }

    #[test]
    #[should_panic(expected = "Only minters can call this method")]
    fn test_mint_unauthorized() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = VotingToken::new(accounts(0), U128(0));

        // Try to mint without being a minter
        testing_env!(get_context(accounts(1)).build());
        contract.mint(accounts(2), U128(500));
    }

    #[test]
    fn test_burn() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = VotingToken::new(accounts(0), U128(1000));

        // Add owner as burner and burn
        contract.add_burner(accounts(0));
        contract.burn(U128(300));

        assert_eq!(contract.ft_balance_of(accounts(0)).0, 700);
        assert_eq!(contract.ft_total_supply().0, 700);
    }

    #[test]
    fn test_transfer_ownership() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = VotingToken::new(accounts(0), U128(1000));

        contract.set_owner(accounts(1));
        assert_eq!(contract.get_owner(), accounts(1));

        // Old owner can no longer add minters
        // New owner can
        testing_env!(get_context(accounts(1)).build());
        contract.add_minter(accounts(2));
        assert!(contract.is_minter(accounts(2)));
    }
}
