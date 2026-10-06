use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Intent {
    General,
    Analysis,
    Coding,
    Research,
    Creative,
    Critic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionPath {
    Fast,
    Deliberate,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChatRequest {
    pub input: String,
    pub mode: Option<String>,
    pub memory_limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CandidateEvidence {
    pub model: String,
    pub output: String,
    pub latency_ms: u128,
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatResponse {
    pub run_id: Uuid,
    pub intent: Intent,
    pub path: ExecutionPath,
    pub final_answer: String,
    pub evidence: Vec<CandidateEvidence>,
    pub critique: Option<String>,
    pub verification: Option<String>,
    pub memory_hits: Vec<MemoryItem>,
    pub elapsed_ms: u128,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MemoryWrite {
    pub content: String,
    #[serde(default)]
    pub tags: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryItem {
    pub id: i64,
    pub content: String,
    pub tags: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunView {
    pub run_id: String,
    pub input: String,
    pub intent: String,
    pub path: String,
    pub models_json: String,
    pub evidence_json: String,
    pub critique: Option<String>,
    pub verification: Option<String>,
    pub final_answer: String,
    pub elapsed_ms: i64,
    pub created_at: String,
}
