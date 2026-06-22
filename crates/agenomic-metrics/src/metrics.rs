//! The pure agentic metric functions.
//!
//! Every function in this module is total and deterministic. Inputs that
//! could otherwise produce a value outside the metric's natural range are
//! handled with an explicit, documented convention (clamping, or a vacuous
//! result for an empty denominator) rather than panicking.
//!
//! ## Range conventions
//!
//! * **Completeness / coverage / adherence** metrics are *vacuously perfect*
//!   when there is nothing to measure: an empty denominator yields `1.0`.
//!   (If there are no applicable policy rules, adherence is trivially total.)
//! * **Contamination** is the opposite: with no influence at all, the
//!   contaminated *fraction* is `0.0`.
//! * All ratio-style metrics are clamped to `[0, 1]` to defend against
//!   malformed inputs (e.g. `passed > total`).

use crate::divergence::{jensen_shannon_divergence, Distribution};

/// Clamps a value into the closed unit interval `[0, 1]`. A `NaN` collapses
/// to `0.0` (the conservative/worst-case choice for a quality metric).
fn clamp01(x: f64) -> f64 {
    if x.is_nan() {
        0.0
    } else {
        x.clamp(0.0, 1.0)
    }
}

/// `numerator / denominator` clamped to `[0, 1]`, with a *vacuous* `1.0` when
/// the denominator is zero. Used by completeness/coverage/adherence metrics.
fn coverage_ratio(numerator: f64, denominator: f64) -> f64 {
    if denominator <= 0.0 {
        1.0
    } else {
        clamp01(numerator / denominator)
    }
}

// ---------------------------------------------------------------------------
// Trace Completeness Score
// ---------------------------------------------------------------------------

/// **Trace Completeness Score (TCS).**
///
/// `TCS = |captured ∩ required| / |required|`, computed over the *distinct*
/// required event types. An empty `expected` set is vacuously complete
/// (`1.0`).
///
/// `MVP threshold: TCS ≥ 0.95` (see [`Thresholds`](crate::Thresholds)).
///
/// The element type is left generic (anything `Eq + Hash`); in practice it is
/// [`EventType`](crate::EventType).
///
/// # Examples
/// ```
/// use agenomic_metrics::{trace_completeness, EventType};
/// use EventType::*;
/// let required = [RunStarted, Decision, RunCompleted];
/// let captured = [RunStarted, Decision, RunCompleted];
/// assert_eq!(trace_completeness(&captured, &required), 1.0);
///
/// let partial = [RunStarted, RunCompleted];
/// assert!((trace_completeness(&partial, &required) - 2.0 / 3.0).abs() < 1e-12);
/// ```
pub fn trace_completeness<T: std::hash::Hash + Eq>(captured: &[T], expected: &[T]) -> f64 {
    use std::collections::HashSet;
    let required: HashSet<&T> = expected.iter().collect();
    if required.is_empty() {
        return 1.0;
    }
    let seen: HashSet<&T> = captured.iter().collect();
    let present = required.iter().filter(|r| seen.contains(*r)).count();
    coverage_ratio(present as f64, required.len() as f64)
}

// ---------------------------------------------------------------------------
// Replay
// ---------------------------------------------------------------------------

/// **Exact Replay Match.**
///
/// Exact replay is a **boolean equality of hashes**, never a continuous
/// score — do not use [`replay_fidelity`] for exact replay. Two *absent*
/// (empty) hashes do **not** count as a match, so an unrecorded replay can
/// never be reported as a verified exact reproduction.
///
/// # Examples
/// ```
/// use agenomic_metrics::exact_replay_match;
/// assert!(exact_replay_match("blake3:ab", "blake3:ab"));
/// assert!(!exact_replay_match("blake3:ab", "blake3:cd"));
/// assert!(!exact_replay_match("", "")); // absent hashes never match
/// ```
pub fn exact_replay_match(hash_a: &str, hash_b: &str) -> bool {
    !hash_a.is_empty() && hash_a == hash_b
}

