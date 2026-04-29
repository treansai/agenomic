//! Sample-size calibration helpers (Hoeffding-style bounds).

use crate::schema::{FingerprintSchema, MetricId, MetricKind};

/// Conservative recommended sample size for unbounded metrics.
pub const CONSERVATIVE_UNBOUNDED_KN: usize = 1000;

/// Inverts Hoeffding's inequality: returns the smallest `KN` such that for
/// a metric in `[0, 1]`, `P(|μ̂ − μ| > ε) ≤ δ`.
///
/// The closed form is `KN ≥ ln(2 / δ) / (2 · ε²)`. The result is clamped to
/// at least 2 so that variance estimation remains feasible.
///
/// # Examples
/// ```
/// use agentlock_fingerprint::hoeffding_sample_size;
/// // Classical AgentLock target ε=0.02, δ=0.01 → 6620.
/// assert!(hoeffding_sample_size(0.02, 0.01) >= 6620);
/// ```
pub fn hoeffding_sample_size(epsilon: f64, delta: f64) -> usize {
    if !(epsilon.is_finite() && delta.is_finite()) || epsilon <= 0.0 || delta <= 0.0 || delta >= 1.0
    {
        return usize::MAX;
    }
    let ln_2_delta = (2.0_f64 / delta).ln();
    let eps_sq = epsilon * epsilon;
    let kn = (ln_2_delta / (2.0 * eps_sq)).ceil();
    if kn.is_finite() && kn >= 2.0 {
        kn as usize
    } else {
        2
    }
}

/// Recommends a sample size per metric of the given schema.
///
/// For [`MetricKind::Bounded01`] metrics, [`hoeffding_sample_size`] is used
/// directly. For all other kinds Hoeffding is not applicable, so a
/// conservative default of [`CONSERVATIVE_UNBOUNDED_KN`] is returned, but
/// never below the Hoeffding figure (so a tighter `epsilon` keeps tightening
/// the recommendation).
///
/// # Examples
/// ```
/// use agentlock_fingerprint::{
///     recommended_sample_size, FingerprintSchema, MetricId, MetricKind, MetricSpec,
/// };
/// let schema = FingerprintSchema {
///     schema_id: "s".into(),
///     version: 1,
///     metrics: vec![MetricSpec {
///         id: MetricId::new("acc"),
///         kind: MetricKind::Bounded01,
///         higher_is_better: true,
///         description: String::new(),
///     }],
/// };
/// let recs = recommended_sample_size(&schema, 0.02, 0.01);
/// assert_eq!(recs.len(), 1);
/// assert!(recs[0].1 >= 6620);
/// ```
pub fn recommended_sample_size(
    schema: &FingerprintSchema,
    target_epsilon: f64,
    delta: f64,
) -> Vec<(MetricId, usize)> {
    let hoeffding = hoeffding_sample_size(target_epsilon, delta);
    schema
        .metrics
        .iter()
        .map(|m| {
            let kn = match m.kind {
                MetricKind::Bounded01 => hoeffding,
                _ => CONSERVATIVE_UNBOUNDED_KN.max(hoeffding),
            };
            (m.id.clone(), kn)
        })
        .collect()
}
