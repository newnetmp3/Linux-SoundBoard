# Online Source Integration Status

Linux Soundboard treats the 100 requested websites as a **candidate registry**. A site is only marked **Working** after its current download flow has been validated and an adapter has been implemented. A site's own claim that it offers downloads is not enough by itself.

## Implemented and working

| ID | Source | Integration |
| ---: | --- | --- |
| 1 | Myinstants | Country-index integration using the site's public `?page=N` listing pagination, clean filenames, validation, and local source metadata |
| 9 | Sound-Buttons.com | Browse/filter public meme/reaction sounds, preview full MP3, select individual sounds, validated download, JSON source metadata |
| 16 | Movie Sound Clips | Browse/filter the site's Sound Effects library, preview/select public audio, CC BY-NC 3.0 metadata and attribution |
| 20 | My-Instants.com | Browse/filter public trending clips, resolve full MP3 from detail pages, preview/select, validated download and terms metadata |
| 21 | Freesound | Existing APIv2 OAuth2 integration; original uploaded files only; previews are deliberately not substituted |
| 69 | OpenGameArt | Existing curated fantasy/RPG pack integration |
| 70 | Kenney | Existing RPG Audio pack integration |
| 99 | Tabletop Audio | Existing public 10-minute ambience integration |
| 100 | Ambient Mixer | Existing curated atmosphere integration when a public downloadable-audio link is exposed |

The existing RPG Soundboard Medieval Fantasy pack is also supported even though it is not one of the 100 requested candidates.

## Priority candidates 1–20 still under validation

| ID | Source | Current status / limitation |
| ---: | --- | --- |
| 2 | Voicy | Site and official API expose search/download concepts and creator metadata, but the API requires an issued API key and the public full-file endpoint has not yet been independently validated by this integration |
| 3 | Voicemod Tuna | Public sound detail pages support play/download language and expose creator data; full downloadable media endpoint not yet validated |
| 4 | Soundboards.gg | Sound pages expose download UI, uploader and MP3 metadata; full public media endpoint not yet validated, and whole-board download has authentication constraints |
| 5 | 101soundboards | Candidate; automated validation currently encounters access restrictions |
| 6 | Soundboard.com | Candidate; download flow presents human verification, so no bypass is implemented |
| 7 | BoardSounds | Candidate; full public audio endpoint not yet validated |
| 8 | TapSounds | Site clearly advertises account-free MP3 downloads and exposes titles/creators/durations; underlying full-file URL still needs independent validation |
| 10 | SoundButtons.io | Candidate; full public audio endpoint not yet validated |
| 11 | SoundButtons.net | Detail pages advertise MP3 download; underlying full-file endpoint not yet validated |
| 12 | SoundButtonsWorld | Candidate; full public audio endpoint not yet validated |
| 13 | Meme Instants | Candidate; full public audio endpoint not yet validated |
| 14 | Realm of Darkness | Large browsable soundboard catalogue confirmed; button audio endpoint not yet validated |
| 15 | WavSource | Candidate; downloadable full-file path not yet validated |
| 17 | Zedge | Candidate; no public full-quality flow has been validated for this integration |
| 18 | Myinstants.net | Candidate; full public audio endpoint not yet validated |
| 19 | Myinstants.app | Site advertises free MP3 download without registration; underlying full-file endpoint still needs independent validation |

## Candidates 22–98

All remaining requested sources are present in `src/app/source_registry.rs` as candidates. They are intentionally **not** presented as working integrations until their download/API/pack flow is validated. Commercial libraries, account-gated services and marketplaces will require their documented authenticated flow where available; Linux Soundboard will not bypass login walls, paywalls, CAPTCHAs, DRM or anti-bot controls.

## Download integrity and provenance

For integrations using the common downloader:

- completed and cached downloads are signature-checked before import/reuse;
- HTML/JSON error responses are rejected instead of being saved under audio filenames;
- filenames are sanitized and collisions receive human-readable numeric suffixes;
- a `.source.json` sidecar records source URL, title, creator, media/preview URL, license/terms and attribution when available;
- packs are kept as archives and extracted separately by the pack-oriented adapters;
- the online-download root is scanned into the library so each source subfolder appears under **FOLDERS**.

## Folder visibility behavior

The **FOLDERS** list acts as a playlist visibility filter. Selecting a source folder shows that source's files in the main sound list; selecting the same active folder again returns to **General** (all visible sounds). The source subfolders are derived from actual on-disk locations rather than a separate synthetic source list.
