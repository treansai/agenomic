//! Schema definitions for behavioral fingerprints.
//!
//! A [`FingerprintSchema`] is the canonical, ordered list of metrics measured
//! by a probe set. The order of [`MetricSpec`] entries is part of the schema
//! contract: it determines the index of each metric in every fingerprint
//! vector and covariance matrix.

use serde::{Deserialize, Serialize};

/// Identifier of a metric. Owned `String`, since fingerprints commonly
/// outlive any single borrow and can travel between threads.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Ord, PartialOrd)]
pub struct MetricId(pub String);

impl MetricId {
    /// Creates a new [`MetricId`] from anything that can be converted to a
    /// `String`.
    ///
    /// # Examples
    /// ```
    /// use agentlock_fingerprint::MetricId;
    /// let id = MetricId::new("accuracy");
    /// assert_eq!(id.0, "accuracy");
    /// ```
    pub fn new(s: impl Into<String>) -> Self {
        MetricId(s.into())
    }
}

/// Kind of a metric. Determines the statistical bounds applicable for
/// sample-size calibration.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum MetricKind {
    /// Metric in `[0, 1]` (rates, accuracy, etc.). Hoeffding's inequality
    /// applies.
    Bounded01,
    /// Latency in milliseconds, strictly positive. Hoeffding does not apply;
    /// prefer Bernstein-style or empirical-Bernstein bounds.
    Latency,
    /// Cost in currency units, strictly positive. Same caveats as
    /// [`MetricKind::Latency`].
    Cost,
    /// Arbitrary real-valued metric. No closed-form Hoeffding bound.
    Unbounded,
}

/// Specification of a single metric within a [`FingerprintSchema`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MetricSpec {
    /// Stable identifier of the metric.
    pub id: MetricId,
    /// Kind of the metric.
    pub kind: MetricKind,
    /// Whether higher values are considered better. Used by reporting tools.
    pub higher_is_better: bool,
    /// Free-form description, included in human-readable reports.
    pub description: String,
}

/// Ordered list of metrics measured by a canonical probe set.
///
/// The schema is stable across versions of an agent: a newer agent that
/// optimizes the same KPIs as an older agent must reuse the same
/// `schema_id` and metric ordering.
///
/// # Examples
/// ```
/// use agentlock_fingerprint::{FingerprintSchema, MetricId, MetricKind, MetricSpec};
/// let schema = FingerprintSchema {
///     schema_id: "demo-v1".into(),
///     version: 1,
///     metrics: vec![MetricSpec {
///         id: MetricId::new("accuracy"),
///         kind: MetricKind::Bounded01,
///         higher_is_better: true,
///         description: "Top-1 accuracy".into(),
///     }],
/// };
/// assert_eq!(schema.dimension(), 1);
/// ```
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FingerprintSchema {
    /// Stable identifier (e.g. `"insurance-claims-canonical-v3"`).
    pub schema_id: String,
    /// Monotonically increasing version of the schema.
    pub version: u32,
    /// Ordered list of metrics. The position of a metric in this vector is
    /// its index in every fingerprint vector and covariance matrix.
    pub metrics: Vec<MetricSpec>,
}

impl FingerprintSchema {
    /// Number of metrics defined by the schema.
    pub fn dimension(&self) -> usize {
        self.metrics.len()
    }

    /// Returns the index of a metric, or `None` if absent.
    pub fn index_of(&self, id: &MetricId) -> Option<usize> {
        self.metrics.iter().position(|m| &m.id == id)
    }
}
