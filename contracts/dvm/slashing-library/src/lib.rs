use near_sdk::json_types::U128;
use near_sdk::{env, near, require, AccountId, PanicOnDefault};

/// Basis points denominator (100% = 10000 basis points)
const BASIS_POINTS_DENOMINATOR: u128 = 10_000;

/// SlashingLibrary - Calculates slashing penalties for incorrect votes.
///
/// When a vote resolves in the DVM, voters who voted against the majority
/// can have their staked tokens slashed. This contract calculates the
/// slashing amount based on configurable parameters.
///
/// The slashing formula is:
/// slashing_amount = min(wrong_vote_tokens * slashing_percentage, wrong_vote_tokens)
///
/// The slashing percentage can be configured by the owner.
#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct SlashingLibrary {
    /// Contract owner - can configure slashing parameters
    owner: AccountId,

    /// Base slashing percentage in basis points (e.g., 1000 = 10%)
    /// This is the percentage of wrong voters' stake that gets slashed
    base_slashing_rate: u64,
}

#[near]
impl SlashingLibrary {
    /// Initialize the SlashingLibrary contract.
    ///
    /// # Arguments
    /// * `owner` - Account that can configure slashing parameters
    /// * `base_slashing_rate` - Initial slashing rate in basis points (max 10000)
    #[init]
    pub fn new(owner: AccountId, base_slashing_rate: u64) -> Self {
        require!(
            base_slashing_rate <= BASIS_POINTS_DENOMINATOR as u64,
            "Slashing rate cannot exceed 100%"
        );

        Self {
            owner,
            base_slashing_rate,
        }
    }

    // ==================== Slashing Calculation ====================

    /// Calculate the slashing amount for wrong voters.
    ///
    /// This is a view function that computes how much should be slashed
    /// from voters who voted incorrectly.
    ///
    /// # Arguments
    /// * `wrong_vote_total_stake` - Total stake that voted incorrectly
    ///
    /// # Returns
    /// The amount to slash from wrong voters
    pub fn calculate_slashing(&self, wrong_vote_total_stake: U128) -> U128 {
        let stake = wrong_vote_total_stake.0;
        let slashing_amount = (stake * self.base_slashing_rate as u128) / BASIS_POINTS_DENOMINATOR;
        U128(slashing_amount)
    }

    /// Calculate slashing with custom parameters.
    ///
    /// Allows calculating slashing with specific vote totals for more
    /// complex slashing logic (e.g., proportional to vote margin).
    ///
    /// # Arguments
    /// * `wrong_vote_total_stake` - Total stake that voted incorrectly
    /// * `correct_vote_total_stake` - Total stake that voted correctly
    /// * `total_stake_at_snapshot` - Total stake at the time of vote snapshot
    ///
    /// # Returns
    /// The amount to slash from wrong voters
    pub fn calculate_slashing_with_context(
        &self,
        wrong_vote_total_stake: U128,
        _correct_vote_total_stake: U128,
        _total_stake_at_snapshot: U128,
    ) -> U128 {
        // For now, use the base calculation
        // This can be extended to implement more sophisticated slashing logic
        // based on vote margins or participation rates
        self.calculate_slashing(wrong_vote_total_stake)
    }

    // ==================== Configuration ====================

    /// Set the base slashing rate.
    /// Only the owner can call this method.
    ///
    /// # Arguments
    /// * `new_rate` - New slashing rate in basis points (max 10000)
    pub fn set_base_slashing_rate(&mut self, new_rate: u64) {
        self.assert_owner();
        require!(
            new_rate <= BASIS_POINTS_DENOMINATOR as u64,
            "Slashing rate cannot exceed 100%"
        );
        self.base_slashing_rate = new_rate;

        env::log_str(&format!(
            "EVENT_JSON:{{\"standard\":\"slashing_library\",\"version\":\"1.0.0\",\"event\":\"slashing_rate_updated\",\"data\":{{\"new_rate\":{}}}}}",
            new_rate
        ));
    }

