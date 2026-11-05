//! Web Push message encryption (RFC 8291): each message is encrypted to
//! one browser's key, so the push service that carries it sees only
//! ciphertext. A message is one `aes128gcm` record (RFC 8188).

use aws_lc_rs::aead::{AES_128_GCM, Aad, LessSafeKey, Nonce, UnboundKey};
use aws_lc_rs::agreement::{self, ECDH_P256, PrivateKey, UnparsedPublicKey};
use aws_lc_rs::hmac::{HMAC_SHA256, Key, sign};
use aws_lc_rs::rand::{self, SystemRandom};

/// The record size the header declares. A message is a single record, so
/// it must fit in this many octets once encrypted.
const RECORD_SIZE: u32 = 4096;

/// The most plaintext one record holds: the record size, less the padding
/// delimiter and the AEAD tag.
pub(crate) const MOST: usize = 4096 - 1 - 16;

/// A browser's subscription keys, from `PushSubscription.getKey`: its
/// P-256 public key and its authentication secret.
pub(crate) struct Receiver<'a> {
    pub(crate) public: &'a [u8],
    pub(crate) auth: &'a [u8],
}

/// Encrypts `plaintext` to `receiver`, with a new key and salt.
pub(crate) fn encrypt(plaintext: &[u8], receiver: &Receiver) -> Result<Vec<u8>, String> {
    let sender = PrivateKey::generate(&ECDH_P256).expect("a P-256 key can be made");
    let salt = rand::generate(&SystemRandom::new())
        .expect("the system has randomness")
        .expose();
    encrypt_with(plaintext, receiver, &sender, salt)
}

/// Encrypts `plaintext` to `receiver` with the sender's key and salt
/// given, as section 3.4 of RFC 8291 describes.
pub(crate) fn encrypt_with(
    plaintext: &[u8],
    receiver: &Receiver,
    sender: &PrivateKey,
    salt: [u8; 16],
) -> Result<Vec<u8>, String> {
    if plaintext.len() > MOST {
        return Err(format!("a push message holds at most {MOST} octets"));
    }
    let sender_public = sender
        .compute_public_key()
        .expect("a P-256 key has a public half");
    let secret = ecdh(sender, receiver.public)?;
    let (key, nonce) = content_keys(&secret, receiver, sender_public.as_ref(), &salt);
    let mut record = plaintext.to_vec();
    record.push(2);
    key.seal_in_place_append_tag(nonce, Aad::empty(), &mut record)
        .expect("one record fits within AES-GCM's limits");
    let mut message = salt.to_vec();
    message.extend_from_slice(&RECORD_SIZE.to_be_bytes());
    message.push(65);
    message.extend_from_slice(sender_public.as_ref());
    message.extend_from_slice(&record);
    Ok(message)
}

/// The ECDH secret of a private key and a peer's public key.
fn ecdh(private: &PrivateKey, peer: &[u8]) -> Result<Vec<u8>, String> {
    let peer = UnparsedPublicKey::new(&ECDH_P256, peer);
    agreement::agree(
        private,
        peer,
        "the browser's key is not a P-256 public key".to_owned(),
        |secret| Ok(secret.to_vec()),
    )
}

/// The content encryption key and nonce for one message, from the ECDH
/// secret, both public keys, the receiver's authentication secret, and the
/// salt. Both ends derive the same ones.
fn content_keys(
    secret: &[u8],
    receiver: &Receiver,
    sender_public: &[u8],
    salt: &[u8],
) -> (LessSafeKey, Nonce) {
    let mut key_info = b"WebPush: info\0".to_vec();
    key_info.extend_from_slice(receiver.public);
    key_info.extend_from_slice(sender_public);
    let ikm = expand(&extract(receiver.auth, secret), &key_info);
    let prk = extract(salt, &ikm);
    let cek = expand(&prk, b"Content-Encoding: aes128gcm\0");
    let nonce = expand(&prk, b"Content-Encoding: nonce\0");
    let key = UnboundKey::new(&AES_128_GCM, &cek[..16]).expect("the key is 16 octets");
    let nonce = Nonce::try_assume_unique_for_key(&nonce[..12]).expect("the nonce is 12 octets");
    (LessSafeKey::new(key), nonce)
}

/// HKDF-Extract with SHA-256.
fn extract(salt: &[u8], ikm: &[u8]) -> Vec<u8> {
    sign(&Key::new(HMAC_SHA256, salt), ikm).as_ref().to_vec()
}

/// HKDF-Expand with SHA-256, for one block: every length RFC 8291 asks
/// for fits in the first.
fn expand(prk: &[u8], info: &[u8]) -> Vec<u8> {
    let mut input = info.to_vec();
    input.push(1);
    sign(&Key::new(HMAC_SHA256, prk), &input).as_ref().to_vec()
}

/// Decrypts a message as a browser would, with its private key, for the
/// tests to read what transom sent.
#[cfg(test)]
pub(crate) fn decrypt(message: &[u8], private: &PrivateKey, auth: &[u8]) -> Vec<u8> {
    let (salt, rest) = message.split_at(16);
    let sender_public = &rest[5..5 + usize::from(rest[4])];
    let mut record = rest[5 + sender_public.len()..].to_vec();
    let public = private.compute_public_key().unwrap();
    let receiver = Receiver {
        public: public.as_ref(),
        auth,
    };
    let secret = ecdh(private, sender_public).unwrap();
    let (key, nonce) = content_keys(&secret, &receiver, sender_public, salt);
    let plaintext = key
        .open_in_place(nonce, Aad::empty(), &mut record)
        .unwrap()
        .to_vec();
    assert_eq!(plaintext.last(), Some(&2));
    plaintext[..plaintext.len() - 1].to_vec()
}
