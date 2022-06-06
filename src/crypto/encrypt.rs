use crate::crypto::key::{argon2_hash, gen_salt};
use crate::crypto::streams::init_encryption_stream;
use crate::global::header::create_aad;
use crate::global::secret::Secret;
use crate::global::states::{Algorithm, BenchMode, CipherMode, HashMode, OutputFile};
use crate::global::structs::{HeaderType, Header};
use crate::global::{BLOCK_SIZE, VERSION};
use aead::Payload;
use anyhow::anyhow;
use anyhow::Context;
use anyhow::Result;
use paris::success;
use rand::{SeedableRng, Rng};
use rand::prelude::StdRng;
use std::fs::File;
use std::io::Read;
use std::result::Result::Ok;
use std::time::Instant;
use zeroize::Zeroize;

use super::memory::init_memory_cipher;

// this encrypts data in memory mode
// it takes the data and a Secret<> key
// it generates the nonce, hashes the key and encrypts the data
// it writes the header and then the encrypted data to the output file
#[allow(clippy::too_many_lines)]
// !!! delegate the `match algorithm` to a file like `memory.rs`
// similar to stream initialisation
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

    let key = argon2_hash(raw_key, &salt, &header_type.header_version)?;

    // let nonce_bytes = StdRng::from_entropy().gen::<[u8; 12]>();

    let nonce_bytes = match header_type.algorithm {
        Algorithm::Aes256Gcm => StdRng::from_entropy().gen::<[u8; 12]>().to_vec(),
        Algorithm::XChaCha20Poly1305 => StdRng::from_entropy().gen::<[u8; 24]>().to_vec(),
        Algorithm::DeoxysII256 => StdRng::from_entropy().gen::<[u8; 15]>().to_vec(),
    };

    let ciphers = init_memory_cipher(key, &algorithm)?;


    let header = Header {
        salt: salt,
        nonce: nonce_bytes.to_vec(),
        header_type,
    };

    let aad = create_aad(&header);

    let payload = Payload {
        aad: &aad,
        msg: data.expose().as_slice(),
    };

    let encrypted_bytes = match ciphers.encrypt(&header.nonce, payload) {
        Ok(bytes) => bytes,
        Err(_) => return Err(anyhow!("Unable to encrypt the data")),
    };

    drop(data);

    if bench == BenchMode::WriteToFilesystem {
        let write_start_time = Instant::now();
        crate::global::header::write_to_file(output, &header)?;
        output.write_all(&encrypted_bytes)?;
        let write_duration = write_start_time.elapsed();
        success!("Wrote to file [took {:.2}s]", write_duration.as_secs_f32());
    }

    let mut hasher = blake3::Hasher::new();
    if hash == HashMode::CalculateHash {
        let hash_start_time = Instant::now();
        crate::global::header::hash(&mut hasher, &header);
        hasher.update(&encrypted_bytes);
        let hash = hasher.finalize().to_hex().to_string();
        let hash_duration = hash_start_time.elapsed();
        success!(
            "Hash of the encrypted file is: {} [took {:.2}s]",
            hash,
            hash_duration.as_secs_f32()
        );
    }

    Ok(())
}

// this encrypts data in stream mode
// it takes an input file handle, an output file handle, a Secret<> key, and enums for specific modes
// it gets the nonce, salt and streams enum from `init_encryption_stream` and then reads the file in blocks
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
    let header_type = HeaderType {
        header_version: VERSION,
        cipher_mode: CipherMode::StreamMode,
        algorithm,
    };

    let (mut streams, header) = init_encryption_stream(raw_key, header_type)?;

    if bench == BenchMode::WriteToFilesystem {
        crate::global::header::write_to_file(output, &header)?;
    }

    let mut hasher = blake3::Hasher::new();

    if hash == HashMode::CalculateHash {
        crate::global::header::hash(&mut hasher, &header);
    }

    let aad = create_aad(&header);

    let mut read_buffer = vec![0u8; BLOCK_SIZE].into_boxed_slice();

    loop {
        let read_count = input
            .read(&mut read_buffer)
            .context("Unable to read from the input file")?;
        if read_count == BLOCK_SIZE {
            // aad is just empty bytes normally
            // create_aad returns empty bytes if the header isn't V3+
            // this means we don't need to do anything special in regards to older versions
            let payload = Payload {
                aad: &aad,
                msg: read_buffer.as_ref(),
            };

            let encrypted_data = match streams.encrypt_next(payload) {
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
            let payload = Payload {
                aad: &aad,
                msg: &read_buffer[..read_count],
            };

            let encrypted_data = match streams.encrypt_last(payload) {
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

    read_buffer.zeroize();

    if bench == BenchMode::WriteToFilesystem {
        output.flush().context("Unable to flush the output file")?;
    }
    if hash == HashMode::CalculateHash {
        let hash = hasher.finalize().to_hex().to_string();
        success!("Hash of the encrypted file is: {}", hash,);
    }
    Ok(())
}
