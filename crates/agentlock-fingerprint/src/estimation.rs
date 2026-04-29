//! Empirical estimation of a [`Fingerprint`] from a batch of run results.

use chrono::Utc;

use crate::error::FingerprintError;
use crate::fingerprint::Fingerprint;
use crate::schema::FingerprintSchema;

/// Result of replaying an agent on a single probe.
#[derive(Clone, Debug)]
pub struct RunResult {
    /// Identifier of the probe (e.g. `"probe-0007"`).
    pub probe_id: String,
    /// Metric values, ordered like `FingerprintSchema::metrics`.
    pub metric_values: Vec<f64>,
}

fn validate_finite(name: &str, x: f64) -> Result<(), FingerprintError> {
    if x.is_finite() {
        Ok(())
    } else {
        Err(FingerprintError::InvalidValue { field: name.into() })
    }
}

/// Estimates a [`Fingerprint`] from a batch of [`RunResult`]s.
///
/// `mean[i]` is the arithmetic mean across all runs of metric `i`.
/// `covariance[i,j]` is the unbiased empirical covariance with denominator
/// `KN - 1`, stored row-major. `variance[i]` mirrors the diagonal.
///
/// # Errors
/// * [`FingerprintError::InsufficientSamples`] if fewer than 2 runs are
///   provided.
/// * [`FingerprintError::DimensionMismatch`] if any run vector has a
///   different length than the schema.
/// * [`FingerprintError::InvalidValue`] if a metric value is `NaN`/`Inf`.
///
/// # Examples
/// ```
/// use agentlock_fingerprint::{
///     estimate_fingerprint, FingerprintSchema, MetricId, MetricKind,
///     MetricSpec, RunResult,
/// };
/// let schema = FingerprintSchema {
///     schema_id: "demo".into(),
///     version: 1,
///     metrics: vec![MetricSpec {
///         id: MetricId::new("acc"),
///         kind: MetricKind::Bounded01,
///         higher_is_better: true,
///         description: String::new(),
///     }],
/// };
/// let runs = vec![
///     RunResult { probe_id: "p".into(), metric_values: vec![0.8] },
///     RunResult { probe_id: "p".into(), metric_values: vec![0.9] },
/// ];
/// let fp = estimate_fingerprint(&schema, "agent", &runs).unwrap();
/// assert!((fp.mean[0] - 0.85).abs() < 1e-12);
/// ```
pub fn estimate_fingerprint(
    schema: &FingerprintSchema,
    agent_id: &str,
    runs: &[RunResult],
) -> Result<Fingerprint, FingerprintError> {
    let kn = runs.len();
    if kn < 2 {
        return Err(FingerprintError::InsufficientSamples(kn));
    }

    let n = schema.dimension();
    if n == 0 {
        return Err(FingerprintError::DimensionMismatch {
            schema: 0,
            observed: 0,
        });
    }

    for run in runs {
        if run.metric_values.len() != n {
            return Err(FingerprintError::DimensionMismatch {
                schema: n,
                observed: run.metric_values.len(),
            });
        }
        for (i, v) in run.metric_values.iter().enumerate() {
            validate_finite(&format!("metric_values[{i}]"), *v)?;
        }
    }

    let kn_f = kn as f64;
    let mut mean = vec![0.0_f64; n];
    for run in runs {
        for (i, v) in run.metric_values.iter().enumerate() {
            mean[i] += *v;
        }
    }
    for m in &mut mean {
        *m /= kn_f;
    }

    let mut covariance = vec![0.0_f64; n * n];
    let denom = kn_f - 1.0;
    for run in runs {
        for i in 0..n {
            let di = run.metric_values[i] - mean[i];
            for j in 0..n {
                let dj = run.metric_values[j] - mean[j];
                covariance[i * n + j] += di * dj;
            }
        }
    }
    for c in &mut covariance {
        *c /= denom;
    }

    let variance: Vec<f64> = (0..n).map(|i| covariance[i * n + i]).collect();

    let probes_count = {
        let mut ids: Vec<&str> = runs.iter().map(|r| r.probe_id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        ids.len()
    };
    let runs_per_probe = if probes_count == 0 { 0 } else { kn / probes_count };

    let mut fp = Fingerprint {
        schema_id: schema.schema_id.clone(),
        schema_version: schema.version,
        agent_id: agent_id.to_string(),
        computed_at: Utc::now(),
        probes_count,
        runs_per_probe,
        mean,
        variance,
        covariance,
        content_hash: [0u8; 32],
    };
    fp.content_hash = fp.compute_content_hash()?;
    Ok(fp)
}
