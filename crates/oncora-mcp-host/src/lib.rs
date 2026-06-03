//! # oncora-mcp-host
//!
//! Hosts and routes tool calls over the Model Context Protocol. In production
//! this wraps the official Rust SDK (`rmcp`) to act as both an MCP host (to
//! in-house genomics/imaging/calculator servers) and to expose Oncora's own
//! tools; see `docs/05-tech-decisions.md`.
//!
//! The key governance property lives here: **every tool call is audited** and,
//! when the tool is a deterministic oracle, the result is replayable. The
//! [`Tool`] trait is the local extension point; [`McpHost`] implements the
//! [`ToolHost`] boundary that agents depend on.

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use oncora_core::{OncoraError, Result, ToolCallId, ToolHost, ToolResult};
use serde_json::{json, Value};

/// A tool that can be invoked by name with JSON arguments.
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    /// Deterministic oracles (calculators, KG queries) can ground uncertainty.
    fn deterministic(&self) -> bool {
        true
    }
    async fn call(&self, args: Value) -> Result<Value>;
}

/// One recorded tool invocation, retained for the audit trail.
#[derive(Clone, Debug)]
pub struct AuditRecord {
    pub call_id: ToolCallId,
    pub tool: String,
    pub args: Value,
    pub ok: bool,
}

/// The tool host: a registry of [`Tool`]s plus an append-only audit log.
#[derive(Default)]
pub struct McpHost {
    tools: HashMap<String, Box<dyn Tool>>,
    audit: Mutex<Vec<AuditRecord>>,
}

impl McpHost {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a tool. Returns `self` for builder-style wiring.
    pub fn with_tool(mut self, tool: Box<dyn Tool>) -> Self {
        self.tools.insert(tool.name().to_string(), tool);
        self
    }

    /// Snapshot of the audit log (for provenance / replay).
    pub fn audit_log(&self) -> Vec<AuditRecord> {
        self.audit.lock().map(|g| g.clone()).unwrap_or_default()
    }
}

#[async_trait]
impl ToolHost for McpHost {
    async fn list_tools(&self) -> Result<Vec<String>> {
        Ok(self.tools.keys().cloned().collect())
    }

    async fn call_tool(&self, name: &str, args: Value) -> Result<ToolResult> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| OncoraError::NotFound(format!("tool {name}")))?;
        let call_id = ToolCallId::random();
        tracing::info!(tool = name, call_id = %call_id, "tool call");
        let result = tool.call(args.clone()).await;
        if let Ok(mut log) = self.audit.lock() {
            log.push(AuditRecord {
                call_id: call_id.clone(),
                tool: name.to_string(),
                args,
                ok: result.is_ok(),
            });
        }
        let output = result?;
        Ok(ToolResult {
            call_id,
            tool: name.to_string(),
            output,
            deterministic: tool.deterministic(),
        })
    }
}

/// A deterministic clinical calculator: body-surface-area (Mosteller).
/// Stands in for the in-house clinical-calculators MCP server.
pub struct BsaCalculator;

#[async_trait]
impl Tool for BsaCalculator {
    fn name(&self) -> &str {
        "bsa_mosteller"
    }

    async fn call(&self, args: Value) -> Result<Value> {
        let h = args
            .get("height_cm")
            .and_then(Value::as_f64)
            .ok_or_else(|| OncoraError::Invalid("height_cm required".into()))?;
        let w = args
            .get("weight_kg")
            .and_then(Value::as_f64)
            .ok_or_else(|| OncoraError::Invalid("weight_kg required".into()))?;
        let bsa = ((h * w) / 3600.0).sqrt();
        Ok(json!({ "bsa_m2": (bsa * 1000.0).round() / 1000.0 }))
    }
}

/// Echo tool, useful for wiring tests.
pub struct EchoTool;

#[async_trait]
impl Tool for EchoTool {
    fn name(&self) -> &str {
        "echo"
    }
    async fn call(&self, args: Value) -> Result<Value> {
        Ok(args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn calls_calculator_and_audits() {
        let host = McpHost::new().with_tool(Box::new(BsaCalculator));
        let res = host
            .call_tool("bsa_mosteller", json!({"height_cm": 170, "weight_kg": 70}))
            .await
            .unwrap();
        assert!(res.deterministic);
        assert_eq!(res.output["bsa_m2"], json!(1.818));
        assert_eq!(host.audit_log().len(), 1);
    }
}
