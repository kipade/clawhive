use anyhow::{anyhow, Result};
use async_trait::async_trait;
use clawhive_bus::BusPublisher;
use clawhive_provider::ToolDef;
use clawhive_schema::BusMessage;
use serde::Deserialize;

use crate::tool::{ToolContext, ToolExecutor, ToolOutput};

pub const MESSAGE_TOOL_NAME: &str = "message";

pub struct MessageTool {
    bus: BusPublisher,
}

impl MessageTool {
    pub fn new(bus: BusPublisher) -> Self {
        Self { bus }
    }
}

#[derive(Debug, Deserialize)]
struct MessageInput {
    action: String,
    #[serde(default)]
    channel: Option<String>,
    #[serde(default)]
    connector_id: Option<String>,
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

#[async_trait]
impl ToolExecutor for MessageTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: MESSAGE_TOOL_NAME.to_string(),
            description: "Send messages to channels (Discord, Telegram, Slack, etc). \
                Use for proactive cross-channel messaging and notifications."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["send"],
                        "description": "Action to perform"
                    },
                    "channel": {
                        "type": "string",
                        "description": "Channel type: discord, telegram, slack, whatsapp"
                    },
                    "connector_id": {
                        "type": "string",
                        "description": "Connector ID (defaults to {channel}_main if not specified)"
                    },
                    "target": {
                        "type": "string",
                        "description": "Target conversation scope (e.g. guild:123:channel:456, chat:789)"
                    },
                    "message": {
                        "type": "string",
                        "description": "Message text to send"
                    }
                },
                "required": ["action", "channel", "target", "message"]
            }),
        }
    }

    async fn execute(&self, input: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let parsed: MessageInput = serde_json::from_value(input)
            .map_err(|e| anyhow!("invalid message tool input: {e}"))?;

        match parsed.action.as_str() {
            "send" => {
                let channel = parsed
                    .channel
                    .ok_or_else(|| anyhow!("channel is required"))?;
                let target = parsed.target.ok_or_else(|| anyhow!("target is required"))?;
                let message = parsed
                    .message
                    .ok_or_else(|| anyhow!("message is required"))?;

                let connector_id = parsed
                    .connector_id
                    .unwrap_or_else(|| format!("{}_main", channel));

                self.bus
                    .publish(BusMessage::DeliverAnnounce {
                        channel_type: channel.clone(),
                        connector_id,
                        conversation_scope: target.clone(),
                        text: message,
                    })
                    .await
                    .map_err(|e| anyhow!("failed to publish message: {e}"))?;

                Ok(ToolOutput {
                    content: format!("Message sent to {channel}:{target}"),
                    is_error: false,
                })
            }
            other => Ok(ToolOutput {
                content: format!("Unknown action: {other}"),
                is_error: true,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clawhive_bus::EventBus;
    use clawhive_bus::Topic;
    use clawhive_schema::BusMessage;

    #[tokio::test]
    async fn send_action_publishes_deliver_announce() {
        let bus = EventBus::new(16);
        let publisher = bus.publisher();
        let mut rx = bus.subscribe(Topic::DeliverAnnounce).await;

        let tool = MessageTool::new(publisher);
        let ctx = ToolContext::builtin();

        let result = tool
            .execute(
                serde_json::json!({
                    "action": "send",
                    "channel": "discord",
                    "connector_id": "dc_main",
                    "target": "guild:123:channel:456",
                    "message": "Hello from agent!"
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("discord"));

        let msg = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv())
            .await
            .unwrap()
            .unwrap();

        match msg {
            BusMessage::DeliverAnnounce {
                channel_type,
                connector_id,
                conversation_scope,
                text,
            } => {
                assert_eq!(channel_type, "discord");
                assert_eq!(connector_id, "dc_main");
                assert_eq!(conversation_scope, "guild:123:channel:456");
                assert_eq!(text, "Hello from agent!");
            }
            _ => panic!("expected DeliverAnnounce"),
        }
    }

    #[tokio::test]
    async fn send_action_defaults_connector_id() {
        let bus = EventBus::new(16);
        let publisher = bus.publisher();
        let mut rx = bus.subscribe(Topic::DeliverAnnounce).await;

        let tool = MessageTool::new(publisher);
        let ctx = ToolContext::builtin();

        let result = tool
            .execute(
                serde_json::json!({
                    "action": "send",
                    "channel": "telegram",
                    "target": "chat:789",
                    "message": "Auto connector"
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error);

        let msg = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv())
            .await
            .unwrap()
            .unwrap();

        match msg {
            BusMessage::DeliverAnnounce { connector_id, .. } => {
                assert_eq!(connector_id, "telegram_main");
            }
            _ => panic!("expected DeliverAnnounce"),
        }
    }

    #[tokio::test]
    async fn send_action_requires_channel() {
        let bus = EventBus::new(16);
        let publisher = bus.publisher();
        let tool = MessageTool::new(publisher);
        let ctx = ToolContext::builtin();

        let result = tool
            .execute(
                serde_json::json!({
                    "action": "send",
                    "target": "chat:789",
                    "message": "Missing channel"
                }),
                &ctx,
            )
            .await;

        match result {
            Ok(output) => assert!(output.is_error || output.content.contains("channel")),
            Err(e) => assert!(e.to_string().contains("channel")),
        }
    }

    #[tokio::test]
    async fn unknown_action_returns_error() {
        let bus = EventBus::new(16);
        let publisher = bus.publisher();
        let tool = MessageTool::new(publisher);
        let ctx = ToolContext::builtin();

        let result = tool
            .execute(
                serde_json::json!({
                    "action": "delete",
                    "channel": "discord",
                    "target": "chat:1",
                    "message": "nope"
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(result.is_error);
        assert!(result.content.contains("Unknown action"));
    }
}
