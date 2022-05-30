use std::{
    fs::File,
    io::{Read, Write},
    path::PathBuf,
    str::FromStr,
    time::Instant,
};

use anyhow::{Context, Result};
use paris::Logger;
use rand::distributions::{Alphanumeric, DistString};
use zip::write::FileOptions;

use crate::{
    file::get_paths_in_dir,
    global::enums::{Algorithm, DirectoryMode, HeaderFile, PrintMode, SkipMode},
    global::structs::{CryptoParams, PackMode},
    global::BLOCK_SIZE,
    prompt::get_answer,
};

// this first indexes the input directory
// once it has the total number of files/folders, it creates a temporary zip file
// it compresses all of the files into the temporary archive
// once compressed, it encrypts the zip file
// it erases the temporary archive afterwards, to stop any residual data from remaining
#[allow(clippy::too_many_lines)]
pub fn encrypt_directory(
    input: &str,
    output: &str,
    pack_params: &PackMode,
    params: &CryptoParams,
    algorithm: Algorithm,
) -> Result<()> {
    let mut logger = Logger::new();

    if pack_params.dir_mode == DirectoryMode::Recursive {
        logger.loading(format!("Traversing {} recursively", input));
    } else {
        logger.loading(format!("Traversing {}", input));
    }

    let index_start_time = Instant::now();
    let (files, dirs) = get_paths_in_dir(
        input,
        pack_params.dir_mode,
        &pack_params.exclude,
        &pack_params.hidden,
        &pack_params.print_mode,
    )?;
    let index_duration = index_start_time.elapsed();
    let file_count = files.len();
    logger.done().success(format!(
        "Indexed {} files [took {:.2}s]",
        file_count,
        index_duration.as_secs_f32()
    ));

    let random_extension: String = Alphanumeric.sample_string(&mut rand::thread_rng(), 8);
    let tmp_name = format!("{}.{}", output, random_extension); // e.g. "output.kjHSD93l"

    let file = std::io::BufWriter::new(
        File::create(&tmp_name)
            .with_context(|| format!("Unable to create the output file: {}", output))?,
    );

    logger.loading(format!("Creating and compressing files into {}", tmp_name));

    let zip_start_time = Instant::now();

    let mut zip = zip::ZipWriter::new(file);
    let options = FileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .large_file(true)
        .unix_permissions(0o755);

    zip.add_directory(input, options)
        .context("Unable to add directory to zip")?;

    if pack_params.dir_mode == DirectoryMode::Recursive {
        let directories = dirs.context("Error unwrapping Vec containing list of directories.")?; // this should always be *something* anyway
        for dir in directories {
            zip.add_directory(
                dir.to_str()
                    .context("Error converting directory path to string")?,
                options,
            )
            .context("Unable to add directory to zip")?;
        }
    }

    for file in files {
        zip.start_file(
            file.to_str()
                .context("Error converting file path to string")?,
            options,
        )
        .context("Unable to add file to zip")?;

        if pack_params.print_mode == PrintMode::Verbose {
            logger.info(format!(
                "Compressing {} into {}",
                file.to_str().unwrap(),
                tmp_name
            ));
        }

        let zip_writer = zip.by_ref();
        let mut file_reader = File::open(file)?;
        let file_size = file_reader.metadata().unwrap().len();

        if file_size <= BLOCK_SIZE.try_into().unwrap() {
            let mut data = Vec::new();
            file_reader.read_to_end(&mut data)?;
            zip_writer.write_all(&data)?;
        } else {
            // stream read/write here
            let mut buffer = [0u8; BLOCK_SIZE];

            loop {
                let read_count = file_reader.read(&mut buffer)?;
                if read_count == BLOCK_SIZE {
                    zip_writer
                        .write_all(&buffer[..read_count])
                        .with_context(|| {
                            format!("Unable to write to the output file: {}", output)
                        })?;
                } else {
                    zip_writer
                        .write_all(&buffer[..read_count])
                        .with_context(|| {
                            format!("Unable to write to the output file: {}", output)
                        })?;
                    break;
                }
            }
        }
    }
    zip.finish()?;
    drop(zip);

    let zip_duration = zip_start_time.elapsed();
    logger.done().success(format!(
        "Compressed {} files into {}! [took {:.2}s]",
        file_count,
        tmp_name,
        zip_duration.as_secs_f32()
    ));

    crate::encrypt::stream_mode(&tmp_name, output, params, algorithm)?;

    crate::erase::secure_erase(&tmp_name, 2)?; // cleanup our tmp file

    logger.success(format!("Your output file is: {}", output));

    Ok(())
}

