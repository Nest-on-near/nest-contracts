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

#[derive(BorshStorageKey, BorshSerialize)]
#[borsh(crate = "near_sdk::borsh")]
enum StorageKey {
    FungibleToken,
    Metadata,
    Minters,
    Burners,
    TransferWhitelist,
}

/// VotingToken - NEST governance/staking token.
///
/// Security model:
/// - Mint/burn is permissioned via owner-managed minter/burner roles.
/// - Wallet-to-wallet transfer can be restricted.
/// - Protocol routes remain available when one side of transfer is allowlisted
///   (e.g. user -> voting via ft_transfer_call, voting -> winner/treasury payouts).
#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct VotingToken {
    token: FungibleToken,
    metadata: LazyOption<FungibleTokenMetadata>,
    owner: AccountId,
    minters: LookupSet<AccountId>,
    burners: LookupSet<AccountId>,
    transfer_whitelist: LookupSet<AccountId>,
    transfer_restricted: bool,
    vault_account: Option<AccountId>,
}

#[near]
impl VotingToken {
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
                    decimals: 24,
                }),
            ),
            owner: owner.clone(),
            minters: LookupSet::new(StorageKey::Minters),
            burners: LookupSet::new(StorageKey::Burners),
            transfer_whitelist: LookupSet::new(StorageKey::TransferWhitelist),
            transfer_restricted: true,
            vault_account: None,
        };

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

    pub fn add_minter(&mut self, account_id: AccountId) {
        self.assert_owner();
        self.minters.insert(&account_id);
    }

    pub fn remove_minter(&mut self, account_id: AccountId) {
        self.assert_owner();
        self.minters.remove(&account_id);
    }

    pub fn add_burner(&mut self, account_id: AccountId) {
        self.assert_owner();
        self.burners.insert(&account_id);
    }

    pub fn remove_burner(&mut self, account_id: AccountId) {
        self.assert_owner();
        self.burners.remove(&account_id);
    }

    /// Sets or clears vault authority in one operation.
    ///
    /// When set, the vault is granted:
    /// - minter role (for deposit mint)
    /// - burner role (for redeem burn)
    /// - transfer route allowlist membership
    pub fn set_vault_account(&mut self, vault_account: Option<AccountId>) {
        self.assert_owner();

        if let Some(old_vault) = self.vault_account.take() {
            self.minters.remove(&old_vault);
            self.burners.remove(&old_vault);
            self.transfer_whitelist.remove(&old_vault);
        }

        if let Some(new_vault) = vault_account {
            self.minters.insert(&new_vault);
            self.burners.insert(&new_vault);
            self.transfer_whitelist.insert(&new_vault);
            self.vault_account = Some(new_vault);
        }
    }

    /// Adds protocol account that can be sender or receiver in restricted mode.
    pub fn add_transfer_router(&mut self, account_id: AccountId) {
        self.assert_owner();
        self.transfer_whitelist.insert(&account_id);
    }

    pub fn remove_transfer_router(&mut self, account_id: AccountId) {
        self.assert_owner();
        if self.vault_account.as_ref() == Some(&account_id) {
            env::panic_str("Use set_vault_account(None) to remove vault routing permissions");
        }
        self.transfer_whitelist.remove(&account_id);
    }

    pub fn set_transfer_restricted(&mut self, restricted: bool) {
        self.assert_owner();
        self.transfer_restricted = restricted;
    }

    pub fn set_owner(&mut self, new_owner: AccountId) {
        self.assert_owner();
        self.owner = new_owner;
    }

    // ==================== Minting & Burning ====================

    pub fn mint(&mut self, account_id: AccountId, amount: U128) {
        self.assert_minter();
        require!(amount.0 > 0, "Amount must be positive");
        require!(
            self.token.accounts.contains_key(&account_id),
            "Account must be registered via storage_deposit before mint"
        );

        self.token.internal_deposit(&account_id, amount.0);

        near_contract_standards::fungible_token::events::FtMint {
            owner_id: &account_id,
            amount,
            memo: Some("Minted by minter"),
        }
        .emit();
    }

    pub fn burn(&mut self, amount: U128) {
        self.assert_burner();
        require!(amount.0 > 0, "Amount must be positive");

        let account_id = env::predecessor_account_id();
        self.token.internal_withdraw(&account_id, amount.0);

        near_contract_standards::fungible_token::events::FtBurn {
            owner_id: &account_id,
            amount,
            memo: Some("Burned by burner"),
        }
        .emit();
    }

    pub fn burn_from(&mut self, account_id: AccountId, amount: U128) {
        self.assert_burner();
        require!(amount.0 > 0, "Amount must be positive");

        self.token.internal_withdraw(&account_id, amount.0);

        near_contract_standards::fungible_token::events::FtBurn {
            owner_id: &account_id,
            amount,
            memo: Some("Burned by burner"),
        }
        .emit();
    }

    // ==================== View Methods ====================

    pub fn get_owner(&self) -> AccountId {
        self.owner.clone()
    }

    pub fn get_vault_account(&self) -> Option<AccountId> {
        self.vault_account.clone()
    }

    pub fn is_minter(&self, account_id: AccountId) -> bool {
        self.minters.contains(&account_id)
    }

    pub fn is_burner(&self, account_id: AccountId) -> bool {
        self.burners.contains(&account_id)
    }

    pub fn is_transfer_router(&self, account_id: AccountId) -> bool {
        self.transfer_whitelist.contains(&account_id)
    }

    pub fn get_transfer_restricted(&self) -> bool {
        self.transfer_restricted
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

    fn assert_transfer_allowed(&self, sender_id: &AccountId, receiver_id: &AccountId) {
        if !self.transfer_restricted {
            return;
        }

        // Restricted mode allows transfers only when protocol controls either side.
        // This blocks casual wallet-to-wallet movement while preserving:
        // - user -> voting stake commits (receiver allowlisted)
        // - voting -> winner/treasury rewards or slash payouts (sender allowlisted)
        let allowed = self.transfer_whitelist.contains(sender_id)
            || self.transfer_whitelist.contains(receiver_id);

        require!(
            allowed,
            "Transfer blocked: restricted to protocol routes (sender or receiver must be allowlisted)"
        );
    }
}

