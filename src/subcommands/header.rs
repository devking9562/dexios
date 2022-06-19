use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, Write},
    process::exit,
};

use super::key::get_secret;
use super::prompt::{get_answer, overwrite_check};
use crate::global::states::{PasswordMode, PasswordState};
use crate::global::states::{KeyFile, SkipMode};
use anyhow::{Context, Result};
use dexios_core::cipher::Ciphers;
use dexios_core::header::{Header, HeaderVersion};
use dexios_core::primitives::Mode;
use dexios_core::protected::Protected;
use dexios_core::Zeroize;
use dexios_core::{key::balloon_hash, primitives::gen_nonce};
use paris::info;
use paris::{success, Logger};
use std::io::Cursor;
use std::time::Instant;

pub fn update_key(input: &str, keyfile_old: &KeyFile, keyfile_new: &KeyFile) -> Result<()> {
    if !keyfile_old.is_present() {
        info!("Please enter your old password below");
    }
    let raw_key_old = get_secret(keyfile_old, &PasswordState::Direct, PasswordMode::NormalKeySourcePriority)?;

    if !keyfile_new.is_present() {
        info!("Please enter your new password below");
    }
    let raw_key_new = get_secret(keyfile_new, &PasswordState::Validate, PasswordMode::NormalKeySourcePriority)?;

    let mut input_file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(input)
        .with_context(|| format!("Unable to open input file: {}", input))?;

    let (header, _) = dexios_core::header::Header::deserialize(&mut input_file)?;

    if header.header_type.version < HeaderVersion::V4 {
        return Err(anyhow::anyhow!(
            "Updating a key is not supported in header versions below V4."
        ));
    }

    let header_size: i64 = header
        .get_size()
        .try_into()
        .context("Unable to convert header size (u64) to i64")?;

    let master_key_encrypted = header.master_key_encrypted.unwrap();
    let master_key_nonce = header.master_key_nonce.unwrap();

    let hash_start_time = Instant::now();
    let key_old = balloon_hash(raw_key_old, &header.salt, &header.header_type.version)?;
    let hash_duration = hash_start_time.elapsed();
    success!(
        "Successfully hashed your old key [took {:.2}s]",
        hash_duration.as_secs_f32()
    );

    let hash_start_time = Instant::now();
    let key_new = balloon_hash(raw_key_new, &header.salt, &header.header_type.version)?;
    let hash_duration = hash_start_time.elapsed();
    success!(
        "Successfully hashed your new key [took {:.2}s]",
        hash_duration.as_secs_f32()
    );

    let cipher = Ciphers::initialize(key_old, &header.header_type.algorithm)?;

    let master_key_result = cipher.decrypt(&master_key_nonce, master_key_encrypted.as_slice());
    let mut master_key_decrypted = match master_key_result {
        std::result::Result::Ok(bytes) => bytes,
        Err(_) => {
            return Err(anyhow::anyhow!(
                "Unable to decrypt your master key (maybe you supplied a wrong key?)"
            ))
        }
    };

    let mut master_key = [0u8; 32];

    for (i, byte) in master_key_decrypted.iter().enumerate() {
        master_key[i] = *byte;
    }

    master_key_decrypted.zeroize();
    let master_key = Protected::new(master_key);

    drop(cipher);

    let cipher = Ciphers::initialize(key_new, &header.header_type.algorithm)?;

    let master_key_nonce_new = gen_nonce(&header.header_type.algorithm, &Mode::MemoryMode);
    let master_key_result = cipher.encrypt(&master_key_nonce_new, master_key.expose().as_slice());

    drop(master_key);

    let master_key_encrypted = match master_key_result {
        std::result::Result::Ok(bytes) => bytes,
        Err(_) => return Err(anyhow::anyhow!("Unable to encrypt your master key")),
    };

    let header_new = Header {
        header_type: header.header_type,
        nonce: header.nonce,
        salt: header.salt,
        master_key_encrypted: Some(master_key_encrypted),
        master_key_nonce: Some(master_key_nonce_new),
    };

    input_file
        .seek(std::io::SeekFrom::Current(-header_size))
        .context("Unable to seek back to the start of your input file")?;
    header_new.write(&mut input_file)?;

    success!("Key successfully updated for {}", input);

    Ok(())
}

