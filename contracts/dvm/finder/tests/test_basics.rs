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
    let oracle_contract = sandbox.dev_create_account().await?;
    let store_contract = sandbox.dev_create_account().await?;

    // Initialize the contract
    let outcome = contract
        .call("new")
        .args_json(json!({
            "owner": owner.id()
        }))
        .transact()
        .await?;
    assert!(outcome.is_success(), "{:#?}", outcome.into_result().unwrap_err());

    // Register Oracle interface
    let outcome = owner
        .call(contract.id(), "change_implementation_address")
        .args_json(json!({
            "interface_name": "Oracle",
            "implementation_address": oracle_contract.id()
        }))
        .transact()
        .await?;
    assert!(outcome.is_success());

    // Register Store interface
    let outcome = owner
        .call(contract.id(), "change_implementation_address")
        .args_json(json!({
            "interface_name": "Store",
            "implementation_address": store_contract.id()
        }))
        .transact()
        .await?;
    assert!(outcome.is_success());

    // Verify Oracle is registered
    let has_oracle: bool = contract
        .view("has_implementation")
        .args_json(json!({"interface_name": "Oracle"}))
        .await?
        .json()?;
    assert!(has_oracle);

    // Get Oracle address
    let oracle_address: String = contract
        .view("get_implementation_address")
        .args_json(json!({"interface_name": "Oracle"}))
        .await?
        .json()?;
    assert_eq!(oracle_address, oracle_contract.id().to_string());

    // Get Store address
    let store_address: String = contract
        .view("get_implementation_address")
        .args_json(json!({"interface_name": "Store"}))
        .await?
        .json()?;
    assert_eq!(store_address, store_contract.id().to_string());

    Ok(())
}
