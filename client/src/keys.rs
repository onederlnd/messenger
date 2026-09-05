// client/src/keys.rs

use rand_core::OsRng;
use std::fs;
use std::path::PathBuf;
use x25519_dalek::{PublicKey, StaticSecret};

fn key_file_path() -> PathBuf {
    let data_dir = app_data_dir();
    fs::create_dir_all(&data_dir).expect("failed to create app data directory");
    data_dir.join("identity.key")
}

#[cfg(target_os = "android")]
fn app_data_dir() -> PathBuf {
    // TODO: hardcoded to match AndroidManifest package name (rust.client).
    // If the package is ever renamed, this breaks silently (wrong/missing
    // storage dir). Replace with the real path from Android's Context
    // (via JNI) instead of hardcoding it.
    PathBuf::from("/data/data/rust.client/files")
}

#[cfg(not(target_os = "android"))]
fn app_data_dir() -> PathBuf {
    use directories::ProjectDirs;
    ProjectDirs::from("com", "yourorg", "messenger")
        .expect("could not determine app data directory")
        .data_dir()
        .to_path_buf()
}

pub fn load_or_generate_keypair() -> (StaticSecret, PublicKey) {
    let path = key_file_path();

    if path.exists() {
        let bytes = fs::read(&path).expect("failed to read identity key file");
        let key_bytes: [u8; 32] = bytes
            .try_into()
            .expect("identity key file is corrupt (wrong length)");
        let secret = StaticSecret::from(key_bytes);
        let public = PublicKey::from(&secret);
        (secret, public)
    } else {
        let secret = StaticSecret::random_from_rng(OsRng);
        let public = PublicKey::from(&secret);
        fs::write(&path, secret.to_bytes()).expect("failed to write identity key file");
        (secret, public)
    }
}
