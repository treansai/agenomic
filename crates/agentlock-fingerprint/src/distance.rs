//! Mahalanobis distance and χ²-based same-agent identity test.

use nalgebra::{DMatrix, DVector};
use statrs::distribution::{ChiSquared, ContinuousCDF};

use crate::error::FingerprintError;
use crate::fingerprint::Fingerprint;

/// Default significance level for [`same_agent_test`].
pub const DEFAULT_ALPHA: f64 = 1e-3;

/// Outcome of a same-agent identity test.
#[derive(Clone, Debug)]
pub struct SameAgentResult {
    /// `true` if the test fails to reject H₀ at the chosen level (i.e. the
    /// two fingerprints are statistically indistinguishable).
    pub passes: bool,
    /// Tail probability of observing a Mahalanobis distance at least as
    /// extreme as the one measured, under H₀.
    pub p_value: f64,
    /// Mahalanobis distance (`d_M`).
    pub d_mahalanobis: f64,
    /// Squared Mahalanobis distance (`d_M²`), the χ²(n) statistic.
    pub d_squared: f64,
    /// Critical value `√χ²⁻¹(1-α, n)`.
    pub threshold_at_alpha: f64,
}

fn require_same_shape(f1: &Fingerprint, f2: &Fingerprint) -> Result<usize, FingerprintError> {
    if f1.schema_id != f2.schema_id {
        return Err(FingerprintError::SchemaMismatch {
            expected: f1.schema_id.clone(),
            actual: f2.schema_id.clone(),
        });
    }
    let n = f1.dimension();
    if f2.dimension() != n {
        return Err(FingerprintError::DimensionMismatch {
            schema: n,
            observed: f2.dimension(),
        });
    }
    if f1.covariance.len() != n * n || f2.covariance.len() != n * n {
        return Err(FingerprintError::DimensionMismatch {
            schema: n * n,
            observed: f1.covariance.len().min(f2.covariance.len()),
        });
    }
    Ok(n)
}

fn pooled_inverse(f1: &Fingerprint, f2: &Fingerprint, n: usize) -> Result<DMatrix<f64>, FingerprintError> {
    let mut data = vec![0.0_f64; n * n];
    for i in 0..n {
        for j in 0..n {
            data[j * n + i] = 0.5 * (f1.covariance[i * n + j] + f2.covariance[i * n + j]);
        }
    }
    let pooled = DMatrix::from_vec(n, n, data);

    if let Some(chol) = pooled.clone().cholesky() {
        return Ok(chol.inverse());
    }
    pooled
        .try_inverse()
        .ok_or(FingerprintError::SingularCovariance)
}

/// Computes the Mahalanobis distance between two fingerprints.
///
/// The pooled covariance `(Σ₁ + Σ₂) / 2` is used. Inversion is attempted via
/// Cholesky (since the pooled covariance is symmetric and, for healthy
/// inputs, positive-definite); on failure, a fallback to general inversion
/// is attempted, and finally [`FingerprintError::SingularCovariance`] is
/// returned.
///
/// # Examples
/// ```
/// use agentlock_fingerprint::{mahalanobis_distance, Fingerprint};
/// # let now = chrono::Utc::now();
/// # let mk = |m: Vec<f64>, c: Vec<f64>| Fingerprint {
/// #     schema_id: "s".into(), schema_version: 1, agent_id: "a".into(),
/// #     computed_at: now, probes_count: 1, runs_per_probe: 2,
/// #     variance: vec![c[0]], mean: m, covariance: c, content_hash: [0u8; 32],
/// # };
/// let f = mk(vec![0.5], vec![0.04]);
/// assert!(mahalanobis_distance(&f, &f).unwrap().abs() < 1e-12);
/// ```
pub fn mahalanobis_distance(f1: &Fingerprint, f2: &Fingerprint) -> Result<f64, FingerprintError> {
    let n = require_same_shape(f1, f2)?;
    let inv = pooled_inverse(f1, f2, n)?;
    let delta = DVector::from_iterator(n, f1.mean.iter().zip(&f2.mean).map(|(a, b)| a - b));
    let d_squared = (delta.transpose() * &inv * &delta)[(0, 0)];
    let d_squared = d_squared.max(0.0);
    Ok(d_squared.sqrt())
}

/// Tests whether two fingerprints describe the same agent.
///
/// Under H₀ (identical underlying behavior), `d_M²` follows a χ²(n) law.
/// The test rejects H₀ when the p-value falls below `alpha`. The default
/// significance level used elsewhere in AgentLock is [`DEFAULT_ALPHA`]
/// (1e-3).
///
/// # Examples
/// ```
/// use agentlock_fingerprint::{same_agent_test, Fingerprint, DEFAULT_ALPHA};
/// # let now = chrono::Utc::now();
/// # let mk = |m: Vec<f64>, c: Vec<f64>| Fingerprint {
/// #     schema_id: "s".into(), schema_version: 1, agent_id: "a".into(),
/// #     computed_at: now, probes_count: 1, runs_per_probe: 2,
/// #     variance: vec![c[0]], mean: m, covariance: c, content_hash: [0u8; 32],
/// # };
/// let f = mk(vec![0.5], vec![0.04]);
/// let res = same_agent_test(&f, &f, DEFAULT_ALPHA).unwrap();
/// assert!(res.passes);
/// ```
pub fn same_agent_test(
    f1: &Fingerprint,
    f2: &Fingerprint,
    alpha: f64,
) -> Result<SameAgentResult, FingerprintError> {
    if !(0.0 < alpha && alpha < 1.0) {
        return Err(FingerprintError::InvalidValue { field: "alpha".into() });
    }
    let n = require_same_shape(f1, f2)?;
    let inv = pooled_inverse(f1, f2, n)?;
    let delta = DVector::from_iterator(n, f1.mean.iter().zip(&f2.mean).map(|(a, b)| a - b));
    let d_squared = (delta.transpose() * &inv * &delta)[(0, 0)].max(0.0);
    let d_mahalanobis = d_squared.sqrt();

    let chi2 = ChiSquared::new(n as f64)
        .map_err(|e| FingerprintError::Statistics(e.to_string()))?;
    let p_value = 1.0 - chi2.cdf(d_squared);
    let threshold_at_alpha = chi2.inverse_cdf(1.0 - alpha).sqrt();
    let passes = p_value > alpha;

    Ok(SameAgentResult {
        passes,
        p_value,
        d_mahalanobis,
        d_squared,
        threshold_at_alpha,
    })
}
