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
    let registered_contract = sandbox.dev_create_account().await?;

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

    // Register a contract
    let outcome = owner
        .call(contract.id(), "register_contract")
        .args_json(json!({
            "contract_address": registered_contract.id()
        }))
        .transact()
        .await?;
    assert!(outcome.is_success());

    // Check if contract is registered
    let is_registered: bool = contract
        .view("is_contract_registered")
        .args_json(json!({"contract_address": registered_contract.id()}))
        .await?
        .json()?;
    assert!(is_registered);

    // Check non-registered contract
    let is_registered: bool = contract
        .view("is_contract_registered")
        .args_json(json!({"contract_address": owner.id()}))
        .await?
        .json()?;
    assert!(!is_registered);

    // Unregister the contract
    let outcome = owner
        .call(contract.id(), "unregister_contract")
        .args_json(json!({
            "contract_address": registered_contract.id()
        }))
        .transact()
        .await?;
    assert!(outcome.is_success());

    // Verify it's unregistered
    let is_registered: bool = contract
        .view("is_contract_registered")
        .args_json(json!({"contract_address": registered_contract.id()}))
        .await?
        .json()?;
    assert!(!is_registered);

    Ok(())
}
