//! This module contains all cryptographic primitives used by `dexios-core`

/// This is the streaming block size
///
/// NOTE: Stream mode can be used to encrypt files less than this size, provided the implementation is correct
pub const BLOCK_SIZE: usize = 1_048_576; // 1024*1024 bytes

/// This is the length of the salt used for `argon2id` hashing
pub const SALT_LEN: usize = 16; // bytes

/// This is an `enum` containing all AEADs supported by `dexios-core`
#[derive(Copy, Clone)]
pub enum Algorithm {
    Aes256Gcm,
    XChaCha20Poly1305,
    DeoxysII256,
}

/// This is an array containing all AEADs supported by `dexios-core`.
///
/// It can be used by and end-user application to show a list of AEADs that they may use
pub static ALGORITHMS: [Algorithm; 3] = [
    Algorithm::XChaCha20Poly1305,
    Algorithm::Aes256Gcm,
    Algorithm::DeoxysII256,
];

impl std::fmt::Display for Algorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Algorithm::Aes256Gcm => write!(f, "AES-256-GCM"),
            Algorithm::XChaCha20Poly1305 => write!(f, "XChaCha20-Poly1305"),
            Algorithm::DeoxysII256 => write!(f, "Deoxys-II-256"),
        }
    }
}

/// This defines the possible modes used for encrypting/decrypting
#[derive(PartialEq, Eq)]
pub enum Mode {
    MemoryMode,
    StreamMode,
}

/// This can be used to generate a nonce for encryption
/// It requires both the algorithm and the mode, so it can correctly determine the nonce length
/// This nonce can be passed directly to `EncryptionStreams::initialize()`
///
/// # Examples
///
/// ```
/// let nonce = gen_nonce(Algorithm::XChaCha20Poly1305, Mode::StreamMode);
/// ```
///
#[must_use]
pub fn gen_nonce(algorithm: Algorithm, mode: Mode) -> Vec<u8> {
    use rand::{prelude::StdRng, RngCore, SeedableRng};

    let mut nonce_len = match algorithm {
        Algorithm::Aes256Gcm => 12,
        Algorithm::XChaCha20Poly1305 => 24,
        Algorithm::DeoxysII256 => 15,
    };

    if mode == Mode::StreamMode {
        nonce_len -= 4;
    }

    let mut nonce = vec![0u8; nonce_len];
    StdRng::from_entropy().fill_bytes(&mut nonce);
    nonce
}
