//! Bilateral CUSUM drift detector with Siegmund-approximation calibration.

use serde::{Deserialize, Serialize};

use crate::error::FingerprintError;
use crate::schema::MetricId;

/// Direction of a detected drift.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum DriftDirection {
    /// Sentinel metric drifted upward (mean increased).
    Upward,
    /// Sentinel metric drifted downward (mean decreased).
    Downward,
}

/// Alert produced by [`CusumState::update`] when a threshold is crossed.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CusumAlert {
    /// Identifier of the sentinel metric.
    pub metric_id: MetricId,
    /// Direction of the drift.
    pub direction: DriftDirection,
    /// Number of samples observed when the alert fired.
    pub samples_to_detection: u64,
    /// Current value of the firing CUSUM statistic.
    pub s_value: f64,
    /// Threshold `h` against which `s_value` was compared.
    pub threshold: f64,
}

/// Serializable state of a bilateral CUSUM detector.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CusumState {
    /// Sentinel metric id.
    pub metric_id: MetricId,
    /// Reference mean μ₀.
    pub mu_0: f64,
    /// Reference standard deviation σ₀ (must be > 0).
    pub sigma_0: f64,
    /// Half-amplitude `k` of the smallest drift the detector ignores.
    pub k: f64,
    /// Decision threshold `h`.
    pub h: f64,
    /// Upward statistic.
    pub s_plus: f64,
    /// Downward statistic.
    pub s_minus: f64,
    /// Number of observations consumed.
    pub samples_seen: u64,
}

/// Siegmund's approximation of the in-control ARL for a one-sided CUSUM
/// with reference value `k` and threshold `h`, both in raw (unnormalized)
/// units.
///
/// Returns +∞ when the formula degenerates (e.g. `k = 0`).
fn siegmund_arl(h: f64, k: f64, sigma_0: f64) -> f64 {
    if !(h.is_finite() && k.is_finite() && sigma_0.is_finite()) || sigma_0 <= 0.0 || k <= 0.0 || h <= 0.0 {
        return f64::INFINITY;
    }
    let b = h / sigma_0;
    let z = 2.0 * b * k / sigma_0;
    let denom = 2.0 * k * k / (sigma_0 * sigma_0);
    let numer = z.exp() - z - 1.0;
    if denom == 0.0 || !numer.is_finite() {
        f64::INFINITY
    } else {
        numer / denom
    }
}

/// Inverts the Siegmund ARL approximation by binary search to find `h` such
/// that ARL₀(h) ≈ `target_arl` within 1 %.
fn calibrate_h_for_arl(k: f64, sigma_0: f64, target_arl: f64) -> Result<f64, FingerprintError> {
    if !(target_arl.is_finite()) || target_arl <= 1.0 {
        return Err(FingerprintError::Calibration(format!(
            "target_arl must be > 1, got {target_arl}"
        )));
    }
    let (mut lo, mut hi) = (0.1 * sigma_0, 20.0 * sigma_0);
    let mut arl_hi = siegmund_arl(hi, k, sigma_0);
    let mut grow_iters = 0;
    while arl_hi < target_arl && grow_iters < 20 {
        hi *= 2.0;
        arl_hi = siegmund_arl(hi, k, sigma_0);
        grow_iters += 1;
    }
    if arl_hi < target_arl {
        return Err(FingerprintError::Calibration(format!(
            "could not bracket target ARL {target_arl}: max reached {arl_hi}"
        )));
    }

    let tolerance = 0.01;
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        let arl = siegmund_arl(mid, k, sigma_0);
        if (arl - target_arl).abs() / target_arl <= tolerance {
            return Ok(mid);
        }
        if arl < target_arl {
            lo = mid;
        } else {
            hi = mid;
        }
        if (hi - lo).abs() < 1e-12 * sigma_0.max(1.0) {
            return Ok(mid);
        }
    }
    Err(FingerprintError::Calibration(
        "binary search on h did not converge".into(),
    ))
}

impl CusumState {
    /// Builds a calibrated CUSUM detector.
    ///
    /// `delta` is the smallest mean shift to detect (in raw units of the
    /// metric); `k` is set to `delta / 2`. The threshold `h` is calibrated
    /// numerically from `target_arl_no_drift` via Siegmund's approximation.
    ///
    /// # Errors
    /// * [`FingerprintError::InvalidValue`] when `sigma_0 ≤ 0`, `delta ≤ 0`,
    ///   or any input is non-finite.
    /// * [`FingerprintError::Calibration`] when the binary search cannot
    ///   converge.
    ///
    /// # Examples
    /// ```
    /// use agentlock_fingerprint::{CusumState, MetricId};
    /// let s = CusumState::new(
    ///     MetricId::new("error_rate"),
    ///     0.05, 0.01, 0.005, 500.0,
    /// ).unwrap();
    /// assert!(s.h > 0.0);
    /// ```
    pub fn new(
        metric_id: MetricId,
        mu_0: f64,
        sigma_0: f64,
        delta: f64,
        target_arl_no_drift: f64,
    ) -> Result<Self, FingerprintError> {
        if !mu_0.is_finite() {
            return Err(FingerprintError::InvalidValue { field: "mu_0".into() });
        }
        if !sigma_0.is_finite() || sigma_0 <= 0.0 {
            return Err(FingerprintError::InvalidValue {
                field: "sigma_0".into(),
            });
        }
        if !delta.is_finite() || delta <= 0.0 {
            return Err(FingerprintError::InvalidValue { field: "delta".into() });
        }
        let k = delta / 2.0;
        let h = calibrate_h_for_arl(k, sigma_0, target_arl_no_drift)?;
        Ok(CusumState {
            metric_id,
            mu_0,
            sigma_0,
            k,
            h,
            s_plus: 0.0,
            s_minus: 0.0,
            samples_seen: 0,
        })
    }

    /// Consumes one observation. Returns an alert if a threshold is crossed.
    /// The state continues to update after an alert; callers wishing to
    /// suppress repeated alerts can call [`CusumState::reset`].
    ///
    /// # Errors
    /// * [`FingerprintError::InvalidValue`] when `x` is not finite.
    pub fn update(&mut self, x: f64) -> Result<Option<CusumAlert>, FingerprintError> {
        if !x.is_finite() {
            return Err(FingerprintError::InvalidValue { field: "x".into() });
        }
        self.samples_seen = self.samples_seen.saturating_add(1);

        let centered = x - self.mu_0;
        self.s_plus = (self.s_plus + centered - self.k).max(0.0);
        self.s_minus = (self.s_minus - centered - self.k).max(0.0);

        if self.s_plus > self.h {
            return Ok(Some(CusumAlert {
                metric_id: self.metric_id.clone(),
                direction: DriftDirection::Upward,
                samples_to_detection: self.samples_seen,
                s_value: self.s_plus,
                threshold: self.h,
            }));
        }
        if self.s_minus > self.h {
            return Ok(Some(CusumAlert {
                metric_id: self.metric_id.clone(),
                direction: DriftDirection::Downward,
                samples_to_detection: self.samples_seen,
                s_value: self.s_minus,
                threshold: self.h,
            }));
        }
        Ok(None)
    }

    /// Resets the running statistics to zero. Calibration parameters
    /// (`k`, `h`, `mu_0`, `sigma_0`) are preserved.
    pub fn reset(&mut self) {
        self.s_plus = 0.0;
        self.s_minus = 0.0;
        self.samples_seen = 0;
    }
}
