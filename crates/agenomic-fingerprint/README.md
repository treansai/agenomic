# agenomic-fingerprint

Behavioral fingerprints for Agenomic: empirical estimation, Mahalanobis
identity tests, sample-size calibration, and CUSUM drift detection.

A `Fingerprint` is the empirical mean and covariance of an agent's metric
vector across `KN` replays of a canonical probe set. Two agents whose
fingerprints fail to be rejected by a χ² test are operationally the same
agent. Fingerprints are content-hashed (BLAKE3 over canonical CBOR) and can
be signed with ed25519.

## End-to-end example

```rust
use agenomic_fingerprint::{
    estimate_fingerprint, same_agent_test, CusumState, FingerprintSchema,
    MetricId, MetricKind, MetricSpec, RunResult, DEFAULT_ALPHA,
};

let schema = FingerprintSchema {
    schema_id: "claims-v1".into(),
    version: 1,
    metrics: vec![MetricSpec {
        id: MetricId::new("accuracy"),
        kind: MetricKind::Bounded01,
        higher_is_better: true,
        description: "Top-1 accuracy".into(),
    }],
};

// 1. Estimate a fingerprint from KN run results.
let runs: Vec<RunResult> = collect_runs(&schema);
let fp_a = estimate_fingerprint(&schema, "agent-A", &runs).unwrap();

// 2. Compare two fingerprints with a χ² same-agent test.
let fp_b = estimate_fingerprint(&schema, "agent-B", &collect_runs(&schema)).unwrap();
let res = same_agent_test(&fp_a, &fp_b, DEFAULT_ALPHA).unwrap();
println!("same agent? {} (p={:.4})", res.passes, res.p_value);

// 3. Watch for behavioral drift in production via CUSUM.
let mut detector = CusumState::new(
    MetricId::new("accuracy"),
    fp_a.mean[0], fp_a.variance[0].sqrt().max(1e-6),
    /* delta = */ 0.05,
    /* target_arl = */ 500.0,
).unwrap();
for x in observed_stream() {
    if let Some(alert) = detector.update(x).unwrap() {
        eprintln!("drift: {alert:?}");
    }
}
# fn collect_runs(_: &FingerprintSchema) -> Vec<RunResult> { vec![
#     RunResult { probe_id: "p".into(), metric_values: vec![0.85] },
#     RunResult { probe_id: "p".into(), metric_values: vec![0.87] },
# ] }
# fn observed_stream() -> Vec<f64> { vec![] }
```

## Default thresholds

| Setting | Symbol | Default | Source |
|---|---|---|---|
| Same-agent significance | α | 1e-3 | `DEFAULT_ALPHA` |
| Hoeffding precision | ε | caller-supplied | `hoeffding_sample_size` |
| Hoeffding confidence | δ | caller-supplied | `hoeffding_sample_size` |
| Conservative `KN` for unbounded metrics | — | 1000 | `CONSERVATIVE_UNBOUNDED_KN` |
| CUSUM reference | k | δ/2 | `CusumState::new` |
| CUSUM threshold | h | inverted from target ARL | Siegmund approximation |

The Siegmund approximation in `CusumState::new` is a leading-order
asymptotic and may underestimate ARL by a meaningful factor at moderate
thresholds; for production deployments calibrate `h` via Monte Carlo on the
target metric.
