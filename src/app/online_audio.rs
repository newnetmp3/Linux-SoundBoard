use rayon::prelude::*;
use serde::Deserialize;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use walkdir::WalkDir;

use crate::myinstants::{DownloadProgress, DownloadReport};

const USER_AGENT: &str =
    concat!("Linux-SoundBoard/", env!("CARGO_PKG_VERSION"), " online audio downloader");
const TABLETOP_HOME: &str = "https://tabletopaudio.com/";
const TABLETOP_AUDIO_ROOT: &str = "https://sounds.tabletopaudio.com";
const FREESOUND_SEARCH: &str = "https://freesound.org/apiv2/search/text/";
const RPG_SOUNDBOARD_HOME: &str = "https://rpgsoundboard.com/";
const TABLETOP_WORKERS: usize = 3;
const FREESOUND_WORKERS: usize = 4;
const MAX_FILE_STEM_BYTES: usize = 180;

pub const TABLETOP_COLLECTIONS: &[&str] = &[
    "Fantasy / D&D / RPG focused",
    "All 10-minute ambiences (large download)",
];

pub const OPENGAMEART_PACKS: &[(&str, &str)] = &[
    ("all", "All curated fantasy/RPG packs"),
    ("kenney-50", "50 RPG Sound Effects — Kenney — CC0"),
    ("rubberduck-80", "80 CC0 RPG SFX — rubberduck — CC0"),
    (
        "fantasy-library",
        "Fantasy Sound Effects Library — Little Robot Sound Factory — CC BY 3.0",
    ),
];

