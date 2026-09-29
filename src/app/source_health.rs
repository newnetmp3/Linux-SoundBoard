use crate::myinstants;
use crate::online_audio;
use crate::public_clip_sites::{self, PublicClipSource};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceHealthStatus {
    Working,
    Failed,
}

#[derive(Debug, Clone)]
pub struct SourceHealth {
    pub source_index: u32,
    pub name: &'static str,
    pub status: SourceHealthStatus,
    pub detail: String,
}

impl SourceHealth {
    pub fn display_line(&self) -> String {
        let icon = match self.status {
            SourceHealthStatus::Working => "✓",
            SourceHealthStatus::Failed => "✗",
        };
        format!("{icon} {} — {}", self.name, self.detail)
    }
}

pub fn probe_release_source(source_index: u32) -> SourceHealth {
    let (name, result) = match source_index {
        0 => ("MyInstants", myinstants::probe_download_start()),
        1 => ("Tabletop Audio", online_audio::probe_tabletop_audio_start()),
        2 => ("OpenGameArt", online_audio::probe_opengameart_start()),
        3 => ("RPG Soundboard", online_audio::probe_rpg_soundboard_start()),
        4 => ("Kenney", online_audio::probe_kenney_start()),
        5 => (
            "Sound-Buttons.com",
            public_clip_sites::probe_download_start(PublicClipSource::SoundButtonsCom),
        ),
        6 => (
            "Movie Sound Clips",
            public_clip_sites::probe_download_start(PublicClipSource::MovieSoundClips),
        ),
        7 => (
            "My-Instants.com",
            public_clip_sites::probe_download_start(PublicClipSource::MyInstantsCom),
        ),
        8 => (
            "Orange Free Sounds",
            public_clip_sites::probe_download_start(PublicClipSource::OrangeFreeSounds),
        ),
        9 => (
            "SFX Library",
            public_clip_sites::probe_download_start(PublicClipSource::SfxLibrary),
        ),
        10 => (
            "Soundimage",
            public_clip_sites::probe_download_start(PublicClipSource::Soundimage),
        ),
        11 => ("OtoLogic", online_audio::probe_otologic_start()),
        _ => (
            "Unknown source",
            Err(format!("release source index {source_index} is not registered")),
        ),
    };

    source_health(source_index, name, result)
}

pub fn run_release() -> Vec<SourceHealth> {
    let mut results = Vec::with_capacity(12);

    push_probe(&mut results, 0, "MyInstants", myinstants::probe_download_start());
    push_probe(
        &mut results,
        1,
        "Tabletop Audio",
        online_audio::probe_tabletop_audio_start(),
    );
    push_probe(
        &mut results,
        2,
        "OpenGameArt",
        online_audio::probe_opengameart_start(),
    );

    push_probe(
        &mut results,
        3,
        "RPG Soundboard",
        online_audio::probe_rpg_soundboard_start(),
    );
    push_probe(
        &mut results,
        4,
        "Kenney",
        online_audio::probe_kenney_start(),
    );
    push_probe(
        &mut results,
        5,
        "Sound-Buttons.com",
        public_clip_sites::probe_download_start(PublicClipSource::SoundButtonsCom),
    );
    push_probe(
        &mut results,
        6,
        "Movie Sound Clips",
        public_clip_sites::probe_download_start(PublicClipSource::MovieSoundClips),
    );
    push_probe(
        &mut results,
        7,
        "My-Instants.com",
        public_clip_sites::probe_download_start(PublicClipSource::MyInstantsCom),
    );
    push_probe(
        &mut results,
        8,
        "Orange Free Sounds",
        public_clip_sites::probe_download_start(PublicClipSource::OrangeFreeSounds),
    );
    push_probe(
        &mut results,
        9,
        "SFX Library",
        public_clip_sites::probe_download_start(PublicClipSource::SfxLibrary),
    );
    push_probe(
        &mut results,
        10,
        "Soundimage",
        public_clip_sites::probe_download_start(PublicClipSource::Soundimage),
    );
    push_probe(
        &mut results,
        11,
        "OtoLogic",
        online_audio::probe_otologic_start(),
    );

    results
}

fn push_probe(
    results: &mut Vec<SourceHealth>,
    source_index: u32,
    name: &'static str,
    result: Result<String, String>,
) {
    results.push(source_health(source_index, name, result));
}

fn source_health(
    source_index: u32,
    name: &'static str,
    result: Result<String, String>,
) -> SourceHealth {
    match result {
        Ok(detail) => SourceHealth {
            source_index,
            name,
            status: SourceHealthStatus::Working,
            detail,
        },
        Err(detail) => SourceHealth {
            source_index,
            name,
            status: SourceHealthStatus::Failed,
            detail,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_lines_use_clear_status_markers() {
        let failed = SourceHealth {
            source_index: 5,
            name: "Ambient Mixer",
            status: SourceHealthStatus::Failed,
            detail: "HTML instead of MP3".to_string(),
        };
        assert!(failed.display_line().starts_with("✗ Ambient Mixer"));
    }
}
