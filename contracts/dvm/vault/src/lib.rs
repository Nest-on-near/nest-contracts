use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
use near_sdk::json_types::U128;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{
    env, ext_contract, near, require, AccountId, Gas, NearToken, PanicOnDefault, Promise,
    PromiseOrValue, PromiseResult,
};

const GAS_FOR_MINT: Gas = Gas::from_tgas(5);
const GAS_FOR_BURN: Gas = Gas::from_tgas(5);
const GAS_FOR_COLLATERAL_TRANSFER: Gas = Gas::from_tgas(10);
const GAS_FOR_CALLBACK: Gas = Gas::from_tgas(5);

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
#[serde(tag = "action")]
pub enum VaultFtMessage {
    DepositCollateral,
}

#[near(serializers = [json])]
pub struct InvariantDiagnostics {
    pub total_locked_collateral: U128,
    pub total_minted_liability: U128,
    pub backing_ratio_bps: Option<U128>,
    pub invariant_ok: bool,
    pub redemptions_paused: bool,
}

#[near(serializers = [json])]
struct VaultEventData {
    account_id: AccountId,
    amount: U128,
}

#[ext_contract(ext_nest)]
#[allow(dead_code)]
trait ExtNestToken {
    fn mint(&mut self, account_id: AccountId, amount: U128);
    fn burn_from(&mut self, account_id: AccountId, amount: U128);
}

#[ext_contract(ext_collateral)]
#[allow(dead_code)]
trait ExtCollateralToken {
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>);
}

#[ext_contract(ext_self)]
#[allow(dead_code)]
trait ExtVaultCallbacks {
    fn on_deposit_mint_complete(&mut self, depositor: AccountId, amount: U128) -> U128;
    fn on_redeem_burn_complete(&mut self, redeemer: AccountId, amount: U128);
    fn on_redeem_transfer_complete(&mut self, redeemer: AccountId, amount: U128) -> bool;
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Vault {
    owner: AccountId,
    collateral_token: AccountId,
    nest_token: AccountId,
    emergency_recipient: AccountId,
    redemptions_paused: bool,
    total_locked_collateral: u128,
    total_minted_liability: u128,
}

#[near]
impl Vault {
    #[init]
    pub fn new(
        owner: AccountId,
        collateral_token: AccountId,
        nest_token: AccountId,
        emergency_recipient: Option<AccountId>,
    ) -> Self {
        Self {
            emergency_recipient: emergency_recipient.unwrap_or_else(|| owner.clone()),
            owner,
            collateral_token,
            nest_token,
            redemptions_paused: false,
            total_locked_collateral: 0,
            total_minted_liability: 0,
        }
    }

    pub fn redeem_collateral(&mut self, amount: U128) -> Promise {
        require!(!self.redemptions_paused, "Redemptions are paused");
        require!(amount.0 > 0, "Amount must be positive");

        let redeemer = env::predecessor_account_id();
        require!(
            self.total_minted_liability >= amount.0,
            "Vault liability is below requested redemption"
        );

        ext_nest::ext(self.nest_token.clone())
            .with_static_gas(GAS_FOR_BURN)
            .burn_from(redeemer.clone(), amount)
            .then(
                ext_self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_CALLBACK)
                    .on_redeem_burn_complete(redeemer, amount),
            )
    }

    #[allow(deprecated)]
    #[private]
    pub fn on_deposit_mint_complete(&mut self, depositor: AccountId, amount: U128) -> U128 {
        require!(
            env::promise_results_count() == 1,
            "Expected one promise result"
        );

        match env::promise_result(0) {
            PromiseResult::Successful(_) => {
                self.total_locked_collateral =
                    self.total_locked_collateral.saturating_add(amount.0);
                self.total_minted_liability = self.total_minted_liability.saturating_add(amount.0);
                self.assert_invariant();
                self.emit_event("collateral_deposit", &depositor, amount);
                self.emit_event("nest_mint", &depositor, amount);
                U128(0)
            }
            _ => {
                env::log_str(
                    "Vault mint failed; collateral will be refunded by ft_resolve_transfer",
                );
                amount
            }
        }
    }

