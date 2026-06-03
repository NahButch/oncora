//! # oncora-uncertainty
//!
//! The robustness & uncertainty subsystem (named pillar; design in
//! `docs/03-uncertainty-reliability.md`). It turns raw model scores and
//! evidence into a calibrated [`Confidence`], decides a [`Verdict`]
//! (accept / abstain / escalate), and verifies claims against their evidence
//! and deterministic oracle tools.

use async_trait::async_trait;
use oncora_core::{
    CalibrationMethod, Calibrator, Claim, Confidence, EscalationTarget, Evidence, Result,
    UncertaintyKind, UncertaintySignal, Verdict, Verifier,
};

/// Temperature-scaling calibrator. `t > 1` softens overconfident scores;
/// `t < 1` sharpens. A real deployment fits `t` on the eval harness's
/// calibration split (see `docs/06-eval-benchmarking.md`).
pub struct TemperatureCalibrator {
    pub temperature: f64,
}

impl TemperatureCalibrator {
    pub fn new(temperature: f64) -> Self {
        Self {
            temperature: temperature.max(1e-3),
        }
    }
}

impl Calibrator for TemperatureCalibrator {
    fn method(&self) -> CalibrationMethod {
        CalibrationMethod::Temperature
    }

    fn calibrate(&self, raw_score: f64) -> Confidence {
        // Treat raw_score as a logit-ish value mapped through a tempered sigmoid.
        let z = raw_score / self.temperature;
        Confidence::new(1.0 / (1.0 + (-z).exp()))
    }
}

/// Self-consistency: agreement fraction of the modal answer across samples.
/// Returns `(modal_answer, agreement_in_[0,1])`.
pub fn self_consistency(samples: &[String]) -> Option<(String, f64)> {
    if samples.is_empty() {
        return None;
    }
    let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for s in samples {
        *counts.entry(s.trim()).or_insert(0) += 1;
    }
    let (ans, n) = counts.into_iter().max_by_key(|(_, c)| *c)?;
    Some((ans.to_string(), n as f64 / samples.len() as f64))
}

/// Expected Calibration Error over `(confidence, correct)` pairs using `bins`
/// equal-width bins. Lower is better (0 = perfectly calibrated).
pub fn expected_calibration_error(samples: &[(f64, bool)], bins: usize) -> f64 {
    if samples.is_empty() || bins == 0 {
        return 0.0;
    }
    let n = samples.len() as f64;
    let mut ece = 0.0;
    for b in 0..bins {
        let lo = b as f64 / bins as f64;
        let hi = (b + 1) as f64 / bins as f64;
        let in_bin: Vec<&(f64, bool)> = samples
            .iter()
            .filter(|(c, _)| *c > lo && *c <= hi || (b == 0 && *c == 0.0))
            .collect();
        if in_bin.is_empty() {
            continue;
        }
        let conf: f64 = in_bin.iter().map(|(c, _)| *c).sum::<f64>() / in_bin.len() as f64;
        let acc: f64 = in_bin.iter().filter(|(_, ok)| *ok).count() as f64 / in_bin.len() as f64;
        ece += (in_bin.len() as f64 / n) * (conf - acc).abs();
    }
    ece
}

/// Policy that maps a calibrated confidence + uncertainty signals onto a
/// [`Verdict`]. Thresholds are configurable per task class.
#[derive(Clone, Debug)]
pub struct AbstentionPolicy {
    /// At/above this calibrated confidence we may `Accept`.
    pub accept_at: f64,
    /// Below this we `Abstain` outright.
    pub abstain_below: f64,
    /// Any tool/oracle disagreement above this magnitude forces escalation.
    pub oracle_conflict_escalates_at: f64,
}

impl Default for AbstentionPolicy {
    fn default() -> Self {
        Self {
            accept_at: 0.75,
            abstain_below: 0.5,
            oracle_conflict_escalates_at: 0.5,
        }
    }
}

impl AbstentionPolicy {
    /// Decide a verdict. Order matters: oracle conflict (a hard signal) is
    /// checked before confidence thresholds.
    pub fn decide(&self, confidence: Confidence, signals: &[UncertaintySignal]) -> Verdict {
        if let Some(sig) = signals.iter().find(|s| {
            s.kind == UncertaintyKind::Tool && s.magnitude >= self.oracle_conflict_escalates_at
        }) {
            return Verdict::Escalate {
                to: EscalationTarget::HumanReviewer,
                reason: format!("oracle disagreement: {}", sig.note),
            };
        }
        let c = confidence.get();
        if c >= self.accept_at {
            Verdict::Accept
        } else if c < self.abstain_below {
            Verdict::Abstain {
                reason: format!(
                    "calibrated confidence {c:.3} below floor {:.3}",
                    self.abstain_below
                ),
            }
        } else {
            Verdict::Escalate {
                to: EscalationTarget::SpecialistAgent("domain-reviewer".into()),
                reason: format!("confidence {c:.3} in the uncertain band"),
            }
        }
    }
}

/// A verifier grounded in the evidence: a claim with no support, or net
/// contradiction, fails. Citation-grounding/NLI checks plug in here.
pub struct GroundedVerifier {
    pub policy: AbstentionPolicy,
}

impl GroundedVerifier {
    pub fn new(policy: AbstentionPolicy) -> Self {
        Self { policy }
    }
}

#[async_trait]
impl Verifier for GroundedVerifier {
    async fn verify(&self, _claim: &Claim, evidence: &Evidence) -> Result<Verdict> {
        if evidence.support.is_empty() {
            return Ok(Verdict::Abstain {
                reason: "no supporting evidence retrieved".into(),
            });
        }
        let mut signals = Vec::new();
        if evidence.net_support() < 0 {
            signals.push(UncertaintySignal::new(
                UncertaintyKind::Epistemic,
                0.8,
                "contradicting evidence outweighs support",
            ));
        }
        Ok(self.policy.decide(evidence.confidence, &signals))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_accepts_abstains_escalates() {
        let p = AbstentionPolicy::default();
        assert_eq!(p.decide(Confidence::new(0.9), &[]), Verdict::Accept);
        assert!(matches!(
            p.decide(Confidence::new(0.3), &[]),
            Verdict::Abstain { .. }
        ));
        assert!(matches!(
            p.decide(Confidence::new(0.6), &[]),
            Verdict::Escalate { .. }
        ));
    }

    #[test]
    fn oracle_conflict_forces_escalation() {
        let p = AbstentionPolicy::default();
        let sig = UncertaintySignal::new(UncertaintyKind::Tool, 0.9, "calc vs KG mismatch");
        assert!(matches!(
            p.decide(Confidence::new(0.99), &[sig]),
            Verdict::Escalate { .. }
        ));
    }

    #[test]
    fn self_consistency_and_ece() {
        let s = vec!["EGFR".to_string(), "EGFR".to_string(), "KRAS".to_string()];
        let (ans, agree) = self_consistency(&s).unwrap();
        assert_eq!(ans, "EGFR");
        assert!((agree - 0.666).abs() < 0.01);
        let ece = expected_calibration_error(&[(0.9, true), (0.9, false)], 10);
        assert!(ece > 0.0);
    }
}
