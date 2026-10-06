use crate::{
    config::AppConfig,
    domain::{CandidateEvidence, ChatRequest, ChatResponse, ExecutionPath, Intent, MemoryItem},
    provider::ProviderRegistry,
    store::Store,
};
use anyhow::{Result, bail};
use futures::future::join_all;
use std::{sync::Arc, time::Instant};
use uuid::Uuid;

#[derive(Clone)]
pub struct Orchestrator {
    cfg: Arc<AppConfig>,
    providers: ProviderRegistry,
    store: Store,
}

impl Orchestrator {
    pub fn new(cfg: AppConfig, providers: ProviderRegistry, store: Store) -> Self {
        Self {
            cfg: Arc::new(cfg),
            providers,
            store,
        }
    }

    pub fn model_names(&self) -> Vec<String> {
        self.providers.names()
    }

    pub fn add_memory(&self, content: &str, tags: &str) -> Result<i64> {
        self.store.add_memory(content, tags)
    }

    pub fn search_memory(&self, query: &str, limit: usize) -> Result<Vec<MemoryItem>> {
        self.store.search_memory(query, limit)
    }

    pub fn get_run(&self, run_id: &str) -> Result<Option<crate::domain::RunView>> {
        self.store.get_run(run_id)
    }

    pub async fn execute(&self, request: ChatRequest) -> Result<ChatResponse> {
        if request.input.trim().is_empty() {
            bail!("input cannot be empty");
        }
        let started = Instant::now();
        let intent = detect_intent(&request.input);
        let complexity = complexity_score(&request.input);
        let path = match request.mode.as_deref().unwrap_or("auto") {
            "auto" => {
                if complexity >= self.cfg.router.complexity_threshold {
                    ExecutionPath::Deliberate
                } else {
                    ExecutionPath::Fast
                }
            }
            "fast" => ExecutionPath::Fast,
            "deliberate" => ExecutionPath::Deliberate,
            other => bail!("unsupported mode: {other}; use auto, fast, or deliberate"),
        };

        let memory_hits = self
            .store
            .search_memory(&request.input, request.memory_limit.unwrap_or(5).min(20))?;
        let system = system_prompt(&intent, &memory_hits);
        let (evidence, critique, verification, final_answer) = match path {
            ExecutionPath::Fast => {
                let model = self.cfg.router.fast_model.clone();
                let evidence = vec![
                    self.call_candidate(model, system.clone(), request.input.clone())
                        .await,
                ];
                let answer = evidence
                    .iter()
                    .find(|e| e.success)
                    .map(|e| e.output.clone())
                    .unwrap_or_else(|| "No model produced a successful response.".to_string());
                (evidence, None, None, answer)
            }
            ExecutionPath::Deliberate => self.deliberate(&request.input, &system).await,
        };

        let response = ChatResponse {
            run_id: Uuid::new_v4(),
            intent,
            path,
            final_answer,
            evidence,
            critique,
            verification,
            memory_hits,
            elapsed_ms: started.elapsed().as_millis(),
        };
        self.store.save_run(&request.input, &response)?;
        Ok(response)
    }

    async fn call_candidate(
        &self,
        model: String,
        system: String,
        user: String,
    ) -> CandidateEvidence {
        let started = Instant::now();
        match self.providers.invoke(&model, &system, &user).await {
            Ok(output) => CandidateEvidence {
                model,
                output,
                latency_ms: started.elapsed().as_millis(),
                success: true,
                error: None,
            },
            Err(error) => CandidateEvidence {
                model,
                output: String::new(),
                latency_ms: started.elapsed().as_millis(),
                success: false,
                error: Some(error),
            },
        }
    }

