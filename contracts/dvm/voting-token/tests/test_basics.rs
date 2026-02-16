use serde_json::json;

#[tokio::test]
async fn test_contract_is_operational() -> Result<(), Box<dyn std::error::Error>> {
    let contract_wasm = near_workspaces::compile_project("./").await?;

    test_basics_on(&contract_wasm).await?;
    Ok(())
}

async fn test_basics_on(contract_wasm: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let sandbox = near_workspaces::sandbox().await?;
    let contract = sandbox.dev_deploy(contract_wasm).await?;

    let owner = sandbox.dev_create_account().await?;
    let minter = sandbox.dev_create_account().await?;
    let recipient = sandbox.dev_create_account().await?;

    // Initialize the contract
    let outcome = contract
        .call("new")
        .args_json(json!({
            "owner": owner.id(),
            "total_supply": "1000000000000000000000000" // 1 token with 24 decimals
        }))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "{:#?}",
        outcome.into_result().unwrap_err()
    );

    // Check initial supply
    let total_supply: String = contract
        .view("ft_total_supply")
        .args_json(json!({}))
        .await?
        .json()?;
    assert_eq!(total_supply, "1000000000000000000000000");

    // Check owner balance
    let owner_balance: String = contract
        .view("ft_balance_of")
        .args_json(json!({"account_id": owner.id()}))
        .await?
        .json()?;
    assert_eq!(owner_balance, "1000000000000000000000000");

    // Add minter
    let outcome = owner
        .call(contract.id(), "add_minter")
        .args_json(json!({"account_id": minter.id()}))
        .transact()
        .await?;
    assert!(outcome.is_success());

    // Verify minter was added
    let is_minter: bool = contract
        .view("is_minter")
        .args_json(json!({"account_id": minter.id()}))
        .await?
        .json()?;
    assert!(is_minter);

    // Recipient must register storage before minting
    let outcome = recipient
        .call(contract.id(), "storage_deposit")
        .args_json(json!({
            "account_id": recipient.id(),
            "registration_only": true
        }))
        .deposit(near_workspaces::types::NearToken::from_millinear(10))
        .transact()
        .await?;
    assert!(outcome.is_success());

    // Mint tokens to recipient
    let outcome = minter
        .call(contract.id(), "mint")
        .args_json(json!({
            "account_id": recipient.id(),
            "amount": "500000000000000000000000" // 0.5 tokens
        }))
        .transact()
        .await?;
    assert!(outcome.is_success());

    // Check recipient balance
    let recipient_balance: String = contract
        .view("ft_balance_of")
        .args_json(json!({"account_id": recipient.id()}))
        .await?
        .json()?;
    assert_eq!(recipient_balance, "500000000000000000000000");

    // Check new total supply
    let total_supply: String = contract
        .view("ft_total_supply")
        .args_json(json!({}))
        .await?
        .json()?;
    assert_eq!(total_supply, "1500000000000000000000000"); // 1 + 0.5 = 1.5 tokens

    Ok(())
}
