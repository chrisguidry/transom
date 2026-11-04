//! The issuer's Ed25519 signing keys, each made from a fixed seed so a
//! test can name the key it wants.

use aws_lc_rs::signature::{Ed25519KeyPair, KeyPair};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde_json::{Value, json};

/// The DER prefix that wraps a 32-byte Ed25519 seed as a PKCS #8 key.
const PKCS8_ED25519_PREFIX: [u8; 16] = [
    0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
];

pub(crate) struct Key {
    kid: String,
    seed: [u8; 32],
}

impl Key {
    pub(crate) fn new(kid: &str, seed: u8) -> Self {
        Self {
            kid: kid.to_owned(),
            seed: [seed; 32],
        }
    }

    pub(crate) fn jwk(&self) -> Value {
        let pair = Ed25519KeyPair::from_seed_unchecked(&self.seed).unwrap();
        json!({
            "kty": "OKP",
            "crv": "Ed25519",
            "alg": "EdDSA",
            "use": "sig",
            "kid": self.kid,
            "x": URL_SAFE_NO_PAD.encode(pair.public_key()),
        })
    }

    fn encoding_key(&self) -> EncodingKey {
        let mut der = PKCS8_ED25519_PREFIX.to_vec();
        der.extend_from_slice(&self.seed);
        EncodingKey::from_ed_der(&der)
    }

    /// `claims` signed with this key, with its kid in the header.
    pub(crate) fn sign(&self, claims: &Value) -> String {
        let mut header = Header::new(Algorithm::EdDSA);
        header.kid = Some(self.kid.clone());
        self.sign_with(&header, claims)
    }

    pub(crate) fn sign_with(&self, header: &Header, claims: &Value) -> String {
        jsonwebtoken::encode(header, claims, &self.encoding_key()).unwrap()
    }
}
