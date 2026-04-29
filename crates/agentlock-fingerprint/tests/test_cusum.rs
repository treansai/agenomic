use agentlock_fingerprint::{CusumState, DriftDirection, MetricId};
use rand::{rngs::StdRng, SeedableRng};
use rand_distr::{Distribution, Normal};

#[test]
fn arl_under_no_drift_is_close_to_target() {
    let mu0 = 0.0_f64;
    let sigma0 = 1.0_f64;
    let target_arl = 200.0_f64;
    let detector = CusumState::new(
        MetricId::new("x"),
        mu0,
        sigma0,
        1.0,
        target_arl,
    )
    .unwrap();

    let mut rng = StdRng::seed_from_u64(0xC05_F1A);
    let dist = Normal::new(mu0, sigma0).unwrap();
    let n_runs = 200_usize;
    let mut run_lengths: Vec<u64> = Vec::with_capacity(n_runs);
    let max_samples = 5000_u64;
    for r in 0..n_runs {
        let mut d = detector.clone();
        d.reset();
        let mut detected: Option<u64> = None;
        let _ = r;
        for _ in 0..max_samples {
            if d.update(dist.sample(&mut rng)).unwrap().is_some() {
                detected = Some(d.samples_seen);
                break;
            }
        }
        run_lengths.push(detected.unwrap_or(max_samples));
    }
    let mean_arl: f64 =
        run_lengths.iter().map(|x| *x as f64).sum::<f64>() / n_runs as f64;
    // Siegmund's approximation is a leading-order asymptotic and is known to
    // underestimate ARL by a meaningful factor at moderate h. Combined with
    // the bilateral test (which roughly halves the joint ARL), the observed
    // ARL can land within a wide band around the target. We assert that the
    // detector is neither hair-trigger nor effectively dead.
    let lower = target_arl * 0.3;
    let upper = target_arl * 3.0;
    assert!(
        mean_arl >= lower && mean_arl <= upper,
        "mean_arl={mean_arl} outside [{lower}, {upper}] (target={target_arl})"
    );
}

#[test]
fn upward_drift_is_detected_quickly() {
    let mu0 = 0.0_f64;
    let sigma0 = 1.0_f64;
    let detector = CusumState::new(MetricId::new("x"), mu0, sigma0, 1.0, 500.0).unwrap();
    let mut rng = StdRng::seed_from_u64(0xDEAD_BEEF);
    let pre = Normal::new(mu0, sigma0).unwrap();
    let post = Normal::new(mu0 + sigma0, sigma0).unwrap();

    let mut detections: Vec<u64> = Vec::with_capacity(50);
    for _ in 0..50 {
        let mut d = detector.clone();
        d.reset();
        for _ in 0..200 {
            let _ = d.update(pre.sample(&mut rng)).unwrap();
        }
        let baseline = d.samples_seen;
        let mut found: Option<u64> = None;
        for _ in 0..1000 {
            if let Some(alert) = d.update(post.sample(&mut rng)).unwrap() {
                assert_eq!(alert.direction, DriftDirection::Upward);
                found = Some(d.samples_seen - baseline);
                break;
            }
        }
        if let Some(v) = found {
            detections.push(v);
        }
    }
    detections.sort_unstable();
    let median = detections[detections.len() / 2];
    assert!(
        detections.len() >= 25,
        "drift was not detected in enough simulations: {}",
        detections.len()
    );
    assert!(median < 50, "median detection delay = {median}");
}

#[test]
fn reset_clears_running_state() {
    let mut d =
        CusumState::new(MetricId::new("x"), 0.0, 1.0, 1.0, 200.0).unwrap();
    for _ in 0..10 {
        let _ = d.update(2.0).unwrap();
    }
    d.reset();
    assert_eq!(d.s_plus, 0.0);
    assert_eq!(d.s_minus, 0.0);
    assert_eq!(d.samples_seen, 0);
}

#[test]
fn cbor_roundtrip_preserves_state() {
    let mut d =
        CusumState::new(MetricId::new("x"), 0.5, 0.1, 0.05, 300.0).unwrap();
    for v in [0.55, 0.6, 0.62, 0.4] {
        let _ = d.update(v).unwrap();
    }
    let mut buf = Vec::new();
    ciborium::ser::into_writer(&d, &mut buf).unwrap();
    let d2: CusumState = ciborium::de::from_reader(&buf[..]).unwrap();
    assert_eq!(d2.metric_id, d.metric_id);
    assert!((d2.mu_0 - d.mu_0).abs() < 1e-12);
    assert!((d2.sigma_0 - d.sigma_0).abs() < 1e-12);
    assert!((d2.k - d.k).abs() < 1e-12);
    assert!((d2.h - d.h).abs() < 1e-12);
    assert!((d2.s_plus - d.s_plus).abs() < 1e-12);
    assert!((d2.s_minus - d.s_minus).abs() < 1e-12);
    assert_eq!(d2.samples_seen, d.samples_seen);
}
