# agenomic-metrics

Pure, deterministic **agentic evaluation metrics** for Agenomic — the Phase 2
measurement layer.

Where Phase 0/1 produce *static* proofs (a canonical trace, an immutable
ledger, signed attestations, an evidence package), this crate turns them into a
*dynamic, quantitative* picture: how complete is the trace, how faithfully does
a run replay, how well does behavior adhere to policy, and how far has it
drifted.

Every function here is **pure** (no I/O, no clock, no global state),
**deterministic**, **bounded** (completeness/coverage/adherence metrics live in
`[0, 1]`), and carries an **executable doctest**. The crate is shared verbatim
by the CLI and the cloud.

## Design rules

- **Reuse, don't reinvent.** The shared statistical toolkit — Mahalanobis
  distance, the χ² same-agent test, the calibrated bilateral CUSUM detector,
  Hoeffding sample-size calibration — comes from
  [`agenomic-fingerprint`](../agenomic-fingerprint) and is re-exported from the
  [`stats`](src/stats.rs) module. The only statistic added here is the
  categorical **Jensen–Shannon divergence** used for behavioral drift, which
  the fingerprint crate does not provide.
- **Exact replay is a boolean**, never a continuous score:
  `exact_replay_match(a, b)` is hash equality.
- **Replay fidelity is `RFS = (1 − GED_norm) × CosSim`** — never
  `GED × CosSim`. This is guarded by a regression test.
- **Thresholds are extrapolations to calibrate**, not universal truths. They
  live in one place: [`Thresholds::mvp()`](src/thresholds.rs).

## Metrics

| Function | Formula | MVP threshold |
| --- | --- | --- |
| `trace_completeness` (TCS) | `|captured ∩ required| / |required|` | ≥ 0.95 |
| `exact_replay_match` | hash equality (boolean) | — |
| `replay_fidelity` (RFS) | `(1 − GED_norm) × CosSim` | ≥ 0.95 |
| `policy_adherence` (PAS) | `passed / total` (or weighted) | ≥ 0.98 (critical) |
| `behavioral_drift` (BDS) | `JS(P₁ ‖ P₂)` | alert if > 0.10 |
| `drift_bound` (ABC) | `D* = α / γ` | reference `< 0.27` |
| `tool_risk` (TRS) | `∏ factors` | review if > 0.70 |
| `memory_contamination` (MCS) | `untrusted / total` | < 0.05 |
| `prompt_mutation` (PMS) | `edit_norm × semantic_delta` | — *(calibrate)* |
| `runtime_variance` | `E[d(τ_A, τ_B)]` | — |
| `decision_explainability` (DES) | `with_provenance / total` | — |
| `audit_evidence_completeness` (AEC) | `available / required` | ≥ 0.90 (0.95 evidentiary) |
| `compliance_confidence` | `Σ(wᵢ·cᵢ) / Σ(wᵢ)` | — |
| `alignment_stability` | `1 − Var(PAS)` | — |
| `controllability` | `respected / control_signals` | — |
| `causal_coverage` | `with_parent / non_root` | — |
| `provenance_coverage` | `with_support / total_claims` | — |

## Usage

```rust
use agenomic_metrics::{metrics_from_trace_v03, TraceV03, Thresholds};

let trace = TraceV03::from_json(trace_json)?;
let report = metrics_from_trace_v03(&trace);

let thresholds = Thresholds::mvp();
if report.trace_completeness.unwrap_or(0.0) < thresholds.trace_completeness_min {
    // run is not auditable
}
# Ok::<(), agenomic_metrics::MetricsError>(())
```

`MetricsReport` is `serde`-serializable; every metric is `Option`, because not
every metric is computable in every context (a single sealed run yields
trace/policy/coverage metrics, but behavioral drift needs two windows). This is
the shape embedded as `metrics.json` in the Evidence Package (P2-4).

## Testing

```bash
cargo test -p agenomic-metrics        # unit + integration + doctests
cargo clippy -p agenomic-metrics --all-targets -- -D warnings
```

Coverage includes example-based unit tests, **proptest** invariants (bounds,
JS symmetry, RFS/TRS monotonicity) and a doctest on every public function.
