use std::fs::File;
use std::io::Write;
use anyhow::{Context, Result};
use clap::ArgMatches;

pub struct CryptoParameters {
    pub hash_mode: HashMode,
    pub skip: SkipMode,
    pub bench: BenchMode,
    pub password: PasswordMode,
    pub erase: EraseMode,
    pub cipher_type: CipherType,
}

pub struct HeaderType {
    pub dexios_mode: CipherMode,
    pub cipher_type: CipherType,
}

pub struct PackMode {
    pub dir_mode: DirectoryMode,
    pub hidden: HiddenFilesMode,
    pub exclude: Vec<String>,
    pub memory: bool,
    pub compression_level: i32,
    pub print_mode: PrintMode,
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum DirectoryMode {
    Singular,
    Recursive,
}

#[derive(PartialEq, Eq)]
pub enum HiddenFilesMode {
    Include,
    Exclude,
}

#[derive(PartialEq, Eq)]
pub enum PrintMode {
    Verbose,
    Quiet,
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum EraseMode {
    EraseFile(i32),
    IgnoreFile(i32),
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum HashMode {
    CalculateHash,
    NoHash,
}

#[derive(PartialEq, Eq, Copy, Clone)]
pub enum SkipMode {
    ShowPrompts,
    HidePrompts,
}

#[derive(PartialEq, Eq, Copy, Clone)]
pub enum BenchMode {
    WriteToFilesystem,
    BenchmarkInMemory,
}

#[derive(PartialEq, Eq, Copy, Clone)]
pub enum PasswordMode {
    ForceUserProvidedPassword,
    NormalKeySourcePriority,
}

pub enum OutputFile {
    Some(File),
    None,
}

#[derive(Copy, Clone)]
pub enum CipherType {
    AesGcm,
    XChaCha20Poly1305,
}

impl EraseMode {
    pub fn get_passes(self) -> i32 {
        match self {
            EraseMode::EraseFile(passes) => passes,
            EraseMode::IgnoreFile(_) => 0,
        }
    }
}


impl OutputFile {
    pub fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        match self {
            OutputFile::Some(file) => file.write_all(buf),
            OutputFile::None => Ok(()),
        }
    }
    pub fn flush(&mut self) -> std::io::Result<()> {
        match self {
            OutputFile::Some(file) => file.flush(),
            OutputFile::None => Ok(()),
        }
    }
}

impl std::fmt::Display for CipherType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            CipherType::AesGcm => write!(f, "AES-256-GCM"),
            CipherType::XChaCha20Poly1305 => write!(f, "XChaCha20-Poly1305"),
        }
    }
}

#[derive(PartialEq, Eq)]
pub enum CipherMode {
    // could do with a better name
    MemoryMode,
    StreamMode,
}

impl std::fmt::Display for CipherMode {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            CipherMode::MemoryMode => write!(f, "memory mode"),
            CipherMode::StreamMode => write!(f, "stream mode"),
        }
    }
}

pub fn parameter_handler(sub_matches: &ArgMatches) -> Result<(&str, CryptoParameters)> {
    let mut keyfile = "";
    if sub_matches.is_present("keyfile") {
        keyfile = sub_matches
            .value_of("keyfile")
            .context("No keyfile/invalid text provided")?;
    }

    let hash_mode = if sub_matches.is_present("hash") {
        //specify to emit hash after operation
        HashMode::CalculateHash
    } else {
        // default
        HashMode::NoHash
    };

    let skip = if sub_matches.is_present("skip") {
        //specify to hide promps during operation
        SkipMode::HidePrompts
    } else {
        // default
        SkipMode::ShowPrompts
    };

    let erase = if sub_matches.is_present("erase") {
        let result = sub_matches
            .value_of("erase")
            .context("No amount of passes specified")?
            .parse();

        let passes = if let Ok(value) = result {
            value
        } else {
            println!("Unable to read number of passes provided - using the default.");
            16
        };
        EraseMode::EraseFile(passes)
    } else {
        EraseMode::IgnoreFile(0)
    };

    let bench = if sub_matches.is_present("bench") {
        //specify to not write to filesystem, for benchmarking and saving wear on hardware
        BenchMode::BenchmarkInMemory
    } else {
        // default
        BenchMode::WriteToFilesystem
    };

    let password = if sub_matches.is_present("password") {
        //Overwrite, so the user provided password is used and ignore environment supplied one?!
        PasswordMode::ForceUserProvidedPassword
    } else {
        // default
        PasswordMode::NormalKeySourcePriority
    };

    let cipher_type = if sub_matches.is_present("gcm") {
        // specify gcm manually
        CipherType::AesGcm
    } else {
        // default
        CipherType::XChaCha20Poly1305
    };

    Ok((
        keyfile,
        CryptoParameters {
            hash_mode,
            skip,
            bench,
            password,
            erase,
            cipher_type,
        },
    ))
}

pub fn header_type_handler(sub_matches: &ArgMatches) -> Result<HeaderType> {
    if !sub_matches.is_present("memory") && !sub_matches.is_present("stream") {
        return Err(anyhow::anyhow!(
            "You need to specify if the file was created in memory or stream mode."
        ));
    }

    if !sub_matches.is_present("xchacha") && !sub_matches.is_present("gcm") {
        return Err(anyhow::anyhow!(
            "You need to specify if the file was created using XChaCha20-Poly1305 or AES-256-GCM."
        ));
    }

    let dexios_mode = if sub_matches.is_present("memory") {
        CipherMode::MemoryMode
    } else {
        CipherMode::StreamMode
    };

    let cipher_type = if sub_matches.is_present("gcm") {
        CipherType::AesGcm
    } else {
        CipherType::XChaCha20Poly1305
    };

    Ok(HeaderType {
        dexios_mode,
        cipher_type,
    })
}
