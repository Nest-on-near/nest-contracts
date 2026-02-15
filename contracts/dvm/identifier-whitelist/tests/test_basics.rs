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

    // Add an identifier
    let outcome = owner
        .call(contract.id(), "add_supported_identifier")
        .args_json(json!({
            "identifier": "YES_OR_NO_QUERY"
        }))
        .transact()
        .await?;
    assert!(outcome.is_success());

    // Check if identifier is supported
    let is_supported: bool = contract
        .view("is_identifier_supported")
        .args_json(json!({"identifier": "YES_OR_NO_QUERY"}))
        .await?
        .json()?;
    assert!(is_supported);

    // Check non-existent identifier
    let is_supported: bool = contract
        .view("is_identifier_supported")
        .args_json(json!({"identifier": "UNKNOWN"}))
        .await?
        .json()?;
    assert!(!is_supported);

    // Remove the identifier
    let outcome = owner
        .call(contract.id(), "remove_supported_identifier")
        .args_json(json!({
            "identifier": "YES_OR_NO_QUERY"
        }))
        .transact()
        .await?;
    assert!(outcome.is_success());

    // Verify it's removed
    let is_supported: bool = contract
        .view("is_identifier_supported")
        .args_json(json!({"identifier": "YES_OR_NO_QUERY"}))
        .await?
        .json()?;
    assert!(!is_supported);

    Ok(())
}