    #[allow(deprecated)]
    #[private]
    pub fn on_redeem_burn_complete(&mut self, redeemer: AccountId, amount: U128) {
        require!(
            env::promise_results_count() == 1,
            "Expected one promise result"
        );

        match env::promise_result(0) {
            PromiseResult::Successful(_) => {
                require!(
                    self.total_locked_collateral >= amount.0,
                    "Insufficient locked collateral"
                );
                require!(
                    self.total_minted_liability >= amount.0,
                    "Insufficient minted liability"
                );

                self.total_locked_collateral -= amount.0;
                self.total_minted_liability -= amount.0;
                self.assert_invariant();
                self.emit_event("nest_burn", &redeemer, amount);

                let _ = ext_collateral::ext(self.collateral_token.clone())
                    .with_attached_deposit(NearToken::from_yoctonear(1))
                    .with_static_gas(GAS_FOR_COLLATERAL_TRANSFER)
                    .ft_transfer(redeemer.clone(), amount, Some("vault redeem".to_string()))
                    .then(
                        ext_self::ext(env::current_account_id())
                            .with_static_gas(GAS_FOR_CALLBACK)
                            .on_redeem_transfer_complete(redeemer, amount),
                    );
            }
            _ => {
                env::panic_str("NEST burn failed during redemption");
            }
        }
    }

    #[allow(deprecated)]
    #[private]
    pub fn on_redeem_transfer_complete(&mut self, redeemer: AccountId, amount: U128) -> bool {
        require!(
            env::promise_results_count() == 1,
            "Expected one promise result"
        );

        match env::promise_result(0) {
            PromiseResult::Successful(_) => {
                self.emit_event("collateral_redeem", &redeemer, amount);
                true
            }
            _ => {
                // Best-effort rollback: restore accounting and re-mint burned NEST.
                self.total_locked_collateral =
                    self.total_locked_collateral.saturating_add(amount.0);
                self.total_minted_liability = self.total_minted_liability.saturating_add(amount.0);
                self.assert_invariant();
                env::log_str(
                    "Collateral transfer failed during redeem; attempting NEST re-mint rollback",
                );
                let _ = ext_nest::ext(self.nest_token.clone())
                    .with_static_gas(GAS_FOR_MINT)
                    .mint(redeemer, amount);
                false
            }
        }
    }

    pub fn pause_redemptions(&mut self) {
        self.assert_owner();
        self.redemptions_paused = true;
    }

    pub fn resume_redemptions(&mut self) {
        self.assert_owner();
        self.redemptions_paused = false;
    }

    pub fn emergency_withdraw_collateral(&mut self, amount: U128) -> Promise {
        self.assert_owner();
        require!(
            self.redemptions_paused,
            "Pause redemptions before emergency withdrawal"
        );
        require!(amount.0 > 0, "Amount must be positive");
        require!(
            self.total_locked_collateral >= amount.0,
            "Amount exceeds tracked collateral"
        );

        self.total_locked_collateral -= amount.0;

        ext_collateral::ext(self.collateral_token.clone())
            .with_attached_deposit(NearToken::from_yoctonear(1))
            .with_static_gas(GAS_FOR_COLLATERAL_TRANSFER)
            .ft_transfer(
                self.emergency_recipient.clone(),
                amount,
                Some("vault emergency withdrawal".to_string()),
            )
    }

    pub fn set_owner(&mut self, new_owner: AccountId) {
        self.assert_owner();
        self.owner = new_owner;
    }

    pub fn set_collateral_token(&mut self, collateral_token: AccountId) {
        self.assert_owner();
        self.collateral_token = collateral_token;
    }

