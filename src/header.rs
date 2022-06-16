//! The Dexios header is an encrypted file/data header that stores specific information needed for decryption.
//!
//! This includes:
//! * header version
//! * salt
//! * nonce
//! * encryption algorithm
//! * whether the file was encrypted in "memory" or stream mode
//!
//! It allows for serialization, deserialization, and has a convenience function for quickly writing the header to a file.
//!
//! # Examples
//!
//! ```
//! let header_bytes: [u8; 64] = [
//!     222, 2, 14, 1, 12, 1, 142, 88, 243, 144, 119, 187, 189, 190, 121, 90, 211, 56, 185, 14, 76,
//!     45, 16, 5, 237, 72, 7, 203, 13, 145, 13, 155, 210, 29, 128, 142, 241, 233, 42, 168, 243,
//!     129, 0, 0, 0, 0, 0, 0, 214, 45, 3, 4, 11, 212, 129, 123, 192, 157, 185, 109, 151, 225, 233,
//!     161,
//! ];
//! let mut cursor = Cursor::new(header_bytes);
//!
//! // the cursor may be a file, this is just an example
//!
//! let (header, aad) = Header::deserialize(&mut cursor).unwrap();
//! ```
//!
//! ```
//! let mut output_file = File::create("test").unwrap();
//!
//! header.write(&mut output_file).unwrap();
//! ```
//!

use super::primitives::{Algorithm, Mode, SALT_LEN};
use anyhow::{Context, Result};
use std::io::{Cursor, Read, Seek, Write};

/// This defines the latest header version, so program's using this can easily stay up to date.
///
/// It's also here to just help users keep track
pub const HEADER_VERSION: HeaderVersion = HeaderVersion::V4;

/// This stores all possible versions of the header
#[allow(clippy::module_name_repetitions)]
#[derive(PartialEq, Clone, Copy)]
pub enum HeaderVersion {
    V1,
    V2,
    V3,
    V4,
}

/// This is the Header's type - it contains the specific details that are needed to decrypt the data
///
/// It contains the header's version, the "mode" that was used to encrypt the data, and the algorithm used.
///
/// This needs to be manually created for encrypting data
#[allow(clippy::module_name_repetitions)]
pub struct HeaderType {
    pub version: HeaderVersion,
    pub algorithm: Algorithm,
    pub mode: Mode,
}

/// This is the `HeaderType` struct, but in the format of raw bytes
///
/// This does not need to be used outside of this core library
struct HeaderTag {
    pub version: [u8; 2],
    pub algorithm: [u8; 2],
    pub mode: [u8; 2],
}

/// This function calculates the length of the nonce, depending on the data provided
///
/// Stream mode nonces are 4 bytes less than their "memory" mode counterparts, due to `aead::StreamLE31`
///
/// `StreamLE31` contains a 31-bit little endian counter, and a 1-bit "last block" flag, stored as the last 4 bytes of the nonce
///
/// This is done to prevent nonce-reuse
fn calc_nonce_len(header_info: &HeaderType) -> usize {
    let mut nonce_len = match header_info.algorithm {
        Algorithm::XChaCha20Poly1305 => 24,
        Algorithm::Aes256Gcm => 12,
        Algorithm::DeoxysII256 => 15,
    };

    if header_info.mode == Mode::StreamMode {
        nonce_len -= 4; // the last 4 bytes are dynamic in stream mode
    }

    nonce_len
}

/// This is the main `Header` struct, and it contains all of the information about the encrypted data
///
/// It contains the `HeaderType`, the nonce, and the salt
///
/// This needs to be manually created for encrypting data
pub struct Header {
    pub header_type: HeaderType,
    pub nonce: Vec<u8>,
    pub salt: [u8; SALT_LEN],
    pub master_key_encrypted: Option<Vec<u8>>,
    pub master_key_nonce: Option<Vec<u8>>,
}

impl Header {
    /// This is a private function (used by other header functions) for returning the `HeaderType`'s raw bytes
    ///
    /// It's used for serialization, and has it's own dedicated function as it will be used often
    fn get_tag(&self) -> HeaderTag {
        let version = self.serialize_version();
        let algorithm = self.serialize_algorithm();
        let mode = self.serialize_mode();
        HeaderTag {
            version,
            algorithm,
            mode,
        }
    }

