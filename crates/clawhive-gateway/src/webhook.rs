use std::time::Duration;

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use clawhive_core::HardBaseline;
use reqwest::Client;
use serde::Serialize;
use tokio::sync::OnceCell;

const WEBHOOK_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RETRIES: u32 = 2;
const RETRY_DELAY: Duration = Duration::from_secs(2);
static WEBHOOK_CLIENT: OnceCell<Client> = OnceCell::const_new();

#[derive(Debug, Serialize)]
pub struct WebhookPayload {
    pub schedule_id: String,
    pub status: String,
    pub response: Option<String>,
    pub error: Option<String>,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub duration_ms: u64,
}

fn validate_webhook_url(url: &str) -> Result<()> {
    let parsed = reqwest::Url::parse(url).map_err(|e| anyhow!("invalid webhook_url: {e}"))?;
    match parsed.scheme() {
        "http" | "https" => {}
        other => {
            return Err(anyhow!(
                "invalid webhook_url scheme '{other}', expected http or https"
            ));
        }
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow!("invalid webhook_url: missing host"))?;
    let port = parsed
        .port_or_known_default()
        .ok_or_else(|| anyhow!("invalid webhook_url: unknown default port"))?;

    if HardBaseline::network_denied(host, port) {
        return Err(anyhow!(
            "webhook_url denied by hard baseline: {host}:{port} (private/internal target blocked)"
        ));
    }

    Ok(())
}

pub async fn deliver_webhook(url: &str, payload: &WebhookPayload) -> Result<()> {
    validate_webhook_url(url)?;

    let client = WEBHOOK_CLIENT
        .get_or_try_init(|| async {
            Client::builder()
                .timeout(WEBHOOK_TIMEOUT)
                .build()
                .map_err(anyhow::Error::from)
        })
        .await?;

    let mut last_error = None;

    for attempt in 0..=MAX_RETRIES {
        if attempt > 0 {
            tokio::time::sleep(RETRY_DELAY * attempt).await;
        }

        match client
            .post(url)
            .header("Content-Type", "application/json")
            .header("User-Agent", "ClawhHive-Scheduler/1.0")
            .json(payload)
            .send()
            .await
        {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    return Ok(());
                }
                let body = resp.text().await.unwrap_or_default();
                if status.is_server_error() {
                    last_error = Some(anyhow!("webhook returned {}: {}", status, body));
                    continue;
                }
                return Err(anyhow!("webhook returned {}: {}", status, body));
            }
            Err(e) => {
                last_error = Some(anyhow!("webhook request failed: {e}"));
                continue;
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow!("webhook delivery failed after retries")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webhook_payload_serializes_correctly() {
        let now = Utc::now();
        let payload = WebhookPayload {
            schedule_id: "test-job".into(),
            status: "ok".into(),
            response: Some("result text".into()),
            error: None,
            started_at: now,
            ended_at: now,
            duration_ms: 1500,
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("test-job"));
        assert!(json.contains("result text"));
        assert!(json.contains("1500"));
    }

    #[test]
    fn webhook_payload_error_case() {
        let now = Utc::now();
        let payload = WebhookPayload {
            schedule_id: "fail-job".into(),
            status: "error".into(),
            response: None,
            error: Some("timeout after 300s".into()),
            started_at: now,
            ended_at: now,
            duration_ms: 300000,
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("fail-job"));
        assert!(json.contains("timeout after 300s"));
        assert!(!json.contains("result text"));
    }

    #[test]
    fn webhook_url_blocks_private_targets() {
        let err = validate_webhook_url("http://127.0.0.1:8080/hook").unwrap_err();
        assert!(err.to_string().contains("hard baseline"));
    }

    #[test]
    fn webhook_url_blocks_metadata_targets() {
        let err = validate_webhook_url("http://169.254.169.254/latest/meta-data").unwrap_err();
        assert!(err.to_string().contains("hard baseline"));
    }

    #[test]
    fn webhook_url_allows_public_https() {
        validate_webhook_url("https://example.com/hook").unwrap();
    }
}
