use std::collections::HashMap;
use std::sync::Arc;

use clawhive_memory::embedding::EmbeddingProvider;

use crate::config::{FullAgentConfig, RoutingConfig};
use crate::persona::Persona;
use crate::router::LlmRouter;
use crate::tool::ToolRegistry;

/// Immutable snapshot of all config-derived state.
pub struct ConfigView {
    pub generation: u64,
    pub agents: HashMap<String, Arc<FullAgentConfig>>,
    pub personas: HashMap<String, Arc<Persona>>,
    pub routing: RoutingConfig,
    pub router: LlmRouter,
    pub tool_registry: ToolRegistry,
    pub embedding_provider: Arc<dyn EmbeddingProvider>,
}

impl ConfigView {
    pub fn new(
        generation: u64,
        agents: Vec<FullAgentConfig>,
        personas: HashMap<String, Persona>,
        routing: RoutingConfig,
        router: LlmRouter,
        tool_registry: ToolRegistry,
        embedding_provider: Arc<dyn EmbeddingProvider>,
    ) -> Self {
        let agents = agents
            .into_iter()
            .filter(|a| a.enabled)
            .map(|a| (a.agent_id.clone(), Arc::new(a)))
            .collect();
        let personas = personas
            .into_iter()
            .map(|(k, v)| (k, Arc::new(v)))
            .collect();

        Self {
            generation,
            agents,
            personas,
            routing,
            router,
            tool_registry,
            embedding_provider,
        }
    }

    pub fn agent(&self, agent_id: &str) -> Option<&Arc<FullAgentConfig>> {
        self.agents.get(agent_id)
    }

    pub fn persona(&self, agent_id: &str) -> Option<&Arc<Persona>> {
        self.personas.get(agent_id)
    }
}
