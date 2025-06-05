use std::sync::Arc;

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use mcp_core::role::Role;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, error, instrument};
use uuid::Uuid;

use crate::message::Message;
use crate::providers::base::Provider;
use crate::recipe::Recipe;
use crate::agents::Agent;

/// Status of a subagent
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SubAgentStatus {
    Initializing,      // Setting up recipe and extensions
    Ready,             // Ready to process messages
    Processing,        // Currently working on a task
    WaitingForInput,   // Waiting for next user message
    Completed,         // Task completed successfully
    Failed(String),    // Failed with error message
    Terminated,        // Manually terminated
}

/// Configuration for a subagent
#[derive(Debug)]
pub struct SubAgentConfig {
    pub id: String,
    pub recipe: Recipe,
    pub max_turns: Option<usize>,
    pub timeout_seconds: Option<u64>,
}

impl SubAgentConfig {
    pub fn new(recipe: Recipe) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            recipe,
            max_turns: None,
            timeout_seconds: None,
        }
    }

    pub fn with_max_turns(mut self, max_turns: usize) -> Self {
        self.max_turns = Some(max_turns);
        self
    }

    pub fn with_timeout(mut self, timeout_seconds: u64) -> Self {
        self.timeout_seconds = Some(timeout_seconds);
        self
    }
}

/// Progress information for a subagent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgentProgress {
    pub subagent_id: String,
    pub status: SubAgentStatus,
    pub message: String,
    pub turn: usize,
    pub max_turns: Option<usize>,
    pub timestamp: DateTime<Utc>,
}

/// A specialized agent that can handle specific tasks independently
pub struct SubAgent {
    pub id: String,
    pub conversation: Arc<Mutex<Vec<Message>>>,
    pub status: Arc<RwLock<SubAgentStatus>>,
    pub config: SubAgentConfig,
    pub agent: Arc<Agent>,
    pub turn_count: Arc<Mutex<usize>>,
    pub created_at: DateTime<Utc>,
}

impl SubAgent {
    /// Create a new subagent with the given configuration and provider
    #[instrument(skip(config, provider))]
    pub async fn new(
        config: SubAgentConfig,
        provider: Arc<dyn Provider>,
    ) -> Result<(Arc<Self>, tokio::task::JoinHandle<()>)> {
        debug!("Creating new subagent with id: {}", config.id);

        // Create the internal agent
        let agent = Arc::new(Agent::new());
        agent.update_provider(provider).await?;

        // Set up extensions from the recipe
        if let Some(extensions) = &config.recipe.extensions {
            for extension in extensions {
                if let Err(e) = agent.add_extension(extension.clone()).await {
                    error!("Failed to add extension to subagent {}: {}", config.id, e);
                    return Err(anyhow!("Failed to add extension: {}", e));
                }
            }
        }

        // Set up custom system prompt from recipe instructions
        if let Some(instructions) = &config.recipe.instructions {
            agent.extend_system_prompt(instructions.clone()).await;
        }

        let subagent = Arc::new(SubAgent {
            id: config.id.clone(),
            conversation: Arc::new(Mutex::new(Vec::new())),
            status: Arc::new(RwLock::new(SubAgentStatus::Initializing)),
            config,
            agent,
            turn_count: Arc::new(Mutex::new(0)),
            created_at: Utc::now(),
        });

        // Set status to ready
        {
            let mut status = subagent.status.write().await;
            *status = SubAgentStatus::Ready;
        }

        // Create a background task handle (for future use with streaming/monitoring)
        let subagent_clone = Arc::clone(&subagent);
        let handle = tokio::spawn(async move {
            // This could be used for background monitoring, cleanup, etc.
            debug!("Subagent {} background task started", subagent_clone.id);
        });

        debug!("Subagent {} created successfully", subagent.id);
        Ok((subagent, handle))
    }

    /// Get the current status of the subagent
    pub async fn get_status(&self) -> SubAgentStatus {
        self.status.read().await.clone()
    }

    /// Update the status of the subagent
    async fn set_status(&self, status: SubAgentStatus) {
        let mut current_status = self.status.write().await;
        *current_status = status;
    }

