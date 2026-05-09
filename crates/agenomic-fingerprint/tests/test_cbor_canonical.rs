use agenomic_fingerprint::Fingerprint;

fn fp() -> Fingerprint {
    let mut f = Fingerprint {
        schema_id: "demo-v1".into(),
        schema_version: 1,
        agent_id: "agent-A".into(),
        computed_at: chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
        probes_count: 4,
        runs_per_probe: 50,
        mean: vec![0.5, 0.6, 0.7],
        variance: vec![0.01, 0.02, 0.005],
        covariance: vec![
            0.01, 0.001, 0.0, 0.001, 0.02, 0.0, 0.0, 0.0, 0.005,
        ],
        content_hash: [0u8; 32],
    };
    f.content_hash = f.compute_content_hash().unwrap();
    f
}

#[test]
fn cbor_roundtrip_is_byte_identical() {
    let f = fp();
    let mut buf1 = Vec::new();
    ciborium::ser::into_writer(&f, &mut buf1).unwrap();
    let f2: Fingerprint = ciborium::de::from_reader(&buf1[..]).unwrap();
    let mut buf2 = Vec::new();
    ciborium::ser::into_writer(&f2, &mut buf2).unwrap();
    assert_eq!(buf1, buf2);
}

#[test]
fn content_hash_survives_cbor_roundtrip() {
    let f = fp();
    let mut buf = Vec::new();
    ciborium::ser::into_writer(&f, &mut buf).unwrap();
    let f2: Fingerprint = ciborium::de::from_reader(&buf[..]).unwrap();
    let h2 = f2.compute_content_hash().unwrap();
    assert_eq!(f.content_hash, h2);
    assert_eq!(f.content_hash, f2.content_hash);
}
