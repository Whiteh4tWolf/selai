//! RSA authentication and generation use OpenSSL, never RustCrypto's RSA private operations
//! (RUSTSEC-2023-0071). russh still handles key decoding and public verification.

use openssl::{
    bn::{BigNum, BigNumContext},
    hash::MessageDigest,
    pkey::{PKey, Private},
    rsa::{Padding, Rsa, RsaPrivateKeyBuilder},
    sign::Signer,
};
use russh::keys::{agent::AgentIdentity, Algorithm, HashAlg, PrivateKey, PublicKey};

use crate::SshError;

/// Generate supported RSA sizes using OpenSSL's RNG and private operations.
/// The temporary PEM is wiped on drop and never written to a file.
pub fn generate_rsa_key(bits: u32) -> Result<PrivateKey, SshError> {
    if ![2048, 3072, 4096, 8192].contains(&bits) {
        return Err(SshError::KeyLoad(
            "Choose RSA 2048, 3072, 4096, or 8192 bits.".into(),
        ));
    }
    let rsa =
        Rsa::generate(bits).map_err(|_| SshError::KeyLoad("RSA key generation failed.".into()))?;
    let pem = selai_secrets::SecretValue::new(
        rsa.private_key_to_pem()
            .map_err(|_| SshError::KeyLoad("RSA key encoding failed.".into()))?,
    );
    let text = std::str::from_utf8(pem.expose_for_transport())
        .map_err(|_| SshError::KeyLoad("RSA key encoding failed.".into()))?;
    russh::keys::decode_secret_key(text, None)
        .map_err(|_| SshError::KeyLoad("Generated RSA key could not be loaded.".into()))
}

/// Enforce the same key policy at import and connection time.
pub fn validate_private_key(key: &PrivateKey) -> Result<(), SshError> {
    match key.algorithm() {
        Algorithm::Rsa { .. } => RsaSigner::new(key).map(|_| ()),
        Algorithm::Dsa => Err(SshError::KeyLoad(
            "DSA keys are not supported. Import RSA (2048-8192 bits), Ed25519, or ECDSA.".into(),
        )),
        Algorithm::Ed25519 | Algorithm::Ecdsa { .. } => Ok(()),
        _ => Err(SshError::KeyLoad(
            "Unsupported private key algorithm.".into(),
        )),
    }
}

pub(crate) struct RsaSigner {
    key: PKey<Private>,
    public: PublicKey,
}

impl RsaSigner {
    pub(crate) fn new(key: &PrivateKey) -> Result<Self, SshError> {
        let rsa = key
            .key_data()
            .rsa()
            .ok_or_else(|| SshError::KeyLoad("Expected an RSA private key.".into()))?;
        if !(2048..=8192).contains(&rsa.key_size()) {
            return Err(SshError::KeyLoad("RSA keys must be 2048-8192 bits.".into()));
        }
        let convert = || -> Result<PKey<Private>, openssl::error::ErrorStack> {
            let n = BigNum::from_slice(rsa.public().n().as_bytes())?;
            let e = BigNum::from_slice(rsa.public().e().as_bytes())?;
            let d = private_bn(rsa.private().d().as_bytes())?;
            let p = private_bn(rsa.private().p().as_bytes())?;
            let q = private_bn(rsa.private().q().as_bytes())?;
            let iqmp = private_bn(rsa.private().iqmp().as_bytes())?;
            // OpenSSH stores d, p, q and q^-1 mod p. Compute the remaining CRT
            // parameters inside OpenSSL for efficient, blinded signing.
            let mut p_minus_one = private_bn(rsa.private().p().as_bytes())?;
            p_minus_one.sub_word(1)?;
            let mut q_minus_one = private_bn(rsa.private().q().as_bytes())?;
            q_minus_one.sub_word(1)?;
            let mut ctx = BigNumContext::new_secure()?;
            let mut dmp1 = BigNum::new_secure()?;
            dmp1.nnmod(&d, &p_minus_one, &mut ctx)?;
            let mut dmq1 = BigNum::new_secure()?;
            dmq1.nnmod(&d, &q_minus_one, &mut ctx)?;
            let native = RsaPrivateKeyBuilder::new(n, e, d)?
                .set_factors(p, q)?
                .set_crt_params(dmp1, dmq1, iqmp)?
                .build();
            if !native.check_key()? {
                return Err(openssl::error::ErrorStack::get());
            }
            PKey::from_rsa(native)
        };
        let native = convert().map_err(|_| {
            SshError::KeyLoad("RSA private key has invalid or inconsistent components.".into())
        })?;
        Ok(Self {
            key: native,
            public: key.public_key().clone(),
        })
    }
}

