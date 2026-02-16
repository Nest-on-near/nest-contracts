use serde_json::json;

#[tokio::test]
#[ignore = "Flaky under constrained CI sandboxes; run manually for end-to-end lifecycle validation"]
async fn test_vault_collateral_stake_redeem_lifecycle() -> Result<(), Box<dyn std::error::Error>> {
    let sandbox = near_workspaces::sandbox().await?;

    let token_wasm = near_workspaces::compile_project("../contracts/dvm/voting-token").await?;
    let voting_wasm = near_workspaces::compile_project("../contracts/dvm/voting").await?;
    let vault_wasm = near_workspaces::compile_project("../contracts/dvm/vault").await?;

    let collateral = sandbox.dev_deploy(&token_wasm).await?;
    let nest = sandbox.dev_deploy(&token_wasm).await?;
    let voting = sandbox.dev_deploy(&voting_wasm).await?;
    let vault = sandbox.dev_deploy(&vault_wasm).await?;

    let owner = sandbox.dev_create_account().await?;
    let user = sandbox.dev_create_account().await?;
    let treasury = sandbox.dev_create_account().await?;

    collateral
        .call("new")
        .args_json(json!({
            "owner": owner.id(),
            "total_supply": "1000000"
        }))
        .transact()
        .await?
        .into_result()?;

    nest.call("new")
        .args_json(json!({
            "owner": owner.id(),
            "total_supply": "0"
        }))
        .transact()
        .await?
        .into_result()?;

    voting
        .call("new")
        .args_json(json!({ "owner": owner.id() }))
        .transact()
        .await?
        .into_result()?;

    vault
        .call("new")
        .args_json(json!({
            "owner": owner.id(),
            "collateral_token": collateral.id(),
            "nest_token": nest.id(),
            "emergency_recipient": treasury.id()
        }))
        .transact()
        .await?
        .into_result()?;

    owner
        .call(voting.id(), "set_voting_token")
        .args_json(json!({ "voting_token": nest.id() }))
        .transact()
        .await?
        .into_result()?;

    owner
        .call(voting.id(), "set_treasury")
        .args_json(json!({ "treasury": treasury.id() }))
        .transact()
        .await?
        .into_result()?;

    owner
        .call(nest.id(), "set_vault_account")
        .args_json(json!({ "vault_account": vault.id() }))
        .transact()
        .await?
        .into_result()?;

    // Defensive explicit grants in case old deployments rely on manual role wiring.
    owner
        .call(nest.id(), "add_minter")
        .args_json(json!({ "account_id": vault.id() }))
        .transact()
        .await?
        .into_result()?;
    owner
        .call(nest.id(), "add_burner")
        .args_json(json!({ "account_id": vault.id() }))
        .transact()
        .await?
        .into_result()?;

    owner
        .call(nest.id(), "add_transfer_router")
        .args_json(json!({ "account_id": voting.id() }))
        .transact()
        .await?
        .into_result()?;

    owner
        .call(collateral.id(), "set_transfer_restricted")
        .args_json(json!({ "restricted": false }))
        .transact()
        .await?
        .into_result()?;

    for account in [&user, &vault.as_account()] {
        account
            .call(collateral.id(), "storage_deposit")
            .args_json(json!({
                "account_id": account.id(),
                "registration_only": true
            }))
            .deposit(near_workspaces::types::NearToken::from_millinear(10))
            .transact()
            .await?
            .into_result()?;
    }

    let voting_account = voting.as_account();
    for account in [&user, &vault.as_account(), &voting_account, &treasury] {
        account
            .call(nest.id(), "storage_deposit")
            .args_json(json!({
                "account_id": account.id(),
                "registration_only": true
            }))
            .deposit(near_workspaces::types::NearToken::from_millinear(10))
            .transact()
            .await?
            .into_result()?;
    }

    let user_nest_storage: Option<serde_json::Value> = nest
        .view("storage_balance_of")
        .args_json(json!({ "account_id": user.id() }))
        .await?
        .json()?;
    assert!(user_nest_storage.is_some());

    owner
        .call(collateral.id(), "ft_transfer")
        .args_json(json!({
            "receiver_id": user.id(),
            "amount": "300"
        }))
        .deposit(near_workspaces::types::NearToken::from_yoctonear(1))
        .transact()
        .await?
        .into_result()?;

    let is_vault_minter: bool = nest
        .view("is_minter")
        .args_json(json!({ "account_id": vault.id() }))
        .await?
        .json()?;
    assert!(is_vault_minter);

    let deposit_outcome = user
        .call(collateral.id(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": vault.id(),
            "amount": "200",
            "msg": serde_json::to_string(&json!({"action":"DepositCollateral"}))?
        }))
        .deposit(near_workspaces::types::NearToken::from_yoctonear(1))
        .transact()
        .await?;
    assert!(deposit_outcome.is_success(), "{deposit_outcome:?}");

    let user_nest_after_deposit: String = nest
        .view("ft_balance_of")
        .args_json(json!({ "account_id": user.id() }))
        .await?
        .json()?;
    assert_eq!(user_nest_after_deposit, "200");

    user.call(nest.id(), "ft_transfer")
        .args_json(json!({
            "receiver_id": voting.id(),
            "amount": "50"
        }))
        .deposit(near_workspaces::types::NearToken::from_yoctonear(1))
        .transact()
        .await?
        .into_result()?;

    voting_account
        .call(nest.id(), "ft_transfer")
        .args_json(json!({
            "receiver_id": treasury.id(),
            "amount": "20"
        }))
        .deposit(near_workspaces::types::NearToken::from_yoctonear(1))
        .transact()
        .await?
        .into_result()?;

    user.call(vault.id(), "redeem_collateral")
        .args_json(json!({ "amount": "100" }))
        .transact()
        .await?
        .into_result()?;

    let user_collateral_balance: String = collateral
        .view("ft_balance_of")
        .args_json(json!({ "account_id": user.id() }))
        .await?
        .json()?;
    assert_eq!(user_collateral_balance, "200");

    let user_nest_balance: String = nest
        .view("ft_balance_of")
        .args_json(json!({ "account_id": user.id() }))
        .await?
        .json()?;
    assert_eq!(user_nest_balance, "50");

    let treasury_nest_balance: String = nest
        .view("ft_balance_of")
        .args_json(json!({ "account_id": treasury.id() }))
        .await?
        .json()?;
    assert_eq!(treasury_nest_balance, "20");

    let diagnostics: serde_json::Value = vault
        .view("get_invariant_diagnostics")
        .args_json(json!({}))
        .await?
        .json()?;
    assert_eq!(diagnostics["total_locked_collateral"], "100");
    assert_eq!(diagnostics["total_minted_liability"], "100");
    assert_eq!(diagnostics["invariant_ok"], true);

    Ok(())
}
