//! Re-exports of the shared statistical toolkit from `agenomic-fingerprint`.
//!
//! Phase 2 deliberately **reuses** the fingerprint crate's primitives instead
//! of reimplementing them. This module is the single, documented import point
//! for that toolkit, so that the CLI and the cloud drift detector (P2-2) build
//! on exactly the same Mahalanobis distance, χ² same-agent test, calibrated
//! [`CusumState`] detector and Hoeffding sample-size calibration.
//!
//! What this crate adds *on top of* (never duplicating) the fingerprint
//! toolkit is the categorical Jensen–Shannon divergence in
//! [`crate::divergence`], which the fingerprint crate does not provide.
//!
//! # Examples
//! ```
//! use agenomic_metrics::stats::{CusumState, MetricId};
//! // A calibrated CUSUM detector, reused verbatim from agenomic-fingerprint.
//! let detector = CusumState::new(
//!     MetricId::new("behavioral_drift"),
//!     0.02, 0.01, 0.01, 500.0,
//! )
//! .unwrap();
//! assert!(detector.h > 0.0);
//! ```

pub use agenomic_fingerprint::{
    hoeffding_sample_size, mahalanobis_distance, recommended_sample_size, same_agent_test,
    CusumAlert, CusumState, DriftDirection, Fingerprint, FingerprintError, FingerprintSchema,
    MetricId, MetricKind, MetricSpec, SameAgentResult, SignedFingerprint, DEFAULT_ALPHA,
};
