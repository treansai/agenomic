//! Categorical probability distributions and Jensen–Shannon divergence.
//!
//! The Behavioral Drift Score (`BDS`) compares two *categorical* action
//! distributions (e.g. the relative frequency of `tool.call.executed`,
//! `decision.made`, `error.raised`, …). The natural distance for such
//! distributions is the **Jensen–Shannon divergence**, which — unlike the
//! Mahalanobis distance the fingerprint crate uses for *continuous*
//! fingerprints — is symmetric, always finite, and (with a base-2 logarithm)
//! bounded in `[0, 1]`.
//!
//! This is why `JS` lives here rather than in `agenomic-fingerprint`: it is a
//! complementary statistic, not a duplicate of one already provided.

use std::collections::BTreeMap;

use crate::error::MetricsError;

/// A non-negative weighting over named categories — i.e. an unnormalized
/// categorical distribution.
///
/// Weights are stored verbatim (not normalized on insert) in a deterministic
/// [`BTreeMap`] so that iteration order — and therefore every derived metric —
/// is reproducible. Probabilities are computed lazily by
/// [`Distribution::probability`].
///
/// # Examples
/// ```
/// use agenomic_metrics::Distribution;
/// let d = Distribution::from_pairs([("a", 3.0), ("b", 1.0)]).unwrap();
/// assert_eq!(d.total(), 4.0);
/// assert_eq!(d.probability("a"), 0.75);
/// assert_eq!(d.probability("missing"), 0.0);
/// ```
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Distribution {
    weights: BTreeMap<String, f64>,
}

impl Distribution {
    /// Creates an empty distribution (total mass `0`).
    pub fn new() -> Self {
        Self {
            weights: BTreeMap::new(),
        }
    }

    /// Builds a distribution from `(category, weight)` pairs, summing the
    /// weights of duplicate categories.
    ///
    /// # Errors
    /// Returns [`MetricsError::InvalidWeight`] if any weight is negative or
    /// non-finite.
    ///
    /// # Examples
    /// ```
    /// use agenomic_metrics::Distribution;
    /// let d = Distribution::from_pairs([("x", 1.0), ("x", 2.0)]).unwrap();
    /// assert_eq!(d.probability("x"), 1.0);
    /// assert!(Distribution::from_pairs([("bad", -1.0)]).is_err());
    /// ```
    pub fn from_pairs<K, I>(pairs: I) -> Result<Self, MetricsError>
    where
        K: Into<String>,
        I: IntoIterator<Item = (K, f64)>,
    {
        let mut dist = Self::new();
        for (k, v) in pairs {
            dist.add(k, v)?;
        }
        Ok(dist)
    }

    /// Builds a distribution from integer counts. Counts are always valid
    /// weights, so this is infallible.
    ///
    /// # Examples
    /// ```
    /// use agenomic_metrics::Distribution;
    /// let d = Distribution::from_counts([("hit", 9u64), ("miss", 1)]);
    /// assert_eq!(d.probability("hit"), 0.9);
    /// ```
    pub fn from_counts<K, I>(counts: I) -> Self
    where
        K: Into<String>,
        I: IntoIterator<Item = (K, u64)>,
    {
        let mut dist = Self::new();
        for (k, c) in counts {
            // Counts are finite and non-negative by construction.
            let _ = dist.add(k, c as f64);
        }
        dist
    }

    /// Adds `weight` to `category` (accumulating onto any existing weight).
    ///
    /// # Errors
    /// Returns [`MetricsError::InvalidWeight`] if `weight` is negative or
    /// non-finite.
    ///
    /// # Examples
    /// ```
    /// use agenomic_metrics::Distribution;
    /// let mut d = Distribution::new();
    /// d.add("a", 2.0).unwrap();
    /// d.add("a", 3.0).unwrap();
    /// assert_eq!(d.probability("a"), 1.0);
    /// ```
    pub fn add(&mut self, category: impl Into<String>, weight: f64) -> Result<(), MetricsError> {
        let category = category.into();
        if !weight.is_finite() || weight < 0.0 {
            return Err(MetricsError::InvalidWeight {
                category,
                value: weight,
            });
        }
        *self.weights.entry(category).or_insert(0.0) += weight;
        Ok(())
    }

    /// Total accumulated weight across all categories.
    pub fn total(&self) -> f64 {
        self.weights.values().sum()
    }

    /// `true` if the distribution carries no mass (no categories, or all
    /// weights zero).
    pub fn is_empty(&self) -> bool {
        self.total() == 0.0
    }

    /// Normalized probability of `category` (`0.0` for absent categories or
    /// an empty distribution).
    pub fn probability(&self, category: &str) -> f64 {
        let total = self.total();
        if total == 0.0 {
            return 0.0;
        }
        self.weights.get(category).copied().unwrap_or(0.0) / total
    }