/// **Replay Fidelity Score (RFS).**
///
/// `RFS = (1 − GED_norm) × CosSim`, where `GED_norm ∈ [0, 1]` is the
/// normalized graph-edit distance between the original and replayed execution
/// graphs and `CosSim ∈ [0, 1]` is the cosine similarity of their semantic
/// embeddings. Both inputs are clamped to `[0, 1]`; the result is in `[0, 1]`.
///
/// Invariants (all guarded by tests):
/// * `RFS = 1` ⟺ `GED_norm = 0` **and** `CosSim = 1`;
/// * `RFS = 0` whenever `CosSim = 0`;
/// * `RFS` is monotonically **decreasing** in `GED_norm` and
///   **increasing** in `CosSim`.
///
/// This is the single most safety-critical formula in the crate. It must
/// **never** be coded as `GED × CosSim`.
///
/// `MVP threshold: RFS ≥ 0.95` for a passing functional replay.
///
/// # Examples
/// ```
/// use agenomic_metrics::replay_fidelity;
/// assert_eq!(replay_fidelity(0.0, 1.0), 1.0);
/// assert_eq!(replay_fidelity(0.4, 0.0), 0.0);
/// assert!((replay_fidelity(0.1, 0.9) - 0.81).abs() < 1e-12);
/// ```
pub fn replay_fidelity(ged_norm: f64, cos_sim: f64) -> f64 {
    let ged = clamp01(ged_norm);
    let cos = clamp01(cos_sim);
    clamp01((1.0 - ged) * cos)
}

// ---------------------------------------------------------------------------
// Policy Adherence
// ---------------------------------------------------------------------------

/// **Policy Adherence Score (PAS).**
///
/// Unweighted: `PAS = passed / total` (the fraction of applicable rules that
/// passed). With per-rule `weights`, the *weighted* form
/// `Σ(wᵢ·passᵢ) / Σ(wᵢ)` is used instead.
///
/// Weighted convention: `weights[i]` is the weight of the `i`-th applicable
/// rule, ordered so that the `passed` passing rules occupy indices
/// `[0, passed)`. Extra/short weight slices are handled gracefully
/// (`min(passed, weights.len())` passing slots, denominator = sum of supplied
/// weights). A zero weight-sum (or `total == 0`) is vacuously `1.0`.
///
/// `MVP threshold: PAS ≥ 0.98 on critical actions`.
///
/// # Examples
/// ```
/// use agenomic_metrics::policy_adherence;
/// assert_eq!(policy_adherence(49, 50, None), 0.98);
/// assert_eq!(policy_adherence(0, 0, None), 1.0); // no applicable rules
///
/// // Weighted: 1 critical rule (weight 9) passed out of {9, 1} → 0.9.
/// let w = [9.0, 1.0];
/// assert!((policy_adherence(1, 2, Some(&w)) - 0.9).abs() < 1e-12);
/// ```
pub fn policy_adherence(passed: usize, total: usize, weights: Option<&[f64]>) -> f64 {
    match weights {
        None => coverage_ratio(passed as f64, total as f64),
        Some(w) => {
            let denom: f64 = w.iter().copied().filter(|x| x.is_finite()).sum();
            if denom <= 0.0 {
                return 1.0;
            }
            let passing = passed.min(w.len());
            let numer: f64 = w[..passing].iter().copied().filter(|x| x.is_finite()).sum();
            clamp01(numer / denom)
        }
    }
}

// ---------------------------------------------------------------------------
// Behavioral Drift
// ---------------------------------------------------------------------------

/// **Behavioral Drift Score (BDS).**
///
/// `BDS = JS(P₁ ‖ P₂)`, the Jensen–Shannon divergence (in bits, so `[0, 1]`)
/// between two categorical action distributions. `0.0` means the two
/// distributions are identical.
///
/// `MVP threshold: alert if BDS > 0.10`.
///
/// # Examples
/// ```
/// use agenomic_metrics::{behavioral_drift, Distribution};
/// let v1 = Distribution::from_counts([("tool", 8u64), ("decision", 2)]);
/// let v2 = Distribution::from_counts([("tool", 8u64), ("decision", 2)]);
/// assert_eq!(behavioral_drift(&v1, &v2), 0.0);
/// ```
pub fn behavioral_drift(p_actions_v1: &Distribution, p_actions_v2: &Distribution) -> f64 {
    jensen_shannon_divergence(p_actions_v1, p_actions_v2)
}

