use serde_json::json;

#[tokio::test]
async fn test_contract_is_operational() -> Result<(), Box<dyn std::error::Error>> {
    let contract_wasm = near_workspaces::compile_project("./").await?;

    let sandbox = near_workspaces::sandbox().await?;
    let contract = sandbox.dev_deploy(&contract_wasm).await?;

    let owner = sandbox.dev_create_account().await?;
    let currency = sandbox.dev_create_account().await?;

    // Initialize the oracle
    let outcome = owner
        .call(contract.id(), "new")
        .args_json(json!({
            "owner": owner.id(),
            "default_currency": currency.id()
        }))
        .transact()
        .await?;
    assert!(outcome.is_success(), "Init failed: {:#?}", outcome.into_result().unwrap_err());

    // Verify default currency
    let default_currency: String = contract
        .view("default_currency")
        .args_json(json!({}))
        .await?
        .json()?;
    assert_eq!(default_currency, currency.id().to_string());

    // Verify default identifier (returns [u8; 32])
    let default_id: Vec<u8> = contract
        .view("default_identifier")
        .args_json(json!({}))
        .await?
        .json()?;
    // Default identifier is "ASSERT_TRUTH" encoded as bytes
    assert_eq!(default_id.len(), 32);

    // Whitelist a currency
    owner
        .call(contract.id(), "whitelist_currency")
        .args_json(json!({
            "currency": currency.id(),
            "final_fee": "1000000000000000000"
        }))
        .transact()
        .await?
        .into_result()?;

    // Verify currency is whitelisted
    let is_whitelisted: bool = contract
        .view("is_currency_whitelisted")
        .args_json(json!({ "currency": currency.id() }))
        .await?
        .json()?;
    assert!(is_whitelisted);

    println!("Optimistic Oracle initialized and configured successfully!");

    Ok(())
}
