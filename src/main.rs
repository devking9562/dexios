use anyhow::{Context, Result};
use clap::{Arg, Command};
use std::result::Result::Ok;

mod decrypt;
mod encrypt;
mod erase;
mod file;
mod global;
mod hashing;
mod key;
mod prompt;

fn main() -> Result<()> {
    let matches = Command::new("dexios")
        .version(clap::crate_version!())
        .author("brxken128 <github.com/brxken128>")
        .about("Secure command-line encryption of files.")
        .subcommand_required(true)
        .subcommand(
            Command::new("encrypt")
                .short_flag('e')
                .about("encrypt a file")
                .arg(
                    Arg::new("input")
                        .value_name("input")
                        .takes_value(true)
                        .required(true)
                        .help("the input file"),
                )
                .arg(
                    Arg::new("output")
                        .value_name("output")
                        .takes_value(true)
                        .required(true)
                        .help("the output file"),
                )
                .arg(
                    Arg::new("keyfile")
                        .short('k')
                        .long("keyfile")
                        .value_name("file")
                        .takes_value(true)
                        .help("use a keyfile instead of a password"),
                )
                .arg(
                    Arg::new("erase")
                        .long("erase")
                        .value_name("# of passes")
                        .takes_value(true)
                        .require_equals(true)
                        .help("securely erase the input file once complete (default is 16 passes)")
                        .min_values(0)
                        .default_missing_value("16"),
                )
                .arg(
                    Arg::new("hash")
                        .short('H')
                        .long("hash")
                        .takes_value(false)
                        .help("return a blake3 hash of the encrypted file"),
                )
                .arg(
                    Arg::new("skip")
                        .short('y')
                        .long("skip")
                        .takes_value(false)
                        .help("skip all prompts"),
                )
                .arg(
                    Arg::new("bench")
                        .short('b')
                        .long("benchmark")
                        .takes_value(false)
                        .help("don't write the output file to the disk, to prevent wear on flash storage when benchmarking"),
                )
                .arg(
                    Arg::new("stream")
                        .short('s')
                        .long("stream")
                        .takes_value(false)
                        .help("use stream encryption - default")
                        .conflicts_with("memory"),
                )
                .arg(
                    Arg::new("memory")
                        .short('m')
                        .long("memory")
                        .takes_value(false)
                        .help("load the file into memory before encrypting"),
                ),
        )
        .subcommand(
            Command::new("decrypt")
                .short_flag('d')
                .about("decrypt a previously encrypted file")
                .arg(
                    Arg::new("input")
                        .value_name("input")
                        .takes_value(true)
                        .required(true)
                        .help("the input file"),
                )
                .arg(
                    Arg::new("output")
                        .value_name("output")
                        .takes_value(true)
                        .required(true)
                        .help("the output file"),
                )
                .arg(
                    Arg::new("keyfile")
                        .short('k')
                        .long("keyfile")
                        .value_name("file")
                        .takes_value(true)
                        .help("use a keyfile instead of a password"),
                )
                .arg(
                    Arg::new("erase")
                        .long("erase")
                        .value_name("# of passes")
                        .takes_value(true)
                        .require_equals(true)
                        .help("securely erase the input file once complete (default is 16 passes)")
                        .min_values(0)
                        .default_missing_value("16"),
                )
                .arg(
                    Arg::new("hash")
                        .short('H')
                        .long("hash")
                        .takes_value(false)
                        .help("return a blake3 hash of the encrypted file"),
                )
                .arg(
                    Arg::new("skip")
                        .short('y')
                        .long("skip")
                        .takes_value(false)
                        .help("skip all prompts"),
                )
                .arg(
                    Arg::new("bench")
                        .short('b')
                        .long("benchmark")
                        .takes_value(false)
                        .help("don't write the output file to the disk, to prevent wear on flash storage when benchmarking"),
                )
                .arg(
                    Arg::new("stream")
                        .short('s')
                        .long("stream")
                        .takes_value(false)
                        .help("use stream decryption - default")
                        .conflicts_with("memory"),
                )
                .arg(
                    Arg::new("memory")
                        .short('m')
                        .long("memory")
                        .takes_value(false)
                        .help("load the file into memory before encrypting"),
                ),
        )
        .subcommand(
            Command::new("erase")
                .about("erase a file completely")
                .arg(
                    Arg::new("input")
                        .value_name("input")
                        .takes_value(true)
                        .required(true)
                        .help("the file to erase"),
                )
                .arg(
                    Arg::new("passes")
                        .long("passes")
                        .value_name("# of passes")
                        .takes_value(true)
                        .require_equals(true)
                        .help("specify the number of passes (default is 16)")
                        .min_values(0)
                        .default_missing_value("16"),
                ),
        )
        .get_matches();

    match matches.subcommand() {
        Some(("encrypt", sub_matches)) => {
            let mut keyfile = "";
            if sub_matches.is_present("keyfile") {
                keyfile = sub_matches
                    .value_of("keyfile")
                    .context("No keyfile/invalid text provided")?;
            }

            let result = if sub_matches.is_present("memory") {
                encrypt::encrypt_file(
                    sub_matches
                        .value_of("input")
                        .context("No input file/invalid text provided")?,
                    sub_matches
                        .value_of("output")
                        .context("No output file/invalid text provided")?,
                    keyfile,
                    sub_matches.is_present("hash"),
                    sub_matches.is_present("skip"),
                    sub_matches.is_present("bench"),
                )
            } else {
                encrypt::encrypt_file_stream(
                    sub_matches
                        .value_of("input")
                        .context("No input file/invalid text provided")?,
                    sub_matches
                        .value_of("output")
                        .context("No output file/invalid text provided")?,
                    keyfile,
                    sub_matches.is_present("hash"),
                    sub_matches.is_present("skip"),
                    sub_matches.is_present("bench"),
                )
            };

            if result.is_ok() && sub_matches.is_present("erase") {
                let result = sub_matches.value_of("erase").unwrap().parse();
                let passes = if let Ok(result) = result {
                    result
                } else {
                    println!("Unable to read number of passes provided - using the default.");
                    16
                };

                erase::secure_erase(
                    sub_matches
                        .value_of("input")
                        .context("No input file/invalid text provided")?,
                    passes,
                )?;
            }

            return result;
        }
        Some(("decrypt", sub_matches)) => {
            let mut keyfile = "";
            if sub_matches.is_present("keyfile") {
                keyfile = sub_matches
                    .value_of("keyfile")
                    .context("No keyfile/invalid text provided")?;
            }

            let result = if sub_matches.is_present("memory") {
                decrypt::decrypt_file(
                    sub_matches
                        .value_of("input")
                        .context("No input file/invalid text provided")?,
                    sub_matches
                        .value_of("output")
                        .context("No output file/invalid text provided")?,
                    keyfile,
                    sub_matches.is_present("hash"),
                    sub_matches.is_present("skip"),
                    sub_matches.is_present("bench"),
                )
            } else {
                decrypt::decrypt_file_stream(
                    sub_matches
                        .value_of("input")
                        .context("No input file/invalid text provided")?,
                    sub_matches
                        .value_of("output")
                        .context("No output file/invalid text provided")?,
                    keyfile,
                    sub_matches.is_present("hash"),
                    sub_matches.is_present("skip"),
                    sub_matches.is_present("bench"),
                )
            };

            if result.is_ok() && sub_matches.is_present("erase") {
                let result = sub_matches.value_of("erase").unwrap().parse();
                let passes = if let Ok(result) = result {
                    result
                } else {
                    println!("Unable to read number of passes provided - using the default.");
                    16
                };

                erase::secure_erase(
                    sub_matches
                        .value_of("input")
                        .context("No input file/invalid text provided")?,
                    passes,
                )?;
            }

            return result;
        }
        Some(("erase", sub_matches)) => {
            let passes = if sub_matches.is_present("passes") {
                let result = sub_matches.value_of("passes").unwrap().parse::<i32>();
                if let Ok(result) = result {
                    result
                } else {
                    println!("Unable to read number of passes provided - using the default.");
                    16
                }
            } else {
                println!("Number of passes not provided - using the default.");
                16
            };
            erase::secure_erase(
                sub_matches
                    .value_of("input")
                    .context("No input file/invalid text provided")?,
                passes,
            )?;
        }
        _ => (),
    }
    Ok(())
}
