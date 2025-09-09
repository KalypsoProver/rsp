use eyre::Result;
use serde_json::Value;
use std::env;

pub async fn post_to_supabase(
    endpoint: &str,
    json_body: &Value,
) -> Result<String> {
    let client = reqwest::Client::new();
    let supabase_api_key = env::var("SUPABASE_API_KEY").expect("SUPABASE_API_KEY not set in .env");
    let supabase_auth = format!("Bearer {}", supabase_api_key);
    let response = client
        .post(endpoint)
        .header("apikey", &supabase_api_key)
        .header("Authorization", &supabase_auth)
        .header("Content-Type", "application/json")
        .header("Prefer", "return=minimal")
        .json(json_body)
        .send()
        .await?
        .text()
        .await?;
    Ok(response)
}
