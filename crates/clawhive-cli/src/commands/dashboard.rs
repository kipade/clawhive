use std::time::Duration;

use anyhow::Result;
use clawhive_bus::EventBus;
use tokio::time::sleep;

pub(crate) async fn run(port: u16) -> Result<()> {
    let base_url = format!("http://127.0.0.1:{port}");
    let metrics_url = format!("{base_url}/api/events/metrics");
    let stream_url = format!("{base_url}/api/events/stream");

    let client = reqwest::Client::new();
    let probe = client
        .get(&metrics_url)
        .timeout(Duration::from_secs(2))
        .send()
        .await;

    match probe {
        Ok(resp) if resp.status().is_success() => {}
        Ok(resp) => {
            anyhow::bail!(
                "Gateway not ready at {base_url} (HTTP {}). Start it first with `clawhive start`.",
                resp.status()
            );
        }
        Err(err) => {
            anyhow::bail!(
                "Cannot connect to Gateway at {base_url}: {err}. Start it first with `clawhive start`."
            );
        }
    }

    let bus = EventBus::new(1024);
    let publisher = bus.publisher();
    let stream_url_bg = stream_url.clone();
    tokio::spawn(async move {
        if let Err(err) = forward_sse_to_bus(stream_url_bg, publisher).await {
            tracing::error!("dev stream relay stopped: {err}");
        }
    });

    clawhive_tui::run_tui(&bus, None).await
}

async fn forward_sse_to_bus(
    stream_url: String,
    publisher: clawhive_bus::BusPublisher,
) -> Result<()> {
    let client = reqwest::Client::new();

    loop {
        let response = client
            .get(&stream_url)
            .header("accept", "text/event-stream")
            .send()
            .await;

        let mut response = match response {
            Ok(resp) if resp.status().is_success() => resp,
            Ok(resp) => {
                tracing::warn!("dev stream connect failed: HTTP {}", resp.status());
                sleep(Duration::from_millis(800)).await;
                continue;
            }
            Err(err) => {
                tracing::warn!("dev stream connect error: {err}");
                sleep(Duration::from_millis(800)).await;
                continue;
            }
        };

        let mut buffer = String::new();
        let mut event_data: Vec<String> = Vec::new();

        loop {
            let chunk = response.chunk().await;
            let Some(chunk) = (match chunk {
                Ok(v) => v,
                Err(err) => {
                    tracing::warn!("dev stream read error: {err}");
                    None
                }
            }) else {
                break;
            };

            let text = String::from_utf8_lossy(&chunk);
            buffer.push_str(&text);

            while let Some(pos) = buffer.find('\n') {
                let mut line = buffer[..pos].to_string();
                buffer.drain(..=pos);

                if line.ends_with('\r') {
                    line.pop();
                }

                if line.is_empty() {
                    if !event_data.is_empty() {
                        let payload = event_data.join("\n");
                        event_data.clear();
                        match serde_json::from_str::<clawhive_schema::BusMessage>(&payload) {
                            Ok(msg) => {
                                let _ = publisher.publish(msg).await;
                            }
                            Err(err) => {
                                tracing::warn!("dev stream invalid bus payload: {err}");
                            }
                        }
                    }
                    continue;
                }

                if let Some(rest) = line.strip_prefix("data:") {
                    event_data.push(rest.trim_start().to_string());
                }
            }
        }

        sleep(Duration::from_millis(300)).await;
    }
}
