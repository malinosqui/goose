use std::sync::Arc;

use anyhow::Result;

use super::{
    base::Provider,
    databricks::{DatabricksProvider, DatabricksProviderConfig},
    openai::{OpenAiProvider, OpenAiProviderConfig},
    pool::{global_pool_manager, get_pooled_provider, PoolConfig},
};
use crate::model::ModelConfig;

/// Create a new provider instance directly (without pooling)
pub fn create(
    name: &str,
    provider_config: serde_json::Value,
    model: ModelConfig,
) -> Result<Arc<dyn Provider>> {
    // We use Arc instead of Box to be able to clone for multiple async tasks
    match name {
        "openai" => {
            let config: OpenAiProviderConfig = serde_json::from_value(provider_config)?;
            Ok(Arc::new(OpenAiProvider::from_config(config, model)?))
        }
        "databricks" => {
            let config: DatabricksProviderConfig = serde_json::from_value(provider_config)?;
            Ok(Arc::new(DatabricksProvider::from_config(config, model)?))
        }
        _ => Err(anyhow::anyhow!("Unknown provider: {}", name)),
    }
}

/// Create a provider from the pool or directly if the pool is not available
/// 
/// This is an async function that will get a provider from the pool if available,
/// or create a new one if the pool is not available.
pub async fn create_or_get_pooled(
    name: &str, 
    provider_config: serde_json::Value,
    model: ModelConfig,
    use_pool: bool,
) -> Result<Arc<dyn Provider>> {
    if use_pool {
        match get_pooled_provider(name, provider_config.clone(), model.clone()).await {
            Ok(pooled) => Ok(Arc::new(pooled)),
            Err(e) => {
                tracing::warn!("Failed to get provider from pool: {}", e);
                create(name, provider_config, model)
            }
        }
    } else {
        create(name, provider_config, model)
    }
}
