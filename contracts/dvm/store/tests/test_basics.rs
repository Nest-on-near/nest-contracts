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
    let withdrawer = sandbox.dev_create_account().await?;
    let usdc_token = sandbox.dev_create_account().await?;

    // Initialize the contract
    let outcome = contract
        .call("new")
        .args_json(json!({
            "owner": owner.id(),
            "withdrawer": withdrawer.id()
        }))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "{:#?}",
        outcome.into_result().unwrap_err()
    );

    // Set final fee for USDC
    let outcome = owner
        .call(contract.id(), "set_final_fee")
        .args_json(json!({
            "currency": usdc_token.id(),
            "fee": "100000000"  // 100 USDC (6 decimals)
        }))
        .transact()
        .await?;
    assert!(outcome.is_success());

    // Verify fee is set
    let has_fee: bool = contract
        .view("has_final_fee")
        .args_json(json!({"currency": usdc_token.id()}))
        .await?
        .json()?;
    assert!(has_fee);

    // Get fee
    let fee: String = contract
        .view("get_final_fee")
        .args_json(json!({"currency": usdc_token.id()}))
        .await?
        .json()?;
    assert_eq!(fee, "100000000");

    // Get fee for unset currency returns 0
    let unset_fee: String = contract
        .view("get_final_fee")
        .args_json(json!({"currency": "unknown.near"}))
        .await?
        .json()?;
    assert_eq!(unset_fee, "0");

    Ok(())
}
