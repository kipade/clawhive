use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use clawhive_memory::embedding::EmbeddingProvider;
use clawhive_memory::fact_store::{self, Fact, FactStore};
use clawhive_memory::file_store::MemoryFileStore;
use clawhive_memory::search_index::SearchIndex;
use clawhive_provider::ToolDef;

use super::tool::{ToolContext, ToolExecutor, ToolOutput};

pub struct MemorySearchTool {
    search_index: SearchIndex,
    embedding_provider: Arc<dyn EmbeddingProvider>,
}

impl MemorySearchTool {
    pub fn new(search_index: SearchIndex, embedding_provider: Arc<dyn EmbeddingProvider>) -> Self {
        Self {
            search_index,
            embedding_provider,
        }
    }
}

#[async_trait]
impl ToolExecutor for MemorySearchTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "memory_search".into(),
            description: "Search through long-term memory. Returns snippets ranked by relevance. Use memory_get to read full content of interesting results.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query to find relevant memories"
                    },
                    "max_results": {
                        "type": "integer",
                        "description": "Maximum number of results (default: 6)",
                        "default": 6
                    }
                },
                "required": ["query"]
            }),
        }
    }

    async fn execute(&self, input: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let query = input["query"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'query' field"))?;
        let max_results = input["max_results"].as_u64().unwrap_or(6) as usize;

        match self
            .search_index
            .search(query, self.embedding_provider.as_ref(), max_results, 0.35)
            .await
        {
            Ok(results) if results.is_empty() => Ok(ToolOutput {
                content: "No relevant memories found.".into(),
                is_error: false,
            }),
            Ok(results) => {
                let mut output = String::new();
                for r in &results {
                    let snippet: String = r.text.chars().take(200).collect();
                    let truncated = if r.text.chars().count() > 200 {
                        "..."
                    } else {
                        ""
                    };
                    output.push_str(&format!(
                        "- [{path}:{start}-{end}] (score: {score:.2}) {snippet}{truncated}\n",
                        path = r.path,
                        start = r.start_line,
                        end = r.end_line,
                        score = r.score,
                    ));
                }
                Ok(ToolOutput {
                    content: output,
                    is_error: false,
                })
            }
            Err(e) => Ok(ToolOutput {
                content: format!("Search failed: {e}"),
                is_error: true,
            }),
        }
    }
}

pub struct MemoryGetTool {
    file_store: MemoryFileStore,
}

impl MemoryGetTool {
    pub fn new(file_store: MemoryFileStore) -> Self {
        Self { file_store }
    }
}

#[async_trait]
impl ToolExecutor for MemoryGetTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "memory_get".into(),
            description: "Retrieve a specific memory file by key. Use 'MEMORY.md' for long-term memory, or 'YYYY-MM-DD' for a daily file.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "key": {
                        "type": "string",
                        "description": "The memory key: 'MEMORY.md' for long-term, or 'YYYY-MM-DD' for daily file"
                    }
                },
                "required": ["key"]
            }),
        }
    }

    async fn execute(&self, input: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let key = input["key"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'key' field"))?;

        if key == "MEMORY.md" {
            match self.file_store.read_long_term().await {
                Ok(content) => Ok(ToolOutput {
                    content,
                    is_error: false,
                }),
                Err(e) => Ok(ToolOutput {
                    content: format!("Failed to read MEMORY.md: {e}"),
                    is_error: true,
                }),
            }
        } else if let Ok(date) = chrono::NaiveDate::parse_from_str(key, "%Y-%m-%d") {
            match self.file_store.read_daily(date).await {
                Ok(Some(content)) => Ok(ToolOutput {
                    content,
                    is_error: false,
                }),
                Ok(None) => Ok(ToolOutput {
                    content: format!("No daily file for {key}"),
                    is_error: false,
                }),
                Err(e) => Ok(ToolOutput {
                    content: format!("Failed to read daily file: {e}"),
                    is_error: true,
                }),
            }
        } else {
            Ok(ToolOutput {
                content: format!("Unknown memory key: {key}. Use 'MEMORY.md' or 'YYYY-MM-DD'."),
                is_error: true,
            })
        }
    }
}

pub struct MemoryWriteTool {
    fact_store: FactStore,
    agent_id: String,
}

