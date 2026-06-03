//! Typed, calibrated confidence — a first-class value, not a log line.

use serde::{Deserialize, Serialize};

/// A calibrated probability in `[0.0, 1.0]`.
///
/// Construction always clamps into range, so a `Confidence` is correct by
/// construction. The value is intended to be **post-hoc calibrated** (see
/// [`CalibrationMethod`]); raw model logits should be passed through a
/// [`crate::Calibrator`] before being wrapped for downstream decisions.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Confidence(f64);

impl Confidence {
    pub const ZERO: Confidence = Confidence(0.0);
    pub const ONE: Confidence = Confidence(1.0);

    /// Wrap a value, clamping to `[0, 1]`.
    pub fn new(v: f64) -> Self {
        Self(v.clamp(0.0, 1.0))
    }

    /// The underlying probability.
    pub fn get(self) -> f64 {
        self.0
    }

    /// True if at or above `threshold`.
    pub fn at_least(self, threshold: f64) -> bool {
        self.0 >= threshold
    }
}

impl std::fmt::Display for Confidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:.3}", self.0)
    }
}

/// How a [`Confidence`] was calibrated. Recorded for provenance/audit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CalibrationMethod {
    /// Raw, uncalibrated score (not for decisions).
    None,
    /// Temperature scaling.
    Temperature,
    /// Isotonic regression.
    Isotonic,
    /// Split-conformal calibration (distribution-free coverage guarantee).
    Conformal,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_into_range() {
        assert_eq!(Confidence::new(1.5).get(), 1.0);
        assert_eq!(Confidence::new(-0.2).get(), 0.0);
        assert!(Confidence::new(0.8).at_least(0.7));
    }
}
