//! Error types for the `agenomic-fingerprint` crate.

/// Errors produced by fingerprint estimation, comparison, calibration,
/// drift detection and signing.
#[derive(Debug, thiserror::Error)]
pub enum FingerprintError {
    /// The schema attached to a fingerprint does not match the one expected
    /// by the caller.
    #[error("schema mismatch: expected {expected}, got {actual}")]
    SchemaMismatch {
        /// Schema id expected by the caller.
        expected: String,
        /// Schema id actually present.
        actual: String,
    },

    /// A vector or matrix has the wrong number of metrics.
    #[error("dimension mismatch: schema has {schema} metrics, observed {observed}")]
    DimensionMismatch {
        /// Number of metrics defined in the schema.
        schema: usize,
        /// Number of metrics actually observed.
        observed: usize,
    },

    /// A `f64` value was `NaN` or `±Inf` where it must be finite.
    #[error("invalid value: {field} contains NaN or Inf")]
    InvalidValue {
        /// Human-readable field name.
        field: String,
    },

    /// At least two runs are required to estimate a covariance.
    #[error("insufficient samples: at least 2 runs required, got {0}")]
    InsufficientSamples(usize),

    /// The covariance matrix could not be inverted.
    #[error("covariance matrix is not invertible (likely singular)")]
    SingularCovariance,

    /// CBOR (de)serialization failure.
    #[error("cbor error: {0}")]
    Cbor(String),

    /// ed25519 signature creation or verification failure.
    #[error("signature error: {0}")]
    Signature(String),

    /// A statistical-distribution constructor rejected its arguments.
    #[error("statistics error: {0}")]
    Statistics(String),

    /// A numeric calibration routine failed to converge.
    #[error("calibration failure: {0}")]
    Calibration(String),
}
