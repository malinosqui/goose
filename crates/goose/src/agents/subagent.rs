use std::sync::Arc;

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use mcp_core::role::Role;
use mcp_core::handler::ToolError;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, error, instrument};
use uuid::Uuid;

use crate::agents::Agent;
use crate::message::Message;
use crate::providers::base::Provider;
use crate::providers::errors::ProviderError;
use crate::recipe::Recipe;

/// Status of a subagent
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SubAgentStatus {
    Ready,           // Ready to process messages
    Processing,      // Currently working on a task
    Completed(String), // Task completed (with optional message for success/error)
    Terminated,      // Manually terminated
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
    pub parent_agent: Arc<Agent>,  // Reference to parent agent
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
        let parent_agent = Arc::new(Agent::new());
        parent_agent.update_provider(provider).await?;

        // Set up extensions from the recipe
        if let Some(extensions) = &config.recipe.extensions {
            for extension in extensions {
                if let Err(e) = parent_agent.add_extension(extension.clone()).await {
                    error!("Failed to add extension to subagent {}: {}", config.id, e);
                    return Err(anyhow!("Failed to add extension: {}", e));
                }
            }
        }

        // Set up custom system prompt from recipe instructions
        if let Some(instructions) = &config.recipe.instructions {
            parent_agent.extend_system_prompt(instructions.clone()).await;
        }

        let subagent = Arc::new(SubAgent {
            id: config.id.clone(),
            conversation: Arc::new(Mutex::new(Vec::new())),
            status: Arc::new(RwLock::new(SubAgentStatus::Ready)),
            config,
            parent_agent,
            turn_count: Arc::new(Mutex::new(0)),
            created_at: Utc::now(),
        });

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
                SubAgentStatus::Ready => "Ready to process messages".to_string(),
                SubAgentStatus::Processing => "Processing request...".to_string(),
                SubAgentStatus::Completed(msg) => msg.clone(),
                SubAgentStatus::Terminated => "Subagent terminated".to_string(),
            },
            turn: turn_count,
            max_turns: self.config.max_turns,
            timestamp: Utc::now(),
        }
    }

    /// Process a message and generate a response using the subagent's provider
    #[instrument(skip(self, message))]
    pub async fn reply_subagent(&self, message: String) -> Result<Message> {
        debug!("Processing message for subagent {}", self.id);

        // Check if we've exceeded max turns
        {
            let turn_count = *self.turn_count.lock().await;
            if let Some(max_turns) = self.config.max_turns {
                if turn_count >= max_turns {
                    self.set_status(SubAgentStatus::Completed("Maximum turns exceeded".to_string())).await;
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

        // Get the current conversation for context
        let messages = self.get_conversation().await;

        // Get tools and system prompt from the agent
        let (tools, toolshim_tools, system_prompt) = self.parent_agent.prepare_tools_and_prompt().await?;

        // Generate response from provider
        match Agent::generate_response_from_provider(
            self.parent_agent.provider().await?,
            &system_prompt,
            &messages,
            &tools,
            &toolshim_tools,
        ).await {
            Ok((response, _usage)) => {
                // Add the assistant's response to the conversation
                self.add_message(response.clone()).await;

                // Process any tool calls in the response
                let (_, remaining_requests, filtered_response) = 
                    self.parent_agent.categorize_tool_requests(&response).await;

                // Process all tool requests directly without permission checks
                let mut final_response = filtered_response.clone();

                // Handle remaining tools
                for request in &remaining_requests {
                    if let Ok(tool_call) = &request.tool_call {
                        let extension_manager = self.parent_agent.extension_manager.lock().await;
                        match extension_manager.dispatch_tool_call(tool_call.clone()).await {
                            Ok(tool_result) => {
                                let tool_response = tool_result.result.await;
                                final_response = final_response.with_tool_response(request.id.clone(), tool_response);
                            }
                            Err(e) => {
                                final_response = final_response.with_tool_response(
                                    request.id.clone(),
                                    Err(ToolError::ExecutionError(e.to_string()))
                                );
                            }
                        }
                    }
                }

                // Set status back to ready
                self.set_status(SubAgentStatus::Ready).await;
                Ok(final_response)
            },
            Err(ProviderError::ContextLengthExceeded(_)) => {
                self.set_status(SubAgentStatus::Completed("Context length exceeded".to_string())).await;
                Ok(Message::assistant().with_context_length_exceeded(
                    "The context length of the model has been exceeded. Please start a new session and try again.",
                ))
            },
            Err(e) => {
                self.set_status(SubAgentStatus::Completed(format!("Error: {}", e))).await;
                error!("Error: {}", e);
                Ok(Message::assistant().with_text(format!("Ran into this error: {e}.\n\nPlease retry if you think this is a transient or recoverable error.")))
            }
        }
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
            SubAgentStatus::Completed(_) | SubAgentStatus::Terminated
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
        formatted.push_str(&format!(
            "Created: {}\n",
            self.created_at.format("%Y-%m-%d %H:%M:%S UTC")
        ));

        let progress = self.get_progress().await;
        formatted.push_str(&format!("Status: {:?}\n", progress.status));
        formatted.push_str(&format!("Turn: {}", progress.turn));
        if let Some(max_turns) = progress.max_turns {
            formatted.push_str(&format!("/{}", max_turns));
        }
        formatted.push_str("\n\n");

        for (i, message) in conversation.iter().enumerate() {
            formatted.push_str(&format!(
                "{}. {}: {}\n",
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
