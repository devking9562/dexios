use crate::prompt::*;
use crate::structs::*;
use aes_gcm::aead::{Aead, NewAead};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use anyhow::{Context, Ok, Result};
use rand::{prelude::StdRng, Rng, RngCore, SeedableRng};
use sha3::Digest;
use sha3::Sha3_512;
use std::num::NonZeroU32;
use std::time::Instant;
use std::{
    fs::{metadata, File},
    io::{BufReader, Read, Write},
    process::exit,
};

pub fn encrypt_file(input: &str, output: &str, keyfile: &str, sha_sum: bool) -> Result<()> {
    let mut use_keyfile = false;
    if !keyfile.is_empty() {
        use_keyfile = true;
    }

    if metadata(output).is_ok() {
        // if the output file exists
        let answer = get_answer(
            "Output file already exists, would you like to overwrite?",
            true,
        )?;
        if !answer {
            exit(0);
        }
    }

    let file = File::open(input).context("Unable to open the input file")?;
    let mut reader = BufReader::new(file);
    let mut data = Vec::new(); // our file bytes
    reader
        .read_to_end(&mut data)
        .context("Unable to read the input file")?;
    drop(reader);

    let raw_key;

    if !use_keyfile {
        // if we're not using a keyfile, read from stdin
        loop {
            let input =
                rpassword::prompt_password("Password: ").context("Unable to read password")?;
            let input_validation = rpassword::prompt_password("Password (for validation): ")
                .context("Unable to read password")?;
            if input == input_validation {
                raw_key = input.as_bytes().to_vec();
                break;
            } else {
                println!("The passwords aren't the same, please try again.");
            }
        }
    } else {
        let file = File::open(keyfile).context("Error opening keyfile")?;
        let mut reader = BufReader::new(file);
        let mut buffer = Vec::new(); // our file bytes
        reader
            .read_to_end(&mut buffer)
            .context("Error reading keyfile")?;
        raw_key = buffer.clone();
    }

    let mut key = [0u8; 32];

    let mut salt: [u8; 256] = [0; 256];
    StdRng::from_entropy().fill_bytes(&mut salt);

    let start_time = Instant::now();

    ring::pbkdf2::derive(
        ring::pbkdf2::PBKDF2_HMAC_SHA512,
        NonZeroU32::new(122880).unwrap(),
        &salt,
        &raw_key,
        &mut key,
    );

    let nonce_bytes = rand::thread_rng().gen::<[u8; 12]>();
    let nonce = Nonce::from_slice(nonce_bytes.as_slice());
    let cipher_key = Key::from_slice(key.as_slice());
    let cipher = Aes256Gcm::new(cipher_key);
    let encrypted_bytes = cipher
        .encrypt(nonce, data.as_slice())
        .expect("Unable to encrypt the data");
    let encrypted_bytes_base64 = base64::encode(encrypted_bytes);
    let salt_base64 = base64::encode(salt);
    let nonce_base64 = base64::encode(nonce);

    let data = DexiosFile {
        salt: salt_base64,
        nonce: nonce_base64,
        data: encrypted_bytes_base64,
    };

    let mut writer = File::create(output).context("Can't create output file")?;
    serde_json::to_writer(&writer, &data).context("Can't write to the output file")?;
    writer.flush().context("Unable to flush output file")?;

    let duration = start_time.elapsed();

    println!("Encryption successful - written to {}", output);
    println!("That took {:.2}s", duration.as_secs_f32());
    if sha_sum {
        let mut file = File::open(output).context("Unable to open the encrypted file for hashing")?;
        let mut hasher = Sha3_512::new();
        std::io::copy(&mut file, &mut hasher)
            .context("Unable to copy encrypted file bytes into sha512 hasher")?;
        let hash = hasher.finalize();
        let hash_b64 = base64::encode(hash);
        println!("Hash of the encrypted file is: {}", hash_b64);
        println!("Write this down for later verification - it is not for security, but to ensure your file is exactly how it was in the first place.");
    }

    Ok(())
}