    /// This is a private function used for serialization
    ///
    /// It converts a `HeaderVersion` into the associated raw bytes
    fn serialize_version(&self) -> [u8; 2] {
        match self.header_type.version {
            HeaderVersion::V1 => {
                let info: [u8; 2] = [0xDE, 0x01];
                info
            }
            HeaderVersion::V2 => {
                let info: [u8; 2] = [0xDE, 0x02];
                info
            }
            HeaderVersion::V3 => {
                let info: [u8; 2] = [0xDE, 0x03];
                info
            }
            HeaderVersion::V4 => {
                let info: [u8; 2] = [0xDE, 0x04];
                info
            }
        }
    }

    /// This is used for deserializing raw bytes from a reader into a `Header` struct
    ///
    /// This also returns the AAD, read from the header. AAD is only generated in `HeaderVersion::V3` and above - it will be blank in older versions.
    ///
    /// The AAD needs to be passed to decryption functions in order to validate the header, and decrypt the data.
    ///
    /// The AAD for older versions is empty as no AAD is the default for AEADs, and the header validation was not in place prior to V3.
    ///
    /// NOTE: This leaves the cursor at 64 bytes into the buffer, as that is the size of the header
    ///
    /// # Examples
    ///
    /// ```
    /// let header_bytes: [u8; 64] = [
    ///     222, 2, 14, 1, 12, 1, 142, 88, 243, 144, 119, 187, 189, 190, 121, 90, 211, 56, 185, 14, 76,
    ///     45, 16, 5, 237, 72, 7, 203, 13, 145, 13, 155, 210, 29, 128, 142, 241, 233, 42, 168, 243,
    ///     129, 0, 0, 0, 0, 0, 0, 214, 45, 3, 4, 11, 212, 129, 123, 192, 157, 185, 109, 151, 225, 233,
    ///     161,
    /// ];
    /// let mut cursor = Cursor::new(header_bytes);
    ///
    /// // the cursor may be a file, this is just an example
    ///
    /// let (header, aad) = Header::deserialize(&mut cursor).unwrap();
    /// ```
    ///
    pub fn deserialize(reader: &mut (impl Read + Seek)) -> Result<(Self, Vec<u8>)> {
        let mut version_bytes = [0u8; 2];
        reader
            .read_exact(&mut version_bytes)
            .context("Unable to read version from the header")?;
        reader
            .seek(std::io::SeekFrom::Current(-2))
            .context("Unable to seek back to start of header")?;

        let version = match version_bytes {
            [0xDE, 0x01] => HeaderVersion::V1,
            [0xDE, 0x02] => HeaderVersion::V2,
            [0xDE, 0x03] => HeaderVersion::V3,
            [0xDE, 0x04] => HeaderVersion::V4,
            _ => return Err(anyhow::anyhow!("Error getting version from header")),
        };

        let header_length: usize = match version {
            HeaderVersion::V1 | HeaderVersion::V2 | HeaderVersion::V3 => 64,
            HeaderVersion::V4 => 128,
        };

        let mut full_header_bytes = vec![0u8; header_length];
        reader
            .read_exact(&mut full_header_bytes)
            .context("Unable to read full bytes of the header")?;

        let mut cursor = Cursor::new(full_header_bytes.clone());
        cursor
            .seek(std::io::SeekFrom::Start(2))
            .context("Unable to seek past version bytes")?; // seek past the version bytes as we already have those

        let mut algorithm_bytes = [0u8; 2];
        cursor
            .read_exact(&mut algorithm_bytes)
            .context("Unable to read algorithm's bytes from header")?;

        let algorithm = match algorithm_bytes {
            [0x0E, 0x01] => Algorithm::XChaCha20Poly1305,
            [0x0E, 0x02] => Algorithm::Aes256Gcm,
            [0x0E, 0x03] => Algorithm::DeoxysII256,
            _ => return Err(anyhow::anyhow!("Error getting encryption mode from header")),
        };

        let mut mode_bytes = [0u8; 2];
        cursor
            .read_exact(&mut mode_bytes)
            .context("Unable to read encryption mode's bytes from header")?;

        let mode = match mode_bytes {
            [0x0C, 0x01] => Mode::StreamMode,
            [0x0C, 0x02] => Mode::MemoryMode,
            _ => return Err(anyhow::anyhow!("Error getting cipher mode from header")),
        };

        let header_type = HeaderType {
            version: version.clone(),
            algorithm,
            mode,
        };

        let nonce_len = calc_nonce_len(&header_type);
        let mut salt = [0u8; 16];
        let mut nonce = vec![0u8; nonce_len];

        let (master_key_encrypted, master_key_nonce): (Option<Vec<u8>>, Option<Vec<u8>>) =
            match header_type.version {
                HeaderVersion::V1 | HeaderVersion::V3 => {
                    cursor
                        .read_exact(&mut salt)
                        .context("Unable to read salt from header")?;
                    cursor
                        .read_exact(&mut [0; 16])
                        .context("Unable to read empty bytes from header")?;
                    cursor
                        .read_exact(&mut nonce)
                        .context("Unable to read nonce from header")?;
                    cursor
                        .read_exact(&mut vec![0u8; 26 - nonce_len])
                        .context("Unable to read final padding from header")?;

                    (None, None)
                }
                HeaderVersion::V2 => {
                    cursor
                        .read_exact(&mut salt)
                        .context("Unable to read salt from header")?;
                    cursor
                        .read_exact(&mut nonce)
                        .context("Unable to read nonce from header")?;
                    cursor
                        .read_exact(&mut vec![0u8; 26 - nonce_len])
                        .context("Unable to read empty bytes from header")?;
                    cursor
                        .read_exact(&mut [0u8; 16])
                        .context("Unable to read final padding from header")?;

                    (None, None)
                }
                HeaderVersion::V4 => {
                    let mut master_key_encrypted = vec![0u8; 48];
                    let master_key_nonce_len = calc_nonce_len(&HeaderType {
                        version,
                        algorithm,
                        mode: Mode::MemoryMode,
                    });
                    let mut master_key_nonce = vec![0u8; master_key_nonce_len];
                    cursor
                        .read_exact(&mut salt)
                        .context("Unable to read salt from header")?;
                    cursor
                        .read_exact(&mut nonce)
                        .context("Unable to read nonce from header")?;
                    cursor
                        .read_exact(&mut vec![0u8; 26 - nonce_len])
                        .context("Unable to read padding from header")?;
                    cursor
                        .read_exact(&mut master_key_encrypted)
                        .context("Unable to read encrypted master key from header")?;
                    cursor
                        .read_exact(&mut master_key_nonce)
                        .context("Unable to read master key nonce from header")?;
                    cursor
                        .read_exact(&mut vec![0u8; 32 - master_key_nonce_len])
                        .context("Unable to read padding from header")?;
                    (Some(master_key_encrypted), Some(master_key_nonce))
                }
            };

        let aad = match header_type.version {
            HeaderVersion::V1 | HeaderVersion::V2 => Vec::<u8>::new(),
            HeaderVersion::V3 => full_header_bytes.to_vec(),
            HeaderVersion::V4 => {
                let master_key_nonce_len = calc_nonce_len(&HeaderType {
                    version,
                    algorithm,
                    mode: Mode::MemoryMode,
                });
                let mut aad = Vec::new();

                // this is for the version/algorithm/mode/salt/nonce
                aad.extend_from_slice(&full_header_bytes[..48]);

                // this is for the padding that's appended to the end of the master key's nonce
                // the master key/master key nonce aren't included as they may change
                // the master key nonce length will be fixed, as otherwise the algorithm has changed
                // and that requires re-encrypting anyway
                aad.extend_from_slice(&full_header_bytes[(96 + master_key_nonce_len)..]);
                aad
            }
        };

        Ok((
            Header {
                header_type,
                nonce,
                salt,
                master_key_encrypted,
                master_key_nonce,
            },
            aad,
        ))
    }