    /// Iterator over the distinct category keys, in deterministic (sorted)
    /// order.
    pub fn categories(&self) -> impl Iterator<Item = &str> {
        self.weights.keys().map(String::as_str)
    }
}

/// Binary-entropy-style `p * log2(p / q)` term, with the limit convention
/// `0 * log2(0/q) = 0`. Callers must guarantee `q > 0` whenever `p > 0`
/// (the Jensen–Shannon construction below always does, because the mixture
/// `M` has support wherever either input does).
fn kl_term(p: f64, q: f64) -> f64 {
    if p <= 0.0 {
        0.0
    } else {
        p * (p / q).log2()
    }
}

/// Jensen–Shannon divergence between two categorical distributions, in
/// **bits** (base-2 logarithm), hence bounded in `[0, 1]`.
///
/// `JS(P ‖ Q) = ½·KL(P ‖ M) + ½·KL(Q ‖ M)` where `M = ½·(P + Q)`. It is
/// symmetric and always finite.
///
/// Edge cases (handled without panicking):
/// * two empty distributions → `0.0` (both describe "no behavior", so they
///   are identical);
/// * exactly one empty distribution → `1.0` (maximal divergence: one window
///   exhibits behavior, the other none).
///
/// # Examples
/// ```
/// use agenomic_metrics::{Distribution, jensen_shannon_divergence};
/// let p = Distribution::from_pairs([("a", 1.0), ("b", 1.0)]).unwrap();
/// assert_eq!(jensen_shannon_divergence(&p, &p), 0.0);
///
/// // Disjoint supports → maximal divergence of 1 bit.
/// let q = Distribution::from_pairs([("c", 1.0), ("d", 1.0)]).unwrap();
/// let js = jensen_shannon_divergence(&p, &q);
/// assert!((js - 1.0).abs() < 1e-12);
/// ```
pub fn jensen_shannon_divergence(p: &Distribution, q: &Distribution) -> f64 {
    match (p.is_empty(), q.is_empty()) {
        (true, true) => return 0.0,
        (true, false) | (false, true) => return 1.0,
        (false, false) => {}
    }

    let mut categories: BTreeMap<&str, ()> = BTreeMap::new();
    for c in p.categories() {
        categories.insert(c, ());
    }
    for c in q.categories() {
        categories.insert(c, ());
    }

    let mut kl_pm = 0.0;
    let mut kl_qm = 0.0;
    for &cat in categories.keys() {
        let pp = p.probability(cat);
        let qq = q.probability(cat);
        let m = 0.5 * (pp + qq);
        kl_pm += kl_term(pp, m);
        kl_qm += kl_term(qq, m);
    }

    let js = 0.5 * kl_pm + 0.5 * kl_qm;
    // Clamp away any floating-point spill outside the theoretical [0, 1].
    js.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_distributions_have_zero_divergence() {
        let p = Distribution::from_pairs([("a", 3.0), ("b", 7.0)]).unwrap();
        assert_eq!(jensen_shannon_divergence(&p, &p), 0.0);
    }

    #[test]
    fn divergence_is_symmetric() {
        let p = Distribution::from_pairs([("a", 1.0), ("b", 3.0)]).unwrap();
        let q = Distribution::from_pairs([("a", 4.0), ("b", 1.0), ("c", 1.0)]).unwrap();
        assert_eq!(
            jensen_shannon_divergence(&p, &q),
            jensen_shannon_divergence(&q, &p)
        );
    }

    #[test]
    fn disjoint_supports_reach_one_bit() {
        let p = Distribution::from_counts([("x", 5u64)]);
        let q = Distribution::from_counts([("y", 5u64)]);
        assert!((jensen_shannon_divergence(&p, &q) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn empty_pair_conventions() {
        let empty = Distribution::new();
        let some = Distribution::from_counts([("a", 1u64)]);
        assert_eq!(jensen_shannon_divergence(&empty, &empty), 0.0);
        assert_eq!(jensen_shannon_divergence(&empty, &some), 1.0);
        assert_eq!(jensen_shannon_divergence(&some, &empty), 1.0);
    }

    #[test]
    fn unnormalized_weights_do_not_matter() {
        // Same shape, different scale → identical distribution.
        let p = Distribution::from_pairs([("a", 1.0), ("b", 1.0)]).unwrap();
        let q = Distribution::from_pairs([("a", 100.0), ("b", 100.0)]).unwrap();
        assert!(jensen_shannon_divergence(&p, &q) < 1e-12);
    }

    #[test]
    fn rejects_negative_and_nan_weights() {
        assert!(Distribution::from_pairs([("a", -0.1)]).is_err());
        assert!(Distribution::from_pairs([("a", f64::NAN)]).is_err());
        assert!(Distribution::from_pairs([("a", f64::INFINITY)]).is_err());
    }
}
