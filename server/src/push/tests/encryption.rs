use aws_lc_rs::agreement::{ECDH_P256, PrivateKey};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

use crate::push::encryption::{MOST, Receiver, encrypt, encrypt_with};

fn decoded(text: &str) -> Vec<u8> {
    URL_SAFE_NO_PAD.decode(text).unwrap()
}

/// The example of RFC 8291 section 5, whose sender key and salt are fixed.
#[test]
fn encrypts_the_rfc_8291_example() {
    let receiver_public = decoded(
        "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4",
    );
    let auth = decoded("BTBZMqHH6r4Tts7J_aSIgg");
    let sender = PrivateKey::from_private_key(
        &ECDH_P256,
        &decoded("yfWPiYE-n46HLnH0KqZOF1fJJU3MYrct3AELtAQ-oRw"),
    )
    .unwrap();
    let salt: [u8; 16] = decoded("DGv6ra1nlYgDCS1FRnbzlw").try_into().unwrap();
    let receiver = Receiver {
        public: &receiver_public,
        auth: &auth,
    };
    let message = encrypt_with(
        b"When I grow up, I want to be a watermelon",
        &receiver,
        &sender,
        salt,
    )
    .unwrap();
    assert_eq!(
        URL_SAFE_NO_PAD.encode(message),
        "DGv6ra1nlYgDCS1FRnbzlwAAEABBBP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27ml\
         mlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A_yl95bQpu6cVPT\
         pK4Mqgkf1CXztLVBSt2Ks3oZwbuwXPXLWyouBWLVWGNWQexSgSxsj_Qulcy4a-fN"
    );
}

#[test]
fn a_message_must_fit_in_one_record() {
    let private = PrivateKey::generate(&ECDH_P256).unwrap();
    let public = private.compute_public_key().unwrap();
    let receiver = Receiver {
        public: public.as_ref(),
        auth: &[0; 16],
    };
    assert!(encrypt(&[b'a'; MOST], &receiver).is_ok());
    assert!(encrypt(&[b'a'; MOST + 1], &receiver).is_err());
}