fn private_bn(bytes: &[u8]) -> Result<BigNum, openssl::error::ErrorStack> {
    let mut value = BigNum::new_secure()?;
    value.copy_from_slice(bytes)?;
    value.set_const_time();
    Ok(value)
}

impl russh::Signer for RsaSigner {
    type Error = SshError;

    async fn auth_sign(
        &mut self,
        key: &AgentIdentity,
        hash_alg: Option<HashAlg>,
        mut to_sign: Vec<u8>,
    ) -> Result<Vec<u8>, Self::Error> {
        if key.public_key().key_data() != self.public.key_data() {
            return Err(SshError::KeyLoad(
                "RSA signing identity does not match the key.".into(),
            ));
        }
        let (digest, name) = match hash_alg {
            Some(HashAlg::Sha512) => (MessageDigest::sha512(), "rsa-sha2-512"),
            Some(HashAlg::Sha256) => (MessageDigest::sha256(), "rsa-sha2-256"),
            _ => return Err(SshError::KeyLoad(
                "RSA authentication requires SHA-256 or SHA-512; legacy ssh-rsa/SHA-1 is disabled."
                    .into(),
            )),
        };
        let sign = || -> Result<Vec<u8>, openssl::error::ErrorStack> {
            let mut signer = Signer::new(digest, &self.key)?;
            signer.set_rsa_padding(Padding::PKCS1)?;
            signer.update(&to_sign)?;
            signer.sign_to_vec()
        };
        let signature = sign().map_err(|_| SshError::KeyLoad("RSA signing failed.".into()))?;
        // SSH signature: string(string algorithm || string signature). russh
        // expects it appended to the original userauth signing payload.
        let size = 8 + name.len() + signature.len();
        to_sign.extend_from_slice(&(size as u32).to_be_bytes());
        to_sign.extend_from_slice(&(name.len() as u32).to_be_bytes());
        to_sign.extend_from_slice(name.as_bytes());
        to_sign.extend_from_slice(&(signature.len() as u32).to_be_bytes());
        to_sign.extend_from_slice(&signature);
        Ok(to_sign)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use openssl::{rsa::Rsa, sign::Verifier};
    use russh::{keys::ssh_key::Signature, Signer as _};
    use std::sync::OnceLock;

    fn key() -> &'static PrivateKey {
        static KEY: OnceLock<PrivateKey> = OnceLock::new();
        KEY.get_or_init(|| generate_rsa_key(2048).unwrap())
    }

    #[test]
    fn unsupported_generation_sizes_are_rejected() {
        for bits in [1024, 2050, 16384] {
            assert!(generate_rsa_key(bits).is_err());
        }
    }

    fn rsa_key(bits: u32) -> PrivateKey {
        let pem = Rsa::generate(bits).unwrap().private_key_to_pem().unwrap();
        russh::keys::decode_secret_key(std::str::from_utf8(&pem).unwrap(), None).unwrap()
    }

    #[tokio::test]
    async fn rsa_signatures_use_sha2_and_correct_ssh_encoding() {
        validate_private_key(key()).unwrap();
        let mut signer = RsaSigner::new(key()).unwrap();
        let identity = key().public_key().clone().into();
        let payload = b"SSH userauth signing payload";
        for hash in [HashAlg::Sha256, HashAlg::Sha512] {
            let signed = signer
                .auth_sign(&identity, Some(hash), payload.to_vec())
                .await
                .unwrap();
            assert_eq!(&signed[..payload.len()], payload);
            let size =
                u32::from_be_bytes(signed[payload.len()..payload.len() + 4].try_into().unwrap());
            let blob = &signed[payload.len() + 4..];
            assert_eq!(size as usize, blob.len());
            let signature = Signature::try_from(blob).expect("SSH signature encoding");
            assert_eq!(signature.algorithm(), Algorithm::Rsa { hash: Some(hash) });
            let digest = match hash {
                HashAlg::Sha256 => MessageDigest::sha256(),
                _ => MessageDigest::sha512(),
            };
            let mut verifier = Verifier::new(digest, &signer.key).unwrap();
            verifier.update(payload).unwrap();
            assert!(verifier.verify(signature.as_ref()).unwrap());
        }
    }

    #[tokio::test]
    async fn signer_refuses_sha1_and_wrong_identity() {
        let mut signer = RsaSigner::new(key()).unwrap();
        let identity = key().public_key().clone().into();
        assert!(signer.auth_sign(&identity, None, vec![1]).await.is_err());
        let other = rsa_key(2048).public_key().clone().into();
        assert!(signer
            .auth_sign(&other, Some(HashAlg::Sha256), vec![1])
            .await
            .is_err());
    }

    #[test]
    fn weak_rsa_key_is_rejected_before_use() {
        let err = validate_private_key(&rsa_key(1024)).expect_err("weak key");
        assert!(err.to_string().contains("2048-8192"));
    }
}
