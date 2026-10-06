use crate::config::{AppConfig, ModelConfig};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use std::{collections::HashMap, env, sync::Arc};

#[async_trait]
pub trait ModelProvider: Send + Sync {
    async fn invoke(&self, system: &str, user: &str) -> Result<String, String>;
}

#[derive(Clone)]
pub struct ProviderRegistry {
    providers: Arc<HashMap<String, Arc<dyn ModelProvider>>>,
}

impl ProviderRegistry {
    pub fn from_config(cfg: &AppConfig) -> Result<Self, String> {
        let mut providers: HashMap<String, Arc<dyn ModelProvider>> = HashMap::new();
        for model in cfg.models.iter().filter(|m| m.enabled) {
            let provider: Arc<dyn ModelProvider> = match model.kind.as_str() {
                "mock" => Arc::new(MockProvider {
                    name: model.name.clone(),
                    role: model.model.clone(),
                }),
                "openai_compatible" => Arc::new(OpenAiCompatibleProvider::new(model)?),
                other => return Err(format!("unsupported provider kind: {other}")),
            };
            providers.insert(model.name.clone(), provider);
        }
        Ok(Self {
            providers: Arc::new(providers),
        })
    }

    pub async fn invoke(&self, name: &str, system: &str, user: &str) -> Result<String, String> {
        let provider = self
            .providers
            .get(name)
            .ok_or_else(|| format!("provider not found: {name}"))?;
        provider.invoke(system, user).await
    }

    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<_> = self.providers.keys().cloned().collect();
        names.sort();
        names
    }
}

struct MockProvider {
    name: String,
    role: String,
}

#[async_trait]
impl ModelProvider for MockProvider {
    async fn invoke(&self, system: &str, user: &str) -> Result<String, String> {
        let clipped: String = user.chars().take(900).collect();
        let output = match self.role.as_str() {
            "critic" => format!(
                "MOCK_CRITIC: compared candidate outputs; flag disagreements and unsupported claims. Context={clipped}"
            ),
            "verifier" => format!(
                "MOCK_VERIFIER: verification stage executed; external factual validation is not claimed in mock mode. Context={clipped}"
            ),
            "synthesizer" => format!(
                "MOCK_SYNTHESIS: pipeline completed end-to-end. Replace mock models with configured providers for a real model answer. Context={clipped}"
            ),
            _ => format!(
                "MOCK_CANDIDATE[{}:{}]: system={} | input={clipped}",
                self.name,
                self.role,
                system.chars().take(160).collect::<String>()
            ),
        };
        Ok(output)
    }
}

struct OpenAiCompatibleProvider {
    client: Client,
    base_url: String,
    api_key: String,
    model: String,
}

impl OpenAiCompatibleProvider {
    fn new(cfg: &ModelConfig) -> Result<Self, String> {
        let base_url = cfg
            .base_url
            .clone()
            .ok_or_else(|| format!("{} missing base_url", cfg.name))?;
        let env_name = cfg
            .api_key_env
            .clone()
            .ok_or_else(|| format!("{} missing api_key_env", cfg.name))?;
        let api_key = env::var(&env_name)
            .map_err(|_| format!("{} requires environment variable {env_name}", cfg.name))?;
        Ok(Self {
            client: Client::new(),
            base_url,
            api_key,
            model: cfg.model.clone(),
        })
    }
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Debug, Deserialize)]
struct Message {
    content: String,
}

#[async_trait]
impl ModelProvider for OpenAiCompatibleProvider {
    async fn invoke(&self, system: &str, user: &str) -> Result<String, String> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let response = self
            .client
            .post(url)
            .bearer_auth(&self.api_key)
            .json(&json!({
                "model": self.model,
                "messages": [
                    {"role": "system", "content": system},
                    {"role": "user", "content": user}
                ],
                "temperature": 0.2
            }))
            .send()
            .await
            .map_err(|e| format!("request failed: {e}"))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(format!(
                "provider returned {status}: {}",
                body.chars().take(500).collect::<String>()
            ));
        }

        let parsed: ChatResponse = response
            .json()
            .await
            .map_err(|e| format!("invalid provider response: {e}"))?;
        parsed
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| "provider response had no choices".to_string())
    }
}
