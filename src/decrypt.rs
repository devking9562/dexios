use crate::decrypt::crypto::decrypt_bytes;
use crate::decrypt::crypto::decrypt_bytes_stream;
use crate::decrypt::key::get_user_key;
use crate::file::get_encrypted_file_data;
use crate::file::overwrite_check;
use crate::file::write_bytes_to_file;
use crate::hashing::hash_data_blake3;

use crate::prompt::get_answer;
use crate::global::DexiosFile;
use anyhow::{Context, Ok, Result};
use std::fs::File;

use std::process::exit;
use std::time::Instant;
mod crypto;
mod key;

pub fn decrypt_file(
    input: &str,
    output: &str,
    keyfile: &str,
    hash_mode: bool,
    skip: bool,
    bench: bool,
) -> Result<()> {
    if !overwrite_check(output, skip)? {
        exit(0);
    }

    let read_start_time = Instant::now();
    let (salt, nonce, encrypted_data) = get_encrypted_file_data(input)?;
    let data = DexiosFile {
        salt,
        nonce,
        data: encrypted_data,
    };
    let read_duration = read_start_time.elapsed();
    println!("Read {} [took {:.2}s]", input, read_duration.as_secs_f32());

    if hash_mode {
        let start_time = Instant::now();
        let hash = hash_data_blake3(&data)?;
        let duration = start_time.elapsed();
        println!(
            "Hash of the encrypted file is: {} [took {:.2}s]",
            hash,
            duration.as_secs_f32()
        );

        let answer = get_answer(
            "Would you like to continue with the decryption?",
            true,
            skip,
        )
        .context("Unable to read provided answer")?;
        if !answer {
            exit(0);
        }
    }

    let raw_key = get_user_key(keyfile)?;

    println!(
        "Decrypting {} in legacy mode (this may take a while)",
        input
    );
    let decrypt_start_time = Instant::now();
    let decrypted_bytes = decrypt_bytes(data, raw_key)?;
    let decrypt_duration = decrypt_start_time.elapsed();
    println!(
        "Decryption successful! [took {:.2}s]",
        decrypt_duration.as_secs_f32()
    );

    if !bench {
        let write_start_time = Instant::now();
        write_bytes_to_file(output, decrypted_bytes)?;
        let write_duration = write_start_time.elapsed();
        println!(
            "Wrote to {} [took {:.2}s]",
            output,
            write_duration.as_secs_f32()
        );
    }

    Ok(())
}

pub fn decrypt_file_stream(
    input: &str,
    output: &str,
    keyfile: &str,
    hash_mode: bool,
    skip: bool,
    bench: bool,
) -> Result<()> {
    if !overwrite_check(output, skip)? {
        exit(0);
    }

    let raw_key = get_user_key(keyfile)?;

    let mut input_file = File::open(input).context("Unable to open file")?;
    let mut output_file = File::create(output).context("Unable to open file")?;

    println!(
        "Decrypting {} in stream mode (this may take a while)",
        input
    );
    let decrypt_start_time = Instant::now();
    decrypt_bytes_stream(&mut input_file, &mut output_file, raw_key, bench, hash_mode)?;
    let decrypt_duration = decrypt_start_time.elapsed();
    println!(
        "Decryption successful! [took {:.2}s]",
        decrypt_duration.as_secs_f32()
    );

    Ok(())
}