    /// This is a private function used for serialization
    ///
    /// It converts an `Algorithm` into the associated raw bytes
    fn serialize_algorithm(&self) -> [u8; 2] {
        match self.header_type.algorithm {
            Algorithm::XChaCha20Poly1305 => {
                let info: [u8; 2] = [0x0E, 0x01];
                info
            }
            Algorithm::Aes256Gcm => {
                let info: [u8; 2] = [0x0E, 0x02];
                info
            }
            Algorithm::DeoxysII256 => {
                let info: [u8; 2] = [0x0E, 0x03];
                info
            }
        }
    }

    /// This is a private function used for serialization
    ///
    /// It converts a `Mode` into the associated raw bytes
    fn serialize_mode(&self) -> [u8; 2] {
        match self.header_type.mode {
            Mode::StreamMode => {
                let info: [u8; 2] = [0x0C, 0x01];
                info
            }
            Mode::MemoryMode => {
                let info: [u8; 2] = [0x0C, 0x02];
                info
            }
        }
    }

    /// This is a private function (called by `serialize()`)
    ///
    /// It serializes V3 headers
    fn serialize_v3(&self, tag: &HeaderTag) -> Vec<u8> {
        let padding = vec![0u8; 26 - calc_nonce_len(&self.header_type)];
        let mut header_bytes = Vec::<u8>::new();
        header_bytes.extend_from_slice(&tag.version);
        header_bytes.extend_from_slice(&tag.algorithm);
        header_bytes.extend_from_slice(&tag.mode);
        header_bytes.extend_from_slice(&self.salt);
        header_bytes.extend_from_slice(&[0; 16]);
        header_bytes.extend_from_slice(&self.nonce);
        header_bytes.extend_from_slice(&padding);
        header_bytes
    }