/// **ABC Drift Bound — `D* = α / γ`.**
///
/// `[SOURCÉ] ABC.` `α` is the natural drift rate and `γ` the recovery force;
/// their ratio bounds the steady-state drift. A common reference point from
/// the master document is `D* < 0.27`, but **both parameters must be
/// estimated empirically** — this is not a universal constant.
///
/// When `γ ≤ 0` (no recovery force) the bound is unbounded; this returns
/// [`f64::INFINITY`] rather than panicking.
///
/// # Examples
/// ```
/// use agenomic_metrics::drift_bound;
/// assert!((drift_bound(0.03, 0.12) - 0.25).abs() < 1e-12);
/// assert!(drift_bound(0.1, 0.0).is_infinite());
/// ```
pub fn drift_bound(alpha: f64, gamma: f64) -> f64 {
    if gamma <= 0.0 || !gamma.is_finite() {
        f64::INFINITY
    } else {
        alpha / gamma
    }
}

// ---------------------------------------------------------------------------
// Tool Risk
// ---------------------------------------------------------------------------

/// **Tool Risk Score (TRS).**
///
/// `TRS = impact × privilege × externality × irreversibility × uncertainty`.
/// Each factor is clamped to `[0, 1]`, so the product is in `[0, 1]` and is
/// monotonically non-decreasing in every factor.
///
/// `MVP threshold: TRS > 0.70 ⇒ human review`.
///
/// # Examples
/// ```
/// use agenomic_metrics::tool_risk;
/// assert_eq!(tool_risk(1.0, 1.0, 1.0, 1.0, 1.0), 1.0);
/// assert_eq!(tool_risk(0.9, 0.0, 0.9, 0.9, 0.9), 0.0);
/// ```
pub fn tool_risk(
    impact: f64,
    privilege: f64,
    externality: f64,
    irreversibility: f64,
    uncertainty: f64,
) -> f64 {
    clamp01(impact)
        * clamp01(privilege)
        * clamp01(externality)
        * clamp01(irreversibility)
        * clamp01(uncertainty)
}

// ---------------------------------------------------------------------------
// Memory Contamination
// ---------------------------------------------------------------------------

/// **Memory Contamination Score (MCS).**
///
/// `MCS = untrusted_influence / total_influence`, the fraction of memory
/// influence that originates from untrusted sources. With no influence at all
/// (`total_influence ≤ 0`) the contaminated fraction is `0.0`.
///
/// `MVP target: MCS < 0.05`.
///
/// # Examples
/// ```
/// use agenomic_metrics::memory_contamination;
/// assert_eq!(memory_contamination(0.0, 10.0), 0.0);
/// assert_eq!(memory_contamination(1.0, 4.0), 0.25);
/// assert_eq!(memory_contamination(0.0, 0.0), 0.0); // no influence at all
/// ```
pub fn memory_contamination(untrusted_influence: f64, total_influence: f64) -> f64 {
    if total_influence <= 0.0 {
        return 0.0;
    }
    clamp01(untrusted_influence / total_influence)
}

// ---------------------------------------------------------------------------
// Prompt Mutation
// ---------------------------------------------------------------------------

/// **Prompt Mutation Score (PMS).**
///
/// `PMS = edit_distance_norm × semantic_delta`.
///
/// `[EXTRAPOLATION]` — to be calibrated per domain. Both inputs are clamped
/// to `[0, 1]`.
///
/// # Examples
/// ```
/// use agenomic_metrics::prompt_mutation;
/// assert_eq!(prompt_mutation(0.0, 0.9), 0.0);
/// assert!((prompt_mutation(0.5, 0.4) - 0.2).abs() < 1e-12);
/// ```
pub fn prompt_mutation(edit_distance_norm: f64, semantic_delta: f64) -> f64 {
    clamp01(clamp01(edit_distance_norm) * clamp01(semantic_delta))
}

