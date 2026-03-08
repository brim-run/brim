use crate::models::BrewPackageResult;
use serde::Serialize;
use std::time::Duration;

/// Returns a default machine identifier (hostname from env or "unknown"). Used when no machine_id is provided.
pub fn default_machine_id() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}

#[derive(Serialize)]
pub struct WebhookPayload {
    pub status: String,
    pub total: usize,
    pub completed: usize,
    pub failed: usize,
    pub packages: Vec<BrewPackageResult>,
    pub elapsed_seconds: u64,
    pub machine_id: String,
}

pub async fn post_webhook(url: &str, payload: WebhookPayload) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    client
        .post(url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Failed to post webhook: {}", e))?;

    Ok(())
}
