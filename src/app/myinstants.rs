use rayon::prelude::*;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use crate::config::Config;

const ORIGIN: &str = "https://www.myinstants.com";
const MAX_PAGES: usize = 1_000;
const STABLE_PAGE_LIMIT: usize = 8;
const DOWNLOAD_WORKERS: usize = 8;
const AUDIO_EXTENSION: &str = concat!(".", "mp3");
const USER_AGENT: &str =
    concat!("Linux-SoundBoard/", env!("CARGO_PKG_VERSION"), " MyInstants downloader");

pub const ALL_ENGLISH_ID: &str = "all-en";
pub const COUNTRY_CHOICES: &[(&str, &str)] = &[
    ("us", "United States"),
    ("ca", "Canada"),
    ("gb", "United Kingdom"),
    ("au", "Australia"),
    ("nz", "New Zealand"),
    ("ie", "Ireland"),
    ("za", "South Africa"),
    ("sg", "Singapore"),
];

#[derive(Debug, Clone)]
pub struct DownloadProgress {
    pub message: String,
    pub completed: usize,
    pub total: Option<usize>,
}

#[derive(Debug, Default)]
pub struct DownloadReport {
    pub paths: Vec<String>,
    pub downloaded: usize,
    pub reused: usize,
    pub failed: usize,
    pub cancelled: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum MyInstantsError {
    #[error("curl is required to download sounds from MyInstants")]
    CurlMissing,
    #[error("failed to create MyInstants download directory: {0}")]
    CreateDirectory(#[source] io::Error),
    #[error("failed to write MyInstants source manifest: {0}")]
    WriteManifest(#[source] io::Error),
    #[error("MyInstants request failed for {url}: {message}")]
    Request { url: String, message: String },
    #[error("unsupported MyInstants country selection '{0}'")]
    InvalidCountry(String),
    #[error("failed to create MyInstants worker pool: {0}")]
    WorkerPool(String),
}

pub fn download_directory() -> PathBuf {
    Config::config_path()
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("myinstants")
}

pub fn download(
    selection: &str,
    progress: Sender<DownloadProgress>,
    cancelled: Arc<AtomicBool>,
) -> Result<DownloadReport, MyInstantsError> {
    if which::which("curl").is_err() {
        return Err(MyInstantsError::CurlMissing);
    }

    let output_dir = download_directory();
    fs::create_dir_all(&output_dir).map_err(MyInstantsError::CreateDirectory)?;

    let countries = selected_countries(selection)?;
    send_progress(&progress, "Discovering MyInstants sounds…", 0, None);
    let pages = discover_sound_pages(&countries, &progress, &cancelled)?;

    let mut manifest = pages.clone();
    manifest.sort();
    manifest.dedup();
    fs::write(output_dir.join("_sound_pages.txt"), manifest.join("\n"))
        .map_err(MyInstantsError::WriteManifest)?;

    if cancelled.load(Ordering::Relaxed) {
        return Ok(DownloadReport {
            cancelled: true,
            ..DownloadReport::default()
        });
    }

    if pages.is_empty() {
        return Ok(DownloadReport::default());
    }

    send_progress(
        &progress,
        &format!("Resolving and downloading {} sounds…", pages.len()),
        0,
        Some(pages.len()),
    );

    let processed = AtomicUsize::new(0);
    let downloaded = AtomicUsize::new(0);
    let reused = AtomicUsize::new(0);
    let failed = AtomicUsize::new(0);
    let paths = Mutex::new(Vec::<String>::new());
    let seen_media = Mutex::new(HashSet::<String>::new());

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(DOWNLOAD_WORKERS)
        .thread_name(|index| format!("myinstants-{index}"))
        .build()
        .map_err(|error| MyInstantsError::WorkerPool(error.to_string()))?;

    pool.install(|| {
        pages.par_iter().for_each(|page_url| {
            if cancelled.load(Ordering::Relaxed) {
                return;
            }

            let result = resolve_and_download(page_url, &output_dir, &seen_media, &cancelled);
            match result {
                Ok(Some((path, was_downloaded))) => {
                    if was_downloaded {
                        downloaded.fetch_add(1, Ordering::Relaxed);
                    } else {
                        reused.fetch_add(1, Ordering::Relaxed);
                    }
                    if let Ok(mut paths) = paths.lock() {
                        paths.push(path.to_string_lossy().into_owned());
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    failed.fetch_add(1, Ordering::Relaxed);
                    log::warn!("MyInstants sound failed ({}): {}", page_url, error);
                }
            }

            let current = processed.fetch_add(1, Ordering::Relaxed) + 1;
            send_progress(
                &progress,
                &format!("Downloading MyInstants sounds… {current}/{}", pages.len()),
                current,
                Some(pages.len()),
            );
        });
    });

    let mut paths = paths.into_inner().unwrap_or_default();
    paths.sort();
    paths.dedup();

    Ok(DownloadReport {
        paths,
        downloaded: downloaded.load(Ordering::Relaxed),
        reused: reused.load(Ordering::Relaxed),
        failed: failed.load(Ordering::Relaxed),
        cancelled: cancelled.load(Ordering::Relaxed),
    })
}

fn selected_countries(selection: &str) -> Result<Vec<&'static str>, MyInstantsError> {
    if selection == ALL_ENGLISH_ID {
        return Ok(COUNTRY_CHOICES.iter().map(|(code, _)| *code).collect());
    }

    COUNTRY_CHOICES
        .iter()
        .find(|(code, _)| *code == selection)
        .map(|(code, _)| vec![*code])
        .ok_or_else(|| MyInstantsError::InvalidCountry(selection.to_string()))
}

fn discover_sound_pages(
    countries: &[&str],
    progress: &Sender<DownloadProgress>,
    cancelled: &AtomicBool,
) -> Result<Vec<String>, MyInstantsError> {
    let mut found = HashSet::new();

    for country in countries {
        let mut stable_pages = 0usize;
        let mut page = 1usize;
        let mut page_limit = MAX_PAGES;

        while page <= page_limit && stable_pages < STABLE_PAGE_LIMIT {
            if cancelled.load(Ordering::Relaxed) {
                break;
            }

            send_progress(
                progress,
                &format!("Scanning MyInstants {country} page {page}…"),
                found.len(),
                None,
            );

            let url = format!("{ORIGIN}/en/index/{country}/?page={page}");
            let html = match fetch_text(&url) {
                Ok(html) => html,
                Err(error) if page > 1 => {
                    log::debug!("Stopping MyInstants {country} scan at page {page}: {error}");
                    break;
                }
                Err(error) => return Err(error),
            };

            if page == 1 {
                if let Some(hint) = pagination_page_hint(&html) {
                    page_limit = hint.clamp(1, MAX_PAGES);
                }
            }

            let before = found.len();
            found.extend(parse_instant_links(&html));
            if found.len() == before {
                stable_pages += 1;
            } else {
                stable_pages = 0;
            }

            page += 1;
        }
    }

    let mut pages: Vec<_> = found.into_iter().collect();
    pages.sort();
    Ok(pages)
}

fn resolve_and_download(
    page_url: &str,
    output_dir: &Path,
    seen_media: &Mutex<HashSet<String>>,
    cancelled: &AtomicBool,
) -> Result<Option<(PathBuf, bool)>, MyInstantsError> {
    if cancelled.load(Ordering::Relaxed) {
        return Ok(None);
    }

    let html = fetch_text(page_url)?;
    let Some(media_url) = media_url_from_detail(&html) else {
        return Err(MyInstantsError::Request {
            url: page_url.to_string(),
            message: "no downloadable audio URL was found".to_string(),
        });
    };

    {
        let mut seen = seen_media
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !seen.insert(media_url.clone()) {
            return Ok(None);
        }
    }

    let title = title_from_detail(&html).unwrap_or_else(|| slug_from_url(page_url));
    let filename = file_name_for(&title, &media_url);
    let final_path = output_dir.join(filename);

    if final_path
        .metadata()
        .map(|metadata| metadata.len() > 0)
        .unwrap_or(false)
    {
        return Ok(Some((final_path, false)));
    }

    download_media(&media_url, &final_path)?;
    Ok(Some((final_path, true)))
}

fn fetch_text(url: &str) -> Result<String, MyInstantsError> {
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
        .map_err(|error| MyInstantsError::Request {
            url: url.to_string(),
            message: error.to_string(),
        })?;

    if !output.status.success() {
        return Err(MyInstantsError::Request {
            url: url.to_string(),
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    String::from_utf8(output.stdout).map_err(|error| MyInstantsError::Request {
        url: url.to_string(),
        message: format!("response was not valid UTF-8: {error}"),
    })
}

fn download_media(url: &str, final_path: &Path) -> Result<(), MyInstantsError> {
    let part_path = PathBuf::from(format!("{}.part", final_path.display()));
    let resume = part_path
        .metadata()
        .map(|metadata| metadata.len() > 0)
        .unwrap_or(false);

    let mut command = Command::new("curl");
    command.args([
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
    ]);
    if resume {
        command.args(["--continue-at", "-"]);
    }
    command.arg("--output").arg(&part_path).arg(url);

    let output = command.output().map_err(|error| MyInstantsError::Request {
        url: url.to_string(),
        message: error.to_string(),
    })?;

    if !output.status.success() && resume {
        let _ = fs::remove_file(&part_path);
        return download_media(url, final_path);
    }
    if !output.status.success() {
        return Err(MyInstantsError::Request {
            url: url.to_string(),
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    fs::rename(&part_path, final_path).map_err(|error| MyInstantsError::Request {
        url: url.to_string(),
        message: format!("failed to finalize '{}': {error}", final_path.display()),
    })
}

fn parse_instant_links(html: &str) -> Vec<String> {
    let mut links = HashSet::new();
    for href in quoted_attribute_values(html, "href") {
        if !href.contains("/instant/") || href.contains("/embed/") {
            continue;
        }
        if let Some(url) = absolute_url(&href) {
            links.insert(url);
        }
    }
    let mut links: Vec<_> = links.into_iter().collect();
    links.sort();
    links
}

fn media_url_from_detail(html: &str) -> Option<String> {
    for value in all_quoted_values(html) {
        if value.contains("/media/sounds/") {
            if let Some(url) = absolute_url(&value) {
                return Some(url);
            }
        }
    }

    let marker = "/media/sounds/";
    let start = html.find(marker)?;
    let bytes = html.as_bytes();
    let mut end = start;
    while end < bytes.len() {
        let byte = bytes[end];
        if matches!(
            byte,
            b'\'' | b'"' | b'<' | b'>' | b' ' | b'\n' | b'\r' | b'\t'
        ) {
            break;
        }
        end += 1;
    }
    absolute_url(&html[start..end])
}

fn title_from_detail(html: &str) -> Option<String> {
    let h1_start = html.find("<h1")?;
    let content_start = html[h1_start..].find('>')? + h1_start + 1;
    let content_end = html[content_start..].find("</h1>")? + content_start;
    let title = strip_tags(&html[content_start..content_end]);
    let title = decode_html_entities(&title);
    let title = title.trim();
    (!title.is_empty()).then(|| title.to_string())
}

fn pagination_page_hint(html: &str) -> Option<usize> {
    let mut max_page = None;
    let mut rest = html;
    while let Some(index) = rest.find("page=") {
        rest = &rest[index + 5..];
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if let Ok(page) = digits.parse::<usize>() {
            max_page = Some(max_page.map_or(page, |current: usize| current.max(page)));
        }
    }
    max_page
}

fn quoted_attribute_values(html: &str, attribute: &str) -> Vec<String> {
    let mut values = Vec::new();
    let needle = format!("{attribute}=");
    let mut rest = html;
    while let Some(index) = rest.find(&needle) {
        rest = &rest[index + needle.len()..];
        let Some(quote) = rest.as_bytes().first().copied() else {
            break;
        };
        if !matches!(quote, b'\'' | b'"') {
            rest = &rest[1..];
            continue;
        }
        let body = &rest[1..];
        if let Some(end) = body.as_bytes().iter().position(|byte| *byte == quote) {
            values.push(body[..end].to_string());
            rest = &body[end + 1..];
        } else {
            break;
        }
    }
    values
}

fn all_quoted_values(html: &str) -> Vec<String> {
    let mut values = Vec::new();
    let bytes = html.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if matches!(bytes[index], b'\'' | b'"') {
            let quote = bytes[index];
            let start = index + 1;
            index = start;
            while index < bytes.len() && bytes[index] != quote {
                index += 1;
            }
            if index <= bytes.len() {
                values.push(html[start..index].to_string());
            }
        }
        index += 1;
    }
    values
}

fn absolute_url(value: &str) -> Option<String> {
    let value = value.trim();
    if value.starts_with("https://") || value.starts_with("http://") {
        return Some(value.to_string());
    }
    if value.starts_with('/') {
        return Some(format!("{ORIGIN}{value}"));
    }
    None
}

fn file_name_for(title: &str, media_url: &str) -> String {
    let safe_title = sanitize_file_stem(title);
    let digest = Sha256::digest(media_url.as_bytes());
    let short_hash = digest
        .iter()
        .take(4)
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("{safe_title}-{short_hash}{AUDIO_EXTENSION}")
}

fn sanitize_file_stem(value: &str) -> String {
    let mut out = String::new();
    let mut pending_space = false;
    for character in value.chars() {
        if character.is_alphanumeric() || matches!(character, '-' | '_') {
            if pending_space && !out.is_empty() {
                out.push(' ');
            }
            pending_space = false;
            out.push(character);
        } else {
            pending_space = true;
        }
        if out.chars().count() >= 96 {
            break;
        }
    }
    let out = out.trim();
    if out.is_empty() {
        "myinstants-sound".to_string()
    } else {
        out.to_string()
    }
}

fn slug_from_url(url: &str) -> String {
    url.trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|slug| !slug.is_empty())
        .unwrap_or("myinstants-sound")
        .replace('-', " ")
}

fn strip_tags(value: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for character in value.chars() {
        match character {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(character),
            _ => {}
        }
    }
    out
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
    fn parses_instant_links_and_deduplicates_them() {
        let html = r#"
            <a href="/en/instant/air-horn-1/">Air Horn</a>
            <a href='/en/instant/air-horn-1/'>duplicate</a>
            <a href="/en/instant/vine-boom-2/embed/">embed</a>
            <a href="/en/instant/vine-boom-2/">Vine Boom</a>
        "#;
        assert_eq!(
            parse_instant_links(html),
            vec![
                format!("{ORIGIN}/en/instant/air-horn-1/"),
                format!("{ORIGIN}/en/instant/vine-boom-2/")
            ]
        );
    }

    #[test]
    fn extracts_media_and_title_from_detail_page() {
        let media_path = format!("/media/sounds/get-out{AUDIO_EXTENSION}");
        let html = format!(
            "<h1>Tuco: GET OUT &amp; RUN</h1><a href=\"{media_path}\" download>Download</a>"
        );
        assert_eq!(
            title_from_detail(&html).as_deref(),
            Some("Tuco: GET OUT & RUN")
        );
        assert_eq!(
            media_url_from_detail(&html).as_deref(),
            Some(format!("{ORIGIN}{media_path}").as_str())
        );
    }

    #[test]
    fn pagination_hint_uses_largest_page_number() {
        let html = r#"<a href="?page=2">2</a><a href="?page=50">50</a>"#;
        assert_eq!(pagination_page_hint(html), Some(50));
    }

    #[test]
    fn file_names_are_stable_and_safe() {
        let media_url = format!("https://example.test/media/sounds/a{AUDIO_EXTENSION}");
        let first = file_name_for("GET OUT!!", &media_url);
        let second = file_name_for("GET OUT!!", &media_url);
        assert_eq!(first, second);
        assert!(first.starts_with("GET OUT-"));
        assert!(first.ends_with(AUDIO_EXTENSION));
        assert!(!first.contains('!'));
    }
}
