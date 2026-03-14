use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};

use crate::runtime::pid::{is_process_running, read_pid_file, read_port_file};

const INTERNAL_CLI_TOKEN_HEADER: &str = "x-clawhive-cli-token";
const INTERNAL_CLI_TOKEN_FILE: &str = "data/cli_internal_token";
const DEFAULT_PORT: u16 = 8848;

pub(crate) async fn run(root: &Path) -> Result<()> {
    let pid = read_pid_file(root)?.ok_or_else(|| {
        anyhow!("clawhive daemon is not running. Start it with `clawhive up` first.")
    })?;
    if !is_process_running(pid) {
        return Err(anyhow!(
            "clawhive daemon is not running (stale pid: {pid}). Start it with `clawhive up`."
        ));
    }

    let port = read_port_file(root)?.unwrap_or(DEFAULT_PORT);
    let token = ensure_internal_cli_token(root)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .context("failed to initialize HTTP client")?;
    let url = format!("http://127.0.0.1:{port}/api/admin/reload-config");

    let response = client
        .post(&url)
        .header(INTERNAL_CLI_TOKEN_HEADER, token)
        .send()
        .await
        .with_context(|| format!("failed to call daemon API at {url}"))?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();

    match status {
        reqwest::StatusCode::OK => {
            if body.trim().is_empty() {
                println!("Config reloaded.");
            } else {
                println!("{body}");
            }
            Ok(())
        }
        reqwest::StatusCode::UNAUTHORIZED => Err(anyhow!(
            "daemon rejected internal reload token. Restart daemon with `clawhive restart`."
        )),
        _ => Err(anyhow!(
            "daemon returned {status} when reloading config: {body}"
        )),
    }
}

fn ensure_internal_cli_token(root: &Path) -> Result<String> {
    let path = root.join(INTERNAL_CLI_TOKEN_FILE);
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let token = existing.trim();
        if !token.is_empty() {
            return Ok(token.to_string());
        }
    }

    Err(anyhow!(
        "internal CLI token not found. Restart daemon with `clawhive restart` or `clawhive up`."
    ))
}
