//! Claims, evidence, and provenance — the substance behind every conclusion.

use serde::{Deserialize, Serialize};

use crate::confidence::Confidence;
use crate::ids::{ModelPin, SnapshotId, ToolCallId};

/// A reference to a source document/record that grounds a claim.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRef {
    /// Stable id (e.g. PubMed id, KG URI, snapshot-relative path).
    pub id: String,
    /// Optional dereferenceable URI within the trust boundary.
    pub uri: Option<String>,
    /// Human-readable title for display/citation.
    pub title: Option<String>,
}

impl SourceRef {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            uri: None,
            title: None,
        }
    }
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
}

/// A single assertion the system may reason about.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claim {
    pub id: String,
    pub text: String,
}

impl Claim {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            text: text.into(),
        }
    }
}

/// Everything needed to reproduce and audit a conclusion.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    /// Sources that ground the conclusion.
    pub sources: Vec<SourceRef>,
    /// Tool calls made en route (audited, deterministic).
    pub tool_calls: Vec<ToolCallId>,
    /// The generating model pin.
    pub model: ModelPin,
    /// The data snapshot the run read from.
    pub snapshot: SnapshotId,
}

impl Provenance {
    pub fn new(model: ModelPin, snapshot: SnapshotId) -> Self {
        Self {
            sources: Vec::new(),
            tool_calls: Vec::new(),
            model,
            snapshot,
        }
    }
    pub fn cite(mut self, source: SourceRef) -> Self {
        self.sources.push(source);
        self
    }
    pub fn record_tool(mut self, id: ToolCallId) -> Self {
        self.tool_calls.push(id);
        self
    }
}

/// A claim together with the evidence for and against it, a calibrated
/// confidence, and full provenance. This is the unit that flows through the
/// agent runtime and is persisted into provenance/evidence memory.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    pub claim: Claim,
    /// Supporting snippets / facts.
    pub support: Vec<String>,
    /// Contradicting snippets / facts (kept, not silently dropped).
    pub contradiction: Vec<String>,
    pub confidence: Confidence,
    pub provenance: Provenance,
}

impl Evidence {
    pub fn new(claim: Claim, confidence: Confidence, provenance: Provenance) -> Self {
        Self {
            claim,
            support: Vec::new(),
            contradiction: Vec::new(),
            confidence,
            provenance,
        }
    }

    /// Net support: supporting minus contradicting items. Negative means the
    /// weight of evidence is against the claim.
    pub fn net_support(&self) -> i64 {
        self.support.len() as i64 - self.contradiction.len() as i64
    }
}