pub const AMBIENT_MIXER_CHOICES: &[(&str, &str, &str)] = &[
    ("all", "All curated D&D / fantasy atmospheres", ""),
    (
        "fantasy-tavern",
        "Fantasy Tavern",
        "https://rpg.ambient-mixer.com/fantasy-tavern",
    ),
    (
        "dnd-tavern-ambiance",
        "D&D Tavern Ambiance",
        "https://rpg.ambient-mixer.com/d-d-tavern-ambiance",
    ),
    (
        "dnd-fantasy-inn",
        "D&D Fantasy Inn / Pub / Tavern",
        "https://rpg.ambient-mixer.com/d-d-fantasy-inn-pub-tavern",
    ),
    (
        "bustling-tavern",
        "Bustling Tavern",
        "https://rpg.ambient-mixer.com/bustling-tavern",
    ),
    (
        "dnd-tavern-noise",
        "D&D Tavern Noise",
        "https://rpg.ambient-mixer.com/d-d-tavern-noise",
    ),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabletopCollection {
    FantasyRpg,
    All,
}

impl TabletopCollection {
    pub fn from_index(index: u32) -> Self {
        if index == 1 {
            Self::All
        } else {
            Self::FantasyRpg
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OnlineAudioError {
    #[error("curl is required for online sound downloads")]
    CurlMissing,
    #[error("unzip is required to unpack downloaded sound archives")]
    UnzipMissing,
    #[error("failed to create download directory: {0}")]
    CreateDirectory(#[source] io::Error),
    #[error("{source} request failed for {url}: {message}")]
    Request {
        source: &'static str,
        url: String,
        message: String,
    },
    #[error("could not parse {source} response: {message}")]
    Parse {
        source: &'static str,
        message: String,
    },
    #[error("could not extract '{archive}': {message}")]
    Extract { archive: String, message: String },
    #[error("failed to write download metadata: {0}")]
    WriteMetadata(#[source] io::Error),
    #[error("unsupported OpenGameArt pack selection '{0}'")]
    InvalidOpenGameArtPack(String),
    #[error("unsupported Ambient Mixer selection '{0}'")]
    InvalidAmbientMixerSelection(String),
    #[error("Freesound OAuth2 access token is required for original-file downloads")]
    FreesoundOauthMissing,
    #[error("Freesound search text is required")]
    FreesoundQueryMissing,
    #[error("failed to create download worker pool: {0}")]
    WorkerPool(String),
}

#[derive(Debug, Clone)]
struct TabletopTrack {
    title: String,
    filename: String,
    tags: Vec<String>,
    url: String,
}

#[derive(Debug, Clone, Copy)]
struct OpenGameArtPack {
    id: &'static str,
    title: &'static str,
    page_url: &'static str,
    archive_name: &'static str,
    license: &'static str,
    attribution: &'static str,
}

const OPEN_GAME_ART_CURATED: &[OpenGameArtPack] = &[
    OpenGameArtPack {
        id: "kenney-50",
        title: "50 RPG Sound Effects",
        page_url: "https://opengameart.org/content/50-rpg-sound-effects",
        archive_name: "RPGsounds_Kenney.zip",
        license: "CC0",
        attribution: "Kenney / Kenney.nl (credit optional under CC0)",
    },
    OpenGameArtPack {
        id: "rubberduck-80",
        title: "80 CC0 RPG SFX",
        page_url: "https://opengameart.org/content/80-cc0-rpg-sfx",
        archive_name: "80-CC0-RPG-SFX.zip",
        license: "CC0",
        attribution: "rubberduck",
    },
    OpenGameArtPack {
        id: "fantasy-library",
        title: "Fantasy Sound Effects Library",
        page_url: "https://opengameart.org/content/fantasy-sound-effects-library",
        archive_name: "Fantasy Sound Library.zip",
        license: "CC BY 3.0",
        attribution: "Little Robot Sound Factory — www.littlerobotsoundfactory.com",
    },
];

#[derive(Debug, Deserialize)]
struct FreesoundSearchResponse {
    results: Vec<FreesoundResult>,
}

#[derive(Debug, Deserialize)]
struct FreesoundResult {
    id: u64,
    name: String,
    username: String,
    license: String,
    url: String,
    #[serde(rename = "type")]
    file_type: String,
}

pub fn download_tabletop_audio(
    collection: TabletopCollection,
    output_root: &Path,
    progress: Sender<DownloadProgress>,
    cancelled: Arc<AtomicBool>,
) -> Result<DownloadReport, OnlineAudioError> {
    ensure_curl()?;
    let output_dir = output_root.join("Tabletop Audio");
    fs::create_dir_all(&output_dir).map_err(OnlineAudioError::CreateDirectory)?;

    send_progress(&progress, "Reading Tabletop Audio catalogue…", 0, None);
    let html = fetch_text("Tabletop Audio", TABLETOP_HOME, None)?;
    let mut tracks = parse_tabletop_tracks(&html);
    if collection == TabletopCollection::FantasyRpg {
        tracks.retain(tabletop_track_is_rpg);
    }
    tracks.sort_by(|a, b| a.filename.cmp(&b.filename));
    tracks.dedup_by(|a, b| a.filename == b.filename);

    if tracks.is_empty() {
        return Ok(DownloadReport::default());
    }

    write_tabletop_metadata(&output_dir, &tracks)?;
    let total = tracks.len();
    send_progress(
        &progress,
        &format!("Downloading {total} Tabletop Audio tracks…"),
        0,
        Some(total),
    );

    let processed = AtomicUsize::new(0);
    let downloaded = AtomicUsize::new(0);
    let reused = AtomicUsize::new(0);
    let failed = AtomicUsize::new(0);
    let paths = Mutex::new(Vec::<String>::new());

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(TABLETOP_WORKERS)
        .thread_name(|index| format!("tabletop-audio-{index}"))
        .build()
        .map_err(|error| OnlineAudioError::WorkerPool(error.to_string()))?;

    pool.install(|| {
        tracks.par_iter().for_each(|track| {
            if cancelled.load(Ordering::Relaxed) {
                return;
            }

            let final_path = output_dir.join(&track.filename);
            match download_binary("Tabletop Audio", &track.url, &final_path, None) {
                Ok(was_downloaded) => {
                    if was_downloaded {
                        downloaded.fetch_add(1, Ordering::Relaxed);
                    } else {
                        reused.fetch_add(1, Ordering::Relaxed);
                    }
                    if let Ok(mut paths) = paths.lock() {
                        paths.push(final_path.to_string_lossy().into_owned());
                    }
                }
                Err(error) => {
                    failed.fetch_add(1, Ordering::Relaxed);
                    log::warn!("Tabletop Audio download failed ({}): {}", track.url, error);
                }
            }

            let current = processed.fetch_add(1, Ordering::Relaxed) + 1;
            send_progress(
                &progress,
                &format!("Downloading Tabletop Audio… {current}/{total}"),
                current,
                Some(total),
            );
        });
    });

    Ok(report_from_counters(
        paths,
        downloaded,
        reused,
        failed,
        &cancelled,
    ))
}

pub fn download_opengameart(
    selection: &str,
    output_root: &Path,
    progress: Sender<DownloadProgress>,
    cancelled: Arc<AtomicBool>,
) -> Result<DownloadReport, OnlineAudioError> {
    ensure_curl()?;
    ensure_unzip()?;

    let packs = selected_opengameart_packs(selection)?;
    let output_dir = output_root.join("OpenGameArt");
    fs::create_dir_all(&output_dir).map_err(OnlineAudioError::CreateDirectory)?;

    let mut report = DownloadReport::default();
    let mut credits = vec![
        "OpenGameArt local fantasy/RPG sound packs".to_string(),
        "Check each source page for the complete license terms and attribution requirements."
            .to_string(),
        String::new(),
    ];

    for (index, pack) in packs.iter().enumerate() {
        if cancelled.load(Ordering::Relaxed) {
            report.cancelled = true;
            break;
        }

        send_progress(
            &progress,
            &format!(
                "Downloading OpenGameArt pack {}/{}: {}",
                index + 1,
                packs.len(),
                pack.title
            ),
            index,
            Some(packs.len()),
        );

        let pack_dir = output_dir.join(sanitize_file_stem(pack.title));
        fs::create_dir_all(&pack_dir).map_err(OnlineAudioError::CreateDirectory)?;
        let archive_path = pack_dir.join(pack.archive_name);

        let page_html = match fetch_text("OpenGameArt", pack.page_url, None) {
            Ok(html) => html,
            Err(error) => {
                report.failed += 1;
                log::warn!("OpenGameArt pack page failed: {error}");
                continue;
            }
        };
        let Some(archive_href) = find_href_containing(&page_html, ".zip") else {
            report.failed += 1;
            log::warn!("OpenGameArt pack '{}' did not expose a ZIP download", pack.title);
            continue;
        };
        let Some(archive_url) = absolute_url(pack.page_url, &archive_href) else {
            report.failed += 1;
            log::warn!("OpenGameArt pack '{}' ZIP URL could not be resolved", pack.title);
            continue;
        };

        match download_binary("OpenGameArt", &archive_url, &archive_path, None) {
            Ok(true) => report.downloaded += 1,
            Ok(false) => report.reused += 1,
            Err(error) => {
                report.failed += 1;
                log::warn!("OpenGameArt pack download failed: {error}");
                continue;
            }
        }

        let extracted_dir = pack_dir.join("audio");
        fs::create_dir_all(&extracted_dir).map_err(OnlineAudioError::CreateDirectory)?;
        extract_zip(&archive_path, &extracted_dir)?;
        report.paths.extend(collect_audio_files(&extracted_dir));

        credits.push(format!("{} — {}", pack.title, pack.license));
        credits.push(format!("Attribution: {}", pack.attribution));
        credits.push(format!("Source: {}", pack.page_url));
        credits.push(String::new());

        send_progress(
            &progress,
            &format!("Extracted {}", pack.title),
            index + 1,
            Some(packs.len()),
        );
    }

    report.paths.sort();
    report.paths.dedup();
    fs::write(output_dir.join("_OpenGameArt_CREDITS.txt"), credits.join("\n"))
        .map_err(OnlineAudioError::WriteMetadata)?;
    Ok(report)
}

pub fn download_freesound_originals(
    query: &str,
    oauth_access_token: &str,
    limit: usize,
    output_root: &Path,
    progress: Sender<DownloadProgress>,
    cancelled: Arc<AtomicBool>,
) -> Result<DownloadReport, OnlineAudioError> {
    ensure_curl()?;
    let query = query.trim();
    let token = oauth_access_token.trim();
    if query.is_empty() {
        return Err(OnlineAudioError::FreesoundQueryMissing);
    }
    if token.is_empty() {
        return Err(OnlineAudioError::FreesoundOauthMissing);
    }

    let output_dir = output_root.join("Freesound").join(sanitize_file_stem(query));
    fs::create_dir_all(&output_dir).map_err(OnlineAudioError::CreateDirectory)?;

    send_progress(&progress, "Searching Freesound originals…", 0, None);
    let page_size = limit.clamp(1, 150);
    let bearer = format!("Authorization: Bearer {token}");
    let json = fetch_freesound_search(query, page_size, &bearer)?;
    let response: FreesoundSearchResponse =
        serde_json::from_str(&json).map_err(|error| OnlineAudioError::Parse {
            source: "Freesound",
            message: error.to_string(),
        })?;

    if response.results.is_empty() {
        return Ok(DownloadReport::default());
    }

    let results = response.results;
    let total = results.len();

    fs::write(
        output_dir.join("_Freesound_CREDITS.txt"),
        freesound_credits(query, &results),
    )
    .map_err(OnlineAudioError::WriteMetadata)?;

    let processed = AtomicUsize::new(0);
    let downloaded = AtomicUsize::new(0);
    let reused = AtomicUsize::new(0);
    let failed = AtomicUsize::new(0);
    let paths = Mutex::new(Vec::<String>::new());

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(FREESOUND_WORKERS)
        .thread_name(|index| format!("freesound-original-{index}"))
        .build()
        .map_err(|error| OnlineAudioError::WorkerPool(error.to_string()))?;

    pool.install(|| {
        results.par_iter().for_each(|sound| {
            if cancelled.load(Ordering::Relaxed) {
                return;
            }

            let extension = normalized_audio_extension(&sound.file_type).unwrap_or("bin");
            let filename = format!(
                "{}-{}.{}",
                sanitize_file_stem(&sound.name),
                sound.id,
                extension
            );
            let final_path = output_dir.join(filename);
            let download_url = format!("https://freesound.org/apiv2/sounds/{}/download/", sound.id);

            match download_binary("Freesound", &download_url, &final_path, Some(&bearer)) {
                Ok(was_downloaded) => {
                    if was_downloaded {
                        downloaded.fetch_add(1, Ordering::Relaxed);
                    } else {
                        reused.fetch_add(1, Ordering::Relaxed);
                    }
                    if is_audio_extension(extension) {
                        if let Ok(mut paths) = paths.lock() {
                            paths.push(final_path.to_string_lossy().into_owned());
                        }
                    }
                }
                Err(error) => {
                    failed.fetch_add(1, Ordering::Relaxed);
                    log::warn!("Freesound original download failed: {error}");
                }
            }

            let current = processed.fetch_add(1, Ordering::Relaxed) + 1;
            send_progress(
                &progress,
                &format!("Downloading Freesound originals… {current}/{total}"),
                current,
                Some(total),
            );
        });
    });

    Ok(report_from_counters(
        paths,
        downloaded,
        reused,
        failed,
        &cancelled,
    ))
}

pub fn download_rpg_soundboard_pack(
    output_root: &Path,
    progress: Sender<DownloadProgress>,
    cancelled: Arc<AtomicBool>,
) -> Result<DownloadReport, OnlineAudioError> {
    ensure_curl()?;
    ensure_unzip()?;

    let output_dir = output_root.join("RPG Soundboard");
    fs::create_dir_all(&output_dir).map_err(OnlineAudioError::CreateDirectory)?;

    send_progress(
        &progress,
        "Finding RPG Soundboard Medieval Fantasy pack…",
        0,
        None,
    );
    let html = fetch_text("RPG Soundboard", RPG_SOUNDBOARD_HOME, None)?;
    let href = find_href_containing(&html, ".rpsb").ok_or_else(|| OnlineAudioError::Parse {
        source: "RPG Soundboard",
        message: "the free Medieval Fantasy .rpsb download link was not found".to_string(),
    })?;
    let download_url = absolute_url(RPG_SOUNDBOARD_HOME, &href).ok_or_else(|| {
        OnlineAudioError::Parse {
            source: "RPG Soundboard",
            message: "the .rpsb download URL could not be resolved".to_string(),
        }
    })?;

    let archive_path = output_dir.join("medieval_fantasy.rpsb");
    let was_downloaded =
        download_binary("RPG Soundboard", &download_url, &archive_path, None)?;

    if cancelled.load(Ordering::Relaxed) {
        return Ok(DownloadReport {
            downloaded: if was_downloaded { 1 } else { 0 },
            reused: if was_downloaded { 0 } else { 1 },
            cancelled: true,
            ..DownloadReport::default()
        });
    }

    send_progress(&progress, "Extracting Medieval Fantasy soundboard…", 1, Some(2));
    let extracted_dir = output_dir.join("Medieval Fantasy");
    fs::create_dir_all(&extracted_dir).map_err(OnlineAudioError::CreateDirectory)?;
    extract_zip(&archive_path, &extracted_dir)?;
    let mut paths = collect_audio_files(&extracted_dir);
    paths.sort();
    paths.dedup();

    fs::write(
        output_dir.join("_RPG_Soundboard_CREDITS.txt"),
        "RPG Soundboard — Medieval Fantasy free soundboard\n\
Source: https://rpgsoundboard.com/\n\
The source site lists track-by-track credits. Preserve those credits when redistributing.\n\
Known music credits include Kira Daly (CC BY), Strobotone (CC BY-ND), cymbalBird (CC BY), and Maarten Schellekens (CC0/Public Domain).\n",
    )
    .map_err(OnlineAudioError::WriteMetadata)?;

    send_progress(&progress, "RPG Soundboard pack ready", 2, Some(2));
    Ok(DownloadReport {
        paths,
        downloaded: if was_downloaded { 1 } else { 0 },
        reused: if was_downloaded { 0 } else { 1 },
        failed: 0,
        cancelled: false,
    })
}

pub fn download_ambient_mixer(
    selection: &str,
    output_root: &Path,
    progress: Sender<DownloadProgress>,
    cancelled: Arc<AtomicBool>,
) -> Result<DownloadReport, OnlineAudioError> {
    ensure_curl()?;
    let choices = selected_ambient_mixer(selection)?;
    let output_dir = output_root.join("Ambient Mixer");
    fs::create_dir_all(&output_dir).map_err(OnlineAudioError::CreateDirectory)?;

    let mut report = DownloadReport::default();
    let mut credits = vec![
        "Ambient Mixer curated D&D/fantasy atmospheres".to_string(),
        "Each atmosphere page lists its own Creative Commons/license details and source samples."
            .to_string(),
        String::new(),
    ];

    for (index, (_, label, page_url)) in choices.iter().enumerate() {
        if cancelled.load(Ordering::Relaxed) {
            report.cancelled = true;
            break;
        }

        send_progress(
            &progress,
            &format!("Resolving Ambient Mixer {}/{}: {label}", index + 1, choices.len()),
            index,
            Some(choices.len()),
        );

        let html = fetch_text("Ambient Mixer", page_url, None)?;
        let href = find_download_audio_href(&html).ok_or_else(|| OnlineAudioError::Parse {
            source: "Ambient Mixer",
            message: format!("download link was not found on {page_url}"),
        })?;
        let download_url =
            absolute_url(page_url, &href).ok_or_else(|| OnlineAudioError::Parse {
                source: "Ambient Mixer",
                message: format!("download URL could not be resolved on {page_url}"),
            })?;

        let final_path = output_dir.join(format!("{}.mp3", sanitize_file_stem(label)));
        match download_binary("Ambient Mixer", &download_url, &final_path, None) {
            Ok(true) => {
                report.downloaded += 1;
                report.paths.push(final_path.to_string_lossy().into_owned());
            }
            Ok(false) => {
                report.reused += 1;
                report.paths.push(final_path.to_string_lossy().into_owned());
            }
            Err(error) => {
                report.failed += 1;
                log::warn!("Ambient Mixer download failed ({page_url}): {error}");
            }
        }

        credits.push(format!("{label} — {page_url}"));
        send_progress(
            &progress,
            &format!("Ambient Mixer… {}/{}", index + 1, choices.len()),
            index + 1,
            Some(choices.len()),
        );
    }

    report.paths.sort();
    report.paths.dedup();
    fs::write(output_dir.join("_Ambient_Mixer_CREDITS.txt"), credits.join("\n"))
        .map_err(OnlineAudioError::WriteMetadata)?;
    Ok(report)
}

fn selected_opengameart_packs(
    selection: &str,
) -> Result<Vec<OpenGameArtPack>, OnlineAudioError> {
    if selection == "all" {
        return Ok(OPEN_GAME_ART_CURATED.to_vec());
    }
    OPEN_GAME_ART_CURATED
        .iter()
        .copied()
        .find(|pack| pack.id == selection)
        .map(|pack| vec![pack])
        .ok_or_else(|| OnlineAudioError::InvalidOpenGameArtPack(selection.to_string()))
}

fn selected_ambient_mixer(
    selection: &str,
) -> Result<Vec<(&'static str, &'static str, &'static str)>, OnlineAudioError> {
    if selection == "all" {
        return Ok(AMBIENT_MIXER_CHOICES.iter().skip(1).copied().collect());
    }
    AMBIENT_MIXER_CHOICES
        .iter()
        .copied()
        .find(|(id, _, _)| *id == selection)
        .map(|choice| vec![choice])
        .ok_or_else(|| OnlineAudioError::InvalidAmbientMixerSelection(selection.to_string()))
}

fn parse_tabletop_tracks(html: &str) -> Vec<TabletopTrack> {
    let marker = "class=\"col-md-3 mix";
    let mut tracks = Vec::new();
    let mut offset = 0usize;

    while let Some(relative_start) = html[offset..].find(marker) {
        let start = offset + relative_start;
        let next = html[start + marker.len()..]
            .find(marker)
            .map(|relative| start + marker.len() + relative)
            .unwrap_or(html.len());
        let block = &html[start..next];

        let Some(raw_name) = between(block, "onclick=\"saveAs('", "')") else {
            offset = next;
            continue;
        };
        let source_name = raw_name.trim();
        if source_name.is_empty() || source_name.contains('/') || source_name.contains('\\') {
            offset = next;
            continue;
        }

        let title = between(block, "<h3>", "</h3>")
            .map(strip_tags)
            .map(|value| decode_html_entities(&value))
            .unwrap_or_else(|| source_name.replace('_', " "));
        let class_tail = between(block, marker, "\"").unwrap_or_default();
        let tags = class_tail
            .split_whitespace()
            .map(str::to_string)
            .collect::<Vec<_>>();
        let filename = format!("{source_name}.mp3");

        tracks.push(TabletopTrack {
            title,
            url: format!("{TABLETOP_AUDIO_ROOT}/{filename}"),
            filename,
            tags,
        });
        offset = next;
    }

    tracks
}

fn tabletop_track_is_rpg(track: &TabletopTrack) -> bool {
    const KEYWORDS: &[&str] = &[
        "fantasy", "rpg", "medieval", "dungeon", "tavern", "inn", "castle", "dragon",
        "village", "forest", "swamp", "cave", "cavern", "battle", "combat", "magic",
        "temple", "ruin", "barrow", "crypt", "witch", "vamp", "giant", "drow",
        "kingdom", "bazaar", "market", "manor", "grave", "haunted", "pirate", "harbor",
        "docks", "monastery", "orc", "goblin", "elf", "fey", "royal", "crown", "mystic",
        "adventure",
    ];
    let searchable = format!("{} {}", track.title, track.tags.join(" ")).to_ascii_lowercase();
    KEYWORDS.iter().any(|keyword| searchable.contains(keyword))
}

fn write_tabletop_metadata(
    output_dir: &Path,
    tracks: &[TabletopTrack],
) -> Result<(), OnlineAudioError> {
    let mut text = String::from(
        "Tabletop Audio 10-minute ambiences\n\
License: Creative Commons Attribution-NonCommercial-NoDerivatives 4.0 International\n\
Source: https://tabletopaudio.com/\n\
Only the site's main 10-minute ambience tracks are downloaded. SoundPad audio is intentionally excluded.\n\n",
    );
    for track in tracks {
        text.push_str(&format!(
            "{} | {} | tags: {}\n",
            track.title,
            track.url,
            track.tags.join(", ")
        ));
    }
    fs::write(output_dir.join("_TabletopAudio_LICENSE.txt"), text)
        .map_err(OnlineAudioError::WriteMetadata)
}

fn freesound_credits(query: &str, results: &[FreesoundResult]) -> String {
    let mut text = format!(
        "Freesound original files downloaded through APIv2 OAuth2.\n\
No previews are downloaded. Each sound remains subject to its listed license.\n\
Search: {query}\n\n"
    );
    for sound in results {
        text.push_str(&format!(
            "{} | by {} | {} | {}\n",
            sound.name, sound.username, sound.license, sound.url
        ));
    }
    text
}

fn fetch_freesound_search(
    query: &str,
    page_size: usize,
    bearer_header: &str,
) -> Result<String, OnlineAudioError> {
    let fields = "id,name,username,license,url,type";
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
            "--header",
            bearer_header,
            "--get",
            "--data-urlencode",
            &format!("query={query}"),
            "--data-urlencode",
            &format!("fields={fields}"),
            "--data-urlencode",
            &format!("page_size={page_size}"),
            FREESOUND_SEARCH,
        ])
        .output()
        .map_err(|error| OnlineAudioError::Request {
            source: "Freesound",
            url: FREESOUND_SEARCH.to_string(),
            message: error.to_string(),
        })?;

    if !output.status.success() {
        return Err(OnlineAudioError::Request {
            source: "Freesound",
            url: FREESOUND_SEARCH.to_string(),
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    String::from_utf8(output.stdout).map_err(|error| OnlineAudioError::Parse {
        source: "Freesound",
        message: format!("response was not valid UTF-8: {error}"),
    })
}

fn ensure_curl() -> Result<(), OnlineAudioError> {
    if which::which("curl").is_err() {
        Err(OnlineAudioError::CurlMissing)
    } else {
        Ok(())
    }
}

fn ensure_unzip() -> Result<(), OnlineAudioError> {
    if which::which("unzip").is_err() {
        Err(OnlineAudioError::UnzipMissing)
    } else {
        Ok(())
    }
}

fn fetch_text(
    source: &'static str,
    url: &str,
    authorization: Option<&str>,
) -> Result<String, OnlineAudioError> {
    let mut command = Command::new("curl");
    command.args([
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
    ]);
    if let Some(header) = authorization {
        command.args(["--header", header]);
    }
    command.arg(url);

    let output = command.output().map_err(|error| OnlineAudioError::Request {
        source,
        url: url.to_string(),
        message: error.to_string(),
    })?;

    if !output.status.success() {
        return Err(OnlineAudioError::Request {
            source,
            url: url.to_string(),
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    String::from_utf8(output.stdout).map_err(|error| OnlineAudioError::Parse {
        source,
        message: format!("response was not valid UTF-8: {error}"),
    })
}

fn download_binary(
    source: &'static str,
    url: &str,
    final_path: &Path,
    authorization: Option<&str>,
) -> Result<bool, OnlineAudioError> {
    if final_path
        .metadata()
        .map(|metadata| metadata.len() > 0)
        .unwrap_or(false)
    {
        return Ok(false);
    }
    if let Some(parent) = final_path.parent() {
        fs::create_dir_all(parent).map_err(OnlineAudioError::CreateDirectory)?;
    }

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
        "600",
        "--retry",
        "3",
        "--retry-delay",
        "1",
        "--user-agent",
        USER_AGENT,
        "--write-out",
        "%{content_type}",
    ]);
    if let Some(header) = authorization {
        command.args(["--header", header]);
    }
    if resume {
        command.args(["--continue-at", "-"]);
    }
    command.arg("--output").arg(&part_path).arg(url);

    let output = command.output().map_err(|error| OnlineAudioError::Request {
        source,
        url: url.to_string(),
        message: error.to_string(),
    })?;

    if !output.status.success() && resume {
        let _ = fs::remove_file(&part_path);
        return download_binary(source, url, final_path, authorization);
    }
    if !output.status.success() {
        return Err(OnlineAudioError::Request {
            source,
            url: url.to_string(),
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    let content_type = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
    if content_type.contains("text/html") {
        let _ = fs::remove_file(&part_path);
        return Err(OnlineAudioError::Request {
            source,
            url: url.to_string(),
            message: "server returned an HTML page instead of an audio/archive file".to_string(),
        });
    }

    fs::rename(&part_path, final_path).map_err(|error| OnlineAudioError::Request {
        source,
        url: url.to_string(),
        message: format!("failed to finalize '{}': {error}", final_path.display()),
    })?;
    Ok(true)
}

fn extract_zip(archive: &Path, output_dir: &Path) -> Result<(), OnlineAudioError> {
    let output = Command::new("unzip")
        .arg("-q")
        .arg("-o")
        .arg(archive)
        .arg("-d")
        .arg(output_dir)
        .output()
        .map_err(|error| OnlineAudioError::Extract {
            archive: archive.display().to_string(),
            message: error.to_string(),
        })?;

    if !output.status.success() {
        return Err(OnlineAudioError::Extract {
            archive: archive.display().to_string(),
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(())
}

fn collect_audio_files(root: &Path) -> Vec<String> {
    let mut paths = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| {
            let path = entry.into_path();
            let extension = path.extension()?.to_string_lossy().to_ascii_lowercase();
            is_audio_extension(&extension).then(|| path.to_string_lossy().into_owned())
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    paths
}

fn is_audio_extension(extension: &str) -> bool {
    matches!(
        extension,
        "mp3" | "wav" | "ogg" | "oga" | "flac" | "aac" | "m4a" | "mp4"
    )
}

fn normalized_audio_extension(file_type: &str) -> Option<&'static str> {
    match file_type.trim().to_ascii_lowercase().as_str() {
        "wav" | "wave" => Some("wav"),
        "aif" | "aiff" => Some("aif"),
        "flac" => Some("flac"),
        "ogg" | "oga" => Some("ogg"),
        "mp3" | "mpeg" => Some("mp3"),
        "m4a" => Some("m4a"),
        _ => None,
    }
}

fn find_href_containing(html: &str, needle: &str) -> Option<String> {
    quoted_attribute_values(html, "href")
        .into_iter()
        .find(|href| href.to_ascii_lowercase().contains(&needle.to_ascii_lowercase()))
}

fn find_download_audio_href(html: &str) -> Option<String> {
    let lowered = html.to_ascii_lowercase();
    for phrase in ["download audio", "download atmosphere", "download"] {
        let mut start = 0usize;
        while let Some(relative) = lowered[start..].find(phrase) {
            let index = start + relative;
            let prefix_start = index.saturating_sub(1200);
            let prefix = &html[prefix_start..index];
            if let Some(anchor_start_rel) = prefix.rfind("<a") {
                let anchor = &prefix[anchor_start_rel..];
                if let Some(href) = quoted_attribute_values(anchor, "href").into_iter().next() {
                    if !href.starts_with('#') && !href.to_ascii_lowercase().contains("login") {
                        return Some(href);
                    }
                }
            }
            start = index + phrase.len();
        }
    }

    quoted_attribute_values(html, "href").into_iter().find(|href| {
        let lower = href.to_ascii_lowercase();
        (lower.contains("download") || lower.contains("audio"))
            && (lower.contains(".mp3") || lower.contains("download"))
    })
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
            if rest.len() > 1 {
                rest = &rest[1..];
                continue;
            }
            break;
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

fn between(value: &str, start: &str, end: &str) -> Option<String> {
    let start_index = value.find(start)? + start.len();
    let end_index = value[start_index..].find(end)? + start_index;
    Some(value[start_index..end_index].to_string())
}

fn sanitize_file_stem(value: &str) -> String {
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
        "online-sound".to_string()
    } else {
        out.to_string()
    }
}

fn strip_tags(value: String) -> String {
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

fn report_from_counters(
    paths: Mutex<Vec<String>>,
    downloaded: AtomicUsize,
    reused: AtomicUsize,
    failed: AtomicUsize,
    cancelled: &AtomicBool,
) -> DownloadReport {
    let mut paths = paths.into_inner().unwrap_or_default();
    paths.sort();
    paths.dedup();
    DownloadReport {
        paths,
        downloaded: downloaded.load(Ordering::Relaxed),
        reused: reused.load(Ordering::Relaxed),
        failed: failed.load(Ordering::Relaxed),
        cancelled: cancelled.load(Ordering::Relaxed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_tabletop_tracks() {
        let html = r#"
            <div class="col-md-3 mix fantasy medieval">
              <div class="track_title"><h3>Dark Tavern</h3></div>
              <span class="saveButton"><a onclick="saveAs('380_The_Great_Lift')">Save</a></span>
            </div>
        "#;
        let tracks = parse_tabletop_tracks(html);
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].filename, "380_The_Great_Lift.mp3");
        assert!(tracks[0].url.ends_with("/380_The_Great_Lift.mp3"));
    }

    #[test]
    fn filters_tabletop_for_rpg_terms() {
        let track = TabletopTrack {
            title: "Dragon's Cave".to_string(),
            filename: "a.mp3".to_string(),
            tags: vec!["fantasy".to_string()],
            url: "https://example.invalid/a.mp3".to_string(),
        };
        assert!(tabletop_track_is_rpg(&track));
    }

    #[test]
    fn resolves_relative_urls() {
        assert_eq!(
            absolute_url("https://example.com/path/page", "/audio/file.mp3").as_deref(),
            Some("https://example.com/audio/file.mp3")
        );
        assert_eq!(
            absolute_url("https://example.com/path/page", "file.mp3").as_deref(),
            Some("https://example.com/path/file.mp3")
        );
    }

    #[test]
    fn finds_archive_href() {
        let html = r#"<a href="/files/medieval_fantasy.rpsb">Download Free</a>"#;
        assert_eq!(
            find_href_containing(html, ".rpsb").as_deref(),
            Some("/files/medieval_fantasy.rpsb")
        );
    }

    #[test]
    fn finds_ambient_download_link_by_text() {
        let html = r#"<div><a href="/download/1234">Download audio</a></div>"#;
        assert_eq!(
            find_download_audio_href(html).as_deref(),
            Some("/download/1234")
        );
    }

    #[test]
    fn original_freesound_types_map_to_local_extensions() {
        assert_eq!(normalized_audio_extension("wav"), Some("wav"));
        assert_eq!(normalized_audio_extension("aiff"), Some("aif"));
        assert_eq!(normalized_audio_extension("mp3"), Some("mp3"));
        assert_eq!(normalized_audio_extension("unknown"), None);
    }
}
