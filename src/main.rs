mod config;
mod domain;
mod provider;
mod router;
mod store;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{Html, IntoResponse},
    routing::{get, post},
};
use config::AppConfig;
use domain::{ChatRequest, MemoryWrite};
use provider::ProviderRegistry;
use router::Orchestrator;
use serde::Deserialize;
use serde_json::json;
use std::{env, sync::Arc};
use tower_http::trace::TraceLayer;
use tracing::{error, info};

#[derive(Clone)]
struct AppState {
    orchestrator: Arc<Orchestrator>,
}

#[derive(Debug, Deserialize)]
struct MemoryQuery {
    q: Option<String>,
    limit: Option<usize>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let config_path =
        env::var("SANCTUM_CONFIG").unwrap_or_else(|_| "config/sanctum.toml".to_string());
    let cfg = AppConfig::load(&config_path)?;
    let providers = ProviderRegistry::from_config(&cfg).map_err(anyhow::Error::msg)?;
    let store = store::Store::open(&cfg.server.database)?;
    let orchestrator = Arc::new(Orchestrator::new(cfg.clone(), providers, store));
    let state = AppState { orchestrator };

    let app = Router::new()
        .route("/", get(index))
        .route("/app.js", get(app_js))
        .route("/manifest.webmanifest", get(manifest))
        .route("/sw.js", get(service_worker))
        .route("/health", get(health))
        .route("/v1/models", get(models))
        .route("/v1/chat", post(chat))
        .route("/v1/memory", post(add_memory).get(search_memory))
        .route("/v1/runs/{run_id}", get(get_run))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&cfg.server.bind).await?;
    info!(bind = %cfg.server.bind, "PERSONAL LLM SANCTUM online");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn index() -> Html<&'static str> {
    Html(include_str!("../web/index.html"))
}

async fn app_js() -> impl IntoResponse {
    (
        [(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )],
        include_str!("../web/app.js"),
    )
}

async fn manifest() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/manifest+json")],
        include_str!("../web/manifest.webmanifest"),
    )
}

async fn service_worker() -> impl IntoResponse {
    (
        [(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )],
        include_str!("../web/sw.js"),
    )
}

async fn health(State(state): State<AppState>) -> impl IntoResponse {
    Json(json!({
        "status": "ok",
        "service": "personal-llm-sanctum",
        "models": state.orchestrator.model_names(),
    }))
}

async fn models(State(state): State<AppState>) -> impl IntoResponse {
    Json(json!({"models": state.orchestrator.model_names()}))
}

async fn chat(
    State(state): State<AppState>,
    Json(request): Json<ChatRequest>,
) -> impl IntoResponse {
    match state.orchestrator.execute(request).await {
        Ok(response) => (StatusCode::OK, Json(json!(response))).into_response(),
        Err(err) => {
            error!(error = %err, "chat execution failed");
            (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": err.to_string()})),
            )
                .into_response()
        }
    }
}

async fn add_memory(
    State(state): State<AppState>,
    Json(memory): Json<MemoryWrite>,
) -> impl IntoResponse {
    if memory.content.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "content cannot be empty"})),
        )
            .into_response();
    }
    match state.orchestrator.add_memory(&memory.content, &memory.tags) {
        Ok(id) => (StatusCode::CREATED, Json(json!({"id": id}))).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": err.to_string()})),
        )
            .into_response(),
    }
}

async fn search_memory(
    State(state): State<AppState>,
    Query(query): Query<MemoryQuery>,
) -> impl IntoResponse {
    let q = query.q.unwrap_or_default();
    let limit = query.limit.unwrap_or(10).min(50);
    match state.orchestrator.search_memory(&q, limit) {
        Ok(items) => (StatusCode::OK, Json(json!({"items": items}))).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": err.to_string()})),
        )
            .into_response(),
    }
}

async fn get_run(State(state): State<AppState>, Path(run_id): Path<String>) -> impl IntoResponse {
    match state.orchestrator.get_run(&run_id) {
        Ok(Some(run)) => (StatusCode::OK, Json(json!(run))).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "run not found"})),
        )
            .into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": err.to_string()})),
        )
            .into_response(),
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install terminate handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
}