#[near]
impl FungibleTokenCore for VotingToken {
    #[payable]
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>) {
        let sender_id = env::predecessor_account_id();
        self.assert_transfer_allowed(&sender_id, &receiver_id);
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
        let sender_id = env::predecessor_account_id();
        self.assert_transfer_allowed(&sender_id, &receiver_id);
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
            self.token
                .internal_ft_resolve_transfer(&sender_id, receiver_id, amount);
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

    fn get_context(predecessor: AccountId, deposit: NearToken) -> VMContextBuilder {
        let mut builder = VMContextBuilder::new();
        builder
            .predecessor_account_id(predecessor)
            .attached_deposit(deposit);
        builder
    }

    fn register_account(contract: &mut VotingToken, registrar: AccountId, account_id: AccountId) {
        testing_env!(get_context(registrar, NearToken::from_millinear(100)).build());
        let _ = contract.storage_deposit(Some(account_id), Some(true));
    }

    #[test]
    fn test_new() {
        testing_env!(get_context(accounts(0), NearToken::from_yoctonear(0)).build());

        let contract = VotingToken::new(accounts(0), U128(1_000_000));
        assert_eq!(contract.ft_total_supply().0, 1_000_000);
        assert_eq!(contract.ft_balance_of(accounts(0)).0, 1_000_000);
        assert_eq!(contract.get_owner(), accounts(0));
        assert!(contract.get_transfer_restricted());
    }

    #[test]
    fn test_add_minter_and_mint() {
        testing_env!(get_context(accounts(0), NearToken::from_yoctonear(0)).build());

        let mut contract = VotingToken::new(accounts(0), U128(0));

        contract.add_minter(accounts(1));
        assert!(contract.is_minter(accounts(1)));

        register_account(&mut contract, accounts(0), accounts(2));

        testing_env!(get_context(accounts(1), NearToken::from_yoctonear(0)).build());
        contract.mint(accounts(2), U128(500));

        assert_eq!(contract.ft_balance_of(accounts(2)).0, 500);
        assert_eq!(contract.ft_total_supply().0, 500);
    }

    #[test]
    #[should_panic(expected = "Account must be registered via storage_deposit before mint")]
    fn test_mint_requires_registration() {
        testing_env!(get_context(accounts(0), NearToken::from_yoctonear(0)).build());

        let mut contract = VotingToken::new(accounts(0), U128(0));
        contract.add_minter(accounts(1));

        testing_env!(get_context(accounts(1), NearToken::from_yoctonear(0)).build());
        contract.mint(accounts(2), U128(500));
    }

    #[test]
    #[should_panic(expected = "Only minters can call this method")]
    fn test_mint_unauthorized() {
        testing_env!(get_context(accounts(0), NearToken::from_yoctonear(0)).build());

        let mut contract = VotingToken::new(accounts(0), U128(0));
        register_account(&mut contract, accounts(0), accounts(2));

        testing_env!(get_context(accounts(1), NearToken::from_yoctonear(0)).build());
        contract.mint(accounts(2), U128(500));
    }

    #[test]
    fn test_burn() {
        testing_env!(get_context(accounts(0), NearToken::from_yoctonear(0)).build());

        let mut contract = VotingToken::new(accounts(0), U128(1000));

        contract.add_burner(accounts(0));
        contract.burn(U128(300));

        assert_eq!(contract.ft_balance_of(accounts(0)).0, 700);
        assert_eq!(contract.ft_total_supply().0, 700);
    }

    #[test]
    fn test_set_vault_grants_roles() {
        testing_env!(get_context(accounts(0), NearToken::from_yoctonear(0)).build());
        let mut contract = VotingToken::new(accounts(0), U128(0));

        contract.set_vault_account(Some(accounts(3)));

        assert_eq!(contract.get_vault_account(), Some(accounts(3)));
        assert!(contract.is_minter(accounts(3)));
        assert!(contract.is_burner(accounts(3)));
        assert!(contract.is_transfer_router(accounts(3)));
    }

    #[test]
    #[should_panic(
        expected = "Transfer blocked: restricted to protocol routes (sender or receiver must be allowlisted)"
    )]
    fn test_wallet_to_wallet_transfer_blocked_when_restricted() {
        testing_env!(get_context(accounts(0), NearToken::from_yoctonear(0)).build());
        let mut contract = VotingToken::new(accounts(0), U128(1_000));

        register_account(&mut contract, accounts(0), accounts(1));

        testing_env!(get_context(accounts(0), NearToken::from_yoctonear(1)).build());
        contract.ft_transfer(accounts(1), U128(10), None);
    }

    #[test]
    fn test_protocol_route_transfer_allowed_when_restricted() {
        testing_env!(get_context(accounts(0), NearToken::from_yoctonear(0)).build());
        let mut contract = VotingToken::new(accounts(0), U128(1_000));

        register_account(&mut contract, accounts(0), accounts(1));
        register_account(&mut contract, accounts(0), accounts(2));

        contract.add_transfer_router(accounts(2));
        testing_env!(get_context(accounts(0), NearToken::from_yoctonear(1)).build());
        contract.ft_transfer(accounts(2), U128(25), None);

        assert_eq!(contract.ft_balance_of(accounts(2)).0, 25);
    }

    #[test]
    fn test_router_sender_transfer_allowed_when_restricted() {
        testing_env!(get_context(accounts(0), NearToken::from_yoctonear(0)).build());
        let mut contract = VotingToken::new(accounts(0), U128(1_000));

        register_account(&mut contract, accounts(0), accounts(2));
        register_account(&mut contract, accounts(0), accounts(3));

        contract.add_transfer_router(accounts(2));

        testing_env!(get_context(accounts(0), NearToken::from_yoctonear(1)).build());
        contract.ft_transfer(accounts(2), U128(100), None);

        testing_env!(get_context(accounts(2), NearToken::from_yoctonear(1)).build());
        contract.ft_transfer(accounts(3), U128(40), None);

        assert_eq!(contract.ft_balance_of(accounts(3)).0, 40);
    }

    #[test]
    fn test_transfer_ownership() {
        testing_env!(get_context(accounts(0), NearToken::from_yoctonear(0)).build());

        let mut contract = VotingToken::new(accounts(0), U128(1000));

        contract.set_owner(accounts(1));
        assert_eq!(contract.get_owner(), accounts(1));

        testing_env!(get_context(accounts(1), NearToken::from_yoctonear(0)).build());
        contract.add_minter(accounts(2));
        assert!(contract.is_minter(accounts(2)));
    }
}
