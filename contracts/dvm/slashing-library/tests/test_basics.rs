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

    // Initialize the contract with 10% slashing rate (1000 basis points)
    let outcome = contract
        .call("new")
        .args_json(json!({
            "owner": owner.id(),
            "base_slashing_rate": 1000
        }))
        .transact()
        .await?;
    assert!(outcome.is_success(), "{:#?}", outcome.into_result().unwrap_err());

    // Calculate slashing for 1000 tokens
    let slashing_amount: String = contract
        .view("calculate_slashing")
        .args_json(json!({"wrong_vote_total_stake": "1000"}))
        .await?
        .json()?;
    assert_eq!(slashing_amount, "100"); // 10% of 1000

    // Update slashing rate to 20%
    let outcome = owner
        .call(contract.id(), "set_base_slashing_rate")
        .args_json(json!({"new_rate": 2000}))
        .transact()
        .await?;
    assert!(outcome.is_success());

    // Verify new rate
    let rate: u64 = contract
        .view("get_base_slashing_rate")
        .args_json(json!({}))
        .await?
        .json()?;
    assert_eq!(rate, 2000);

    // Calculate slashing with new rate
    let slashing_amount: String = contract
        .view("calculate_slashing")
        .args_json(json!({"wrong_vote_total_stake": "1000"}))
        .await?
        .json()?;
    assert_eq!(slashing_amount, "200"); // 20% of 1000

    Ok(())
}