    /// Get the current base slashing rate.
    pub fn get_base_slashing_rate(&self) -> u64 {
        self.base_slashing_rate
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

        let contract = SlashingLibrary::new(accounts(0), 1000); // 10%
        assert_eq!(contract.get_owner(), accounts(0));
        assert_eq!(contract.get_base_slashing_rate(), 1000);
    }

    #[test]
    #[should_panic(expected = "Slashing rate cannot exceed 100%")]
    fn test_new_rate_too_high() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        SlashingLibrary::new(accounts(0), 10001);
    }

    #[test]
    fn test_calculate_slashing_10_percent() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let contract = SlashingLibrary::new(accounts(0), 1000); // 10%

        // 1000 tokens staked wrong, should slash 100
        let result = contract.calculate_slashing(U128(1000));
        assert_eq!(result.0, 100);
    }

    #[test]
    fn test_calculate_slashing_50_percent() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let contract = SlashingLibrary::new(accounts(0), 5000); // 50%

        // 1000 tokens staked wrong, should slash 500
        let result = contract.calculate_slashing(U128(1000));
        assert_eq!(result.0, 500);
    }

    #[test]
    fn test_calculate_slashing_100_percent() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let contract = SlashingLibrary::new(accounts(0), 10000); // 100%

        // 1000 tokens staked wrong, should slash all
        let result = contract.calculate_slashing(U128(1000));
        assert_eq!(result.0, 1000);
    }

    #[test]
    fn test_calculate_slashing_zero() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let contract = SlashingLibrary::new(accounts(0), 1000); // 10%

        // 0 tokens staked wrong, should slash 0
        let result = contract.calculate_slashing(U128(0));
        assert_eq!(result.0, 0);
    }

    #[test]
    fn test_calculate_slashing_large_amount() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let contract = SlashingLibrary::new(accounts(0), 1000); // 10%

        // 1_000_000_000_000_000_000_000_000 (1e24) tokens
        let large_stake = 1_000_000_000_000_000_000_000_000u128;
        let result = contract.calculate_slashing(U128(large_stake));
        assert_eq!(result.0, large_stake / 10); // 10%
    }

    #[test]
    fn test_set_base_slashing_rate() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = SlashingLibrary::new(accounts(0), 1000);

        contract.set_base_slashing_rate(2000); // Change to 20%
        assert_eq!(contract.get_base_slashing_rate(), 2000);

        // Verify new rate is used in calculations
        let result = contract.calculate_slashing(U128(1000));
        assert_eq!(result.0, 200);
    }

    #[test]
    #[should_panic(expected = "Slashing rate cannot exceed 100%")]
    fn test_set_rate_too_high() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = SlashingLibrary::new(accounts(0), 1000);
        contract.set_base_slashing_rate(10001);
    }

    #[test]
    #[should_panic(expected = "Only owner can call this method")]
    fn test_set_rate_unauthorized() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = SlashingLibrary::new(accounts(0), 1000);

        testing_env!(get_context(accounts(1)).build());
        contract.set_base_slashing_rate(2000);
    }

    #[test]
    fn test_calculate_slashing_with_context() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let contract = SlashingLibrary::new(accounts(0), 1000); // 10%

        let result = contract.calculate_slashing_with_context(
            U128(1000),  // wrong votes
            U128(9000),  // correct votes
            U128(10000), // total stake
        );
        assert_eq!(result.0, 100); // 10% of wrong votes
    }

    #[test]
    fn test_transfer_ownership() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let mut contract = SlashingLibrary::new(accounts(0), 1000);

        contract.set_owner(accounts(1));
        assert_eq!(contract.get_owner(), accounts(1));

        // New owner can change rate
        testing_env!(get_context(accounts(1)).build());
        contract.set_base_slashing_rate(2000);
        assert_eq!(contract.get_base_slashing_rate(), 2000);
    }

    #[test]
    fn test_zero_slashing_rate() {
        let context = get_context(accounts(0));
        testing_env!(context.build());

        let contract = SlashingLibrary::new(accounts(0), 0); // 0%

        // No slashing when rate is 0
        let result = contract.calculate_slashing(U128(1000));
        assert_eq!(result.0, 0);
    }
}
