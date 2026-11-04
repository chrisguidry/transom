//! VAPID (RFC 8292): transom signs a short-lived JWT for each push
//! service it posts to, so the push service knows a message comes from
//! the server the browser subscribed with. The browser learns the public
//! key when it subscribes.

use aws_lc_rs::rand::SystemRandom;
use aws_lc_rs::signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair, KeyPair};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::json;

/// How long a JWT is good for. RFC 8292 allows at most a day.
const LIFETIME: i64 = 12 * 60 * 60;

/// A P-256 key that signs VAPID JWTs.
pub(crate) struct Vapid {
    pair: EcdsaKeyPair,
}

impl Vapid {
    /// A new key, as PKCS#8, to keep.
    pub(crate) fn generate() -> Vec<u8> {
        EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &SystemRandom::new())
            .expect("a P-256 key can be made")
            .as_ref()
            .to_vec()
    }

    pub(crate) fn from_pkcs8(pkcs8: &[u8]) -> Result<Self, String> {
        EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, pkcs8)
            .map(|pair| Self { pair })
            .map_err(|error| format!("the push key is not a P-256 key: {error}"))
    }

    /// The public key, uncompressed and in base64url, as a browser's
    /// `applicationServerKey` and the `k` parameter take it.
    pub(crate) fn public_key(&self) -> String {
        URL_SAFE_NO_PAD.encode(self.pair.public_key().as_ref())
    }

    /// The `Authorization` header for a POST to `endpoint`, at `now` in
    /// seconds, naming `subject`, a URL to contact transom's operator by.
    pub(crate) fn authorization(&self, endpoint: &str, subject: &str, now: i64) -> String {
        let audience = url::Url::parse(endpoint)
            .map(|url| url.origin().ascii_serialization())
            .unwrap_or_default();
        let header = URL_SAFE_NO_PAD.encode(json!({ "typ": "JWT", "alg": "ES256" }).to_string());
        let claims = json!({ "aud": audience, "exp": now + LIFETIME, "sub": subject });
        let input = format!("{header}.{}", URL_SAFE_NO_PAD.encode(claims.to_string()));
        let signature = self
            .pair
            .sign(&SystemRandom::new(), input.as_bytes())
            .expect("a P-256 key signs");
        let token = format!("{input}.{}", URL_SAFE_NO_PAD.encode(signature.as_ref()));
        format!("vapid t={token}, k={}", self.public_key())
    }
}
