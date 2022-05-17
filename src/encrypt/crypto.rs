use std::fs::File;

use crate::global::{DexiosFile, BLOCK_SIZE};
use aes_gcm::aead::generic_array::GenericArray;
use aes_gcm::aead::{stream::EncryptorLE31, Aead, NewAead};
use aes_gcm::{Aes256Gcm, Key};
use anyhow::Result;
use anyhow::{Context, Ok};
use argon2::Argon2;
use argon2::Params;
use rand::{prelude::StdRng, Rng, RngCore, SeedableRng};
use std::io::Read;
use std::io::Write;

fn gen_salt() -> [u8; 256] {
    let mut salt: [u8; 256] = [0; 256];
    StdRng::from_entropy().fill_bytes(&mut salt);

    salt
}

fn gen_key(raw_key: Vec<u8>) -> ([u8; 32], [u8; 256]) {
    let mut key = [0u8; 32];
    let salt = gen_salt();

    let argon2 = Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        Params::default(),
    );
    argon2
        .hash_password_into(&raw_key, &salt, &mut key)
        .expect("Unable to hash your password with argon2id");

    (key, salt)
}

fn gen_nonce() -> [u8; 12] {
    rand::thread_rng().gen::<[u8; 12]>()
}

pub fn encrypt_bytes(data: Vec<u8>, raw_key: Vec<u8>) -> DexiosFile {
    let nonce_bytes = gen_nonce();
    let nonce = GenericArray::from_slice(nonce_bytes.as_slice());

    let (key, salt) = gen_key(raw_key);
    let cipher_key = Key::from_slice(key.as_slice());

    let cipher = Aes256Gcm::new(cipher_key);
    let encrypted_bytes = cipher
        .encrypt(nonce, data.as_slice())
        .expect("Unable to encrypt the data");

    drop(data);

    DexiosFile {
        salt,
        nonce: nonce_bytes,
        data: encrypted_bytes,
    }
}

pub fn encrypt_bytes_stream(
    input: &mut File,
    output: &mut File,
    raw_key: Vec<u8>,
    bench: bool,
    hash: bool,
) -> Result<()> {
    let nonce_bytes = rand::thread_rng().gen::<[u8; 8]>(); // only 8 because the last 4 are for the 32-bit AEAD counters
    let nonce = GenericArray::from_slice(nonce_bytes.as_slice());

    let (key, salt) = gen_key(raw_key);
    let cipher_key = Key::from_slice(key.as_slice());

    let cipher = Aes256Gcm::new(cipher_key);
    let mut stream = EncryptorLE31::from_aead(cipher, nonce);

    if !bench {
        output
            .write_all(&salt)
            .context("Unable to write salt to the output file")?;
        output
            .write_all(&nonce_bytes)
            .context("Unable to write nonce to the output file")?;
    }

    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0u8; BLOCK_SIZE];

    loop {
        let read_count = input
            .read(&mut buffer)
            .context("Unable to read from the input file")?;
        if read_count == BLOCK_SIZE {
            // buffer length
            let encrypted_data = stream
                .encrypt_next(buffer.as_slice())
                .expect("Unable to encrypt block");
            if !bench {
                output
                    .write_all(&encrypted_data)
                    .context("Unable to write to the output file")?;
            }
            if hash {
                hasher.update(&encrypted_data);
            }
        } else {
            // if we read something less than BLOCK_SIZE, and have hit the end of the file
            let encrypted_data = stream
                .encrypt_last(&buffer[..read_count])
                .expect("Unable to encrypt final block");
            if !bench {
                output
                    .write_all(&encrypted_data)
                    .context("Unable to write to the output file")?;
            }
            if hash {
                hasher.update(&encrypted_data);
            }
            break;
        }
    }
    if !bench {
        output.flush().context("Unable to flush the output file")?;
    }
    if hash {
        let hash = hasher.finalize().to_hex().to_string();
        println!("Hash of the encrypted file is: {}", hash,);
    }
    Ok(())
}
