use agenomic_fingerprint::hoeffding_sample_size;

#[test]
fn classical_target_yields_at_least_6620() {
    let kn = hoeffding_sample_size(0.02, 0.01);
    assert!(kn >= 6620, "kn={kn}");
    assert!(kn <= 6625, "kn={kn} should be tight to spec");
}

#[test]
fn relaxed_target_yields_at_least_738() {
    let kn = hoeffding_sample_size(0.05, 0.05);
    assert!(kn >= 738, "kn={kn}");
    assert!(kn <= 745, "kn={kn} should be tight");
}

#[test]
fn smaller_epsilon_strictly_increases_sample_size() {
    let mut prev = 0usize;
    for eps in [0.10, 0.05, 0.02, 0.01, 0.005] {
        let kn = hoeffding_sample_size(eps, 0.01);
        assert!(kn > prev, "kn={kn} not greater than prev={prev} at eps={eps}");
        prev = kn;
    }
}
