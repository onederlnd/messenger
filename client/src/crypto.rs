use base64::Engine;
use chacha20poly1305::{aead::Aead, aead::KeyInit, ChaCha20Poly1305, Nonce};
use hkdf::Hkdf;
use rand_core::{OsRng, RngCore};
use sha2::Sha256;
use x25519_dalek::{PublicKey, StaticSecret};

pub struct EncryptedMessage {
    pub ciphertext: String, // base64
    pub nonce: String,      //base64
}

fn derive_key(shared_secret: &x25519_dalek::SharedSecret) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::new(None, shared_secret.as_bytes());
    let mut key = [0u8; 32];
    hk.expand(b"messenger-e2e-v1", &mut key)
        .expect("32 bytes is a valid HKDF output length");
    key
}

pub fn encrypt(
    my_secret: &StaticSecret,
    their_public_b64: &str,
    plaintext: &str,
) -> Option<EncryptedMessage> {
    let their_bytes = base64::engine::general_purpose::STANDARD
        .decode(their_public_b64)
        .ok()?;
    let their_bytes: [u8; 32] = their_bytes.try_into().ok()?;
    let their_public = PublicKey::from(their_bytes);

    let shared_secret = my_secret.diffie_hellman(&their_public);
    let key = derive_key(&shared_secret);
    let cipher = ChaCha20Poly1305::new((&key).into());

    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);

    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher.encrypt(nonce, plaintext.as_bytes()).ok()?;

    Some(EncryptedMessage {
        ciphertext: base64::engine::general_purpose::STANDARD.encode(ciphertext),
        nonce: base64::engine::general_purpose::STANDARD.encode(nonce_bytes),
    })
}

pub fn decrypt(
    my_secret: &StaticSecret,
    their_public_b64: &str,
    ciphertext_b64: &str,
    nonce_b64: &str,
) -> Option<String> {
    let their_bytes = base64::engine::general_purpose::STANDARD
        .decode(their_public_b64)
        .ok()?;
    let their_bytes: [u8; 32] = their_bytes.try_into().ok()?;
    let their_public = PublicKey::from(their_bytes);

    let shared_secret = my_secret.diffie_hellman(&their_public);
    let key = derive_key(&shared_secret);
    let cipher = ChaCha20Poly1305::new((&key).into());

    let nonce_bytes = base64::engine::general_purpose::STANDARD
        .decode(nonce_b64)
        .ok()?;
    if nonce_bytes.len() != 12 {
        return None;
    }
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = base64::engine::general_purpose::STANDARD
        .decode(ciphertext_b64)
        .ok()?;
    let plaintext = cipher.decrypt(nonce, ciphertext.as_slice()).ok()?;

    String::from_utf8(plaintext).ok()
}