    async fn deliberate(
        &self,
        input: &str,
        system: &str,
    ) -> (
        Vec<CandidateEvidence>,
        Option<String>,
        Option<String>,
        String,
    ) {
        let models: Vec<String> = self
            .cfg
            .router
            .deliberation_models
            .iter()
            .take(self.cfg.router.max_parallel)
            .cloned()
            .collect();
        let jobs = models
            .into_iter()
            .map(|model| self.call_candidate(model, system.to_string(), input.to_string()));
        let evidence = join_all(jobs).await;
        let evidence_json = serde_json::to_string_pretty(&evidence).unwrap_or_default();

        let critique_prompt = format!(
            "Original request:\n{input}\n\nCandidate outputs:\n{evidence_json}\n\nIdentify disagreements, unsupported claims, omissions, and the strongest elements. Do not invent evidence."
        );
        let critique = self
            .providers
            .invoke(
                &self.cfg.router.critic_model,
                "You are SANCTUM's cross-model critic. Evaluate candidate outputs, not the user.",
                &critique_prompt,
            )
            .await
            .ok();

        let verification_prompt = format!(
            "Original request:\n{input}\n\nCandidates:\n{evidence_json}\n\nCritique:\n{}\n\nState what is supported by the supplied evidence, what remains uncertain, and what requires external verification.",
            critique.as_deref().unwrap_or("critic unavailable")
        );
        let verification = self.providers.invoke(
            &self.cfg.router.verifier_model,
            "You are SANCTUM's verifier. Never claim external verification unless actual external evidence is present.",
            &verification_prompt,
        ).await.ok();

        let synthesis_prompt = format!(
            "User request:\n{input}\n\nCandidates:\n{evidence_json}\n\nCritique:\n{}\n\nVerification:\n{}\n\nProduce the best final answer. Preserve useful disagreement and uncertainty instead of hiding it.",
            critique.as_deref().unwrap_or("unavailable"),
            verification.as_deref().unwrap_or("unavailable")
        );
        let final_answer = self.providers.invoke(
            &self.cfg.router.synthesizer_model,
            "You are SANCTUM's final synthesizer. Optimize correctness, usefulness, and clarity.",
            &synthesis_prompt,
        ).await.unwrap_or_else(|_| {
            evidence.iter().find(|e| e.success).map(|e| e.output.clone())
                .unwrap_or_else(|| "All candidate and synthesis stages failed.".to_string())
        });

        (evidence, critique, verification, final_answer)
    }
}

pub fn detect_intent(input: &str) -> Intent {
    let lower = input.to_lowercase();
    if lower.starts_with("/create") {
        return Intent::Creative;
    }
    if lower.starts_with("/critic") {
        return Intent::Critic;
    }
    if lower.starts_with("/analyze") || lower.starts_with("/core") {
        return Intent::Analysis;
    }
    if [
        "code",
        "rust",
        "python",
        "typescript",
        "debug",
        "compile",
        "repository",
        "github",
    ]
    .iter()
    .any(|k| lower.contains(k))
    {
        return Intent::Coding;
    }
    if [
        "research", "source", "citation", "paper", "latest", "verify",
    ]
    .iter()
    .any(|k| lower.contains(k))
    {
        return Intent::Research;
    }
    if ["story", "scene", "novel", "character", "dialogue", "poem"]
        .iter()
        .any(|k| lower.contains(k))
    {
        return Intent::Creative;
    }
    if [
        "analyze",
        "architecture",
        "design",
        "compare",
        "reason",
        "strategy",
    ]
    .iter()
    .any(|k| lower.contains(k))
    {
        return Intent::Analysis;
    }
    Intent::General
}

pub fn complexity_score(input: &str) -> f64 {
    let lower = input.to_lowercase();
    let mut score = (input.chars().count() as f64 / 1200.0).min(0.45);
    let markers = [
        "analyze",
        "compare",
        "design",
        "architecture",
        "research",
        "verify",
        "tradeoff",
        "multi-step",
        "production",
        "security",
        "formal",
        "distributed",
    ];
    score += markers.iter().filter(|m| lower.contains(**m)).count() as f64 * 0.09;
    if input.matches('?').count() > 1 {
        score += 0.08;
    }
    if input.lines().count() > 6 {
        score += 0.08;
    }
    score.min(1.0)
}

fn system_prompt(intent: &Intent, memory: &[MemoryItem]) -> String {
    let memory_text = if memory.is_empty() {
        "No relevant SANCTUM memory retrieved.".to_string()
    } else {
        memory
            .iter()
            .map(|m| format!("- [{}] {}", m.tags, m.content))
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "You are a candidate model inside PERSONAL LLM SANCTUM. Intent={intent:?}. Answer the user's request directly. Distinguish facts from inference. Relevant scoped memory:\n{memory_text}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_prefix_controls_intent() {
        assert_eq!(detect_intent("/create write a scene"), Intent::Creative);
        assert_eq!(detect_intent("/critic inspect this"), Intent::Critic);
        assert_eq!(
            detect_intent("/analyze this architecture"),
            Intent::Analysis
        );
    }

    #[test]
    fn coding_intent_is_detected() {
        assert_eq!(detect_intent("debug this Rust repository"), Intent::Coding);
    }

    #[test]
    fn complexity_is_bounded() {
        assert!((0.0..=1.0).contains(&complexity_score("hello")));
        let hard = "analyze compare design architecture research verify tradeoff multi-step production security formal distributed";
        assert!(complexity_score(hard) >= 0.55);
    }
}
