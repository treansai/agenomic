//! Centralized MVP thresholds.
//!
//! `[EXTRAPOLATION]` — every value here is a **starting point to calibrate per
//! domain and sector**, not a universal truth. They are collected in one place
//! precisely so that calibration is a single, auditable edit rather than a
//! hunt through scattered literals.

use serde::{Deserialize, Serialize};

/// The thresholds against which the Phase 2 metrics are evaluated.
///
/// Construct the calibrated MVP defaults with [`Thresholds::mvp`] (also the
/// [`Default`] impl).
///
/// # Examples
/// ```
/// use agenomic_metrics::Thresholds;
/// let t = Thresholds::mvp();
/// assert_eq!(t.trace_completeness_min, 0.95);
/// assert_eq!(t.behavioral_drift_alert, 0.10);
/// assert_eq!(t, Thresholds::default());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Thresholds {
    /// Minimum Trace Completeness Score for an auditable run (`0.95`).
    pub trace_completeness_min: f64,
    /// Minimum Replay Fidelity Score for a passing functional replay (`0.95`).
    pub replay_fidelity_min: f64,
    /// Minimum Policy Adherence Score on critical actions (`0.98`).
    pub policy_adherence_critical_min: f64,
    /// Behavioral Drift Score above which an alert is raised (`0.10`).
    pub behavioral_drift_alert: f64,
    /// Tool Risk Score above which human review is mandated (`0.70`).
    pub tool_risk_human_review: f64,
    /// Maximum tolerated Memory Contamination Score (`0.05`).
    pub memory_contamination_max: f64,
    /// Minimum Audit Evidence Completeness in general (`0.90`).
    pub audit_evidence_completeness_min: f64,
    /// Minimum Audit Evidence Completeness for *evidentiary* replay (`0.95`).
    pub evidentiary_audit_completeness_min: f64,
}

impl Thresholds {
    /// The calibrated MVP defaults.
    ///
    /// `[EXTRAPOLATION]` seuils MVP à calibrer par domaine et secteur.
    pub const fn mvp() -> Self {
        Self {
            trace_completeness_min: 0.95,
            replay_fidelity_min: 0.95,
            policy_adherence_critical_min: 0.98,
            behavioral_drift_alert: 0.10,
            tool_risk_human_review: 0.70,
            memory_contamination_max: 0.05,
            audit_evidence_completeness_min: 0.90,
            evidentiary_audit_completeness_min: 0.95,
        }
    }
}

impl Default for Thresholds {
    fn default() -> Self {
        Self::mvp()
    }
}
