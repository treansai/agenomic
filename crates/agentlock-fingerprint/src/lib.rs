//! # agentlock-fingerprint
//!
//! Behavioral fingerprints for AgentLock agents.
//!
//! This crate produces and compares the empirical *behavioral fingerprint*
//! of an agent, defined as a vector of `n` metrics measured by replaying the
//! agent `KN` times against a canonical probe set. Two agents whose
//! fingerprints are statistically indistinguishable are operationally the
//! same agent.
//!
//! The crate exposes five capabilities, each fully tested:
//!
//! 1. [`estimate_fingerprint`] estimates `(mean, covariance)` from raw
//!    [`RunResult`]s.
//! 2. [`mahalanobis_distance`] returns the Mahalanobis distance between two
//!    fingerprints under a pooled covariance.
//! 3. [`same_agent_test`] decides whether two fingerprints describe the
//!    same agent via a χ² test at significance level α (default
//!    [`DEFAULT_ALPHA`] = 1e-3).
//! 4. [`hoeffding_sample_size`] / [`recommended_sample_size`] calibrate
//!    `KN` for a desired `(ε, δ)` precision.
//! 5. [`CusumState`] is a serializable bilateral CUSUM detector calibrated
//!    against an in-control ARL via Siegmund's approximation.
//!
//! Fingerprints are content-hashed with BLAKE3 over their canonical CBOR
//! encoding (RFC 8949 §4.2) and can be signed with ed25519 via
//! [`sign_fingerprint`].
//!
//! ## End-to-end example
//!
//! ```
//! use agentlock_fingerprint::{
//!     estimate_fingerprint, same_agent_test, CusumState, FingerprintSchema,
//!     MetricId, MetricKind, MetricSpec, RunResult, DEFAULT_ALPHA,
//! };
//!
//! let schema = FingerprintSchema {
//!     schema_id: "demo-v1".into(),
//!     version: 1,
//!     metrics: vec![MetricSpec {
//!         id: MetricId::new("accuracy"),
//!         kind: MetricKind::Bounded01,
//!         higher_is_better: true,
//!         description: "Top-1 accuracy".into(),
//!     }],
//! };
//!
//! let runs: Vec<RunResult> = (0..200)
//!     .map(|i| RunResult {
//!         probe_id: format!("p-{}", i % 10),
//!         metric_values: vec![0.85 + ((i % 7) as f64 - 3.0) * 0.005],
//!     })
//!     .collect();
//! let fp = estimate_fingerprint(&schema, "agent-A", &runs).unwrap();
//! let res = same_agent_test(&fp, &fp, DEFAULT_ALPHA).unwrap();
//! assert!(res.passes);
//!
//! let mut detector = CusumState::new(
//!     MetricId::new("accuracy"), fp.mean[0], fp.variance[0].sqrt().max(1e-6),
//!     0.05, 500.0,
//! ).unwrap();
//! let _ = detector.update(fp.mean[0]).unwrap();
//! ```

#![deny(missing_docs)]
#![warn(rust_2018_idioms)]

pub mod calibration;
pub mod cusum;
pub mod distance;
pub mod error;
pub mod estimation;
pub mod fingerprint;
pub mod schema;

pub use calibration::{hoeffding_sample_size, recommended_sample_size, CONSERVATIVE_UNBOUNDED_KN};
pub use cusum::{CusumAlert, CusumState, DriftDirection};
pub use distance::{mahalanobis_distance, same_agent_test, SameAgentResult, DEFAULT_ALPHA};
pub use error::FingerprintError;
pub use estimation::{estimate_fingerprint, RunResult};
pub use fingerprint::{
    sign_fingerprint, verify_signed_fingerprint, Fingerprint, SignedFingerprint, HASH_DOMAIN,
};
pub use schema::{FingerprintSchema, MetricId, MetricKind, MetricSpec};
