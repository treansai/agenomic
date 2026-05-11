use agenomic_fingerprint::{
    estimate_fingerprint, mahalanobis_distance, same_agent_test, FingerprintError,
    FingerprintSchema, MetricId, MetricKind, MetricSpec, RunResult, DEFAULT_ALPHA,
};
use rand::{rngs::StdRng, SeedableRng};
use rand_distr::{Distribution, Normal};

fn schema(n: usize) -> FingerprintSchema {
    FingerprintSchema {
        schema_id: "s".into(),
        version: 1,
        metrics: (0..n)
            .map(|i| MetricSpec {
                id: MetricId::new(format!("m{i}")),
                kind: MetricKind::Bounded01,
                higher_is_better: true,
                description: String::new(),
            })
            .collect(),
    }
}

fn estimate(rng_seed: u64, schema: &FingerprintSchema, means: &[f64], stds: &[f64]) -> agenomic_fingerprint::Fingerprint {
    let mut rng = StdRng::seed_from_u64(rng_seed);
    let dists: Vec<Normal<f64>> = means
        .iter()
        .zip(stds)
        .map(|(m, s)| Normal::new(*m, *s).unwrap())
        .collect();
    let runs: Vec<RunResult> = (0..2000)
        .map(|i| RunResult {
            probe_id: format!("p-{}", i % 20),
            metric_values: dists.iter().map(|d| d.sample(&mut rng)).collect(),
        })
        .collect();
    estimate_fingerprint(schema, "agent", &runs).unwrap()
}

#[test]
fn distance_to_self_is_zero() {
    let s = schema(3);
    let f = estimate(1, &s, &[0.5, 0.3, 0.7], &[0.1, 0.05, 0.08]);
    let d = mahalanobis_distance(&f, &f).unwrap();
    assert!(d.abs() < 1e-10, "d={d}");
}

#[test]
fn distance_is_symmetric() {
    let s = schema(3);
    let f1 = estimate(1, &s, &[0.5, 0.3, 0.7], &[0.1, 0.05, 0.08]);
    let f2 = estimate(2, &s, &[0.55, 0.32, 0.68], &[0.1, 0.05, 0.08]);
    let d12 = mahalanobis_distance(&f1, &f2).unwrap();
    let d21 = mahalanobis_distance(&f2, &f1).unwrap();
    assert!((d12 - d21).abs() < 1e-9, "d12={d12} d21={d21}");
}

#[test]
fn identical_distributions_pass_test() {
    let s = schema(3);
    let f1 = estimate(11, &s, &[0.5, 0.3, 0.7], &[0.1, 0.05, 0.08]);
    let f2 = estimate(22, &s, &[0.5, 0.3, 0.7], &[0.1, 0.05, 0.08]);
    let res = same_agent_test(&f1, &f2, DEFAULT_ALPHA).unwrap();
    assert!(res.passes, "p_value={}", res.p_value);
    assert!(res.p_value > 0.5 || res.d_squared < res.threshold_at_alpha.powi(2));
}

#[test]
fn six_sigma_drift_on_one_metric_among_nine_is_rejected() {
    let s = schema(9);
    let means = vec![0.5; 9];
    let stds = vec![0.05; 9];
    let f1 = estimate(101, &s, &means, &stds);
    let mut means2 = means.clone();
    means2[3] += 6.0 * 0.05;
    let f2 = estimate(202, &s, &means2, &stds);
    let res = same_agent_test(&f1, &f2, DEFAULT_ALPHA).unwrap();
    assert!(!res.passes, "should reject H0; p_value={}", res.p_value);
    assert!(res.p_value < 1e-3);
}

#[test]
fn singular_covariance_yields_explicit_error() {
    let s = schema(2);
    let f = agenomic_fingerprint::Fingerprint {
        schema_id: "s".into(),
        schema_version: 1,
        agent_id: "a".into(),
        computed_at: chrono::Utc::now(),
        probes_count: 1,
        runs_per_probe: 2,
        mean: vec![0.0, 0.0],
        variance: vec![0.0, 0.0],
        covariance: vec![0.0; 4],
        content_hash: [0u8; 32],
    };
    let _ = s;
    let mut g = f.clone();
    g.mean = vec![0.1, 0.1];
    match mahalanobis_distance(&f, &g) {
        Err(FingerprintError::SingularCovariance) => {}
        other => panic!("expected SingularCovariance, got {other:?}"),
    }
}