impl MemoryWriteTool {
    pub fn new(fact_store: FactStore, agent_id: String) -> Self {
        Self {
            fact_store,
            agent_id,
        }
    }
}

#[async_trait]
impl ToolExecutor for MemoryWriteTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "memory_write".into(),
            description: "Store a fact about the user or conversation for future reference. Use this to remember important preferences, decisions, or events.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "content": {
                        "type": "string",
                        "description": "The fact to remember (e.g., 'User prefers dark mode')"
                    },
                    "fact_type": {
                        "type": "string",
                        "enum": ["preference", "decision", "event", "person", "rule"],
                        "description": "Type of fact"
                    }
                },
                "required": ["content", "fact_type"]
            }),
        }
    }

    async fn execute(&self, input: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let content = input["content"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'content' field"))?;
        let fact_type = input["fact_type"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'fact_type' field"))?;

        let now = chrono::Utc::now().to_rfc3339();
        let fact = Fact {
            id: fact_store::generate_fact_id(&self.agent_id, content),
            agent_id: self.agent_id.clone(),
            content: content.to_owned(),
            fact_type: fact_type.to_owned(),
            importance: 0.5,
            confidence: 1.0,
            status: "active".to_owned(),
            occurred_at: None,
            recorded_at: now.clone(),
            source_type: "agent_write".to_owned(),
            source_session: None,
            access_count: 0,
            last_accessed: None,
            superseded_by: None,
            created_at: now.clone(),
            updated_at: now,
        };

        match self.fact_store.insert_fact(&fact).await {
            Ok(()) => {
                let _ = self.fact_store.record_add(&fact).await;
                Ok(ToolOutput {
                    content: format!("Remembered: {content}"),
                    is_error: false,
                })
            }
            Err(e) => Ok(ToolOutput {
                content: format!("Failed to store fact: {e}"),
                is_error: true,
            }),
        }
    }
}

pub struct MemoryForgetTool {
    fact_store: FactStore,
    agent_id: String,
}

impl MemoryForgetTool {
    pub fn new(fact_store: FactStore, agent_id: String) -> Self {
        Self {
            fact_store,
            agent_id,
        }
    }
}

#[async_trait]
impl ToolExecutor for MemoryForgetTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "memory_forget".into(),
            description: "Forget or retract a previously stored fact. Use when the user says something is no longer true or asks you to forget something.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "content": {
                        "type": "string",
                        "description": "The fact content to forget (must match an existing fact)"
                    },
                    "reason": {
                        "type": "string",
                        "description": "Why this fact is being retracted"
                    }
                },
                "required": ["content", "reason"]
            }),
        }
    }

    async fn execute(&self, input: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let content = input["content"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'content' field"))?;
        let reason = input["reason"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'reason' field"))?;

        match self
            .fact_store
            .find_by_content(&self.agent_id, content)
            .await
        {
            Ok(Some(fact)) if fact.status == "active" => {
                match self
                    .fact_store
                    .update_status(&fact.id, "retracted", reason)
                    .await
                {
                    Ok(()) => Ok(ToolOutput {
                        content: format!("Forgotten: {content}"),
                        is_error: false,
                    }),
                    Err(e) => Ok(ToolOutput {
                        content: format!("Failed to retract fact: {e}"),
                        is_error: true,
                    }),
                }
            }
            Ok(_) => Ok(ToolOutput {
                content: format!("No active fact found matching: {content}"),
                is_error: false,
            }),
            Err(e) => Ok(ToolOutput {
                content: format!("Failed to look up fact: {e}"),
                is_error: true,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clawhive_memory::embedding::StubEmbeddingProvider;
    use clawhive_memory::fact_store::FactStore;
    use clawhive_memory::search_index::SearchIndex;
    use clawhive_memory::{file_store::MemoryFileStore, MemoryStore};
    use std::sync::Arc;
    use tempfile::TempDir;

    fn setup() -> (TempDir, Arc<MemoryStore>, MemorySearchTool, MemoryGetTool) {
        let tmp = TempDir::new().unwrap();
        let memory = Arc::new(MemoryStore::open_in_memory().unwrap());
        let search_index = SearchIndex::new(memory.db(), "test-agent");
        let embedding: Arc<dyn EmbeddingProvider> = Arc::new(StubEmbeddingProvider::new(8));
        let file_store = MemoryFileStore::new(tmp.path());

        let search_tool = MemorySearchTool::new(search_index, embedding);
        let get_tool = MemoryGetTool::new(file_store);
        (tmp, memory, search_tool, get_tool)
    }

    #[test]
    fn memory_search_tool_definition() {
        let (_tmp, _memory, tool, _) = setup();
        let def = tool.definition();
        assert_eq!(def.name, "memory_search");
        assert!(def.input_schema["properties"]["query"].is_object());
    }

    #[test]
    fn memory_get_tool_definition() {
        let (_tmp, _memory, _, tool) = setup();
        let def = tool.definition();
        assert_eq!(def.name, "memory_get");
        assert!(def.input_schema["properties"]["key"].is_object());
    }

    #[test]
    fn memory_write_tool_definition() {
        let (_tmp, memory, _, _) = setup();
        let tool = MemoryWriteTool::new(FactStore::new(memory.db()), "agent-1".to_string());

        let def = tool.definition();

        assert_eq!(def.name, "memory_write");
        assert!(def.input_schema["properties"]["content"].is_object());
        assert!(def.input_schema["properties"]["fact_type"].is_object());
    }

    #[test]
    fn memory_forget_tool_definition() {
        let (_tmp, memory, _, _) = setup();
        let tool = MemoryForgetTool::new(FactStore::new(memory.db()), "agent-1".to_string());

        let def = tool.definition();

        assert_eq!(def.name, "memory_forget");
        assert!(def.input_schema["properties"]["content"].is_object());
        assert!(def.input_schema["properties"]["reason"].is_object());
    }

    #[tokio::test]
    async fn memory_search_returns_results() {
        let (_tmp, _memory, tool, _) = setup();
        let ctx = ToolContext::builtin();
        let result = tool
            .execute(serde_json::json!({"query": "test query"}), &ctx)
            .await
            .unwrap();
        // With empty index, should return empty but not error
        assert!(!result.is_error);
    }

    #[tokio::test]
    async fn memory_get_long_term() {
        let (tmp, _memory, _, tool) = setup();
        let ctx = ToolContext::builtin();
        let file_store = MemoryFileStore::new(tmp.path());
        file_store
            .write_long_term("# Long term memory")
            .await
            .unwrap();

        let result = tool
            .execute(serde_json::json!({"key": "MEMORY.md"}), &ctx)
            .await
            .unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("Long term memory"));
    }

    #[tokio::test]
    async fn memory_write_stores_active_fact() {
        let (_tmp, memory, _, _) = setup();
        let ctx = ToolContext::builtin();
        let fact_store = FactStore::new(memory.db());
        let tool = MemoryWriteTool::new(fact_store.clone(), "agent-1".to_string());

        let result = tool
            .execute(
                serde_json::json!({
                    "content": "User prefers dark mode",
                    "fact_type": "preference"
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        assert_eq!(result.content, "Remembered: User prefers dark mode");

        let fact = fact_store
            .find_by_content("agent-1", "User prefers dark mode")
            .await
            .unwrap()
            .expect("fact should be stored");
        assert_eq!(fact.status, "active");
        assert_eq!(fact.fact_type, "preference");
    }

    #[tokio::test]
    async fn memory_forget_retracts_existing_fact() {
        let (_tmp, memory, _, _) = setup();
        let ctx = ToolContext::builtin();
        let fact_store = FactStore::new(memory.db());
        let write_tool = MemoryWriteTool::new(fact_store.clone(), "agent-1".to_string());
        let forget_tool = MemoryForgetTool::new(fact_store.clone(), "agent-1".to_string());

        write_tool
            .execute(
                serde_json::json!({
                    "content": "User moved to Berlin",
                    "fact_type": "event"
                }),
                &ctx,
            )
            .await
            .unwrap();

        let result = forget_tool
            .execute(
                serde_json::json!({
                    "content": "User moved to Berlin",
                    "reason": "User corrected this"
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        assert_eq!(result.content, "Forgotten: User moved to Berlin");

        let fact = fact_store
            .find_by_content("agent-1", "User moved to Berlin")
            .await
            .unwrap()
            .expect("fact should still exist with updated status");
        assert_eq!(fact.status, "retracted");
    }
}
