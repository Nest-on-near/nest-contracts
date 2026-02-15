use serde_json::json;

#[tokio::test]
async fn test_contract_is_operational() -> Result<(), Box<dyn std::error::Error>> {
    let contract_wasm = near_workspaces::compile_project("./").await?;
    let sandbox = near_workspaces::sandbox().await?;
    let contract = sandbox.dev_deploy(&contract_wasm).await?;

    // Create mock accounts for oracle and bond token
    let oracle = sandbox.dev_create_account().await?;
    let bond_token = sandbox.dev_create_account().await?;

    // Initialize the contract
    let outcome = contract
        .call("new")
        .args_json(json!({
            "oracle": oracle.id(),
            "bond_token": bond_token.id(),
            "min_bond": "1000000000000000000" // 1 token
        }))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "Init failed: {:#?}",
        outcome.into_result().unwrap_err()
    );

    // Verify oracle address
    let stored_oracle: String = contract
        .view("get_oracle")
        .args_json(json!({}))
        .await?
        .json()?;
    assert_eq!(stored_oracle, oracle.id().to_string());

    // Verify bond token address
    let stored_bond_token: String = contract
        .view("get_bond_token")
        .args_json(json!({}))
        .await?
        .json()?;
    assert_eq!(stored_bond_token, bond_token.id().to_string());

    // Verify min bond
    let min_bond: String = contract
        .view("get_min_bond")
        .args_json(json!({}))
        .await?
        .json()?;
    assert_eq!(min_bond, "1000000000000000000");

    // No assertions made yet
    let last_assertion: Option<String> = contract
        .view("get_last_assertion_id")
        .args_json(json!({}))
        .await?
        .json()?;
    assert!(last_assertion.is_none());

    println!("Basic assertion contract initialized successfully!");

    Ok(())
}