    pub fn set_nest_token(&mut self, nest_token: AccountId) {
        self.assert_owner();
        self.nest_token = nest_token;
    }

    pub fn set_emergency_recipient(&mut self, emergency_recipient: AccountId) {
        self.assert_owner();
        self.emergency_recipient = emergency_recipient;
    }

    pub fn get_owner(&self) -> AccountId {
        self.owner.clone()
    }

    pub fn get_collateral_token(&self) -> AccountId {
        self.collateral_token.clone()
    }

    pub fn get_nest_token(&self) -> AccountId {
        self.nest_token.clone()
    }

    pub fn get_redemptions_paused(&self) -> bool {
        self.redemptions_paused
    }

    pub fn get_total_locked_collateral(&self) -> U128 {
        U128(self.total_locked_collateral)
    }

    pub fn get_total_minted_liability(&self) -> U128 {
        U128(self.total_minted_liability)
    }

    pub fn get_backing_ratio_bps(&self) -> Option<U128> {
        if self.total_minted_liability == 0 {
            return None;
        }
        Some(U128(
            self.total_locked_collateral.saturating_mul(10_000) / self.total_minted_liability,
        ))
    }

    pub fn get_invariant_diagnostics(&self) -> InvariantDiagnostics {
        InvariantDiagnostics {
            total_locked_collateral: U128(self.total_locked_collateral),
            total_minted_liability: U128(self.total_minted_liability),
            backing_ratio_bps: self.get_backing_ratio_bps(),
            invariant_ok: self.total_minted_liability <= self.total_locked_collateral,
            redemptions_paused: self.redemptions_paused,
        }
    }

    fn assert_owner(&self) {
        require!(
            env::predecessor_account_id() == self.owner,
            "Only owner can call this method"
        );
    }

    fn assert_invariant(&self) {
        require!(
            self.total_minted_liability <= self.total_locked_collateral,
            "Invariant violated: NEST liability exceeds locked collateral"
        );
    }

    fn emit_event(&self, event: &str, account_id: &AccountId, amount: U128) {
        let data = near_sdk::serde_json::to_string(&VaultEventData {
            account_id: account_id.clone(),
            amount,
        })
        .expect("Event serialization failed");
        env::log_str(&format!(
            "EVENT_JSON:{{\"standard\":\"nest_vault\",\"version\":\"1.0.0\",\"event\":\"{}\",\"data\":{}}}",
            event, data
        ));
    }
}

#[near]
impl FungibleTokenReceiver for Vault {
    fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        require!(amount.0 > 0, "Amount must be positive");
        require!(
            env::predecessor_account_id() == self.collateral_token,
            "Only collateral token can call ft_on_transfer"
        );

        let parsed: VaultFtMessage =
            near_sdk::serde_json::from_str(&msg).expect("Invalid vault deposit message");

