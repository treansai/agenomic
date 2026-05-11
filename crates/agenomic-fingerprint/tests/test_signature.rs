use agenomic_fingerprint::{
    sign_fingerprint, verify_signed_fingerprint, Fingerprint,
};
use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;

fn fp() -> Fingerprint {
    Fingerprint {
        schema_id: "demo".into(),
        schema_version: 1,
        agent_id: "agent-A".into(),
        computed_at: chrono::Utc::now(),
        probes_count: 5,
        runs_per_probe: 100,
        mean: vec![0.85, 110.0, 0.05],
        variance: vec![0.0025, 144.0, 1.0e-5],
        covariance: vec![
            0.0025, 0.0, 0.0, 0.0, 144.0, 0.0, 0.0, 0.0, 1.0e-5,
        ],
        content_hash: [0u8; 32],
    }
}

#[test]
fn sign_then_verify_succeeds() {
    let mut csprng = OsRng;
    let sk = SigningKey::generate(&mut csprng);
    let vk = sk.verifying_key();
    let signed = sign_fingerprint(fp(), &sk, "key-1".into()).unwrap();
    verify_signed_fingerprint(&signed, &vk).expect("verification must succeed");
}

#[test]
fn tampering_with_mean_invalidates_signature() {
    let mut csprng = OsRng;
    let sk = SigningKey::generate(&mut csprng);
    let vk = sk.verifying_key();
    let mut signed = sign_fingerprint(fp(), &sk, "key-1".into()).unwrap();
    signed.fingerprint.mean[0] += 1e-9;
    assert!(verify_signed_fingerprint(&signed, &vk).is_err());
}

#[test]
fn tampering_with_signer_key_id_invalidates_signature() {
    let mut csprng = OsRng;
    let sk = SigningKey::generate(&mut csprng);
    let vk = sk.verifying_key();
    let mut signed = sign_fingerprint(fp(), &sk, "key-1".into()).unwrap();
    signed.signer_key_id = "key-2".into();
    assert!(verify_signed_fingerprint(&signed, &vk).is_err());
}

#[test]
fn content_hash_is_reproducible() {
    let f = {
        let mut x = fp();
        x.content_hash = x.compute_content_hash().unwrap();
        x
    };
    let h1 = f.compute_content_hash().unwrap();
    let h2 = f.compute_content_hash().unwrap();
    assert_eq!(h1, h2);
    assert_eq!(h1, f.content_hash);
}

#[test]
fn flipping_one_bit_of_mean_changes_hash() {
    let mut a = fp();
    a.content_hash = a.compute_content_hash().unwrap();
    let mut b = a.clone();
    b.mean[0] = f64::from_bits(b.mean[0].to_bits() ^ 1);
    let hb = b.compute_content_hash().unwrap();
    assert_ne!(a.content_hash, hb);
}