    /// Get current progress information
    pub async fn get_progress(&self) -> SubAgentProgress {
        let status = self.get_status().await;
        let turn_count = *self.turn_count.lock().await;
        
        SubAgentProgress {
            subagent_id: self.id.clone(),
            status: status.clone(),
            message: match &status {
                SubAgentStatus::Initializing => "Setting up subagent...".to_string(),
                SubAgentStatus::Ready => "Ready to process messages".to_string(),
                SubAgentStatus::Processing => "Processing request...".to_string(),
                SubAgentStatus::WaitingForInput => "Waiting for next message".to_string(),
                SubAgentStatus::Completed => "Task completed successfully".to_string(),
                SubAgentStatus::Failed(err) => format!("Failed: {}", err),
                SubAgentStatus::Terminated => "Subagent terminated".to_string(),
            },
            turn: turn_count,
            max_turns: self.config.max_turns,
            timestamp: Utc::now(),
        }
    }

    /// Process a message and return the conversation stream
    #[instrument(skip(self, message))]
    pub async fn process_message(&self, message: String) -> Result<()> {
        debug!("Processing message for subagent {}", self.id);

        // Check if we've exceeded max turns
        {
            let turn_count = *self.turn_count.lock().await;
            if let Some(max_turns) = self.config.max_turns {
                if turn_count >= max_turns {
                    self.set_status(SubAgentStatus::Completed).await;
                    return Err(anyhow!("Maximum turns ({}) exceeded", max_turns));
                }
            }
        }

        // Set status to processing
        self.set_status(SubAgentStatus::Processing).await;

        // Add user message to conversation
        let user_message = Message::user().with_text(message.clone());
        {
            let mut conversation = self.conversation.lock().await;
            conversation.push(user_message.clone());
        }

        // Increment turn count
        {
            let mut turn_count = self.turn_count.lock().await;
            *turn_count += 1;
        }

        // For now, we'll create a simple assistant response
        // In a full implementation, this would use the agent to generate a proper response
        let assistant_message = Message::assistant().with_text(format!(
            "Subagent {} received message: {}. This is a placeholder response.",
            self.id, message
        ));
        
        {
            let mut conversation = self.conversation.lock().await;
            conversation.push(assistant_message);
        }

        self.set_status(SubAgentStatus::WaitingForInput).await;
        Ok(())
    }

    /// Add a message to the conversation (for tracking agent responses)
    pub async fn add_message(&self, message: Message) {
        let mut conversation = self.conversation.lock().await;
        conversation.push(message);
    }

    /// Get the full conversation history
    pub async fn get_conversation(&self) -> Vec<Message> {
        self.conversation.lock().await.clone()
    }

    /// Check if the subagent has completed its task
    pub async fn is_completed(&self) -> bool {
        matches!(
            self.get_status().await,
            SubAgentStatus::Completed | SubAgentStatus::Failed(_) | SubAgentStatus::Terminated
        )
    }

    /// Terminate the subagent
    pub async fn terminate(&self) -> Result<()> {
        debug!("Terminating subagent {}", self.id);
        self.set_status(SubAgentStatus::Terminated).await;
        Ok(())
    }

    /// Get formatted conversation for display
    pub async fn get_formatted_conversation(&self) -> String {
        let conversation = self.get_conversation().await;
        let mut formatted = format!("=== Subagent {} Conversation ===\n", self.id);
        formatted.push_str(&format!("Recipe: {}\n", self.config.recipe.title));
        formatted.push_str(&format!("Created: {}\n", self.created_at.format("%Y-%m-%d %H:%M:%S UTC")));
        
        let progress = self.get_progress().await;
        formatted.push_str(&format!("Status: {:?}\n", progress.status));
        formatted.push_str(&format!("Turn: {}", progress.turn));
        if let Some(max_turns) = progress.max_turns {
            formatted.push_str(&format!("/{}", max_turns));
        }
        formatted.push_str("\n\n");

        for (i, message) in conversation.iter().enumerate() {
            formatted.push_str(&format!("{}. {}: {}\n", 
                i + 1,
                match message.role {
                    Role::User => "User",
                    Role::Assistant => "Assistant",
                },
                message.as_concat_text()
            ));
        }

        formatted.push_str("=== End Conversation ===\n");
        formatted
    }
}