        match parsed {
            VaultFtMessage::DepositCollateral => PromiseOrValue::Promise(
                ext_nest::ext(self.nest_token.clone())
                    .with_static_gas(GAS_FOR_MINT)
                    .mint(sender_id.clone(), amount)
                    .then(
                        ext_self::ext(env::current_account_id())
                            .with_static_gas(GAS_FOR_CALLBACK)
                            .on_deposit_mint_complete(sender_id, amount),
                    ),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::{testing_env, AccountId, PromiseResult};

    fn account(id: &str) -> AccountId {
        id.parse().unwrap()
    }

    fn get_context(predecessor: AccountId, current: AccountId) -> VMContextBuilder {
        let mut builder = VMContextBuilder::new();
        builder
            .predecessor_account_id(predecessor)
            .current_account_id(current);
        builder
    }

    fn set_context_with_results(
        predecessor: AccountId,
        current: AccountId,
        promise_results: Vec<PromiseResult>,
    ) {
        testing_env!(
            get_context(predecessor, current).build(),
            near_sdk::test_vm_config(),
            near_sdk::RuntimeFeesConfig::test(),
            Default::default(),
            promise_results
        );
    }

    fn setup() -> Vault {
        Vault::new(
            accounts(0),
            account("collateral.testnet"),
            account("nest.testnet"),
            None,
        )
    }

    #[test]
    fn test_deposit_mint_success_updates_liability_and_collateral() {
        let mut contract = setup();
        let vault_account = account("vault.testnet");

        testing_env!(get_context(account("collateral.testnet"), vault_account.clone()).build());
        let msg = near_sdk::serde_json::to_string(&VaultFtMessage::DepositCollateral).unwrap();
        let _ = contract.ft_on_transfer(accounts(1), U128(100), msg);

        set_context_with_results(
            vault_account.clone(),
            vault_account.clone(),
            vec![PromiseResult::Successful(vec![])],
        );
        let refund = contract.on_deposit_mint_complete(accounts(1), U128(100));
        assert_eq!(refund.0, 0);
        assert_eq!(contract.get_total_locked_collateral().0, 100);
        assert_eq!(contract.get_total_minted_liability().0, 100);
        assert_eq!(contract.get_backing_ratio_bps().unwrap().0, 10_000);
    }

    #[test]
    fn test_deposit_mint_failure_refunds_collateral() {
        let mut contract = setup();
        let vault_account = account("vault.testnet");

        set_context_with_results(
            vault_account.clone(),
            vault_account.clone(),
            vec![PromiseResult::Failed],
        );
        let refund = contract.on_deposit_mint_complete(accounts(1), U128(77));

        assert_eq!(refund.0, 77);
        assert_eq!(contract.get_total_locked_collateral().0, 0);
        assert_eq!(contract.get_total_minted_liability().0, 0);
    }

    #[test]
    fn test_redeem_success_path_updates_totals() {
        let mut contract = setup();
        let vault_account = account("vault.testnet");

        set_context_with_results(
            vault_account.clone(),
            vault_account.clone(),
            vec![PromiseResult::Successful(vec![])],
        );
        let _ = contract.on_deposit_mint_complete(accounts(1), U128(250));

        testing_env!(get_context(accounts(1), vault_account.clone()).build());
        let _ = contract.redeem_collateral(U128(100));

        set_context_with_results(
            vault_account.clone(),
            vault_account.clone(),
            vec![PromiseResult::Successful(vec![])],
        );
        contract.on_redeem_burn_complete(accounts(1), U128(100));
        assert_eq!(contract.get_total_locked_collateral().0, 150);
        assert_eq!(contract.get_total_minted_liability().0, 150);

        set_context_with_results(
            vault_account.clone(),
            vault_account,
            vec![PromiseResult::Successful(vec![])],
        );
        assert!(contract.on_redeem_transfer_complete(accounts(1), U128(100)));
    }

    #[test]
    #[should_panic(expected = "Only collateral token can call ft_on_transfer")]
    fn test_ft_on_transfer_rejects_wrong_token() {
        let mut contract = setup();
        testing_env!(get_context(accounts(1), account("vault.testnet")).build());
        let msg = near_sdk::serde_json::to_string(&VaultFtMessage::DepositCollateral).unwrap();
        let _ = contract.ft_on_transfer(accounts(2), U128(1), msg);
    }

    #[test]
    #[should_panic(expected = "Redemptions are paused")]
    fn test_redeem_blocked_when_paused() {
        let mut contract = setup();
        let vault_account = account("vault.testnet");

        testing_env!(get_context(accounts(0), vault_account.clone()).build());
        contract.pause_redemptions();

        testing_env!(get_context(accounts(1), vault_account).build());
        let _ = contract.redeem_collateral(U128(1));
    }

    #[test]
    #[should_panic(expected = "Only owner can call this method")]
    fn test_pause_unauthorized() {
        let mut contract = setup();
        testing_env!(get_context(accounts(1), account("vault.testnet")).build());
        contract.pause_redemptions();
    }
}