    fn serialize_v4(&self, tag: &HeaderTag) -> Vec<u8> {
        let padding = vec![0u8; 26 - calc_nonce_len(&self.header_type)];
        let padding2 = vec![
            0u8;
            32 - calc_nonce_len(&HeaderType {
                version: self.header_type.version,
                algorithm: self.header_type.algorithm,
                mode: Mode::MemoryMode
            })
        ];
        let mut header_bytes = Vec::<u8>::new();
        header_bytes.extend_from_slice(&tag.version);
        header_bytes.extend_from_slice(&tag.algorithm);
        header_bytes.extend_from_slice(&tag.mode);
        header_bytes.extend_from_slice(&self.salt);
        header_bytes.extend_from_slice(&self.nonce);
        header_bytes.extend_from_slice(&padding);
        header_bytes.extend_from_slice(&self.master_key_encrypted.clone().unwrap());
        header_bytes.extend_from_slice(&self.master_key_nonce.clone().unwrap());
        header_bytes.extend_from_slice(&padding2);
        header_bytes
    }

    /// This serializes a `Header` struct, and returns the raw bytes
    ///
    /// The returned bytes may be used as AAD, or written to a file
    ///
    /// NOTE: This should **NOT** be used for validating AAD, only creating it. Use the AAD returned from `deserialize()` for validation.
    ///
    /// # Examples
    ///
    /// ```
    /// let header_bytes = header.serialize().unwrap();
    /// ```
    ///
    pub fn serialize(&self) -> Result<Vec<u8>> {
        let tag = self.get_tag();
        match self.header_type.version {
            HeaderVersion::V1 => Err(anyhow::anyhow!(
                "Serializing V1 headers has been deprecated"
            )),
            HeaderVersion::V2 => Err(anyhow::anyhow!(
                "Serializing V2 headers has been deprecated"
            )),
            HeaderVersion::V3 => Ok(self.serialize_v3(&tag)),
            HeaderVersion::V4 => Ok(self.serialize_v4(&tag)),
        }
    }

    pub fn get_size(&self) -> u64 {
        match self.header_type.version {
            HeaderVersion::V1 | HeaderVersion::V2 | HeaderVersion::V3 => 64,
            HeaderVersion::V4 => 128,
        }
    }

    pub fn create_aad(&self) -> Result<Vec<u8>> {
        let tag = self.get_tag();
        match self.header_type.version {
            HeaderVersion::V1 => Err(anyhow::anyhow!(
                "Serializing V1 headers has been deprecated"
            )),
            HeaderVersion::V2 => Err(anyhow::anyhow!(
                "Serializing V2 headers has been deprecated"
            )),
            HeaderVersion::V3 => Ok(self.serialize_v3(&tag)),
            HeaderVersion::V4 => {
                let padding = vec![0u8; 26 - calc_nonce_len(&self.header_type)];
                let master_key_nonce_len = calc_nonce_len(&HeaderType {
                    version: self.header_type.version,
                    algorithm: self.header_type.algorithm,
                    mode: Mode::MemoryMode,
                });
                let padding2 = vec![0u8; 32 - master_key_nonce_len];
                let mut header_bytes = Vec::<u8>::new();
                header_bytes.extend_from_slice(&tag.version);
                header_bytes.extend_from_slice(&tag.algorithm);
                header_bytes.extend_from_slice(&tag.mode);
                header_bytes.extend_from_slice(&self.salt);
                header_bytes.extend_from_slice(&self.nonce);
                header_bytes.extend_from_slice(&padding);
                header_bytes.extend_from_slice(&padding2);
                Ok(header_bytes)
            }
        }
    }

    /// This is a convenience function for writing a header to a writer
    ///
    /// # Examples
    ///
    /// ```
    /// let mut output_file = File::create("test").unwrap();
    ///
    /// header.write(&mut output_file).unwrap();
    /// ```
    ///
    pub fn write(&self, writer: &mut impl Write) -> Result<()> {
        let header_bytes = self.serialize()?;
        writer
            .write(&header_bytes)
            .context("Unable to write header")?;

        Ok(())
    }
}
