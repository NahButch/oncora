//! The system's first-class ability to decline: accept, abstain, or escalate.

use serde::{Deserialize, Serialize};

/// Where an escalated decision is routed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EscalationTarget {
    /// A human reviewer / domain expert.
    HumanReviewer,
    /// A more specialized agent (named).
    SpecialistAgent(String),
}

/// The outcome of an uncertainty-gated decision.
///
/// An agent that cannot reach calibrated confidence must `Abstain` or
/// `Escalate` rather than confabulate. Every non-`Accept` verdict carries a
/// machine- and human-readable reason and is logged.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verdict {
    /// Confidence (and grounding) cleared the bar; the answer stands.
    Accept,
    /// Declined to answer; the reason explains why.
    Abstain { reason: String },
    /// Routed elsewhere for adjudication.
    Escalate {
        to: EscalationTarget,
        reason: String,
    },
}

impl Verdict {
    pub fn is_accept(&self) -> bool {
        matches!(self, Verdict::Accept)
    }

    /// Short label for logs/telemetry.
    pub fn label(&self) -> &'static str {
        match self {
            Verdict::Accept => "accept",
            Verdict::Abstain { .. } => "abstain",
            Verdict::Escalate { .. } => "escalate",
        }
    }
}
