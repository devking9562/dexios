use crate::decrypt::crypto::decrypt_bytes;
use crate::decrypt::file::get_file_bytes;
use crate::decrypt::file::write_bytes_to_file;
use crate::prompt::*;
use crate::structs::*;

use anyhow::{Context, Ok, Result};

use crate::decrypt::file::overwrite_check;
use std::process::exit;
use std::time::Instant;
mod crypto;
mod file;

pub fn decrypt_file(
    input: &str,
    output: &str,
    keyfile: &str,
    sha_sum: bool,
    skip: bool,
    bench: bool,
) -> Result<()> {
    if !overwrite_check(output, skip)? {
        exit(0);
    }

    let data = get_file_bytes(input)?;

    if sha_sum {
        let start_time = Instant::now();
        let hash = blake3::hash(&data).to_hex().to_string();
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

    let raw_key = if !keyfile.is_empty() {
        get_file_bytes(keyfile)?
    } else {
        let input = rpassword::prompt_password("Password: ").context("Unable to read password")?;
        input.as_bytes().to_vec()
    };

    let data_json: DexiosFile =
        serde_json::from_slice(&data).context("Unable to read JSON from input file")?;

    drop(data);

    let decrypt_start_time = Instant::now();

    let decrypted_bytes = decrypt_bytes(data_json, raw_key)?;

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
