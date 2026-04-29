//! [`Fingerprint`] type, content hashing, and ed25519 signature wrapper.

use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

use crate::error::FingerprintError;

/// Domain separator prepended to the canonical CBOR encoding before hashing.
/// Bumped if the hash construction changes in a way that would otherwise
/// silently collide across versions.
pub const HASH_DOMAIN: &[u8] = b"AGENTLOCK-FP-v1\x00";

/// Empirical behavioral fingerprint of an agent.
///
/// Immutable by convention: callers should construct a fingerprint via
/// [`crate::estimate_fingerprint`] or by deserializing a previously signed
/// instance, then treat it as read-only.
///
/// Vectors are laid out in the order of the schema's metrics. The
/// `covariance` field stores the full `n × n` matrix in row-major order.
///
/// # Examples
/// ```
/// use agentlock_fingerprint::Fingerprint;
/// // Fingerprints are typically produced by `estimate_fingerprint`.
/// let fp = Fingerprint {
///     schema_id: "demo-v1".into(),
///     schema_version: 1,
///     agent_id: "agent-A".into(),
///     computed_at: chrono::Utc::now(),
///     probes_count: 10,
///     runs_per_probe: 100,
///     mean: vec![0.5],
///     variance: vec![0.01],
///     covariance: vec![0.01],
///     content_hash: [0u8; 32],
/// };
/// assert_eq!(fp.dimension(), 1);
/// ```
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Fingerprint {
    /// Schema identifier this fingerprint conforms to.
    pub schema_id: String,
    /// Schema version.
    pub schema_version: u32,
    /// Stable identifier of the agent.
    pub agent_id: String,
    /// UTC timestamp of computation.
    pub computed_at: DateTime<Utc>,
    /// Number of distinct probes from the canonical set.
    pub probes_count: usize,
    /// Number of replays per probe.
    pub runs_per_probe: usize,
    /// Per-metric empirical mean.
    pub mean: Vec<f64>,
    /// Per-metric empirical (unbiased) variance.
    pub variance: Vec<f64>,
    /// Full `n × n` empirical covariance, row-major.
    pub covariance: Vec<f64>,
    /// BLAKE3 hash of the canonical CBOR encoding of this fingerprint
    /// (with `content_hash` zeroed out at hashing time).
    pub content_hash: [u8; 32],
}

impl Fingerprint {
    /// Returns the dimension `n` of the fingerprint, i.e. the number of
    /// metrics.
    pub fn dimension(&self) -> usize {
        self.mean.len()
    }

    /// Computes the canonical content hash of this fingerprint.
    ///
    /// The hash is BLAKE3 of `HASH_DOMAIN || canonical_cbor(self_without_hash)`.
    /// The currently stored `content_hash` is excluded from the hashed
    /// payload so that re-hashing after assignment yields the same value.
    ///
    /// # Examples
    /// ```
    /// use agentlock_fingerprint::Fingerprint;
    /// let mut fp = Fingerprint {
    ///     schema_id: "s".into(), schema_version: 1, agent_id: "a".into(),
    ///     computed_at: chrono::Utc::now(),
    ///     probes_count: 1, runs_per_probe: 2,
    ///     mean: vec![0.0], variance: vec![0.0], covariance: vec![0.0],
    ///     content_hash: [0u8; 32],
    /// };
    /// let h1 = fp.compute_content_hash().unwrap();
    /// fp.content_hash = h1;
    /// let h2 = fp.compute_content_hash().unwrap();
    /// assert_eq!(h1, h2);
    /// ```
    pub fn compute_content_hash(&self) -> Result<[u8; 32], FingerprintError> {
        let view = FingerprintForHashing {
            schema_id: &self.schema_id,
            schema_version: self.schema_version,
            agent_id: &self.agent_id,
            computed_at: self.computed_at,
            probes_count: self.probes_count,
            runs_per_probe: self.runs_per_probe,
            mean: &self.mean,
            variance: &self.variance,
            covariance: &self.covariance,
        };
        let mut buf = Vec::with_capacity(256);
        ciborium::ser::into_writer(&view, &mut buf)
            .map_err(|e| FingerprintError::Cbor(e.to_string()))?;
        let mut hasher = blake3::Hasher::new();
        hasher.update(HASH_DOMAIN);
        hasher.update(&buf);
        Ok(*hasher.finalize().as_bytes())
    }
}

