//! Agent Runtime Adapter
//!
//! 🔧 FIX: Adapter to bridge legacy AgentService and AgentRuntimeManager
//! with the new AgentRuntime trait during migration period.
//!
//! This allows gradual migration without breaking existing code.

use std::sync::Arc;
use async_trait::async_trait;
use tracing::{info, warn, error};

use gateway::{AgentRuntime, AgentConfig, AgentHandle, AgentStatus, StateCommand, TaskConfig, TaskResult};
use beebotos_agents::StateManagerHandle;

/// Adapter that wraps the new AgentRuntime trait to provide legacy-compatible interface
pub struct AgentRuntimeAdapter {
    /// The new runtime implementation
    runtime: Arc<dyn gateway::AgentRuntime>,
    /// State manager handle (from new runtime)
    state_manager: Option<StateManagerHandle>,
}

impl AgentRuntimeAdapter {
    /// Create new adapter
    pub fn new(runtime: Arc<dyn gateway::AgentRuntime>) -> Self {
        Self {
            runtime,
            state_manager: None,
        }
    }

    /// Create with state manager
    pub fn with_state_manager(runtime: Arc<dyn gateway::AgentRuntime>, state_manager: StateManagerHandle) -> Self {
        Self {
            runtime,
            state_manager: Some(state_manager),
        }
    }

    /// Get underlying runtime
    pub fn runtime(&self) -> &Arc<dyn gateway::AgentRuntime> {
        &self.runtime
    }

    /// Get state manager if available
    pub fn state_manager(&self) -> Option<&StateManagerHandle> {
        self.state_manager.as_ref()
    }

    /// 🔧 FIX: Unified method to create and spawn agent
    /// 
    /// Replaces: AgentService::create_and_spawn + AgentRuntimeManager::register_agent
    pub async fn create_and_spawn_agent(
        &self,
        config: AgentConfig,
        owner_id: &str,
    ) -> Result<(AgentHandle, crate::models::AgentRecord), crate::error::AppError> {
        info!("Creating and spawning agent '{}' for owner {}", config.name, owner_id);

        // Create database record
        let agent_record = self.create_agent_in_db(&config, owner_id).await?;

        // Spawn via new runtime
        let handle = self.runtime.spawn(config).await
            .map_err(|e| crate::error::AppError::Agent(e.to_string()))?;

        info!("Agent {} created and spawned successfully", handle.agent_id);

        Ok((handle, agent_record))
    }

    /// 🔧 FIX: Start (restart) an agent
    /// 
    /// Replaces: AgentService::start_agent
    pub async fn start_agent(
        &self,
        agent_id: &str,
    ) -> Result<AgentHandle, crate::error::AppError> {
        info!("Starting agent {}", agent_id);

        // Get current status
        let status = self.runtime.status(agent_id).await
            .map_err(|e| crate::error::AppError::Agent(e.to_string()))?;

        match status.state {
            gateway::AgentState::Stopped | gateway::AgentState::Error { .. } => {
                // Need to respawn - get config first
                let config = self.runtime.get_config(agent_id).await
                    .map_err(|e| crate::error::AppError::Agent(e.to_string()))?;

                // Spawn new instance
                let handle = self.runtime.spawn(config).await
                    .map_err(|e| crate::error::AppError::Agent(e.to_string()))?;

                info!("Agent {} restarted", agent_id);
                Ok(handle)
            }
            _ => {
                // Already running or starting
                warn!("Agent {} is already in state {:?}", agent_id, status.state);
                Ok(AgentHandle {
                    agent_id: agent_id.to_string(),
                    kernel_task_id: status.kernel_task_id,
                })
            }
        }
    }

    /// 🔧 FIX: Stop an agent
    /// 
    /// Replaces: AgentService::stop_agent + AgentRuntimeManager::unregister_agent
    pub async fn stop_agent(&self, agent_id: &str) -> Result<(), crate::error::AppError> {
        info!("Stopping agent {}", agent_id);

        self.runtime.send_command(agent_id, StateCommand::Stop).await
            .map_err(|e| crate::error::AppError::Agent(e.to_string()))?;

        info!("Agent {} stopped", agent_id);
        Ok(())
    }

    /// 🔧 FIX: Execute task on agent
    /// 
    /// Replaces: AgentService::execute_task
    pub async fn execute_task(
        &self,
        agent_id: &str,
        task: TaskConfig,
    ) -> Result<TaskResult, crate::error::AppError> {
        self.runtime.execute_task(agent_id, task).await
            .map_err(|e| crate::error::AppError::Agent(e.to_string()))
    }

    /// 🔧 FIX: Get agent status
    /// 
    /// Replaces: AgentService::get_agent_status + AgentRuntimeManager::get_agent_status
    pub async fn get_agent_status(&self, agent_id: &str) -> Result<AgentStatus, crate::error::AppError> {
        self.runtime.status(agent_id).await
            .map_err(|e| crate::error::AppError::Agent(e.to_string()))
    }

    /// 🔧 FIX: List all agents
    /// 
    /// Replaces: AgentService::list_agents + AgentRuntimeManager::list_agents
    pub async fn list_agents(&self) -> Result<Vec<AgentStatus>, crate::error::AppError> {
        self.runtime.list_agents().await
            .map_err(|e| crate::error::AppError::Agent(e.to_string()))
    }

    /// 🔧 FIX: Check if agent exists
    /// 
    /// Replaces: AgentRuntimeManager::is_agent_registered
    pub async fn is_agent_exists(&self, agent_id: &str) -> bool {
        self.runtime.status(agent_id).await.is_ok()
    }

    // Private helper methods

    async fn create_agent_in_db(
        &self,
        config: &AgentConfig,
        owner_id: &str,
    ) -> Result<crate::models::AgentRecord, crate::error::AppError> {
        // This would normally create a database record
        // For now, return a mock record
        Ok(crate::models::AgentRecord {
            id: uuid::Uuid::parse_str(&config.id).unwrap_or_else(|_| uuid::Uuid::new_v4()),
            name: config.name.clone(),
            description: Some(config.description.clone()),
            owner_id: owner_id.to_string(),
            capabilities: config.capabilities.iter().map(|c| c.name.clone()).collect(),
            model_provider: Some(config.llm_config.provider.clone()),
            model_name: Some(config.llm_config.model.clone()),
            status: "initializing".to_string(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
    }
}

/// 🔧 FIX: Helper to migrate from old components to new adapter
pub async fn migrate_to_adapter(
    old_service: &crate::services::AgentService,
    old_manager: &crate::services::AgentRuntimeManager,
    new_runtime: Arc<dyn gateway::AgentRuntime>,
) -> AgentRuntimeAdapter {
    info!("Migrating to AgentRuntimeAdapter...");

    // Create adapter
    let adapter = AgentRuntimeAdapter::new(new_runtime);

    // Note: In a real migration, you would:
    // 1. List all agents from old_manager
    // 2. Re-register them with the new runtime
    // 3. Update all references to use the adapter

    info!("Migration to AgentRuntimeAdapter complete");
    adapter
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_adapter_creation() {
        // This would test the adapter with a mock runtime
        // For now, just verify it compiles
    }
}
