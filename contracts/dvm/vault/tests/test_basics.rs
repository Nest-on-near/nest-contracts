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

    let outcome = contract
        .call("new")
        .args_json(json!({
            "owner": owner.id(),
            "collateral_token": "mock-near.testnet",
            "nest_token": "nest-token.testnet",
            "emergency_recipient": owner.id(),
        }))
        .transact()
        .await?;
    assert!(
        outcome.is_success(),
        "{:#?}",
        outcome.into_result().unwrap_err()
    );

    let view_owner: String = contract
        .view("get_owner")
        .args_json(json!({}))
        .await?
        .json()?;
    assert_eq!(view_owner, owner.id().to_string());

    let invariant: serde_json::Value = contract
        .view("get_invariant_diagnostics")
        .args_json(json!({}))
        .await?
        .json()?;
    assert_eq!(invariant["invariant_ok"], true);

    Ok(())
}