/// Internal projection of [`Fingerprint`] used solely for canonical hashing.
/// It mirrors the public layout but omits `content_hash`.
#[derive(Serialize)]
struct FingerprintForHashing<'a> {
    schema_id: &'a str,
    schema_version: u32,
    agent_id: &'a str,
    computed_at: DateTime<Utc>,
    probes_count: usize,
    runs_per_probe: usize,
    mean: &'a [f64],
    variance: &'a [f64],
    covariance: &'a [f64],
}

/// A [`Fingerprint`] together with an ed25519 signature on its content hash.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SignedFingerprint {
    /// The fingerprint payload.
    pub fingerprint: Fingerprint,
    /// Caller-supplied identifier for the public key (e.g. a key fingerprint
    /// or DID). Included in the signed payload to bind the signature to the
    /// declared signer.
    pub signer_key_id: String,
    /// Raw ed25519 signature, 64 bytes.
    #[serde(with = "BigArray")]
    pub signature: [u8; 64],
}

/// Bytes actually fed to the signer / verifier:
/// `HASH_DOMAIN || content_hash || signer_key_id`.
fn signing_message(content_hash: &[u8; 32], signer_key_id: &str) -> Vec<u8> {
    let kid = signer_key_id.as_bytes();
    let mut msg = Vec::with_capacity(HASH_DOMAIN.len() + 32 + kid.len());
    msg.extend_from_slice(HASH_DOMAIN);
    msg.extend_from_slice(content_hash);
    msg.extend_from_slice(kid);
    msg
}

/// Signs a fingerprint with a caller-provided ed25519 key.
///
/// The `content_hash` of the supplied fingerprint is recomputed prior to
/// signing so that callers cannot accidentally sign a stale hash.
///
/// # Examples
/// ```
/// use agentlock_fingerprint::{sign_fingerprint, verify_signed_fingerprint, Fingerprint};
/// use ed25519_dalek::SigningKey;
/// use rand::rngs::OsRng;
/// let mut csprng = OsRng;
/// let sk = SigningKey::generate(&mut csprng);
/// let vk = sk.verifying_key();
/// let fp = Fingerprint {
///     schema_id: "s".into(), schema_version: 1, agent_id: "a".into(),
///     computed_at: chrono::Utc::now(),
///     probes_count: 1, runs_per_probe: 2,
///     mean: vec![0.0], variance: vec![0.0], covariance: vec![0.0],
///     content_hash: [0u8; 32],
/// };
/// let signed = sign_fingerprint(fp, &sk, "demo-key".into()).unwrap();
/// verify_signed_fingerprint(&signed, &vk).unwrap();
/// ```
pub fn sign_fingerprint(
    mut fp: Fingerprint,
    signing_key: &SigningKey,
    key_id: String,
) -> Result<SignedFingerprint, FingerprintError> {
    fp.content_hash = fp.compute_content_hash()?;
    let msg = signing_message(&fp.content_hash, &key_id);
    let sig: Signature = signing_key.sign(&msg);
    Ok(SignedFingerprint {
        fingerprint: fp,
        signer_key_id: key_id,
        signature: sig.to_bytes(),
    })
}

/// Verifies a [`SignedFingerprint`] against a verifying key.
///
/// Returns `Ok(())` on success, or a [`FingerprintError::Signature`] on
/// mismatch (including tampered fingerprints, since the recomputed
/// `content_hash` then disagrees with the signed payload).
pub fn verify_signed_fingerprint(
    sfp: &SignedFingerprint,
    verifying_key: &VerifyingKey,
) -> Result<(), FingerprintError> {
    let recomputed = sfp.fingerprint.compute_content_hash()?;
    if recomputed != sfp.fingerprint.content_hash {
        return Err(FingerprintError::Signature(
            "content_hash does not match payload".into(),
        ));
    }
    let msg = signing_message(&recomputed, &sfp.signer_key_id);
    let sig = Signature::from_bytes(&sfp.signature);
    verifying_key
        .verify(&msg, &sig)
        .map_err(|e| FingerprintError::Signature(e.to_string()))
}
