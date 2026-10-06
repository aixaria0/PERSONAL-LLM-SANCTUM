use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::{collections::HashSet, fs};

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub router: RouterConfig,
    pub models: Vec<ModelConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub bind: String,
    pub database: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RouterConfig {
    pub complexity_threshold: f64,
    pub max_parallel: usize,
    pub fast_model: String,
    pub critic_model: String,
    pub verifier_model: String,
    pub synthesizer_model: String,
    pub deliberation_models: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModelConfig {
    pub name: String,
    pub kind: String,
    pub model: String,
    pub base_url: Option<String>,
    pub api_key_env: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

impl AppConfig {
    pub fn load(path: &str) -> Result<Self> {
        let raw =
            fs::read_to_string(path).with_context(|| format!("failed to read config: {path}"))?;
        let cfg: Self =
            toml::from_str(&raw).with_context(|| format!("failed to parse config: {path}"))?;
        cfg.validate()?;
        Ok(cfg)
    }

    fn validate(&self) -> Result<()> {
        if !(0.0..=1.0).contains(&self.router.complexity_threshold) {
            bail!("router.complexity_threshold must be between 0 and 1");
        }
        if self.router.max_parallel == 0 {
            bail!("router.max_parallel must be at least 1");
        }

        let enabled: HashSet<_> = self
            .models
            .iter()
            .filter(|m| m.enabled)
            .map(|m| m.name.as_str())
            .collect();

        let required = [
            self.router.fast_model.as_str(),
            self.router.critic_model.as_str(),
            self.router.verifier_model.as_str(),
            self.router.synthesizer_model.as_str(),
        ];
        for name in required {
            if !enabled.contains(name) {
                bail!("router references disabled or missing model: {name}");
            }
        }
        for name in &self.router.deliberation_models {
            if !enabled.contains(name.as_str()) {
                bail!("deliberation model is disabled or missing: {name}");
            }
        }
        Ok(())
    }
}
