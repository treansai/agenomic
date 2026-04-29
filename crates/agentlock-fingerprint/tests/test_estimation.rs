use agentlock_fingerprint::{
    estimate_fingerprint, FingerprintError, FingerprintSchema, MetricId, MetricKind, MetricSpec,
    RunResult,
};
use approx::assert_relative_eq;
use rand::{rngs::StdRng, SeedableRng};
use rand_distr::{Distribution, Normal};

fn schema(metrics: &[(&str, MetricKind)]) -> FingerprintSchema {
    FingerprintSchema {
        schema_id: "test-schema".into(),
        version: 1,
        metrics: metrics
            .iter()
            .map(|(id, kind)| MetricSpec {
                id: MetricId::new(*id),
                kind: *kind,
                higher_is_better: true,
                description: String::new(),
            })
            .collect(),
    }
}

#[test]
fn estimates_mean_and_covariance_on_synthetic_runs() {
    let s = schema(&[
        ("acc", MetricKind::Bounded01),
        ("lat", MetricKind::Latency),
        ("cost", MetricKind::Cost),
    ]);
    let mut rng = StdRng::seed_from_u64(42);
    let n_acc: Normal<f64> = Normal::new(0.85, 0.05).unwrap();
    let n_lat: Normal<f64> = Normal::new(120.0, 12.0).unwrap();
    let n_cost: Normal<f64> = Normal::new(0.04, 0.005).unwrap();

    let runs: Vec<RunResult> = (0..1000)
        .map(|i| RunResult {
            probe_id: format!("p-{}", i % 10),
            metric_values: vec![
                n_acc.sample(&mut rng).clamp(0.0, 1.0),
                n_lat.sample(&mut rng).max(0.0),
                n_cost.sample(&mut rng).max(0.0),
            ],
        })
        .collect();

    let fp = estimate_fingerprint(&s, "agent-A", &runs).unwrap();

    assert_relative_eq!(fp.mean[0], 0.85, epsilon = 0.02);
    assert_relative_eq!(fp.mean[1], 120.0, epsilon = 1.5);
    assert_relative_eq!(fp.mean[2], 0.04, epsilon = 0.002);

    let n = fp.dimension();
    for i in 0..n {
        for j in 0..n {
            assert_relative_eq!(
                fp.covariance[i * n + j],
                fp.covariance[j * n + i],
                epsilon = 1e-12,
            );
        }
        assert_relative_eq!(fp.variance[i], fp.covariance[i * n + i], epsilon = 1e-12);
        assert!(fp.variance[i] > 0.0);
    }
    assert_eq!(fp.probes_count, 10);
    assert_eq!(fp.runs_per_probe, 100);
}

#[test]
fn dimension_mismatch_is_detected() {
    let s = schema(&[("a", MetricKind::Bounded01), ("b", MetricKind::Bounded01)]);
    let runs = vec![
        RunResult {
            probe_id: "p".into(),
            metric_values: vec![0.5, 0.6],
        },
        RunResult {
            probe_id: "p".into(),
            metric_values: vec![0.5],
        },
    ];
    match estimate_fingerprint(&s, "a", &runs) {
        Err(FingerprintError::DimensionMismatch { schema, observed }) => {
            assert_eq!(schema, 2);
            assert_eq!(observed, 1);
        }
        other => panic!("expected DimensionMismatch, got {other:?}"),
    }
}

#[test]
fn nan_in_run_is_rejected() {
    let s = schema(&[("a", MetricKind::Bounded01)]);
    let runs = vec![
        RunResult {
            probe_id: "p".into(),
            metric_values: vec![0.5],
        },
        RunResult {
            probe_id: "p".into(),
            metric_values: vec![f64::NAN],
        },
    ];
    assert!(matches!(
        estimate_fingerprint(&s, "a", &runs),
        Err(FingerprintError::InvalidValue { .. })
    ));
}

#[test]
fn fewer_than_two_runs_is_rejected() {
    let s = schema(&[("a", MetricKind::Bounded01)]);
    let runs = vec![RunResult {
        probe_id: "p".into(),
        metric_values: vec![0.5],
    }];
    assert!(matches!(
        estimate_fingerprint(&s, "a", &runs),
        Err(FingerprintError::InsufficientSamples(1))
    ));
    let zero: Vec<RunResult> = vec![];
    assert!(matches!(
        estimate_fingerprint(&s, "a", &zero),
        Err(FingerprintError::InsufficientSamples(0))
    ));
}

#[test]
fn covariance_is_symmetric() {
    let s = schema(&[
        ("a", MetricKind::Bounded01),
        ("b", MetricKind::Bounded01),
        ("c", MetricKind::Bounded01),
    ]);
    let mut rng = StdRng::seed_from_u64(7);
    let dists = [
        Normal::new(0.4, 0.1).unwrap(),
        Normal::new(0.6, 0.05).unwrap(),
        Normal::new(0.2, 0.02).unwrap(),
    ];
    let runs: Vec<RunResult> = (0..500)
        .map(|i| RunResult {
            probe_id: format!("p-{}", i % 5),
            metric_values: dists.iter().map(|d| d.sample(&mut rng)).collect(),
        })
        .collect();
    let fp = estimate_fingerprint(&s, "a", &runs).unwrap();
    let n = fp.dimension();
    for i in 0..n {
        for j in 0..n {
            assert_relative_eq!(
                fp.covariance[i * n + j],
                fp.covariance[j * n + i],
                epsilon = 1e-12,
            );
        }
    }
}
