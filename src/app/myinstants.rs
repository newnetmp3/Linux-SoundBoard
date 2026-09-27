use rayon::prelude::*;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::config::Config;
use crate::download_validation::validate_download_as;

const ORIGIN: &str = "https://www.myinstants.com";
const MAX_INDEX_PAGES: usize = 100;
const EMPTY_PAGE_STOP_COUNT: usize = 2;
const PAGE_FETCH_DELAY_MS: u64 = 250;
const DOWNLOAD_WORKERS: usize = 8;
const MAX_FILE_STEM_BYTES: usize = 180;
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
    pub path_migrations: Vec<(String, String)>,
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

#[derive(Debug, Clone)]
struct ResolvedSound {
    page_url: String,
    media_url: String,
    title: String,
}

#[derive(Debug, Clone)]
struct DownloadJob {
    page_url: String,
    title: String,
    media_url: String,
    final_path: PathBuf,
    legacy_path: PathBuf,
}

pub fn default_download_directory() -> PathBuf {
    Config::config_path()
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("online-sounds")
}

pub fn source_download_directory(output_root: &Path) -> PathBuf {
    output_root.join("MyInstants")
}

pub fn download_directory(configured: Option<&str>) -> PathBuf {
    configured
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(default_download_directory)
}

pub fn download(
    selection: &str,
    output_dir: &Path,
    progress: Sender<DownloadProgress>,
    cancelled: Arc<AtomicBool>,
) -> Result<DownloadReport, MyInstantsError> {
    if which::which("curl").is_err() {
        return Err(MyInstantsError::CurlMissing);
    }

    let output_dir = source_download_directory(output_dir);
    fs::create_dir_all(&output_dir).map_err(MyInstantsError::CreateDirectory)?;

    let countries = selected_countries(selection)?;
    send_progress(
        &progress,
        "Launching Chromium for MyInstants infinite-scroll discovery…",
        0,
        None,
    );
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

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(DOWNLOAD_WORKERS)
        .thread_name(|index| format!("myinstants-{index}"))
        .build()
        .map_err(|error| MyInstantsError::WorkerPool(error.to_string()))?;

    send_progress(
        &progress,
        &format!("Reading {} MyInstants sound pages…", pages.len()),
        0,
        Some(pages.len()),
    );

    let resolve_processed = AtomicUsize::new(0);
    let resolve_failed = AtomicUsize::new(0);
    let resolved = Mutex::new(Vec::<ResolvedSound>::new());

    pool.install(|| {
        pages.par_iter().for_each(|page_url| {
            if cancelled.load(Ordering::Relaxed) {
                return;
            }

            match resolve_sound_detail(page_url, &cancelled) {
                Ok(Some(sound)) => {
                    if let Ok(mut resolved) = resolved.lock() {
                        resolved.push(sound);
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    resolve_failed.fetch_add(1, Ordering::Relaxed);
                    log::warn!("MyInstants detail page failed ({}): {}", page_url, error);
                }
            }

            let current = resolve_processed.fetch_add(1, Ordering::Relaxed) + 1;
            send_progress(
                &progress,
                &format!("Reading MyInstants sound pages… {current}/{}", pages.len()),
                current,
                Some(pages.len()),
            );
        });
    });

    if cancelled.load(Ordering::Relaxed) {
        return Ok(DownloadReport {
            failed: resolve_failed.load(Ordering::Relaxed),
            cancelled: true,
            ..DownloadReport::default()
        });
    }

    let mut resolved = resolved.into_inner().unwrap_or_default();
    resolved.sort_by(|left, right| {
        sanitize_file_stem(&left.title)
            .to_lowercase()
            .cmp(&sanitize_file_stem(&right.title).to_lowercase())
            .then(left.page_url.cmp(&right.page_url))
    });

    let mut seen_media = HashSet::<String>::new();
    resolved.retain(|sound| seen_media.insert(sound.media_url.clone()));

    let jobs = build_download_jobs(&output_dir, resolved);
    let path_migrations = migrate_legacy_hash_names(&jobs);

    send_progress(
        &progress,
        &format!("Downloading {} MyInstants sounds…", jobs.len()),
        0,
        Some(jobs.len()),
    );

    let processed = AtomicUsize::new(0);
    let downloaded = AtomicUsize::new(0);
    let reused = AtomicUsize::new(0);
    let download_failed = AtomicUsize::new(0);
    let paths = Mutex::new(Vec::<String>::new());

    pool.install(|| {
        jobs.par_iter().for_each(|job| {
            if cancelled.load(Ordering::Relaxed) {
                return;
            }

            let cached = job
                .final_path
                .metadata()
                .map(|metadata| metadata.len() > 0)
                .unwrap_or(false);
            let result = if cached
                && validate_download_as(&job.final_path, &job.final_path).is_ok()
            {
                Ok(false)
            } else {
                if cached {
                    log::warn!(
                        "Replacing invalid cached MyInstants file '{}'",
                        job.final_path.display()
                    );
                    let _ = fs::remove_file(&job.final_path);
                }
                download_media(&job.media_url, &job.final_path).map(|()| true)
            };

            match result {
                Ok(was_downloaded) => {
                    if was_downloaded {
                        downloaded.fetch_add(1, Ordering::Relaxed);
                    } else {
                        reused.fetch_add(1, Ordering::Relaxed);
                    }
                    if let Err(error) = write_source_metadata(job) {
                        download_failed.fetch_add(1, Ordering::Relaxed);
                        log::warn!(
                            "Could not write MyInstants source metadata for '{}': {}",
                            job.final_path.display(),
                            error
                        );
                    }
                    if let Ok(mut paths) = paths.lock() {
                        paths.push(job.final_path.to_string_lossy().into_owned());
                    }
                }
                Err(error) => {
                    download_failed.fetch_add(1, Ordering::Relaxed);
                    log::warn!(
                        "MyInstants audio download failed ({}): {}",
                        job.media_url,
                        error
                    );
                }
            }

            let current = processed.fetch_add(1, Ordering::Relaxed) + 1;
            send_progress(
                &progress,
                &format!("Downloading MyInstants sounds… {current}/{}", jobs.len()),
                current,
                Some(jobs.len()),
            );
        });
    });

    let mut paths = paths.into_inner().unwrap_or_default();
    paths.sort();
    paths.dedup();

    Ok(DownloadReport {
        paths,
        path_migrations,
        downloaded: downloaded.load(Ordering::Relaxed),
        reused: reused.load(Ordering::Relaxed),
        failed: resolve_failed.load(Ordering::Relaxed)
            + download_failed.load(Ordering::Relaxed),
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
    let mut found = HashSet::<String>::new();

    for country in countries {
        if cancelled.load(Ordering::Relaxed) {
            break;
        }

        let mut no_new_pages = 0usize;
        let mut country_unique = 0usize;

        for page in 1..=MAX_INDEX_PAGES {
            if cancelled.load(Ordering::Relaxed) {
                break;
            }

            let url = if page == 1 {
                format!("{ORIGIN}/en/index/{country}/")
            } else {
                format!("{ORIGIN}/en/index/{country}/?page={page}")
            };

            send_progress(
                progress,
                &format!("Reading MyInstants {country} page {page}…"),
                found.len(),
                None,
            );

            let html = match fetch_text(&url) {
                Ok(html) => html,
                Err(error) if page > 1 => {
                    log::info!(
                        "Stopping MyInstants {country} pagination at page {page}: {error}"
                    );
                    break;
                }
                Err(error) => return Err(error),
            };

            let page_links = sound_page_urls_from_index_html(&html);
            if page_links.is_empty() {
                no_new_pages += 1;
            } else {
                let mut added = 0usize;
                for link in page_links {
                    if found.insert(link) {
                        added += 1;
                        country_unique += 1;
                    }
                }

                if added == 0 {
                    no_new_pages += 1;
                } else {
                    no_new_pages = 0;
                }

                send_progress(
                    progress,
                    &format!(
                        "MyInstants {country} page {page}: {added} new sounds, {country_unique} unique for this country…"
                    ),
                    found.len(),
                    None,
                );
            }

            if no_new_pages >= EMPTY_PAGE_STOP_COUNT {
                send_progress(
                    progress,
                    &format!(
                        "Finished MyInstants {country} after page {page}: {country_unique} unique sounds found."
                    ),
                    found.len(),
                    None,
                );
                break;
            }

            thread::sleep(Duration::from_millis(PAGE_FETCH_DELAY_MS));
        }
    }

    let mut pages = found.into_iter().collect::<Vec<_>>();
    pages.sort();
    Ok(pages)
}

fn resolve_sound_detail(
    page_url: &str,
    cancelled: &AtomicBool,
) -> Result<Option<ResolvedSound>, MyInstantsError> {
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

    let title = title_from_detail(&html).unwrap_or_else(|| slug_from_url(page_url));
    Ok(Some(ResolvedSound {
        page_url: page_url.to_string(),
        media_url,
        title,
    }))
}

fn build_download_jobs(output_dir: &Path, sounds: Vec<ResolvedSound>) -> Vec<DownloadJob> {
    let mut title_counts = HashMap::<String, usize>::new();

    sounds
        .into_iter()
        .map(|sound| {
            let stem = sanitize_file_stem(&sound.title);
            let count = title_counts.entry(stem.clone()).or_insert(0);
            *count += 1;

            let filename = human_file_name(&stem, *count);
            let final_path = output_dir.join(filename);
            let legacy_path = output_dir.join(legacy_hashed_file_name_for(
                &sound.title,
                &sound.media_url,
            ));

            DownloadJob {
                page_url: sound.page_url,
                title: sound.title,
                media_url: sound.media_url,
                final_path,
                legacy_path,
            }
        })
        .collect()
}

fn migrate_legacy_hash_names(jobs: &[DownloadJob]) -> Vec<(String, String)> {
    let mut migrations = Vec::new();

    for job in jobs {
        if job.final_path.exists() || !job.legacy_path.is_file() {
            continue;
        }

        match fs::rename(&job.legacy_path, &job.final_path) {
            Ok(()) => {
                log::info!(
                    "Renamed legacy MyInstants file '{}' to '{}'",
                    job.legacy_path.display(),
                    job.final_path.display()
                );
                migrations.push((
                    job.legacy_path.to_string_lossy().into_owned(),
                    job.final_path.to_string_lossy().into_owned(),
                ));
            }
            Err(error) => log::warn!(
                "Could not rename legacy MyInstants file '{}' to '{}': {}",
                job.legacy_path.display(),
                job.final_path.display(),
                error
            ),
        }
    }

    migrations
}

fn write_source_metadata(job: &DownloadJob) -> io::Result<()> {
    let metadata = json!({
        "source_name": "MyInstants",
        "source_url": &job.page_url,
        "title": &job.title,
        "creator": Value::Null,
        "media_url": &job.media_url,
        "preview_url": &job.media_url,
        "license": Value::Null,
        "usage_terms": "Public MyInstants download; no per-sound reuse license was exposed by the downloader.",
        "attribution": Value::Null
    });
    let sidecar = PathBuf::from(format!("{}.source.json", job.final_path.display()));
    let encoded = serde_json::to_string_pretty(&metadata)
        .map_err(|error| io::Error::other(error.to_string()))?;
    fs::write(sidecar, encoded)
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

    if let Err(message) = validate_download_as(&part_path, final_path) {
        let _ = fs::remove_file(&part_path);
        return Err(MyInstantsError::Request {
            url: url.to_string(),
            message: format!("download validation failed: {message}"),
        });
    }

    fs::rename(&part_path, final_path).map_err(|error| MyInstantsError::Request {
        url: url.to_string(),
        message: format!("failed to finalize '{}': {error}", final_path.display()),
    })
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
            if index < bytes.len() {
                values.push(html[start..index].to_string());
            } else {
                break;
            }
        }
        index += 1;
    }

    values
}

fn sound_page_urls_from_index_html(html: &str) -> Vec<String> {
    let mut pages = HashSet::<String>::new();

    for value in all_quoted_values(html) {
        if value.contains("/instant/") && !value.contains("/embed/") {
            if let Some(url) = absolute_url(&value) {
                pages.insert(url);
            }
        }

        if !value.contains("play(") {
            continue;
        }

        let args = all_quoted_values(&value);
        if let Some(slug) = args.get(2).map(String::as_str).filter(|slug| !slug.is_empty()) {
            pages.insert(format!("{ORIGIN}/en/instant/{slug}/"));
        }
    }

    let mut pages = pages.into_iter().collect::<Vec<_>>();
    pages.sort();
    pages
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

fn human_file_name(stem: &str, occurrence: usize) -> String {
    if occurrence <= 1 {
        format!("{stem}{AUDIO_EXTENSION}")
    } else {
        format!("{stem} ({occurrence}){AUDIO_EXTENSION}")
    }
}

fn legacy_hashed_file_name_for(title: &str, media_url: &str) -> String {
    let safe_title = legacy_sanitize_file_stem(title);
    let digest = Sha256::digest(media_url.as_bytes());
    let short_hash = digest
        .iter()
        .take(4)
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("{safe_title}-{short_hash}{AUDIO_EXTENSION}")
}

fn legacy_sanitize_file_stem(value: &str) -> String {
    let mut out = String::new();
    let mut pending_space = false;
    for character in value.chars() {
        if character.is_alphanumeric() || matches!(character, '-' | '_') {
            let needs_space = pending_space && !out.is_empty();
            let extra_bytes = character.len_utf8() + if needs_space { 1 } else { 0 };
            if out.len().saturating_add(extra_bytes) > MAX_FILE_STEM_BYTES {
                break;
            }
            if needs_space {
                out.push(' ');
            }
            pending_space = false;
            out.push(character);
        } else {
            pending_space = true;
        }
    }
    let out = out.trim();
    if out.is_empty() {
        "myinstants-sound".to_string()
    } else {
        out.to_string()
    }
}

fn sanitize_file_stem(value: &str) -> String {
    let mut out = String::new();
    let mut pending_space = false;

    for character in value.chars() {
        let unsafe_for_filename =
            character == '/' || character == '\\' || character.is_control();

        if unsafe_for_filename || character.is_whitespace() {
            pending_space = true;
            continue;
        }

        let needs_space = pending_space && !out.is_empty();
        let extra_bytes = character.len_utf8() + if needs_space { 1 } else { 0 };
        if out.len().saturating_add(extra_bytes) > MAX_FILE_STEM_BYTES {
            break;
        }
        if needs_space {
            out.push(' ');
        }
        pending_space = false;
        out.push(character);
    }

    let out = out.trim().trim_end_matches('.');
    if out.is_empty() || out == "." || out == ".." {
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
    fn myinstants_uses_source_subfolder_under_download_root() {
        let root = PathBuf::from("/tmp/linux-soundboard-online");
        assert_eq!(source_download_directory(&root), root.join("MyInstants"));
    }

    #[test]
    fn configured_download_directory_overrides_default() {
        let configured = PathBuf::from("/tmp/linux-soundboard-myinstants");
        assert_eq!(
            download_directory(Some(configured.to_string_lossy().as_ref())),
            configured
        );
        assert_eq!(download_directory(Some("   ")), default_download_directory());
        assert_eq!(download_directory(None), default_download_directory());
    }

    #[test]
    fn pagination_urls_match_myinstants_listing_shape() {
        let country = "us";
        let first = format!("{ORIGIN}/en/index/{country}/");
        let second = format!("{ORIGIN}/en/index/{country}/?page=2");
        assert_eq!(first, "https://www.myinstants.com/en/index/us/");
        assert_eq!(
            second,
            "https://www.myinstants.com/en/index/us/?page=2"
        );
    }

    #[test]
    fn index_parser_reads_myinstants_play_controls_and_links() {
        let html = r#"
            <button onclick="play('/media/sounds/vine-boom.mp3', 'VINE BOOM SOUND', 'vine-boom-sound-1234')"></button>
            <a href="/en/instant/bruh-5678/">BRUH</a>
            <iframe src="/instant/ignored-9999/embed/"></iframe>
        "#;
        let pages = sound_page_urls_from_index_html(html);
        assert!(pages.contains(
            &format!("{ORIGIN}/en/instant/vine-boom-sound-1234/")
        ));
        assert!(pages.contains(&format!("{ORIGIN}/en/instant/bruh-5678/")));
        assert!(!pages.iter().any(|page| page.contains("ignored-9999")));
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
        let expected_media_url = format!("{ORIGIN}{media_path}");
        assert_eq!(
            media_url_from_detail(&html).as_deref(),
            Some(expected_media_url.as_str())
        );
    }

    #[test]
    fn clean_file_names_use_page_title_without_hash_suffix() {
        let stem = sanitize_file_stem("\"Wow!\" (anime voice accent)");
        assert_eq!(
            human_file_name(&stem, 1),
            format!("\"Wow!\" (anime voice accent){AUDIO_EXTENSION}")
        );
        assert_eq!(
            human_file_name(&stem, 2),
            format!("\"Wow!\" (anime voice accent) (2){AUDIO_EXTENSION}")
        );
    }

    #[test]
    fn build_jobs_assigns_stable_human_readable_duplicate_names() {
        let output_dir = PathBuf::from("/tmp/myinstants-test");
        let sounds = vec![
            ResolvedSound {
                page_url: format!("{ORIGIN}/en/instant/first/"),
                media_url: format!("{ORIGIN}/media/sounds/first{AUDIO_EXTENSION}"),
                title: "Same Name".to_string(),
            },
            ResolvedSound {
                page_url: format!("{ORIGIN}/en/instant/second/"),
                media_url: format!("{ORIGIN}/media/sounds/second{AUDIO_EXTENSION}"),
                title: "Same Name".to_string(),
            },
        ];

        let jobs = build_download_jobs(&output_dir, sounds);
        assert_eq!(
            jobs[0].final_path,
            output_dir.join(format!("Same Name{AUDIO_EXTENSION}"))
        );
        assert_eq!(
            jobs[1].final_path,
            output_dir.join(format!("Same Name (2){AUDIO_EXTENSION}"))
        );
    }

    #[test]
    fn legacy_hash_name_is_only_used_for_migration() {
        let media_url = format!("https://example.test/media/sounds/a{AUDIO_EXTENSION}");
        let legacy = legacy_hashed_file_name_for("GET OUT!!", &media_url);
        assert!(legacy.starts_with("GET OUT-"));
        assert!(legacy.ends_with(AUDIO_EXTENSION));
        assert_ne!(legacy, format!("GET OUT{AUDIO_EXTENSION}"));
    }

    #[test]
    fn file_names_stay_within_safe_utf8_byte_limits() {
        let title = "界".repeat(200);
        let stem = sanitize_file_stem(&title);
        let filename = human_file_name(&stem, 1);
        assert!(filename.len() <= MAX_FILE_STEM_BYTES + AUDIO_EXTENSION.len());
    }
}
