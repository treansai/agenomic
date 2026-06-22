//! The serializable [`MetricsReport`] roll-up.

use serde::{Deserialize, Serialize};

/// A roll-up of every Phase 2 metric for a run, window or comparison.
///
/// Each field is `Option` because not every metric is computable in every
/// context: a single sealed run yields trace/policy/coverage metrics but no
/// behavioral drift (which needs two windows), and a replay yields fidelity
/// metrics that a plain run does not. Absent metrics serialize to JSON `null`.
///
/// The field order is the canonical reporting order and is mirrored by the
/// `metrics.json` artifact embedded in the Evidence Package (Phase 2, P2-4).
///
/// # Examples
/// ```
/// use agenomic_metrics::MetricsReport;
/// let report = MetricsReport {
///     trace_completeness: Some(1.0),
///     policy_adherence: Some(0.99),
///     ..MetricsReport::empty()
/// };
/// let json = serde_json::to_value(&report).unwrap();
/// assert_eq!(json["trace_completeness"], 1.0);
/// assert!(json["behavioral_drift"].is_null());
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MetricsReport {
    /// Trace Completeness Score (`TCS`).
    pub trace_completeness: Option<f64>,
    /// Replay Fidelity Score (`RFS`).
    pub replay_fidelity: Option<f64>,
    /// Whether an exact (hash-equality) replay matched.
    pub exact_replay_match: Option<bool>,
    /// Policy Adherence Score (`PAS`).
    pub policy_adherence: Option<f64>,
    /// Behavioral Drift Score (`BDS`).
    pub behavioral_drift: Option<f64>,
    /// ABC drift bound `D* = α / γ`.
    pub drift_bound: Option<f64>,
    /// Tool Risk Score (`TRS`).
    pub tool_risk: Option<f64>,
    /// Memory Contamination Score (`MCS`).
    pub memory_contamination: Option<f64>,
    /// Prompt Mutation Score (`PMS`).
    pub prompt_mutation: Option<f64>,
    /// Runtime variance across runtimes.
    pub runtime_variance: Option<f64>,
    /// Decision Explainability Score (`DES`).
    pub decision_explainability: Option<f64>,
    /// Audit Evidence Completeness (`AEC`).
    pub audit_evidence_completeness: Option<f64>,
    /// Weight-normalized compliance confidence.
    pub compliance_confidence: Option<f64>,
    /// Alignment stability (`1 − Var(PAS)`).
    pub alignment_stability: Option<f64>,
    /// Controllability score.
    pub controllability: Option<f64>,
    /// Causal coverage.
    pub causal_coverage: Option<f64>,
    /// Provenance coverage.
    pub provenance_coverage: Option<f64>,
}

impl MetricsReport {
    /// An all-`None` report. Useful as a base for struct-update syntax.
    ///
    /// # Examples
    /// ```
    /// use agenomic_metrics::MetricsReport;
    /// assert_eq!(MetricsReport::empty(), MetricsReport::default());
    /// ```
    pub fn empty() -> Self {
        Self::default()
    }
}
