use crate::models::BrewPackageResult;
use serde::Serialize;
use std::time::Duration;

#[derive(Serialize)]
pub struct WebhookPayload {
    pub status: String,
    pub total: usize,
    pub completed: usize,
    pub failed: usize,
    pub packages: Vec<BrewPackageResult>,
    pub elapsed_seconds: u64,
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
