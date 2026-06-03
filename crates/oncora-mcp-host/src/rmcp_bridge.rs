//! Real MCP transport via the official Rust SDK (`rmcp`).
//!
//! Demonstrates Oncora as **both** an MCP server and an MCP host/client behind
//! the existing [`ToolHost`] trait, exactly as `docs/05-tech-decisions.md`
//! prescribes:
//!
//! * [`OncoraMcpServer`] — an `rmcp` server exposing Oncora's tools (here, the
//!   deterministic BSA clinical calculator) so other agents/systems can call
//!   them over MCP.
//! * [`RmcpToolHost`] — an `rmcp` client that connects to an MCP server and
//!   routes its tools through Oncora's [`ToolHost`] trait, so the agent runtime
//!   can compose the assumed in-house VCF/DICOM/calculator servers.
//! * [`serve_loopback`] / [`RmcpToolHost::connect_loopback`] — wires the two
//!   together over an in-process duplex transport for the conformance test.

use async_trait::async_trait;
// Alias our Result so the rmcp macros' generated code can use the std `Result`
// (the `server_handler` expansion returns `Result<_, rmcp::ErrorData>`).
use oncora_core::Result as OResult;
use oncora_core::{OncoraError, ToolCallId, ToolHost, ToolResult};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolRequestParams;
use rmcp::service::{RoleClient, RunningService};
use rmcp::{ServiceExt, schemars, tool, tool_router};
use serde_json::Value;
use tokio::task::JoinHandle;

fn map_err(e: impl std::fmt::Display) -> OncoraError {
    OncoraError::Tool(format!("rmcp: {e}"))
}

/// Parameters for the BSA tool (schema derived from these fields).
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct BsaParams {
    /// Patient height in centimetres.
    pub height_cm: f64,
    /// Patient weight in kilograms.
    pub weight_kg: f64,
}

/// An `rmcp` MCP server exposing Oncora's deterministic tools.
#[derive(Clone)]
pub struct OncoraMcpServer;

#[tool_router(server_handler)]
impl OncoraMcpServer {
    /// Body-surface-area (Mosteller), a deterministic clinical calculator.
    #[tool(description = "Body-surface-area in m^2 (Mosteller formula)")]
    fn bsa_mosteller(
        &self,
        Parameters(BsaParams {
            height_cm,
            weight_kg,
        }): Parameters<BsaParams>,
    ) -> String {
        let bsa = ((height_cm * weight_kg) / 3600.0).sqrt();
        let bsa = (bsa * 1000.0).round() / 1000.0;
        format!("{{\"bsa_m2\":{bsa}}}")
    }
}

/// An MCP client exposed through Oncora's [`ToolHost`] trait.
pub struct RmcpToolHost {
    client: RunningService<RoleClient, ()>,
    /// Keeps the in-process server alive for loopback usage.
    _server: Option<JoinHandle<()>>,
}

impl RmcpToolHost {
    /// Connect to an [`OncoraMcpServer`] over an in-process duplex transport.
    pub async fn connect_loopback() -> OResult<Self> {
        let (server_io, client_io) = tokio::io::duplex(8 * 1024);

        // The server's `serve()` only returns once the MCP initialize handshake
        // completes — which needs the client connected. So run the whole server
        // serve inside a task and let the client drive initialization, instead
        // of awaiting the server first (which would deadlock).
        let server_task = tokio::spawn(async move {
            if let Ok(server) = OncoraMcpServer.serve(server_io).await {
                let _ = server.waiting().await;
            }
        });

        let client = ().serve(client_io).await.map_err(map_err)?;
        Ok(Self {
            client,
            _server: Some(server_task),
        })
    }
}

#[async_trait]
impl ToolHost for RmcpToolHost {
    async fn list_tools(&self) -> OResult<Vec<String>> {
        let tools = self.client.list_all_tools().await.map_err(map_err)?;
        Ok(tools.into_iter().map(|t| t.name.to_string()).collect())
    }

    async fn call_tool(&self, name: &str, args: Value) -> OResult<ToolResult> {
        let arguments = match args {
            Value::Object(m) => Some(m),
            Value::Null => None,
            other => {
                return Err(OncoraError::Invalid(format!(
                    "tool args must be a JSON object, got {other}"
                )));
            }
        };
        let mut req = CallToolRequestParams::new(name.to_string());
        req.arguments = arguments;
        let result = self.client.call_tool(req).await.map_err(map_err)?;
        let output = serde_json::to_value(&result)?;
        Ok(ToolResult {
            call_id: ToolCallId::random(),
            tool: name.to_string(),
            output,
            deterministic: true,
        })
    }
}

/// Spawn an [`OncoraMcpServer`] over the given duplex half (helper for tests
/// and examples). The serve+handshake runs entirely inside the task so the
/// caller can connect a client without deadlocking.
pub fn serve_loopback(io: tokio::io::DuplexStream) -> JoinHandle<()> {
    tokio::spawn(async move {
        if let Ok(server) = OncoraMcpServer.serve(io).await {
            let _ = server.waiting().await;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rmcp_loopback_lists_and_calls() {
        let host = RmcpToolHost::connect_loopback().await.unwrap();

        let tools = host.list_tools().await.unwrap();
        assert!(
            tools.iter().any(|t| t == "bsa_mosteller"),
            "tools advertised over MCP: {tools:?}"
        );

        let res = host
            .call_tool(
                "bsa_mosteller",
                serde_json::json!({ "height_cm": 170.0, "weight_kg": 70.0 }),
            )
            .await
            .unwrap();

        let rendered = serde_json::to_string(&res.output).unwrap();
        assert!(
            rendered.contains("1.818"),
            "BSA result routed back through ToolHost: {rendered}"
        );
    }
}
