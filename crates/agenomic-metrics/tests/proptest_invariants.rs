//! Property-based invariants for the pure metric functions.
//!
//! These assert the *mathematical* guarantees the metrics promise — bounds,
//! symmetry and monotonicity — over large random input spaces, complementing
//! the example-based unit tests inside the crate.

use agenomic_metrics::{
    alignment_stability, audit_evidence_completeness, behavioral_drift, causal_coverage,
    compliance_confidence, controllability, decision_explainability, jensen_shannon_divergence,
    memory_contamination, policy_adherence, prompt_mutation, provenance_coverage, replay_fidelity,
    tool_risk, trace_completeness, Distribution, EventType, WeightedConfidence,
};
use proptest::prelude::*;

/// A finite f64 in `[0, 1]`.
fn unit() -> impl Strategy<Value = f64> {
    (0.0f64..=1.0).prop_map(|x| x)
}

/// A small categorical distribution over a tiny alphabet, so collisions
/// (shared support) are common.
fn dist() -> impl Strategy<Value = Distribution> {
    proptest::collection::vec((0usize..5usize, 0.0f64..10.0), 0..8).prop_map(|pairs| {
        let mut d = Distribution::new();
        for (cat, w) in pairs {
            let _ = d.add(format!("c{cat}"), w);
        }
        d
    })
}

proptest! {
    #[test]
    fn replay_fidelity_is_bounded(ged in unit(), cos in unit()) {
        let rfs = replay_fidelity(ged, cos);
        prop_assert!((0.0..=1.0).contains(&rfs));
    }

    #[test]
    fn replay_fidelity_decreases_in_ged(ged_a in unit(), ged_b in unit(), cos in unit()) {
        let (lo, hi) = if ged_a <= ged_b { (ged_a, ged_b) } else { (ged_b, ged_a) };
        // More edit distance can never raise fidelity.
        prop_assert!(replay_fidelity(hi, cos) <= replay_fidelity(lo, cos) + 1e-12);
    }

    #[test]
    fn replay_fidelity_increases_in_cossim(ged in unit(), cos_a in unit(), cos_b in unit()) {
        let (lo, hi) = if cos_a <= cos_b { (cos_a, cos_b) } else { (cos_b, cos_a) };
        // More semantic similarity can never lower fidelity.
        prop_assert!(replay_fidelity(ged, hi) >= replay_fidelity(ged, lo) - 1e-12);
    }

    #[test]
    fn tool_risk_is_bounded_and_monotone(
        a in unit(), b in unit(), c in unit(), d in unit(), e in unit(), bump in unit(),
    ) {
        let base = tool_risk(a, b, c, d, e);
        prop_assert!((0.0..=1.0).contains(&base));
        let raised = (a + bump).min(1.0);
        // Raising any single factor never lowers the product.
        prop_assert!(tool_risk(raised, b, c, d, e) >= base - 1e-12);
    }

    #[test]
    fn js_is_symmetric(p in dist(), q in dist()) {
        prop_assert_eq!(jensen_shannon_divergence(&p, &q), jensen_shannon_divergence(&q, &p));
    }

    #[test]
    fn js_is_bounded_and_self_zero(p in dist(), q in dist()) {
        let js = behavioral_drift(&p, &q);
        prop_assert!((0.0..=1.0).contains(&js));
        prop_assert!(!js.is_nan());
        // A distribution never drifts from itself.
        if !p.is_empty() {
            prop_assert!(behavioral_drift(&p, &p).abs() < 1e-9);
        }
    }

    #[test]
    fn coverage_metrics_are_bounded(
        a in 0usize..1000, b in 0usize..1000, c in 0usize..1000, d in 0usize..1000,
    ) {
        for v in [
            decision_explainability(a, b),
            audit_evidence_completeness(a, b),
            causal_coverage(a, b),
            provenance_coverage(a, b),
            policy_adherence(a, b, None),
            controllability(a, b, c, d),
        ] {
            prop_assert!((0.0..=1.0).contains(&v));
        }
    }

    #[test]
    fn contamination_is_bounded(untrusted in 0.0f64..1e6, total in 0.0f64..1e6) {
        let mcs = memory_contamination(untrusted, total);
        prop_assert!((0.0..=1.0).contains(&mcs));
    }

    #[test]
    fn prompt_mutation_is_bounded(edit in unit(), sem in unit()) {
        prop_assert!((0.0..=1.0).contains(&prompt_mutation(edit, sem)));
    }

    #[test]
    fn alignment_stability_is_bounded(series in proptest::collection::vec(unit(), 0..20)) {
        prop_assert!((0.0..=1.0).contains(&alignment_stability(&series)));
    }

    #[test]
    fn compliance_confidence_is_bounded(
        controls in proptest::collection::vec((0.0f64..10.0, unit()), 0..12),
    ) {
        let wc: Vec<WeightedConfidence> = controls
            .into_iter()
            .map(|(w, c)| WeightedConfidence::new(w, c))
            .collect();
        prop_assert!((0.0..=1.0).contains(&compliance_confidence(&wc)));
    }

    #[test]
    fn tcs_is_bounded(captured in proptest::collection::vec(0u8..28, 0..40),
                      expected in proptest::collection::vec(0u8..28, 0..40)) {
        // Map small ints to a couple of distinct event types to exercise set logic.
        let to_evt = |x: u8| {
            if x.is_multiple_of(2) {
                EventType::RunStarted
            } else {
                EventType::RunCompleted
            }
        };
        let cap: Vec<EventType> = captured.into_iter().map(to_evt).collect();
        let exp: Vec<EventType> = expected.into_iter().map(to_evt).collect();
        let tcs = trace_completeness(&cap, &exp);
        prop_assert!((0.0..=1.0).contains(&tcs));
    }
}