// ---------------------------------------------------------------------------
// Runtime Variance
// ---------------------------------------------------------------------------

/// **Runtime Variance Score.**
///
/// `RuntimeVariance = E[d(τ_runtimeA, τ_runtimeB)]`, the mean pairwise
/// distance between traces produced by two runtimes. Non-finite samples are
/// ignored; an empty (or all-non-finite) slice yields `0.0` (no observed
/// variance). The result is **not** clamped to `[0, 1]` — distances may be
/// unbounded — but is guaranteed `≥ 0` for non-negative inputs.
///
/// # Examples
/// ```
/// use agenomic_metrics::runtime_variance;
/// assert_eq!(runtime_variance(&[]), 0.0);
/// assert!((runtime_variance(&[0.1, 0.2, 0.3]) - 0.2).abs() < 1e-12);
/// ```
pub fn runtime_variance(distances: &[f64]) -> f64 {
    let mut sum = 0.0;
    let mut n = 0u64;
    for &d in distances {
        if d.is_finite() {
            sum += d;
            n += 1;
        }
    }
    if n == 0 {
        0.0
    } else {
        sum / n as f64
    }
}

// ---------------------------------------------------------------------------
// Decision Explainability
// ---------------------------------------------------------------------------

/// **Decision Explainability Score (DES).**
///
/// `DES = decisions_with_provenance / total_decisions`. Vacuously `1.0` when
/// there are no decisions.
///
/// # Examples
/// ```
/// use agenomic_metrics::decision_explainability;
/// assert_eq!(decision_explainability(9, 10), 0.9);
/// assert_eq!(decision_explainability(0, 0), 1.0);
/// ```
pub fn decision_explainability(decisions_with_provenance: usize, total_decisions: usize) -> f64 {
    coverage_ratio(decisions_with_provenance as f64, total_decisions as f64)
}

// ---------------------------------------------------------------------------
// Audit Evidence Completeness
// ---------------------------------------------------------------------------

/// **Audit Evidence Completeness (AEC).**
///
/// `AEC = available_required_evidence / required_evidence`. Vacuously `1.0`
/// when no evidence is required.
///
/// `MVP thresholds: AEC ≥ 0.90 in general; AEC ≥ 0.95 for evidentiary replay`.
///
/// # Examples
/// ```
/// use agenomic_metrics::audit_evidence_completeness;
/// assert!((audit_evidence_completeness(94, 100) - 0.94).abs() < 1e-12);
/// ```
pub fn audit_evidence_completeness(available: usize, required: usize) -> f64 {
    coverage_ratio(available as f64, required as f64)
}

// ---------------------------------------------------------------------------
// Compliance Confidence
// ---------------------------------------------------------------------------

/// A control's reported confidence together with its relative weight, used by
/// [`compliance_confidence`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeightedConfidence {
    /// Relative weight of the control (non-negative; non-finite weights are
    /// ignored).
    pub weight: f64,
    /// The control's confidence in `[0, 1]` (clamped on use).
    pub confidence: f64,
}

impl WeightedConfidence {
    /// Convenience constructor.
    ///
    /// # Examples
    /// ```
    /// use agenomic_metrics::WeightedConfidence;
    /// let wc = WeightedConfidence::new(2.0, 0.8);
    /// assert_eq!(wc.weight, 2.0);
    /// ```
    pub fn new(weight: f64, confidence: f64) -> Self {
        Self { weight, confidence }
    }
}

