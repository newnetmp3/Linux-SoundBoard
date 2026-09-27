use std::fs::File;
use std::io::Read;
use std::path::Path;

const PROBE_BYTES: usize = 16 * 1024;

pub fn validate_download(path: &Path) -> Result<(), String> {
    let metadata = path
        .metadata()
        .map_err(|error| format!("could not stat downloaded file: {error}"))?;
    if metadata.len() == 0 {
        return Err("downloaded file is empty".to_string());
    }

    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    let mut file =
        File::open(path).map_err(|error| format!("could not open downloaded file: {error}"))?;
    let mut bytes = vec![0_u8; PROBE_BYTES.min(metadata.len() as usize)];
    let read = file
        .read(&mut bytes)
        .map_err(|error| format!("could not inspect downloaded file: {error}"))?;
    bytes.truncate(read);

    match extension.as_str() {
        "mp3" => validate_mp3(&bytes),
        "wav" | "wave" => validate_wav(&bytes),
        "ogg" | "oga" | "opus" => starts_with(&bytes, b"OggS", "Ogg audio"),
        "flac" => starts_with(&bytes, b"fLaC", "FLAC audio"),
        "aif" | "aiff" => validate_aiff(&bytes),
        "aac" => validate_aac(&bytes),
        "m4a" | "mp4" => validate_mp4(&bytes),
        "zip" | "rpsb" => validate_zip(&bytes),
        _ => Ok(()),
    }
}

fn starts_with(bytes: &[u8], magic: &[u8], kind: &str) -> Result<(), String> {
    if bytes.starts_with(magic) {
        Ok(())
    } else {
        Err(format!("download does not contain valid {kind} data"))
    }
}

fn validate_mp3(bytes: &[u8]) -> Result<(), String> {
    if bytes.starts_with(b"ID3") {
        return Ok(());
    }

    if bytes
        .windows(2)
        .take(4096)
        .any(|pair| pair[0] == 0xff && pair[1] & 0xe0 == 0xe0)
    {
        Ok(())
    } else {
        Err("download does not contain an MP3 frame or ID3 header".to_string())
    }
}

fn validate_wav(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WAVE" {
        Ok(())
    } else {
        Err("download does not contain a RIFF/WAVE header".to_string())
    }
}

fn validate_aiff(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() >= 12
        && &bytes[..4] == b"FORM"
        && (&bytes[8..12] == b"AIFF" || &bytes[8..12] == b"AIFC")
    {
        Ok(())
    } else {
        Err("download does not contain an AIFF/AIFC header".to_string())
    }
}

fn validate_aac(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() >= 2 && bytes[0] == 0xff && bytes[1] & 0xf6 == 0xf0 {
        Ok(())
    } else {
        Err("download does not contain an AAC/ADTS frame".to_string())
    }
}

fn validate_mp4(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
        Ok(())
    } else {
        Err("download does not contain an MP4/M4A ftyp box".to_string())
    }
}

fn validate_zip(bytes: &[u8]) -> Result<(), String> {
    if bytes.starts_with(b"PK\x03\x04")
        || bytes.starts_with(b"PK\x05\x06")
        || bytes.starts_with(b"PK\x07\x08")
    {
        Ok(())
    } else {
        Err("download does not contain a ZIP archive header".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_common_audio_headers() {
        assert!(validate_mp3(b"ID3\x04\x00\x00").is_ok());
        assert!(validate_mp3(&[0xff, 0xfb, 0x90, 0x64]).is_ok());
        assert!(validate_wav(b"RIFF\x04\x00\x00\x00WAVE").is_ok());
        assert!(starts_with(b"OggSdata", b"OggS", "Ogg").is_ok());
        assert!(starts_with(b"fLaCdata", b"fLaC", "FLAC").is_ok());
        assert!(validate_mp4(b"\x00\x00\x00\x18ftypM4A ").is_ok());
    }

    #[test]
    fn rejects_html_saved_as_audio() {
        assert!(validate_mp3(b"<!doctype html><html>").is_err());
        assert!(validate_wav(b"<html>not a wav</html>").is_err());
    }
}
