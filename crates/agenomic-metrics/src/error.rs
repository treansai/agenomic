//! Error types for the `agenomic-metrics` crate.
//!
//! The metric functions themselves are total (they never fail — bad inputs
//! are clamped or treated with an explicit, documented convention). Errors
//! only arise at the *boundaries*: building a [`Distribution`](crate::Distribution)
//! from raw weights, or parsing a canonical trace from JSON.

/// Errors produced when constructing metric inputs or parsing traces.
#[derive(Debug, thiserror::Error)]
pub enum MetricsError {
    /// A probability/weight value was negative or non-finite (`NaN`/`±Inf`).
    #[error("invalid weight for category {category:?}: {value} (must be finite and >= 0)")]
    InvalidWeight {
        /// The offending category key.
        category: String,
        /// The rejected value.
        value: f64,
    },

    /// A canonical trace could not be parsed from its JSON representation.
    #[error("failed to parse canonical v0.3 trace: {0}")]
    TraceParse(String),
}