/// **Compliance Confidence.**
///
/// `ComplianceConfidence = Σ(wᵢ·confidenceᵢ) / Σ(wᵢ)` — the weight-normalized
/// mean control confidence, guaranteed in `[0, 1]`. (Normalizing by `Σ(wᵢ)`
/// keeps the result bounded; with weights summing to `1` it reduces to the
/// raw `Σ(wᵢ·confidenceᵢ)`.) An empty slice, or one with no positive weight,
/// yields `0.0`: absence of controls is not confidence.
///
/// # Examples
/// ```
/// use agenomic_metrics::{compliance_confidence, WeightedConfidence};
/// let controls = [
///     WeightedConfidence::new(1.0, 1.0),
///     WeightedConfidence::new(1.0, 0.0),
/// ];
/// assert_eq!(compliance_confidence(&controls), 0.5);
/// assert_eq!(compliance_confidence(&[]), 0.0);
/// ```
pub fn compliance_confidence(control_confidences: &[WeightedConfidence]) -> f64 {
    let mut weighted_sum = 0.0;
    let mut weight_sum = 0.0;
    for wc in control_confidences {
        if wc.weight.is_finite() && wc.weight > 0.0 {
            weighted_sum += wc.weight * clamp01(wc.confidence);
            weight_sum += wc.weight;
        }
    }
    if weight_sum <= 0.0 {
        0.0
    } else {
        clamp01(weighted_sum / weight_sum)
    }
}

// ---------------------------------------------------------------------------
// Alignment Stability
// ---------------------------------------------------------------------------

/// **Alignment Stability.**
///
/// `AlignmentStability = 1 − Var(PAS)`, using the *population* variance of a
/// Policy-Adherence-Score series, clamped to `[0, 1]`. A series of fewer than
/// two points has no variance and is perfectly stable (`1.0`).
///
/// # Examples
/// ```
/// use agenomic_metrics::alignment_stability;
/// assert_eq!(alignment_stability(&[0.98, 0.98, 0.98]), 1.0);
/// assert_eq!(alignment_stability(&[0.5]), 1.0);
/// assert!(alignment_stability(&[0.0, 1.0]) < 1.0);
/// ```
pub fn alignment_stability(policy_adherence_series: &[f64]) -> f64 {
    let xs: Vec<f64> = policy_adherence_series
        .iter()
        .copied()
        .filter(|x| x.is_finite())
        .collect();
    if xs.len() < 2 {
        return 1.0;
    }
    let n = xs.len() as f64;
    let mean = xs.iter().sum::<f64>() / n;
    let var = xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n;
    clamp01(1.0 - var)
}

// ---------------------------------------------------------------------------
// Controllability
// ---------------------------------------------------------------------------

/// **Controllability Score.**
///
/// `Controllability = (interruptions_respected + overrides_respected +
/// constraints_respected) / total_control_signals`. Vacuously `1.0` when
/// there were no control signals to honor.
///
/// # Examples
/// ```
/// use agenomic_metrics::controllability;
/// assert_eq!(controllability(3, 2, 5, 10), 1.0);
/// assert_eq!(controllability(1, 1, 1, 6), 0.5);
/// assert_eq!(controllability(0, 0, 0, 0), 1.0);
/// ```
pub fn controllability(
    interruptions_respected: usize,
    overrides_respected: usize,
    constraints_respected: usize,
    total_control_signals: usize,
) -> f64 {
    let respected = interruptions_respected + overrides_respected + constraints_respected;
    coverage_ratio(respected as f64, total_control_signals as f64)
}

// ---------------------------------------------------------------------------
// Causal & Provenance Coverage
// ---------------------------------------------------------------------------

/// **Causal Coverage.**
///
/// `CausalCoverage = events_with_parent_links / non_root_events` — the
/// fraction of non-root events that carry an explicit recorded parent link in
/// the execution graph. Vacuously `1.0` when every event is a root.
///
/// # Examples
/// ```
/// use agenomic_metrics::causal_coverage;
/// assert_eq!(causal_coverage(9, 10), 0.9);
/// assert_eq!(causal_coverage(0, 0), 1.0);
/// ```
pub fn causal_coverage(events_with_parent_links: usize, non_root_events: usize) -> f64 {
    coverage_ratio(events_with_parent_links as f64, non_root_events as f64)
}