// this function dumps the first 64 bytes of
// the input file into the output file
// it's used for extracting an encrypted file's header for backups and such
// it implements a check to ensure the header is valid
pub fn dump(input: &str, output: &str, skip: SkipMode) -> Result<()> {
    let mut logger = Logger::new();
    logger.warn("THIS FEATURE IS FOR ADVANCED USERS ONLY AND MAY RESULT IN A LOSS OF DATA - PROCEED WITH CAUTION");

    let mut header = [0u8; 64];

    let mut input_file =
        File::open(input).with_context(|| format!("Unable to open input file: {}", input))?;
    input_file
        .read_exact(&mut header)
        .with_context(|| format!("Unable to read header from file: {}", input))?;

    let mut header_reader = Cursor::new(header);
    if Header::deserialize(&mut header_reader).is_err() {
        logger.error("File does not contain a valid Dexios header - exiting");
        drop(input_file);
        exit(1);
    }

    if !overwrite_check(output, skip)? {
        std::process::exit(0);
    }

    let mut output_file =
        File::create(output).with_context(|| format!("Unable to open output file: {}", output))?;
    output_file
        .write_all(&header)
        .with_context(|| format!("Unable to write header to output file: {}", output))?;

    logger.success(format!("Header dumped to {} successfully.", output));
    Ok(())
}

// this function reads the first 64 bytes (header) from the input file
// and then overwrites the first 64 bytes of the output file with it
// this can be used for restoring a dumped header to a file that had it's header stripped
// it implements a check to ensure the header is valid before restoring to a file
pub fn restore(input: &str, output: &str, skip: SkipMode) -> Result<()> {
    let mut logger = Logger::new();
    logger.warn("THIS FEATURE IS FOR ADVANCED USERS ONLY AND MAY RESULT IN A LOSS OF DATA - PROCEED WITH CAUTION");
    let prompt = format!(
        "Are you sure you'd like to restore the header in {} to {}?",
        input, output
    );
    if !get_answer(&prompt, false, skip == SkipMode::HidePrompts)? {
        exit(0);
    }

    let mut header = vec![0u8; 64];

    let mut input_file =
        File::open(input).with_context(|| format!("Unable to open header file: {}", input))?;
    input_file
        .read_exact(&mut header)
        .with_context(|| format!("Unable to read header from file: {}", input))?;

    let mut header_reader = Cursor::new(header.clone());
    if Header::deserialize(&mut header_reader).is_err() {
        logger.error("File does not contain a valid Dexios header - exiting");
        drop(input_file);
        exit(1);
    }

    let mut output_file = OpenOptions::new()
        .write(true)
        .open(output)
        .with_context(|| format!("Unable to open output file: {}", output))?;

    output_file
        .write_all(&header)
        .with_context(|| format!("Unable to write header to file: {}", output))?;

    logger.success(format!(
        "Header restored to {} from {} successfully.",
        output, input
    ));
    Ok(())
}

// this wipes the first 64 bytes (header) from the provided file
// it can be useful for storing the header separate from the file, to make an attacker's life that little bit harder
// it implements a check to ensure the header is valid before stripping
pub fn strip(input: &str, skip: SkipMode) -> Result<()> {
    let mut logger = Logger::new();
    logger.warn("THIS FEATURE IS FOR ADVANCED USERS ONLY AND MAY RESULT IN A LOSS OF DATA - PROCEED WITH CAUTION");

    let prompt = format!("Are you sure you'd like to wipe the header for {}?", input);
    if !get_answer(&prompt, false, skip == SkipMode::HidePrompts)? {
        exit(0);
    }

    let prompt = "This can be destructive! Make sure you dumped the header first. Would you like to continue?";
    if !get_answer(prompt, false, skip == SkipMode::HidePrompts)? {
        exit(0);
    }

    let buffer = vec![0u8; 64];

    let mut input_file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(input)
        .with_context(|| format!("Unable to open input file: {}", input))?;

    if Header::deserialize(&mut input_file).is_err() {
        logger.error("File does not contain a valid Dexios header - exiting");
        drop(input_file);
        exit(1);
    } else {
        input_file
            .seek(std::io::SeekFrom::Current(-64))
            .context("Unable to seek back to the start of the file")?;
    }

    input_file
        .write_all(&buffer)
        .with_context(|| format!("Unable to wipe header for file: {}", input))?;

    logger.success(format!("Header stripped from {} successfully.", input));
    Ok(())
}
