use crate::global::crypto::EncryptStreamCiphers;
use crate::global::parameters::{
    Algorithm, BenchMode, CipherMode, HashMode, HeaderType, OutputFile,
};
use crate::global::{BLOCK_SIZE, SALT_LEN, VERSION};
use crate::key::argon2_hash;
use aead::stream::EncryptorLE31;
use aead::{Aead, NewAead};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::anyhow;
use anyhow::Context;
use anyhow::Result;
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rand::{prelude::StdRng, Rng, RngCore, SeedableRng};
use secrecy::{ExposeSecret, Secret};
use std::fs::File;
use std::io::Read;
use std::result::Result::Ok;
use std::time::Instant;

// this generates a salt for password hashing
fn gen_salt() -> [u8; SALT_LEN] {
    let mut salt: [u8; SALT_LEN] = [0; SALT_LEN];
    StdRng::from_entropy().fill_bytes(&mut salt);
    salt
}

// this encrypts data in memory mode
// it takes the data and a Secret<> key
// it generates the 12/24 byte nonce, hashes the key and encrypts the data
// it returns the salt, nonce, and encrypted bytes
pub fn encrypt_bytes_memory_mode(
    data: Secret<Vec<u8>>,
    output: &mut OutputFile,
    raw_key: Secret<Vec<u8>>,
    bench: BenchMode,
    hash: HashMode,
    algorithm: Algorithm,
) -> Result<()> {
    let salt = gen_salt();

    let header_type = HeaderType {
        header_version: VERSION,
        cipher_mode: CipherMode::MemoryMode,
        algorithm,
    };

    let (nonce_bytes, encrypted_bytes) = match algorithm {
        Algorithm::AesGcm => {
            let nonce_bytes = StdRng::from_entropy().gen::<[u8; 12]>();
            let nonce = Nonce::from_slice(nonce_bytes.as_slice());

            let key = argon2_hash(raw_key, &salt)?;

            let cipher = match Aes256Gcm::new_from_slice(key.expose_secret()) {
                Ok(cipher) => {
                    drop(key);
                    cipher
                }
                Err(_) => return Err(anyhow!("Unable to create cipher with argon2id hashed key.")),
            };

            let encrypted_bytes = match cipher.encrypt(nonce, data.expose_secret().as_slice()) {
                Ok(bytes) => bytes,
                Err(_) => return Err(anyhow!("Unable to encrypt the data")),
            };

            drop(data);

            (nonce_bytes.to_vec(), encrypted_bytes)
        }
        Algorithm::XChaCha20Poly1305 => {
            let nonce_bytes = StdRng::from_entropy().gen::<[u8; 24]>();
            let nonce = XNonce::from_slice(&nonce_bytes);
            let key = argon2_hash(raw_key, &salt)?;

            let cipher = match XChaCha20Poly1305::new_from_slice(key.expose_secret()) {
                Ok(cipher) => {
                    drop(key);
                    cipher
                }
                Err(_) => return Err(anyhow!("Unable to create cipher with argon2id hashed key.")),
            };

            let encrypted_bytes = match cipher.encrypt(nonce, data.expose_secret().as_slice()) {
                Ok(bytes) => bytes,
                Err(_) => return Err(anyhow!("Unable to encrypt the data")),
            };

            drop(data);

            (nonce_bytes.to_vec(), encrypted_bytes)
        }
    };

    if bench == BenchMode::WriteToFilesystem {
        let write_start_time = Instant::now();
        crate::header::write_to_file(output, &salt, &nonce_bytes, &header_type)?;
        output.write_all(&encrypted_bytes)?;
        let write_duration = write_start_time.elapsed();
        println!("Wrote to file [took {:.2}s]", write_duration.as_secs_f32());
    }

    let mut hasher = blake3::Hasher::new();
    if hash == HashMode::CalculateHash {
        let hash_start_time = Instant::now();
        crate::header::hash(&mut hasher, &salt, &nonce_bytes, &header_type);
        hasher.update(&encrypted_bytes);
        let hash = hasher.finalize().to_hex().to_string();
        let hash_duration = hash_start_time.elapsed();
        println!(
            "Hash of the encrypted file is: {} [took {:.2}s]",
            hash,
            hash_duration.as_secs_f32()
        );
    }

    Ok(())
}

