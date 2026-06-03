//! Oncora HTTP API.
//!
//! Exposes the agent runtime behind the trust boundary. Routes:
//! * `GET  /health` — liveness.
//! * `GET  /tools`  — list the MCP tools the host can route to.
//! * `POST /query`  — run a target-discovery query, returns a cited,
//!   confidence-scored [`Answer`] with its verdict.
//!
//! Auth/RBAC, the egress proxy, and the provenance ledger described in
//! `docs/08-roadmap.md` layer on top of this skeleton.

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use oncora_agents::{run_target_discovery, Answer, Platform};
use oncora_core::{MemoryKey, ProjectId, ScientistId, WorkflowId};
use oncora_ingest::{ingest, Document};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct QueryReq {
    question: String,
    #[serde(default)]
    entity: Option<String>,
}

#[derive(Debug, Serialize)]
struct ToolsResp {
    tools: Vec<String>,
}

type ApiError = (StatusCode, String);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    oncora_telemetry::init();

    let platform = Platform::demo();
    seed(&platform).await?;

    let app = Router::new()
        .route("/health", get(health))
        .route("/tools", get(tools))
        .route("/query", post(query))
        .with_state(platform);

    let addr = std::env::var("ONCORA_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(%addr, "oncora-api listening");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> &'static str {
    "ok"
}

async fn tools(State(p): State<Platform>) -> Result<Json<ToolsResp>, ApiError> {
    let tools = p.tools.list_tools().await.map_err(internal)?;
    Ok(Json(ToolsResp { tools }))
}

async fn query(
    State(p): State<Platform>,
    Json(req): Json<QueryReq>,
) -> Result<Json<Answer>, ApiError> {
    let key = MemoryKey {
        scientist: ScientistId::new("api"),
        project: ProjectId::new("default"),
        workflow: WorkflowId::new("http"),
    };
    let answer = run_target_discovery(&p, &key, &req.question, req.entity.as_deref())
        .await
        .map_err(internal)?;
    Ok(Json(answer))
}

fn internal(e: impl std::fmt::Display) -> ApiError {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

/// Seed a tiny demo corpus so the API returns grounded answers out of the box.
async fn seed(p: &Platform) -> Result<(), Box<dyn std::error::Error>> {
    let docs = vec![
        Document::new(
            "PMID:0001",
            "EGFR signalling in NSCLC",
            "EGFR activating mutations drive non-small-cell lung cancer. EGFR \
             tyrosine kinase inhibitors produce durable responses in mutant tumours.",
        )
        .about("EGFR"),
        Document::new(
            "PMID:0002",
            "BRAF V600E in melanoma",
            "The BRAF V600E mutation activates MAPK signalling in melanoma and \
             confers sensitivity to BRAF inhibitors.",
        )
        .about("BRAF"),
    ];
    ingest(&docs, &*p.embedder, &*p.vectors, &*p.graph, &p.artifacts).await?;
    Ok(())
}
