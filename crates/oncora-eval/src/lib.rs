//! # oncora-eval
//!
//! The benchmarking & evaluation harness (named pillar; design in
//! `docs/06-eval-benchmarking.md`). It runs golden cases through the agent,
//! scores accuracy, calibration (ECE) and abstention quality, and applies a CI
//! [`GatePolicy`]. Production compares against human-expert labels and
//! computational baselines and records every number to the artifact store for
//! reproducibility; this reference harness scores the agent against gold labels.

use oncora_agents::{run_target_discovery, Platform};
use oncora_core::{MemoryKey, ProjectId, Result, ScientistId, Verdict, WorkflowId};
use oncora_uncertainty::expected_calibration_error;
use serde::{Deserialize, Serialize};

/// One labelled benchmark case.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BenchmarkCase {
    pub id: String,
    pub question: String,
    pub focus_entity: Option<String>,
    /// The expert-labelled correct answer token expected in the response.
    pub gold: String,
}

/// The outcome of running one case.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunResult {
    pub case_id: String,
    pub answered: bool,
    pub correct: bool,
    pub confidence: f64,
    pub verdict: String,
}

/// Aggregate metrics over a run set.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Score {
    pub n: usize,
    /// Accuracy over *answered* (non-abstained) cases.
    pub selective_accuracy: f64,
    /// Fraction of cases the agent chose to answer.
    pub coverage: f64,
    /// Expected calibration error over answered cases.
    pub ece: f64,
}

/// CI gate thresholds. A run must clear all of them to pass.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GatePolicy {
    pub min_selective_accuracy: f64,
    pub min_coverage: f64,
    pub max_ece: f64,
}

impl Default for GatePolicy {
    fn default() -> Self {
        Self {
            min_selective_accuracy: 0.7,
            min_coverage: 0.5,
            max_ece: 0.15,
        }
    }
}

/// Outcome of applying a [`GatePolicy`] to a [`Score`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GateReport {
    pub passed: bool,
    pub failures: Vec<String>,
}

fn eval_key(case_id: &str) -> MemoryKey {
    MemoryKey {
        scientist: ScientistId::new("eval"),
        project: ProjectId::new("eval"),
        workflow: WorkflowId::new(case_id),
    }
}

/// Run every case through the agent.
pub async fn run_benchmark(p: &Platform, cases: &[BenchmarkCase]) -> Result<Vec<RunResult>> {
    let mut out = Vec::with_capacity(cases.len());
    for case in cases {
        let key = eval_key(&case.id);
        let ans =
            run_target_discovery(p, &key, &case.question, case.focus_entity.as_deref()).await?;
        let answered = !matches!(ans.verdict, Verdict::Abstain { .. });
        let correct = answered && ans.text.to_lowercase().contains(&case.gold.to_lowercase());
        out.push(RunResult {
            case_id: case.id.clone(),
            answered,
            correct,
            confidence: ans.confidence.get(),
            verdict: ans.verdict.label().to_string(),
        });
    }
    Ok(out)
}

/// Aggregate run results into [`Score`].
pub fn summarize(results: &[RunResult]) -> Score {
    let n = results.len();
    let answered: Vec<&RunResult> = results.iter().filter(|r| r.answered).collect();
    let coverage = if n == 0 {
        0.0
    } else {
        answered.len() as f64 / n as f64
    };
    let selective_accuracy = if answered.is_empty() {
        0.0
    } else {
        answered.iter().filter(|r| r.correct).count() as f64 / answered.len() as f64
    };
    let pairs: Vec<(f64, bool)> = answered.iter().map(|r| (r.confidence, r.correct)).collect();
    let ece = expected_calibration_error(&pairs, 10);
    Score {
        n,
        selective_accuracy,
        coverage,
        ece,
    }
}

/// Apply the CI gate.
pub fn gate(score: &Score, policy: &GatePolicy) -> GateReport {
    let mut failures = Vec::new();
    if score.selective_accuracy < policy.min_selective_accuracy {
        failures.push(format!(
            "selective accuracy {:.3} < {:.3}",
            score.selective_accuracy, policy.min_selective_accuracy
        ));
    }
    if score.coverage < policy.min_coverage {
        failures.push(format!(
            "coverage {:.3} < {:.3}",
            score.coverage, policy.min_coverage
        ));
    }
    if score.ece > policy.max_ece {
        failures.push(format!("ECE {:.3} > {:.3}", score.ece, policy.max_ece));
    }
    GateReport {
        passed: failures.is_empty(),
        failures,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn empty_set_abstains_and_scores_zero() {
        let p = Platform::demo();
        let cases = vec![BenchmarkCase {
            id: "c1".into(),
            question: "Is EGFR a driver in NSCLC?".into(),
            focus_entity: Some("EGFR".into()),
            gold: "EGFR".into(),
        }];
        // No data ingested -> the agent abstains -> coverage 0.
        let results = run_benchmark(&p, &cases).await.unwrap();
        let score = summarize(&results);
        assert_eq!(score.coverage, 0.0);
        let report = gate(&score, &GatePolicy::default());
        assert!(!report.passed);
    }
}