// this encrypts data in stream mode
// it takes an input file handle, an output file handle, a Secret<> key, and bools for if we're in bench/hash mode
// it generates the 8 byte nonce, creates the encryption cipher and then reads the file in blocks
// on each read, it encrypts, writes (if enabled), hashes (if enabled) and repeats until EOF
// it also handles the prep of each individual stream, via the match statement
pub fn encrypt_bytes_stream_mode(
    input: &mut File,
    output: &mut OutputFile,
    raw_key: Secret<Vec<u8>>,
    bench: BenchMode,
    hash: HashMode,
    algorithm: Algorithm,
) -> Result<()> {
    let salt = gen_salt();

    let header_type = HeaderType {
        header_version: VERSION,
        cipher_mode: CipherMode::StreamMode,
        algorithm,
    };

    let (mut streams, nonce_bytes): (EncryptStreamCiphers, Vec<u8>) = match algorithm {
        Algorithm::AesGcm => {
            let nonce_bytes = StdRng::from_entropy().gen::<[u8; 8]>();
            let nonce = Nonce::from_slice(&nonce_bytes);

            let key = argon2_hash(raw_key, &salt)?;
            let cipher = match Aes256Gcm::new_from_slice(key.expose_secret()) {
                Ok(cipher) => {
                    drop(key);
                    cipher
                }
                Err(_) => return Err(anyhow!("Unable to create cipher with argon2id hashed key.")),
            };

            let stream = EncryptorLE31::from_aead(cipher, nonce);
            (
                EncryptStreamCiphers::AesGcm(Box::new(stream)),
                nonce_bytes.to_vec(),
            )
        }
        Algorithm::XChaCha20Poly1305 => {
            let nonce_bytes = StdRng::from_entropy().gen::<[u8; 20]>();

            let key = argon2_hash(raw_key, &salt)?;
            let cipher = match XChaCha20Poly1305::new_from_slice(key.expose_secret()) {
                Ok(cipher) => {
                    drop(key);
                    cipher
                }
                Err(_) => return Err(anyhow!("Unable to create cipher with argon2id hashed key.")),
            };

            let stream = EncryptorLE31::from_aead(cipher, nonce_bytes.as_slice().into());
            (
                EncryptStreamCiphers::XChaCha(Box::new(stream)),
                nonce_bytes.to_vec(),
            )
        }
    };

    if bench == BenchMode::WriteToFilesystem {
        crate::header::write_to_file(output, &salt, &nonce_bytes, &header_type)?;
    }

    let mut hasher = blake3::Hasher::new();

    if hash == HashMode::CalculateHash {
        crate::header::hash(&mut hasher, &salt, &nonce_bytes, &header_type);
    }

    let mut buffer = [0u8; BLOCK_SIZE];

    loop {
        let read_count = input
            .read(&mut buffer)
            .context("Unable to read from the input file")?;
        if read_count == BLOCK_SIZE {
            let encrypted_data = match streams.encrypt_next(buffer.as_slice()) {
                Ok(bytes) => bytes,
                Err(_) => return Err(anyhow!("Unable to encrypt the data")),
            };

            if bench == BenchMode::WriteToFilesystem {
                output
                    .write_all(&encrypted_data)
                    .context("Unable to write to the output file")?;
            }
            if hash == HashMode::CalculateHash {
                hasher.update(&encrypted_data);
            }
        } else {
            // if we read something less than BLOCK_SIZE, and have hit the end of the file
            let encrypted_data = match streams.encrypt_last(&buffer[..read_count]) {
                Ok(bytes) => bytes,
                Err(_) => return Err(anyhow!("Unable to encrypt the data")),
            };

            if bench == BenchMode::WriteToFilesystem {
                output
                    .write_all(&encrypted_data)
                    .context("Unable to write to the output file")?;
            }
            if hash == HashMode::CalculateHash {
                hasher.update(&encrypted_data);
            }
            break;
        }
    }
    if bench == BenchMode::WriteToFilesystem {
        output.flush().context("Unable to flush the output file")?;
    }
    if hash == HashMode::CalculateHash {
        let hash = hasher.finalize().to_hex().to_string();
        println!("Hash of the encrypted file is: {}", hash,);
    }
    Ok(())
}