// this first decrypts the input file to a temporary zip file
// it then unpacks that temporary zip file to the target directory
// once finished, it erases the temporary file to avoid any residual data
pub fn decrypt_directory(
    input: &str,         // encrypted zip file
    output: &str,        // directory
    header: &HeaderFile, // for decrypt function
    print_mode: &PrintMode,
    params: &CryptoParams, // params for decrypt function
) -> Result<()> {
    let mut logger = Logger::new();
    let random_extension: String = Alphanumeric.sample_string(&mut rand::thread_rng(), 8);

    // this is the name of the decrypted zip file
    let tmp_name = format!("{}.{}", input, random_extension); // e.g. "input.kjHSD93l"

    crate::decrypt::stream_mode(input, &tmp_name, header, params)?;

    let zip_start_time = Instant::now();
    let file = File::open(&tmp_name).context("Unable to open temporary archive")?;
    let mut archive = zip::ZipArchive::new(file)
        .context("Temporary archive can't be opened, is it a zip file?")?;

    match std::fs::create_dir(output) {
        Ok(_) => logger.info(format!("Created output directory: {}", output)),
        Err(_) => logger.warn(format!("Output directory ({}) already exists!", output)),
    };

    let file_count = archive.len();

    logger.loading(format!(
        "Decompressing {} items into {}",
        file_count, output
    ));

    for i in 0..file_count {
        let mut full_path = PathBuf::from_str(output)
            .context("Unable to create a PathBuf from your output directory")?;

        let mut file = archive.by_index(i).context("Unable to index the archive")?;
        match file.enclosed_name() {
            Some(path) => full_path.push(path),
            None => continue,
        };

        if file.name().contains("..") {
            // skip directories that may try to zip slip
            continue;
        }

        if file.is_dir() {
            // if it's a directory, recreate the structure
            std::fs::create_dir_all(full_path).context("Unable to create an output directory")?;
        } else {
            // this must be a file
            let file_name: String = full_path
                .clone()
                .file_name()
                .context("Unable to convert file name to OsStr")?
                .to_str()
                .context("Unable to convert file name's OsStr to &str")?
                .to_string();
            if std::fs::metadata(full_path.clone()).is_ok() {
                let answer = get_answer(
                    &format!("{} already exists, would you like to overwrite?", file_name),
                    true,
                    params.skip == SkipMode::HidePrompts,
                )?;
                if !answer {
                    logger.warn(format!("Skipping {}", file_name));
                    continue;
                }
            }
            if print_mode == &PrintMode::Verbose {
                logger.info(format!("Extracting {}", file_name));
            }
            let mut output_file =
                File::create(full_path).context("Error creating an output file")?;
            std::io::copy(&mut file, &mut output_file)
                .context("Error copying data out of archive to the target file")?;
        }
    }

    let zip_duration = zip_start_time.elapsed();
    logger.done().success(format!(
        "Extracted {} items to {} [took {:.2}s]",
        file_count,
        output,
        zip_duration.as_secs_f32()
    ));

    crate::erase::secure_erase(&tmp_name, 2)?; // cleanup the tmp file

    logger.success(format!(
        "Unpacking Successful! You will find your files in {}",
        output
    ));

    Ok(())
}
