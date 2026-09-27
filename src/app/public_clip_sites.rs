use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

use crate::download_validation::validate_download;
use crate::myinstants::{DownloadProgress, DownloadReport};

const USER_AGENT: &str =
    concat!("Linux-SoundBoard/", env!("CARGO_PKG_VERSION"), " public clip downloader");
const MAX_FILE_STEM_BYTES: usize = 180;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublicClipSource {
    SoundButtonsCom,
    MovieSoundClips,
    MyInstantsCom,
}

impl PublicClipSource {
    pub fn name(self) -> &'static str {
        match self {
            Self::SoundButtonsCom => "Sound-Buttons.com",
            Self::MovieSoundClips => "Movie Sound Clips",
            Self::MyInstantsCom => "My-Instants.com",
        }
    }

    pub fn folder_name(self) -> &'static str {
        self.name()
    }

    pub fn browse_url(self) -> &'static str {
        match self {
            Self::SoundButtonsCom => "https://www.sound-buttons.com/popular-sound-buttons",
            Self::MovieSoundClips => "https://www.moviesoundclips.net/sound-effects.php",
            Self::MyInstantsCom => "https://my-instants.com/trending",
        }
    }
}

pub const PUBLIC_CLIP_SOURCES: &[(&str, PublicClipSource)] = &[
    ("Sound-Buttons.com — popular meme/reaction clips", PublicClipSource::SoundButtonsCom),
    ("Movie Sound Clips — CC BY-NC sound-effects library", PublicClipSource::MovieSoundClips),
    ("My-Instants.com — trending meme/reaction clips", PublicClipSource::MyInstantsCom),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicClip {
    pub source_name: String,
    pub source_url: String,
    pub title: String,
    pub creator: Option<String>,
    pub media_url: String,
    pub preview_url: Option<String>,
    pub license: Option<String>,
    pub usage_terms: Option<String>,
    pub attribution: Option<String>,
    pub terms_url: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum PublicClipError {
    #[error("curl is required for public clip downloads")]
    CurlMissing,
    #[error("failed to create source download directory: {0}")]
    CreateDirectory(#[source] io::Error),
    #[error("{provider} request failed for {url}: {message}")]
    Request {
        provider: &'static str,
        url: String,
        message: String,
    },
    #[error("could not parse {provider}: {message}")]
    Parse {
        provider: &'static str,
        message: String,
    },
    #[error("downloaded file failed validation: {0}")]
    InvalidDownload(String),
    #[error("failed to write source metadata: {0}")]
    Metadata(#[source] io::Error),
}

#[derive(Debug, Clone)]
struct Anchor {
    href: String,
    text: String,
}

pub fn browse(
    source: PublicClipSource,
    query: &str,
    limit: usize,
    progress: &Sender<DownloadProgress>,
    cancelled: &AtomicBool,
) -> Result<Vec<PublicClip>, PublicClipError> {
    ensure_curl()?;
    let limit = limit.clamp(1, 250);
    send_progress(
        progress,
        &format!("Browsing {}…", source.name()),
        0,
        None,
    );

    let mut clips = match source {
        PublicClipSource::SoundButtonsCom => browse_sound_buttons(query, limit, cancelled)?,
        PublicClipSource::MovieSoundClips => browse_movie_sound_clips(query, limit, cancelled)?,
        PublicClipSource::MyInstantsCom => {
            browse_my_instants_com(query, limit, progress, cancelled)?
        }
    };
    clips.truncate(limit);
    Ok(clips)
}

pub fn download_selected(
    source: PublicClipSource,
    clips: &[PublicClip],
    output_root: &Path,
    progress: Sender<DownloadProgress>,
    cancelled: Arc<AtomicBool>,
) -> Result<DownloadReport, PublicClipError> {
    ensure_curl()?;
    let output_dir = output_root.join(source.folder_name());
    fs::create_dir_all(&output_dir).map_err(PublicClipError::CreateDirectory)?;

    let mut report = DownloadReport::default();
    let total = clips.len();

    for (index, clip) in clips.iter().enumerate() {
        if cancelled.load(Ordering::Relaxed) {
            report.cancelled = true;
            break;
        }

        send_progress(
            &progress,
            &format!(
                "Downloading {}… {}/{}",
                source.name(),
                index + 1,
                total
            ),
            index,
            Some(total),
        );

        let extension = extension_from_url(&clip.media_url).unwrap_or("mp3");
        let target = choose_target_path(&output_dir, clip, extension);
        match download_clip(source.name(), &clip.media_url, &target) {
            Ok(true) => report.downloaded += 1,
            Ok(false) => report.reused += 1,
            Err(error) => {
                report.failed += 1;
                log::warn!(
                    "{} download failed for '{}': {}",
                    source.name(),
                    clip.title,
                    error
                );
                continue;
            }
        }

        write_metadata_sidecar(&target, clip)?;
        report.paths.push(target.to_string_lossy().into_owned());
    }

    report.paths.sort();
    report.paths.dedup();
    send_progress(
        &progress,
        &format!(
            "{} complete: {} downloaded, {} reused, {} failed",
            source.name(),
            report.downloaded,
            report.reused,
            report.failed
        ),
        total,
        Some(total),
    );
    Ok(report)
}

pub fn download_matching(
    source: PublicClipSource,
    query: &str,
    limit: usize,
    output_root: &Path,
    progress: Sender<DownloadProgress>,
    cancelled: Arc<AtomicBool>,
) -> Result<DownloadReport, PublicClipError> {
    let clips = browse(source, query, limit, &progress, &cancelled)?;
    download_selected(source, &clips, output_root, progress, cancelled)
}

fn browse_sound_buttons(
    query: &str,
    limit: usize,
    cancelled: &AtomicBool,
) -> Result<Vec<PublicClip>, PublicClipError> {
    let page_url = PublicClipSource::SoundButtonsCom.browse_url();
    let html = fetch_text("Sound-Buttons.com", page_url)?;
    let anchors = parse_anchors(&html);
    let mut clips = Vec::new();
    let mut seen = HashSet::new();

    for (index, anchor) in anchors.iter().enumerate() {
        if cancelled.load(Ordering::Relaxed) {
            break;
        }
        if !anchor.href.contains("/download/") || !url_has_audio_extension(&anchor.href) {
            continue;
        }
        let Some(detail) = anchors[..index]
            .iter()
            .rev()
            .take(8)
            .find(|candidate| candidate.href.contains("/sound-button/") && !candidate.text.is_empty())
        else {
            continue;
        };
        if !matches_query(&detail.text, query) {
            continue;
        }

        let media_url = absolute_url(page_url, &anchor.href).unwrap_or_else(|| anchor.href.clone());
        if !seen.insert(media_url.clone()) {
            continue;
        }

        clips.push(PublicClip {
            source_name: "Sound-Buttons.com".to_string(),
            source_url: absolute_url(page_url, &detail.href)
                .unwrap_or_else(|| detail.href.clone()),
            title: detail.text.clone(),
            creator: None,
            media_url: media_url.clone(),
            preview_url: Some(media_url),
            license: None,
            usage_terms: Some(
                "The site states its sound buttons are free to play, download, and share; no per-clip content license was exposed on the validated page."
                    .to_string(),
            ),
            attribution: None,
            terms_url: Some("https://www.sound-buttons.com/disclaimer".to_string()),
        });
        if clips.len() >= limit {
            break;
        }
    }

    if clips.is_empty() {
        return Err(PublicClipError::Parse {
            provider: "Sound-Buttons.com",
            message: "no public MP3 download links were found on the popular page".to_string(),
        });
    }
    Ok(clips)
}

fn browse_movie_sound_clips(
    query: &str,
    limit: usize,
    cancelled: &AtomicBool,
) -> Result<Vec<PublicClip>, PublicClipError> {
    let index_url = PublicClipSource::MovieSoundClips.browse_url();
    let index_html = fetch_text("Movie Sound Clips", index_url)?;
    let anchors = parse_anchors(&index_html);
    let mut pages = vec![index_url.to_string()];

    for anchor in anchors {
        if anchor.href.contains("effects/")
            && !url_has_audio_extension(&anchor.href)
            && !anchor.href.contains("creativecommons")
        {
            if let Some(url) = absolute_url(index_url, &anchor.href) {
                if !pages.contains(&url) {
                    pages.push(url);
                }
            }
        }
    }

    let mut clips = Vec::new();
    let mut seen = HashSet::new();
    for page_url in pages {
        if cancelled.load(Ordering::Relaxed) || clips.len() >= limit {
            break;
        }
        let html = fetch_text("Movie Sound Clips", &page_url)?;
        for anchor in parse_anchors(&html) {
            if cancelled.load(Ordering::Relaxed) || clips.len() >= limit {
                break;
            }
            if !url_has_audio_extension(&anchor.href) || anchor.text.trim().is_empty() {
                continue;
            }
            let media_url =
                absolute_url(&page_url, &anchor.href).unwrap_or_else(|| anchor.href.clone());
            if !seen.insert(media_url.clone()) || !matches_query(&anchor.text, query) {
                continue;
            }

            clips.push(PublicClip {
                source_name: "Movie Sound Clips".to_string(),
                source_url: page_url.clone(),
                title: anchor.text.clone(),
                creator: Some("Moviesoundclips.net".to_string()),
                media_url: media_url.clone(),
                preview_url: Some(media_url),
                license: Some("Creative Commons Attribution-Noncommercial 3.0".to_string()),
                usage_terms: Some(
                    "The site's sound-effects library states these effects are free under CC BY-NC 3.0 unless otherwise noted; commercial use requires contacting the site."
                        .to_string(),
                ),
                attribution: Some("Moviesoundclips.net".to_string()),
                terms_url: Some(
                    "https://creativecommons.org/licenses/by-nc/3.0/".to_string(),
                ),
            });
        }
    }

    if clips.is_empty() {
        return Err(PublicClipError::Parse {
            provider: "Movie Sound Clips",
            message: "no public audio links were found in the sound-effects library".to_string(),
        });
    }
    Ok(clips)
}

fn browse_my_instants_com(
    query: &str,
    limit: usize,
    progress: &Sender<DownloadProgress>,
    cancelled: &AtomicBool,
) -> Result<Vec<PublicClip>, PublicClipError> {
    let index_url = PublicClipSource::MyInstantsCom.browse_url();
    let html = fetch_text("My-Instants.com", index_url)?;
    let mut details = Vec::<(String, String)>::new();
    let mut seen_pages = HashSet::new();

    for anchor in parse_anchors(&html) {
        if looks_like_my_instants_detail(&anchor.href)
            && !anchor.text.is_empty()
            && matches_query(&anchor.text, query)
        {
            let Some(url) = absolute_url(index_url, &anchor.href) else {
                continue;
            };
            if seen_pages.insert(url.clone()) {
                details.push((url, anchor.text));
            }
        }
        if details.len() >= limit {
            break;
        }
    }

    let mut clips = Vec::new();
    for (index, (detail_url, fallback_title)) in details.into_iter().enumerate() {
        if cancelled.load(Ordering::Relaxed) {
            break;
        }
        send_progress(
            progress,
            &format!(
                "Reading My-Instants.com sound pages… {}/{}",
                index + 1,
                limit
            ),
            index,
            Some(limit),
        );

        let detail_html = fetch_text("My-Instants.com", &detail_url)?;
        let anchors = parse_anchors(&detail_html);
        let media = anchors.iter().find_map(|anchor| {
            let lower = anchor.href.to_ascii_lowercase();
            if (lower.contains("soundboard.cloud") || lower.contains("/media/"))
                && url_has_audio_extension(&lower)
            {
                absolute_url(&detail_url, &anchor.href)
            } else {
                None
            }
        });
        let Some(media_url) = media else {
            continue;
        };
        let title = h1_text(&detail_html)
            .map(|value| {
                value
                    .strip_suffix(" Sound Buttons")
                    .unwrap_or(&value)
                    .to_string()
            })
            .filter(|value| !value.is_empty())
            .unwrap_or(fallback_title);

        clips.push(PublicClip {
            source_name: "My-Instants.com".to_string(),
            source_url: detail_url,
            title,
            creator: None,
            media_url: media_url.clone(),
            preview_url: Some(media_url),
            license: None,
            usage_terms: Some(
                "The site exposes a public MP3 download. Its pages do not expose a per-sound reuse license; site terms still apply."
                    .to_string(),
            ),
            attribution: None,
            terms_url: Some("https://my-instants.com/terms-and-conditions".to_string()),
        });
        if clips.len() >= limit {
            break;
        }
    }

    if clips.is_empty() {
        return Err(PublicClipError::Parse {
            provider: "My-Instants.com",
            message: "no public full-audio links were found on the validated trending pages"
                .to_string(),
        });
    }
    Ok(clips)
}

fn ensure_curl() -> Result<(), PublicClipError> {
    if which::which("curl").is_err() {
        Err(PublicClipError::CurlMissing)
    } else {
        Ok(())
    }
}

fn fetch_text(provider: &'static str, url: &str) -> Result<String, PublicClipError> {
    let output = Command::new("curl")
        .args([
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--connect-timeout",
            "15",
            "--max-time",
            "60",
            "--retry",
            "3",
            "--retry-delay",
            "1",
            "--user-agent",
            USER_AGENT,
            url,
        ])
        .output()
        .map_err(|error| PublicClipError::Request {
            provider,
            url: url.to_string(),
            message: error.to_string(),
        })?;

    if !output.status.success() {
        return Err(PublicClipError::Request {
            provider,
            url: url.to_string(),
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    String::from_utf8(output.stdout).map_err(|error| PublicClipError::Parse {
        provider,
        message: format!("response was not valid UTF-8: {error}"),
    })
}

fn download_clip(
    provider: &'static str,
    url: &str,
    final_path: &Path,
) -> Result<bool, PublicClipError> {
    if final_path
        .metadata()
        .map(|metadata| metadata.len() > 0)
        .unwrap_or(false)
    {
        return Ok(false);
    }

    if let Some(parent) = final_path.parent() {
        fs::create_dir_all(parent).map_err(PublicClipError::CreateDirectory)?;
    }

    let part_path = PathBuf::from(format!("{}.part", final_path.display()));
    let output = Command::new("curl")
        .args([
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--connect-timeout",
            "15",
            "--max-time",
            "180",
            "--retry",
            "3",
            "--retry-delay",
            "1",
            "--user-agent",
            USER_AGENT,
            "--write-out",
            "%{content_type}",
            "--output",
        ])
        .arg(&part_path)
        .arg(url)
        .output()
        .map_err(|error| PublicClipError::Request {
            provider,
            url: url.to_string(),
            message: error.to_string(),
        })?;

    if !output.status.success() {
        let _ = fs::remove_file(&part_path);
        return Err(PublicClipError::Request {
            provider,
            url: url.to_string(),
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    let content_type = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
    if content_type.contains("text/html") || content_type.contains("application/json") {
        let _ = fs::remove_file(&part_path);
        return Err(PublicClipError::Request {
            provider,
            url: url.to_string(),
            message: format!("server returned {content_type} instead of audio"),
        });
    }

    validate_download(&part_path).map_err(|message| {
        let _ = fs::remove_file(&part_path);
        PublicClipError::InvalidDownload(message)
    })?;

    fs::rename(&part_path, final_path).map_err(|error| PublicClipError::Request {
        provider,
        url: url.to_string(),
        message: format!("failed to finalize '{}': {error}", final_path.display()),
    })?;
    Ok(true)
}

fn choose_target_path(output_dir: &Path, clip: &PublicClip, extension: &str) -> PathBuf {
    let stem = sanitize_file_stem(&clip.title);
    for occurrence in 1..=10_000_usize {
        let name = if occurrence == 1 {
            format!("{stem}.{extension}")
        } else {
            format!("{stem} ({occurrence}).{extension}")
        };
        let candidate = output_dir.join(name);
        if !candidate.exists() || sidecar_matches_media(&candidate, &clip.media_url) {
            return candidate;
        }
    }
    output_dir.join(format!("{stem}.download.{extension}"))
}

fn metadata_sidecar_path(audio_path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.source.json", audio_path.display()))
}

fn sidecar_matches_media(audio_path: &Path, media_url: &str) -> bool {
    let sidecar = metadata_sidecar_path(audio_path);
    fs::read_to_string(sidecar)
        .ok()
        .and_then(|json| serde_json::from_str::<PublicClip>(&json).ok())
        .is_some_and(|metadata| metadata.media_url == media_url)
}

fn write_metadata_sidecar(audio_path: &Path, clip: &PublicClip) -> Result<(), PublicClipError> {
    let json = serde_json::to_string_pretty(clip).map_err(|error| {
        PublicClipError::Metadata(io::Error::other(format!(
            "could not encode source metadata: {error}"
        )))
    })?;
    fs::write(metadata_sidecar_path(audio_path), json).map_err(PublicClipError::Metadata)
}

fn extension_from_url(url: &str) -> Option<&'static str> {
    let lower = url
        .split('?')
        .next()
        .unwrap_or(url)
        .to_ascii_lowercase();
    for extension in ["mp3", "wav", "ogg", "flac", "aac", "m4a", "mp4"] {
        if lower.ends_with(&format!(".{extension}")) {
            return Some(extension);
        }
    }
    None
}

fn url_has_audio_extension(url: &str) -> bool {
    extension_from_url(url).is_some()
}

fn matches_query(title: &str, query: &str) -> bool {
    let query = query.trim();
    query.is_empty() || title.to_lowercase().contains(&query.to_lowercase())
}

fn looks_like_my_instants_detail(href: &str) -> bool {
    let path = href
        .split('?')
        .next()
        .unwrap_or(href)
        .trim_end_matches('/');
    let segment = path.rsplit('/').next().unwrap_or_default();
    let Some((_, suffix)) = segment.rsplit_once('-') else {
        return false;
    };
    suffix.len() >= 3 && suffix.chars().all(|character| character.is_ascii_digit())
}

fn parse_anchors(html: &str) -> Vec<Anchor> {
    let mut anchors = Vec::new();
    let mut rest = html;

    while let Some(start) = rest.find("<a") {
        rest = &rest[start + 2..];
        let Some(tag_end) = rest.find('>') else {
            break;
        };
        let tag = &rest[..tag_end];
        let body = &rest[tag_end + 1..];
        let Some(close) = body.find("</a>") else {
            rest = body;
            continue;
        };
        let Some(href) = quoted_attribute(tag, "href") else {
            rest = &body[close + 4..];
            continue;
        };
        let text = decode_html_entities(&strip_tags(&body[..close]))
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        anchors.push(Anchor { href, text });
        rest = &body[close + 4..];
    }

    anchors
}

fn quoted_attribute(tag: &str, attribute: &str) -> Option<String> {
    let needle = format!("{attribute}=");
    let index = tag.find(&needle)?;
    let after = &tag[index + needle.len()..];
    let quote = after.as_bytes().first().copied()?;
    if !matches!(quote, b'\'' | b'"') {
        return None;
    }
    let body = &after[1..];
    let end = body.as_bytes().iter().position(|byte| *byte == quote)?;
    Some(body[..end].to_string())
}

fn h1_text(html: &str) -> Option<String> {
    let start = html.find("<h1")?;
    let content = &html[start..];
    let open_end = content.find('>')?;
    let body = &content[open_end + 1..];
    let close = body.find("</h1>")?;
    let value = decode_html_entities(&strip_tags(&body[..close]));
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn absolute_url(page_url: &str, value: &str) -> Option<String> {
    let value = decode_html_entities(value.trim());
    if value.starts_with("https://") || value.starts_with("http://") {
        return Some(value);
    }
    if value.starts_with("//") {
        return Some(format!("https:{value}"));
    }

    let scheme_end = page_url.find("://")?;
    let after_scheme = scheme_end + 3;
    let host_end = page_url[after_scheme..]
        .find('/')
        .map(|offset| after_scheme + offset)
        .unwrap_or(page_url.len());
    let origin = &page_url[..host_end];

    if value.starts_with('/') {
        return Some(format!("{origin}{value}"));
    }

    let base = page_url
        .rsplit_once('/')
        .map(|(prefix, _)| prefix)
        .unwrap_or(page_url);
    Some(format!("{base}/{value}"))
}

fn sanitize_file_stem(value: &str) -> String {
    let mut output = String::new();
    let mut pending_space = false;

    for character in value.chars() {
        if character.is_control() || character == '/' || character == '\\' {
            pending_space = true;
            continue;
        }
        if character.is_whitespace() {
            pending_space = true;
            continue;
        }

        let needs_space = pending_space && !output.is_empty();
        let bytes = character.len_utf8() + if needs_space { 1 } else { 0 };
        if output.len().saturating_add(bytes) > MAX_FILE_STEM_BYTES {
            break;
        }
        if needs_space {
            output.push(' ');
        }
        pending_space = false;
        output.push(character);
    }

    let output = output.trim().trim_end_matches('.');
    if output.is_empty() || output == "." || output == ".." {
        "online-sound".to_string()
    } else {
        output.to_string()
    }
}

fn strip_tags(value: &str) -> String {
    let mut output = String::new();
    let mut in_tag = false;
    for character in value.chars() {
        match character {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => output.push(character),
            _ => {}
        }
    }
    output
}

fn decode_html_entities(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
}

fn send_progress(
    sender: &Sender<DownloadProgress>,
    message: &str,
    completed: usize,
    total: Option<usize>,
) {
    let _ = sender.send(DownloadProgress {
        message: message.to_string(),
        completed,
        total,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sound_buttons_parser_pairs_detail_and_mp3_download_links() {
        let html = r#"
            <a href="/sound-button/monkey-sounds">Monkey sounds</a>
            <a class="download" href="/download/57e2febe.mp3">⇩</a>
        "#;
        let anchors = parse_anchors(html);
        assert_eq!(anchors.len(), 2);
        assert_eq!(anchors[0].text, "Monkey sounds");
        assert_eq!(anchors[1].href, "/download/57e2febe.mp3");
    }

    #[test]
    fn my_instants_detail_detection_requires_numeric_suffix() {
        assert!(looks_like_my_instants_detail("/funny-laughing-1-8277"));
        assert!(!looks_like_my_instants_detail("/trending"));
        assert!(!looks_like_my_instants_detail("/memes"));
    }

    #[test]
    fn h1_parser_keeps_visible_title() {
        let html = "<h1>Funny Laughing 1 Sound Buttons</h1>";
        assert_eq!(
            h1_text(html).as_deref(),
            Some("Funny Laughing 1 Sound Buttons")
        );
    }

    #[test]
    fn filenames_keep_human_readable_punctuation() {
        assert_eq!(
            sanitize_file_stem("Wait... WHAT?! / reaction"),
            "Wait... WHAT?! reaction"
        );
    }
}
