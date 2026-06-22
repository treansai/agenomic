//! # agenomic-metrics
//!
//! Pure, deterministic **agentic evaluation metrics** for Agenomic.
//!
//! This crate is the Phase 2 measurement layer. It turns the static proofs
//! of Phase 0/1 (canonical trace, immutable ledger, attestations, evidence
//! package) into a dynamic, comparable, *quantitative* picture of agent
//! behavior. Every function here is:
//!
//! * **pure** — no I/O, no clock, no global state;
//! * **deterministic** — same inputs always produce the same output;
//! * **bounded** — completeness/coverage/adherence metrics live in `[0, 1]`;
//! * **documented** — every public item carries an executable doctest.
//!
//! ## Relationship to `agenomic-fingerprint`
//!
//! This crate **reuses** the statistical toolkit from
//! [`agenomic_fingerprint`] (Mahalanobis distance, the χ² same-agent test,
//! the calibrated bilateral [`CusumState`](stats::CusumState) drift detector
//! and Hoeffding sample-size calibration) rather than reimplementing it. Those
//! primitives are re-exported from the [`stats`] module so that the CLI and
//! the cloud share one coherent surface.
//!
//! The one statistic the fingerprint crate does *not* provide is the
//! categorical **Jensen–Shannon divergence** used for the Behavioral Drift
//! Score (`BDS`). It is complementary (categorical action distributions, not
//! continuous fingerprints), so it is implemented here in [`divergence`].
//!
//! ## What lives where
//!
//! * [`metrics`] — the pure metric functions (`TCS`, `RFS`, `PAS`, `BDS`,
//!   `TRS`, `MCS`, …).
//! * [`divergence`] — the [`Distribution`] type and
//!   [`jensen_shannon_divergence`].
//! * [`thresholds`] — the centralized [`Thresholds`] with documented MVP
//!   values.
//! * [`report`] — the serializable [`MetricsReport`] roll-up.
//! * [`trace`] — a minimal, faithful model of the canonical v0.3 run trace
//!   plus [`metrics_from_trace_v03`].
//! * [`stats`] — re-exports of the shared `agenomic-fingerprint` toolkit.
//!
//! ## Calibration caveat
//!
//! The MVP thresholds and any extrapolated formulae (e.g. the ABC drift
//! bound `D* = α / γ`, the Prompt Mutation Score) are **starting points to
//! calibrate per domain and sector**, never universal truths. They are
//! marked `[EXTRAPOLATION]` / `[SOURCÉ]` in the documentation of the relevant
//! items.
//!
//! ## End-to-end example
//!
//! ```
//! use agenomic_metrics::{
//!     replay_fidelity, exact_replay_match, behavioral_drift, Distribution,
//!     Thresholds,
//! };
//!
//! // Functional replay: a near-perfect structural + semantic match.
//! let rfs = replay_fidelity(0.02, 0.99);
//! assert!(rfs > 0.95);
//!
//! // Exact replay is a *boolean* hash comparison — never a continuous score.
//! assert!(exact_replay_match(
//!     "blake3-merkle-v1:abc",
//!     "blake3-merkle-v1:abc",
//! ));
//!
//! // Behavioral drift between two action distributions.
//! let v1 = Distribution::from_pairs([("tool.call", 8.0), ("decision", 2.0)]).unwrap();
//! let v2 = Distribution::from_pairs([("tool.call", 8.0), ("decision", 2.0)]).unwrap();
//! assert_eq!(behavioral_drift(&v1, &v2), 0.0);
//!
//! let thresholds = Thresholds::mvp();
//! assert_eq!(thresholds.replay_fidelity_min, 0.95);
//! ```

#![deny(missing_docs)]
#![warn(rust_2018_idioms)]

pub mod divergence;
pub mod error;
pub mod metrics;
pub mod report;
pub mod stats;
pub mod thresholds;
pub mod trace;

pub use divergence::{jensen_shannon_divergence, Distribution};
pub use error::MetricsError;
pub use metrics::{
    alignment_stability, audit_evidence_completeness, behavioral_drift, causal_coverage,
    compliance_confidence, controllability, decision_explainability, drift_bound,
    exact_replay_match, memory_contamination, policy_adherence, prompt_mutation,
    provenance_coverage, replay_fidelity, runtime_variance, tool_risk, trace_completeness,
    WeightedConfidence,
};
pub use report::MetricsReport;
pub use thresholds::Thresholds;
pub use trace::{metrics_from_trace_v03, EventType, TraceV03};
