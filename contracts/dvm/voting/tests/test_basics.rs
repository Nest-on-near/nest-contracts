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

    // Initialize the contract
    let outcome = contract
        .call("new")
        .args_json(json!({
            "owner": owner.id()
        }))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "{:#?}",
        outcome.into_result().unwrap_err()
    );

    // Request a price
    let outcome = owner
        .call(contract.id(), "request_price")
        .args_json(json!({
            "identifier": "YES_OR_NO_QUERY",
            "timestamp": 1000,
            "ancillary_data": [116, 101, 115, 116] // "test" as bytes
        }))
        .transact()
        .await?;
    assert!(outcome.is_success());

    // Verify owner
    let contract_owner: String = contract
        .view("get_owner")
        .args_json(json!({}))
        .await?
        .json()?;
    assert_eq!(contract_owner, owner.id().to_string());

    Ok(())
}