/// **Provenance Coverage.**
///
/// `ProvenanceCoverage = claims_with_support / total_claims` — the fraction
/// of asserted claims (decisions, model answers, …) that cite supporting
/// evidence. Vacuously `1.0` when there are no claims.
///
/// # Examples
/// ```
/// use agenomic_metrics::provenance_coverage;
/// assert!((provenance_coverage(83, 100) - 0.83).abs() < 1e-12);
/// assert_eq!(provenance_coverage(0, 0), 1.0);
/// ```
pub fn provenance_coverage(claims_with_support: usize, total_claims: usize) -> f64 {
    coverage_ratio(claims_with_support as f64, total_claims as f64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace::EventType::*;

    #[test]
    fn tcs_is_one_when_all_required_present() {
        let required = [RunStarted, Decision, RunCompleted];
        let captured = [RunStarted, Decision, RunCompleted, ToolCallExecuted];
        assert_eq!(trace_completeness(&captured, &required), 1.0);
    }

    #[test]
    fn tcs_counts_distinct_required_only() {
        let required = [RunStarted, RunStarted, RunCompleted];
        let captured = [RunStarted];
        // distinct required = {RunStarted, RunCompleted}; 1 of 2 present.
        assert_eq!(trace_completeness(&captured, &required), 0.5);
    }

    #[test]
    fn rfs_perfect_and_zero() {
        assert_eq!(replay_fidelity(0.0, 1.0), 1.0);
        assert_eq!(replay_fidelity(0.0, 0.0), 0.0);
        assert_eq!(replay_fidelity(0.7, 0.0), 0.0);
    }

    #[test]
    fn rfs_is_not_ged_times_cossim() {
        // The wrong formula GED*Cos would give 0.1*0.9 = 0.09.
        // The correct formula (1-GED)*Cos gives 0.9*0.9 = 0.81.
        assert!((replay_fidelity(0.1, 0.9) - 0.81).abs() < 1e-12);
    }

    #[test]
    fn exact_replay_is_boolean_hash_equality() {
        assert!(exact_replay_match("h", "h"));
        assert!(!exact_replay_match("h", "g"));
        assert!(!exact_replay_match("", ""));
    }

    #[test]
    fn bds_zero_for_identical_distributions() {
        let p = Distribution::from_counts([("a", 3u64), ("b", 7)]);
        assert_eq!(behavioral_drift(&p, &p), 0.0);
    }

    #[test]
    fn trs_grows_with_each_factor() {
        let base = tool_risk(0.5, 0.5, 0.5, 0.5, 0.5);
        assert!(tool_risk(0.6, 0.5, 0.5, 0.5, 0.5) >= base);
        assert!(tool_risk(0.5, 0.6, 0.5, 0.5, 0.5) >= base);
        assert!(tool_risk(0.5, 0.5, 0.6, 0.5, 0.5) >= base);
        assert!(tool_risk(0.5, 0.5, 0.5, 0.6, 0.5) >= base);
        assert!(tool_risk(0.5, 0.5, 0.5, 0.5, 0.6) >= base);
    }

    #[test]
    fn mcs_zero_without_untrusted_influence() {
        assert_eq!(memory_contamination(0.0, 12.0), 0.0);
    }

    #[test]
    fn drift_bound_d_star() {
        assert!((drift_bound(0.03, 0.12) - 0.25).abs() < 1e-12);
        assert!(drift_bound(0.1, 0.0).is_infinite());
    }

    #[test]
    fn weighted_policy_adherence_weights_critical_rules() {
        // 1 critical rule (weight 9) passed, 1 trivial rule (weight 1) failed.
        let w = [9.0, 1.0];
        assert!((policy_adherence(1, 2, Some(&w)) - 0.9).abs() < 1e-12);
    }

    #[test]
    fn alignment_stability_penalizes_variance() {
        assert_eq!(alignment_stability(&[0.9, 0.9, 0.9]), 1.0);
        assert!(alignment_stability(&[0.2, 0.8]) < 1.0);
    }

    #[test]
    fn ratios_clamp_on_malformed_input() {
        // passed > total must never exceed 1.0.
        assert_eq!(policy_adherence(10, 5, None), 1.0);
        assert_eq!(audit_evidence_completeness(10, 5), 1.0);
        assert_eq!(memory_contamination(10.0, 5.0), 1.0);
    }
}
