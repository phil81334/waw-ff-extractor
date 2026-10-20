use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[path = "audio.rs"]
mod audio;

// The engine tolerates some premature EOFs. Check the complete zlib stream and
// checksum first, without keeping a second decompressed FF in memory.
pub(crate) fn validate_compressed_data(file: &mut File) -> Result<(), String> {
    use flate2::{Decompress, FlushDecompress, Status};

    let mut decoder = Decompress::new(true);
    let mut input = [0u8; 65_536];
    let mut output = [0u8; 65_536];
    let mut offset = 0;
    let mut length = 0;
    let mut eof = false;

    loop {
        if offset == length && !eof {
            length = file.read(&mut input).map_err(|error| error.to_string())?;
            offset = 0;
            eof = length == 0;
        }
        let previous_in = decoder.total_in();
        let previous_out = decoder.total_out();
        let flush = if eof {
            FlushDecompress::Finish
        } else {
            FlushDecompress::None
        };
        let status = decoder
            .decompress(&input[offset..length], &mut output, flush)
            .map_err(|_| "The FF's compressed data is damaged or incomplete.".to_string())?;
        offset += (decoder.total_in() - previous_in) as usize;

        if decoder.total_out() > u32::MAX as u64 {
            return Err("The decompressed FF exceeds the 32-bit format's size limit.".into());
        }
        if status == Status::StreamEnd {
            // Two sizes, seven block sizes and the 16-byte T4 asset-list header.
            if decoder.total_out() < 52 {
                return Err("The FF does not contain a complete zone header.".into());
            }
            return Ok(());
        }
        if decoder.total_in() == previous_in && decoder.total_out() == previous_out {
            if eof {
                return Err("The FF is truncated: its compressed data ended early.".into());
            }
            if offset < length {
                return Err("The FF contains invalid compressed data.".into());
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub input: PathBuf,
    pub output_root: Option<PathBuf>,
    pub types: Vec<String>,
    pub overwrite: Overwrite,
    #[serde(default)]
    pub convert_sounds: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Overwrite {
    Skip,
    Replace,
    Backup,
}

#[derive(Serialize)]
pub struct Report {
    pub written: usize,
    pub skipped: usize,
    pub destination: String,
    pub log: String,
}

fn safe_name(name: &str) -> bool {
    let device = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"];
    let numbered_device = device.len() == 4
        && (device.starts_with("COM") || device.starts_with("LPT"))
        && device.as_bytes()[3].is_ascii_digit();

    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.ends_with(['.', ' '])
        && !name
            .chars()
            .any(|character| character.is_control() || "<>:\"/\\|?*".contains(character))
        && !reserved.contains(&device.as_str())
        && !numbered_device
}

fn validate_relative(path: &Path) -> Result<(), String> {
    for component in path.components() {
        match component {
            Component::Normal(name) if name.to_str().is_some_and(safe_name) => {}
            _ => return Err(format!("Unsafe output filename: {}", path.display())),
        }
    }
    Ok(())
}

// Junctions and symbolic links can redirect a harmless-looking output path.
// Refuse them anywhere along an existing output path, not just at its final file.
fn check_existing_path(path: &Path) -> Result<(), String> {
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) => {
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    if metadata.file_attributes() & 0x400 != 0 {
                        return Err(format!(
                            "Output uses a link or junction: {}",
                            ancestor.display()
                        ));
                    }
                }
                if metadata.file_type().is_symlink() {
                    return Err(format!(
                        "Output uses a symbolic link: {}",
                        ancestor.display()
                    ));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(())
}

fn collect_files(folder: &Path, root: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(folder).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let kind = entry.file_type().map_err(|error| error.to_string())?;
        if kind.is_symlink() {
            return Err("The extraction engine produced a symbolic link.".into());
        }
        if kind.is_dir() {
            collect_files(&entry.path(), root, files)?;
        } else if kind.is_file() {
            let relative = entry
                .path()
                .strip_prefix(root)
                .map_err(|error| error.to_string())?
                .to_path_buf();
            validate_relative(&relative)?;
            files.push(relative);
        }
    }
    Ok(())
}

// Keep Windows' extended-length paths for filesystem operations, not display.
fn display_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    text.strip_prefix(r"\\?\").unwrap_or(&text).to_string()
}

fn replace_folder(
    destination: &Path,
    staging: &Path,
    files: &[PathBuf],
) -> Result<Option<PathBuf>, String> {
    check_existing_path(destination)?;
    if destination.exists() && !destination.is_dir() {
        return Err("A file occupies the output folder.".into());
    }
    let parent = destination.parent().ok_or("Output folder has no parent.")?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;

    // Finish copying on the destination drive before touching the old folder.
    let pending = tempfile::Builder::new()
        .prefix(".waw-ff-ready-")
        .tempdir_in(parent)
        .map_err(|error| error.to_string())?;
    let ready = pending.path().join("assets");
    fs::create_dir(&ready).map_err(|error| error.to_string())?;
    for relative in files {
        let target = ready.join(relative);
        fs::create_dir_all(target.parent().unwrap()).map_err(|error| error.to_string())?;
        fs::copy(staging.join(relative), &target).map_err(|error| error.to_string())?;
    }

    let mut backup = None;
    if destination.exists() {
        for index in 1..=100_000 {
            let mut name = destination.as_os_str().to_os_string();
            name.push(format!(".backup-{index}"));
            let candidate = PathBuf::from(name);
            if fs::symlink_metadata(&candidate).is_ok() {
                continue;
            }
            check_existing_path(&candidate)?;
            // On Windows rename refuses to replace an existing directory.
            fs::rename(destination, &candidate).map_err(|error| error.to_string())?;
            backup = Some(candidate);
            break;
        }
        if backup.is_none() {
            return Err("Too many backup folders already exist.".into());
        }
    }

    if let Err(error) = fs::rename(&ready, destination) {
        if let Some(previous) = &backup {
            if let Err(restore_error) = fs::rename(previous, destination) {
                return Err(format!("Could not install the new folder: {error}. The old folder remains at {}. Restore failed: {restore_error}", display_path(previous)));
            }
        }
        return Err(format!(
            "Could not install the new folder: {error}. The old folder was preserved."
        ));
    }
    Ok(backup)
}

// Logs are app-generated reports, never extracted assets. Use a new filename
// each run so neither an earlier log nor an FF asset can be overwritten.
fn save_log(
    destination: &Path,
    input: &Path,
    request: &Request,
    report: &mut Report,
) -> Result<(), String> {
    check_existing_path(destination)?;
    fs::create_dir_all(destination).map_err(|error| error.to_string())?;
    let mode = match request.overwrite {
        Overwrite::Skip => "Skip existing files (contents are not compared)",
        Overwrite::Replace => "Replace matching files (other existing files remain)",
        Overwrite::Backup => "Back up whole folder, then create fresh output",
    };
    let contents = format!(
        "WaW FF Extractor — extraction log\nSource: {}\nSaved to: {}\nMode: {}\nTypes: {}\nSaved assets: {}\nSkipped assets: {}\n\nThis report was generated by WaW FF Extractor. Other extracted text files, including files named after the FF, may contain old compiler errors stored inside the FF. Those messages are not errors from this extraction.\n\n{}",
        display_path(input), report.destination, mode, request.types.join(", "),
        report.written, report.skipped, report.log
    );
    for index in 0..100_000 {
        let name = if index == 0 {
            "waw-ff-extractor-log.txt".to_string()
        } else {
            format!("waw-ff-extractor-log-{index}.txt")
        };
        let path = destination.join(name);
        if fs::symlink_metadata(&path).is_ok() {
            continue;
        }
        check_existing_path(&path)?;
        match File::options().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                file.write_all(contents.as_bytes())
                    .map_err(|error| error.to_string())?;
                file.sync_all().map_err(|error| error.to_string())?;
                report
                    .log
                    .push_str(&format!("\nExtraction log: {}\n", display_path(&path)));
                return Ok(());
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                continue;
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    Err("Too many extraction logs in the output folder.".into())
}

pub fn run(request: Request) -> Result<Report, String> {
    if request.types.is_empty()
        || request
            .types
            .iter()
            .any(|kind| !["rawfile", "loadedsound", "stringtable"].contains(&kind.as_str()))
    {
        return Err("Choose raw files, embedded sounds or CSV tables.".into());
    }

    let input =
        fs::canonicalize(&request.input).map_err(|error| format!("Cannot open FF: {error}"))?;
    let stem = input
        .file_stem()
        .and_then(|name| name.to_str())
        .ok_or("Invalid FF filename.")?;
    if !safe_name(stem)
        || input
            .extension()
            .and_then(|name| name.to_str())
            .is_none_or(|name| !name.eq_ignore_ascii_case("ff"))
    {
        return Err("Choose a .ff file with a valid Windows filename.".into());
    }

    let mut header = [0u8; 12];
    let mut file = File::open(&input).map_err(|error| error.to_string())?;
    file.read_exact(&mut header)
        .map_err(|error| format!("Cannot read FF header: {error}"))?;
    if &header[..8] != b"IWffu100" || u32::from_le_bytes(header[8..12].try_into().unwrap()) != 387 {
        return Err("This is not a supported PC World at War FF (version 387).".into());
    }
    validate_compressed_data(&mut file)?;

    let parent = request
        .output_root
        .as_deref()
        .unwrap_or_else(|| input.parent().unwrap());
    if !parent.is_absolute() {
        return Err("The output folder must be an absolute path.".into());
    }
    let destination = parent.join(stem);
    check_existing_path(&destination)?;

    // The parser runs separately and writes only into a disposable staging folder.
    // A failed extraction never copies partial engine output into the user's folder.
    let temporary = tempfile::Builder::new()
        .prefix("waw-ff-extract-")
        .tempdir()
        .map_err(|error| error.to_string())?;
    let engine = temporary.path().join("Unlinker.exe");
    fs::write(&engine, include_bytes!("../../engine/Unlinker.exe"))
        .map_err(|error| error.to_string())?;
    let staging = temporary.path().join("assets");
    fs::create_dir(&staging).map_err(|error| error.to_string())?;
    let log_path = temporary.path().join("engine.log");
    let log_file = File::create(&log_path).map_err(|error| error.to_string())?;
    let mut command = Command::new(&engine);
    command
        .args([
            "--skip-obj",
            "--include-assets",
            &request.types.join(","),
            "-o",
        ])
        .arg(&staging)
        .arg(&input)
        .current_dir(temporary.path())
        .env_remove("OAT_STUDY_EXPORT")
        .env_remove("OAT_STUDY_WORLD_ONLY")
        .stdin(Stdio::null())
        .stderr(log_file.try_clone().map_err(|error| error.to_string())?)
        .stdout(log_file);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }

    let mut child = command
        .spawn()
        .map_err(|error| format!("Cannot start the extraction engine: {error}"))?;
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            break status;
        }
        if started.elapsed() > Duration::from_secs(300) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(
                "Extraction stopped after five minutes. No output files were copied.".into(),
            );
        }
        std::thread::sleep(Duration::from_millis(100));
    };

    let mut log = String::from_utf8_lossy(&fs::read(&log_path).map_err(|error| error.to_string())?)
        .into_owned();
    // Read the engine's final error count. Asset names can contain "error",
    // so searching every log line for that word would reject valid FFs.
    let finished_cleanly = log
        .lines()
        .rev()
        .find(|line| line.starts_with("Finished with ") || line.starts_with("Failed with "))
        .is_some_and(|line| line.starts_with("Finished with ") && line.ends_with(", 0 errors"));
    if !status.success() || !finished_cleanly {
        return Err(format!(
            "Extraction failed. No output files were copied.\n{log}"
        ));
    }

    let mut files = Vec::new();
    collect_files(&staging, &staging, &mut files)?;
    files.sort();

    if request.convert_sounds && request.types.iter().any(|kind| kind == "loadedsound") {
        log.push_str(&audio::convert(&staging, &files, temporary.path())?);
        files.clear();
        collect_files(&staging, &staging, &mut files)?;
        files.sort();
    }

    if matches!(request.overwrite, Overwrite::Backup) {
        let backup = replace_folder(&destination, &staging, &files)?;
        let mut log = log;
        if let Some(path) = backup {
            log.push_str(&format!(
                "\nPrevious output folder: {}\n",
                display_path(&path)
            ));
        }
        let mut report = Report {
            written: files.len(),
            skipped: 0,
            destination: display_path(&destination),
            log,
        };
        save_log(&destination, &input, &request, &mut report)?;
        return Ok(report);
    }

    // Check every destination before writing any files. Directory/file conflicts
    // are errors, rather than an excuse to remove an existing directory.
    for relative in &files {
        let target = destination.join(relative);
        check_existing_path(&target)?;
        if target.is_dir() {
            return Err(format!(
                "A folder occupies an output filename: {}",
                target.display()
            ));
        }
        for ancestor in target.parent().unwrap().ancestors() {
            if ancestor.is_file() {
                return Err(format!(
                    "A file occupies an output folder: {}",
                    ancestor.display()
                ));
            }
        }
    }

    let mut report = Report {
        written: 0,
        skipped: 0,
        destination: display_path(&destination),
        log,
    };
    for relative in files {
        let target = destination.join(&relative);
        check_existing_path(&target)?;
        if target.exists() && matches!(request.overwrite, Overwrite::Skip) {
            report.skipped += 1;
            continue;
        }
        let folder = target.parent().unwrap();
        fs::create_dir_all(folder).map_err(|error| {
            format!(
                "Could not create {}: {error}. {} files already saved.",
                folder.display(),
                report.written
            )
        })?;
        let mut pending =
            tempfile::NamedTempFile::new_in(folder).map_err(|error| error.to_string())?;
        let mut source = File::open(staging.join(&relative)).map_err(|error| error.to_string())?;
        std::io::copy(&mut source, pending.as_file_mut()).map_err(|error| error.to_string())?;
        pending.flush().map_err(|error| error.to_string())?;
        pending
            .as_file()
            .sync_all()
            .map_err(|error| error.to_string())?;

        if matches!(request.overwrite, Overwrite::Skip) {
            match pending.persist_noclobber(&target) {
                Ok(_) => report.written += 1,
                Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                    report.skipped += 1
                }
                Err(error) => return Err(error.to_string()),
            }
        } else {
            pending.persist(&target).map_err(|error| {
                format!(
                    "Could not save {}: {error}. {} files already saved.",
                    target.display(),
                    report.written
                )
            })?;
            report.written += 1;
        }
    }
    save_log(&destination, &input, &request, &mut report)?;
    Ok(report)
}
