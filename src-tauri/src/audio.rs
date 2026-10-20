use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn is_xwma(path: &Path) -> Result<bool, String> {
    let mut header = [0u8; 12];
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    if file.metadata().map_err(|error| error.to_string())?.len() < 12 {
        return Ok(false);
    }
    file.read_exact(&mut header)
        .map_err(|error| error.to_string())?;
    Ok(&header[..4] == b"RIFF" && &header[8..] == b"XWMA")
}

// These two WaW WMAv2 presets advertise 96 kbit/s, but their packets use the
// 20 kbit/s decoder settings. XWMA omits WMA's codec extra data. FFmpeg supplies
// defaults, which do not cover these variants. Supply the settings only on a
// disposable conversion input; the extracted XWMA is never rewritten.
fn conversion_input(source: &Path, temporary: &Path) -> Result<(PathBuf, bool), String> {
    let bytes = fs::read(source).map_err(|error| error.to_string())?;
    if bytes.len() < 38 || &bytes[12..16] != b"fmt " {
        return Ok((source.to_path_buf(), false));
    }
    let size = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
    let codec = u16::from_le_bytes(bytes[20..22].try_into().unwrap());
    let channels = u16::from_le_bytes(bytes[22..24].try_into().unwrap());
    let rate = u32::from_le_bytes(bytes[24..28].try_into().unwrap());
    let average_bytes = u32::from_le_bytes(bytes[28..32].try_into().unwrap());
    let alignment = u16::from_le_bytes(bytes[32..34].try_into().unwrap());
    let extra_size = u16::from_le_bytes(bytes[36..38].try_into().unwrap());
    let known_preset =
        (rate == 22_050 && alignment == 929) || (rate == 32_000 && alignment == 1280);
    if size != 18
        || codec != 0x161
        || channels != 1
        || average_bytes != 12_000
        || extra_size != 0
        || !known_preset
    {
        return Ok((source.to_path_buf(), false));
    }

    let flags = if rate == 32_000 { 23 } else { 31 };
    let mut adjusted = Vec::with_capacity(bytes.len() + 6);
    adjusted.extend_from_slice(&bytes[..38]);
    adjusted.extend_from_slice(&[0, 0, 0, 0, flags, 0]);
    adjusted.extend_from_slice(&bytes[38..]);
    let riff_size = u32::try_from(adjusted.len() - 8).map_err(|_| "XWMA is too large.")?;
    adjusted[4..8].copy_from_slice(&riff_size.to_le_bytes());
    adjusted[16..20].copy_from_slice(&24u32.to_le_bytes());
    adjusted[28..32].copy_from_slice(&2500u32.to_le_bytes());
    adjusted[36..38].copy_from_slice(&6u16.to_le_bytes());
    let path = temporary.join("audio-input.xwma");
    fs::write(&path, adjusted).map_err(|error| error.to_string())?;
    Ok((path, true))
}

// Check the actual encoding, not just the filename or converter exit code.
fn validate_wav(path: &Path) -> Result<(), String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("The converter did not produce a WAV file.".into());
    }
    let mut offset = 12usize;
    let mut format_ok = false;
    let mut has_audio = false;
    while offset + 8 <= bytes.len() {
        let length = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
        let start = offset + 8;
        let end = start.checked_add(length).ok_or("Invalid WAV chunk size.")?;
        if end > bytes.len() {
            return Err("The converted WAV is truncated.".into());
        }
        if &bytes[offset..offset + 4] == b"fmt " && length >= 16 {
            let tag = u16::from_le_bytes(bytes[start..start + 2].try_into().unwrap());
            let channels = u16::from_le_bytes(bytes[start + 2..start + 4].try_into().unwrap());
            let rate = u32::from_le_bytes(bytes[start + 4..start + 8].try_into().unwrap());
            let bits = u16::from_le_bytes(bytes[start + 14..start + 16].try_into().unwrap());
            format_ok = tag == 2 && channels == 2 && rate == 44_100 && bits == 4;
        }
        if &bytes[offset..offset + 4] == b"data" && length > 0 {
            has_audio = true;
        }
        offset = end + (length & 1);
    }
    if !format_ok || !has_audio {
        return Err(
            "The converter did not produce 44.1 kHz stereo 4-bit Microsoft ADPCM audio.".into(),
        );
    }
    Ok(())
}

pub fn convert(staging: &Path, files: &[PathBuf], temporary: &Path) -> Result<String, String> {
    let mut sources = Vec::new();
    for relative in files {
        if relative.starts_with("sound") && is_xwma(&staging.join(relative))? {
            sources.push(relative);
        }
    }
    if sources.is_empty() {
        return Ok("\nWAV conversion: no embedded XWMA sounds found.\n".into());
    }

    // A generated folder must never occupy an extracted asset's path.
    let mut folder = PathBuf::from("converted-wav");
    let mut suffix = 1;
    while staging.join(&folder).exists() {
        folder = PathBuf::from(format!("converted-wav-{suffix}"));
        suffix += 1;
    }
    let converter = temporary.join("waw-audio.exe");
    fs::write(&converter, include_bytes!("../../engine/waw-audio.exe"))
        .map_err(|error| error.to_string())?;
    let mut log = String::from(
        "\nOptional WAV copies: 44100 Hz, stereo, 4-bit Microsoft ADPCM. Originals preserved.\n",
    );

    for relative in &sources {
        let target_relative = folder.join(relative).with_extension("wav");
        let target = staging.join(&target_relative);
        if target.exists() {
            return Err(format!(
                "Two sounds would use the same converted filename: {}",
                target_relative.display()
            ));
        }
        fs::create_dir_all(target.parent().unwrap()).map_err(|error| error.to_string())?;
        let diagnostic_path = temporary.join("audio-conversion.log");
        let diagnostics = File::create(&diagnostic_path).map_err(|error| error.to_string())?;
        let (input, adjusted) = conversion_input(&staging.join(relative), temporary)?;
        let mut command = Command::new(&converter);
        command
            .args([
                "-hide_banner",
                "-nostdin",
                "-loglevel",
                "warning",
                "-xerror",
                "-n",
                "-i",
            ])
            .arg(input)
            .args([
                "-map",
                "0:a:0",
                "-vn",
                "-map_metadata",
                "-1",
                "-af",
                "aresample=44100,asetpts=N/SR/TB",
                "-ar",
                "44100",
                "-ac",
                "2",
                "-c:a",
                "adpcm_ms",
                "-f",
                "wav",
            ])
            .arg(&target)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(diagnostics);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command
            .spawn()
            .map_err(|error| format!("Cannot start WAV converter: {error}"))?;
        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
                break status;
            }
            if started.elapsed() > Duration::from_secs(60) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "WAV conversion timed out for {}. No output files were copied.",
                    relative.display()
                ));
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let diagnostics =
            fs::read_to_string(&diagnostic_path).map_err(|error| error.to_string())?;
        if !status.success() {
            return Err(format!(
                "WAV conversion failed for {}. No output files were copied.\n{diagnostics}",
                relative.display()
            ));
        }
        validate_wav(&target)?;
        if adjusted {
            log.push_str(&format!(
                "Used WaW XWMA decoder settings for {} (original unchanged).\n",
                relative.display()
            ));
        }
        log.push_str(&format!(
            "Converted {} -> {}\n",
            relative.display(),
            target_relative.display()
        ));
        if !diagnostics.trim().is_empty() {
            log.push_str(&diagnostics);
        }
    }
    log.push_str(&format!(
        "Created {} WAV copies in {}.\n",
        sources.len(),
        folder.display()
    ));
    Ok(log)
}
