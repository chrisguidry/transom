//! HTTP Message Signatures (RFC 9421) with Ed25519, and the content digest
//! (RFC 9530) a signature covers so it covers the body too.

use std::fmt::Write;

use aws_lc_rs::digest::{SHA256, digest};
use aws_lc_rs::signature::{Ed25519KeyPair, KeyPair};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use serde_json::{Value, json};

/// The value of a `Content-Digest` header for `body`.
pub(crate) fn content_digest(body: &[u8]) -> String {
    format!("sha-256=:{}:", STANDARD.encode(digest(&SHA256, body)))
}

/// The signature parameters: the covered components in order, then the
/// parameters, as RFC 9421 section 2.3 writes them.
pub(crate) fn parameters(components: &[&str], parameters: &[(&str, Parameter)]) -> String {
    let names: Vec<String> = components
        .iter()
        .map(|name| format!("\"{name}\""))
        .collect();
    let mut value = format!("({})", names.join(" "));
    for (name, parameter) in parameters {
        match parameter {
            Parameter::Integer(number) => write!(value, ";{name}={number}"),
            Parameter::String(text) => write!(value, ";{name}=\"{text}\""),
        }
        .expect("writing to a String cannot fail");
    }
    value
}

pub(crate) enum Parameter {
    Integer(i64),
    String(String),
}

/// The signature base of RFC 9421 section 2.5: each covered component's
/// name and value on a line, then the signature parameters.
pub(crate) fn base(components: &[(&str, &str)], parameters: &str) -> String {
    let mut base = String::new();
    for (name, value) in components {
        writeln!(base, "\"{name}\": {value}").expect("writing to a String cannot fail");
    }
    write!(base, "\"@signature-params\": {parameters}").expect("writing to a String cannot fail");
    base
}

/// An Ed25519 key that signs, and the JWK of its public half.
pub(crate) struct Signer {
    pair: Ed25519KeyPair,
    pub(crate) kid: String,
}

impl Signer {
    pub(crate) fn from_pkcs8(pkcs8: &[u8]) -> Result<Self, String> {
        let pair = Ed25519KeyPair::from_pkcs8_maybe_unchecked(pkcs8)
            .map_err(|error| format!("the signing key is not an Ed25519 key: {error}"))?;
        let kid = thumbprint(pair.public_key().as_ref());
        Ok(Self { pair, kid })
    }

    /// A new key, as PKCS #8.
    pub(crate) fn generate() -> Vec<u8> {
        let rng = aws_lc_rs::rand::SystemRandom::new();
        Ed25519KeyPair::generate_pkcs8(&rng)
            .expect("the system can generate a key")
            .as_ref()
            .to_vec()
    }

    pub(crate) fn sign(&self, base: &str) -> String {
        STANDARD.encode(self.pair.sign(base.as_bytes()).as_ref())
    }

    /// The public key as a JWK, named by its thumbprint.
    pub(crate) fn jwk(&self) -> Value {
        json!({
            "kty": "OKP",
            "crv": "Ed25519",
            "x": URL_SAFE_NO_PAD.encode(self.pair.public_key().as_ref()),
            "kid": self.kid,
            "alg": "EdDSA",
            "use": "sig",
        })
    }
}

/// The JWK thumbprint of an Ed25519 public key (RFC 7638): the SHA-256 of
/// its required members, in order, with no spaces.
fn thumbprint(public_key: &[u8]) -> String {
    let members = format!(
        "{{\"crv\":\"Ed25519\",\"kty\":\"OKP\",\"x\":\"{}\"}}",
        URL_SAFE_NO_PAD.encode(public_key)
    );
    URL_SAFE_NO_PAD.encode(digest(&SHA256, members.as_bytes()))
}

#[cfg(test)]
mod tests {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;

    use super::{Parameter, Signer, base, content_digest, parameters};

    /// RFC 9421's test-key-ed25519, from its appendix B.1.4.
    const TEST_KEY: &str = "MC4CAQAwBQYDK2VwBCIEIJ+DYvh6SEqVTm50DFtMDoQikTmiCqirVv9mWG9qfSnF";

    #[test]
    fn signs_rfc_9421s_ed25519_example_exactly() {
        let signer = Signer::from_pkcs8(&STANDARD.decode(TEST_KEY).unwrap()).unwrap();
        let names = [
            "date",
            "@method",
            "@path",
            "@authority",
            "content-type",
            "content-length",
        ];
        let values = [
            "Tue, 20 Apr 2021 02:07:55 GMT",
            "POST",
            "/foo",
            "example.com",
            "application/json",
            "18",
        ];
        let parameters = parameters(
            &names,
            &[
                ("created", Parameter::Integer(1_618_884_473)),
                ("keyid", Parameter::String("test-key-ed25519".to_owned())),
            ],
        );
        let components: Vec<(&str, &str)> = names.into_iter().zip(values).collect();
        let base = base(&components, &parameters);
        assert_eq!(
            signer.sign(&base),
            "wqcAqbmYJ2ji2glfAMaRy4gruYYnx2nEFN2HN6jrnDnQCK1u02Gb04v9EDgwUPiu4A0w6vuQv5lIp5WPpBKRCw=="
        );
    }

    #[test]
    fn names_its_key_by_its_thumbprint() {
        let signer = Signer::from_pkcs8(&STANDARD.decode(TEST_KEY).unwrap()).unwrap();
        let jwk = signer.jwk();
        assert_eq!(jwk["x"], "JrQLj5P_89iXES9-vFgrIy29clF9CC_oPPsw3c5D0bs");
        assert_eq!(jwk["kid"], signer.kid);
        assert_eq!(signer.kid.len(), 43);
    }

    #[test]
    fn generates_keys_it_can_use() {
        assert!(Signer::from_pkcs8(&Signer::generate()).is_ok());
        assert!(Signer::from_pkcs8(b"not a key").is_err());
    }

    #[test]
    fn digests_the_body() {
        assert_eq!(
            content_digest(b"{\"hello\": \"world\"}\n"),
            "sha-256=:RK/0qy18MlBSVnWgjwz6lZEWjP/lF5HF9bvEF8FabDg=:"
        );
    }
}
