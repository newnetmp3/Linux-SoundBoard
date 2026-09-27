use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use gtk4::prelude::*;
use gtk4::{
    Adjustment, Align, Box as GtkBox, Button, CheckButton, DropDown, Entry, Label, Orientation,
    PasswordEntry, ProgressBar, ScrolledWindow, SpinButton, Stack, Window,
};

use crate::app_state::AppState;
use crate::commands;
use crate::myinstants::{self, DownloadReport};
use crate::online_audio::{self, TabletopCollection};
use crate::public_clip_sites::{self, PublicClipSource};
use crate::source_registry;

use super::sound_list::SoundList;

const SOURCE_MYINSTANTS: u32 = 0;
const SOURCE_TABLETOP_AUDIO: u32 = 1;
const SOURCE_OPENGAMEART: u32 = 2;
const SOURCE_FREESOUND: u32 = 3;
const SOURCE_RPG_SOUNDBOARD: u32 = 4;
const SOURCE_AMBIENT_MIXER: u32 = 5;
const SOURCE_KENNEY: u32 = 6;
const SOURCE_SOUND_BUTTONS_COM: u32 = 7;
const SOURCE_MOVIE_SOUND_CLIPS: u32 = 8;
const SOURCE_MY_INSTANTS_COM: u32 = 9;

const SOURCE_LABELS: &[&str] = &[
    "MyInstants",
    "Tabletop Audio — D&D / fantasy ambience",
    "OpenGameArt — fantasy/RPG sound packs",
    "Freesound — original files only",
    "RPG Soundboard — free Medieval Fantasy pack",
    "Ambient Mixer — D&D / fantasy atmospheres",
    "Kenney — RPG Audio (50 CC0 sounds)",
    "Sound-Buttons.com — memes / reactions",
    "Movie Sound Clips — free sound-effects library",
    "My-Instants.com — trending memes / reactions",
];

pub(super) fn show_downloader(parent: &gtk4::Window, state: Arc<AppState>, sound_list: SoundList) {
    let window = Window::builder()
        .title("Online Sound Downloader")
        .transient_for(parent)
        .modal(true)
        .default_width(700)
        .default_height(560)
        .resizable(true)
        .build();

    let content = GtkBox::new(Orientation::Vertical, 12);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    content.set_margin_start(18);
    content.set_margin_end(18);

    let working_sources = source_registry::working_sources().collect::<Vec<_>>();
    let intro = Label::new(Some(&format!(
        "Download local copies from tested online sound libraries and import supported audio directly into the selected soundboard tab. {} working integrations are active from {} tracked candidate sites.",
        working_sources.len(),
        source_registry::SOURCES.len()
    )));
    intro.set_wrap(true);
    intro.set_xalign(0.0);
    let registry_tooltip = working_sources
        .iter()
        .map(|source| format!("#{} {} — {}", source.id, source.name, source.url))
        .collect::<Vec<_>>()
        .join("\n");
    intro.set_tooltip_text(Some(&format!(
        "Currently tested integrations from the candidate registry:\n{registry_tooltip}"
    )));
    content.append(&intro);

    let source_label = Label::new(Some("Source"));
    source_label.set_xalign(0.0);
    source_label.add_css_class("heading");
    content.append(&source_label);

    let source = DropDown::from_strings(SOURCE_LABELS);
    source.set_selected(SOURCE_MYINSTANTS);
    source.set_hexpand(true);
    content.append(&source);

    let options_stack = Stack::builder()
        .transition_type(gtk4::StackTransitionType::Crossfade)
        .hexpand(true)
        .build();

    let myinstants_box = GtkBox::new(Orientation::Vertical, 6);
    let myinstants_note = source_note(
        "Country index downloads follow MyInstants' paginated listing feed until no new sounds remain. Existing files are reused, interrupted downloads resume, and filenames follow the visible sound name.",
    );
    myinstants_box.append(&myinstants_note);
    let mut country_labels = vec!["All English-speaking indexes"];
    country_labels.extend(myinstants::COUNTRY_CHOICES.iter().map(|(_, label)| *label));
    let country = DropDown::from_strings(&country_labels);
    country.set_selected(1);
    country.set_hexpand(true);
    myinstants_box.append(&country);
    options_stack.add_named(&myinstants_box, Some("myinstants"));

    let tabletop_box = GtkBox::new(Orientation::Vertical, 6);
    tabletop_box.append(&source_note(
        "Downloads Tabletop Audio's public 10-minute ambience tracks as local MP3 files. SoundPad clips are intentionally excluded.",
    ));
    let tabletop_collection = DropDown::from_strings(online_audio::TABLETOP_COLLECTIONS);
    tabletop_collection.set_selected(0);
    tabletop_collection.set_hexpand(true);
    tabletop_box.append(&tabletop_collection);
    options_stack.add_named(&tabletop_box, Some("tabletop"));

    let opengameart_box = GtkBox::new(Orientation::Vertical, 6);
    opengameart_box.append(&source_note(
        "Curated downloadable fantasy/RPG packs. ZIP archives are kept locally, extracted, and their audio files are imported.",
    ));
    let opengameart_labels = online_audio::OPENGAMEART_PACKS
        .iter()
        .map(|(_, label)| *label)
        .collect::<Vec<_>>();
    let opengameart_pack = DropDown::from_strings(&opengameart_labels);
    opengameart_pack.set_selected(0);
    opengameart_pack.set_hexpand(true);
    opengameart_box.append(&opengameart_pack);
    options_stack.add_named(&opengameart_box, Some("opengameart"));

    let freesound_box = GtkBox::new(Orientation::Vertical, 6);
    freesound_box.append(&source_note(
        "Original Freesound uploads only — no previews. Freesound requires an OAuth2 access token for original-file downloads.",
    ));
    let freesound_help = Label::new(None);
    freesound_help.set_xalign(0.0);
    freesound_help.set_wrap(true);
    freesound_help.set_markup(
        "Create Freesound API credentials and authorize your account using the <a href=\"https://freesound.org/docs/api/authentication.html\">official OAuth2 instructions</a>, then paste the temporary access token below.",
    );
    freesound_box.append(&freesound_help);

    let freesound_query = Entry::builder()
        .placeholder_text("Search, e.g. dungeon tavern dragon sword magic")
        .hexpand(true)
        .build();
    freesound_box.append(&freesound_query);

    let freesound_token = PasswordEntry::builder()
        .placeholder_text("OAuth2 access token")
        .show_peek_icon(true)
        .hexpand(true)
        .build();
    freesound_box.append(&freesound_token);

    let limit_row = GtkBox::new(Orientation::Horizontal, 8);
    let limit_label = Label::new(Some("Maximum originals"));
    limit_label.set_xalign(0.0);
    limit_label.set_hexpand(true);
    limit_row.append(&limit_label);
    let limit_adjustment = Adjustment::new(25.0, 1.0, 150.0, 1.0, 10.0, 0.0);
    let freesound_limit = SpinButton::new(Some(&limit_adjustment), 1.0, 0);
    limit_row.append(&freesound_limit);
    freesound_box.append(&limit_row);
    options_stack.add_named(&freesound_box, Some("freesound"));

    let rpg_soundboard_box = GtkBox::new(Orientation::Vertical, 6);
    rpg_soundboard_box.append(&source_note(
        "Downloads RPG Soundboard's publicly offered Medieval Fantasy starter soundboard, keeps the .rpsb archive locally, extracts its bundled audio, and imports supported files.",
    ));
    let rpg_size = Label::new(Some("Published pack size: about 457 MB"));
    rpg_size.set_xalign(0.0);
    rpg_size.add_css_class("dim-label");
    rpg_soundboard_box.append(&rpg_size);
    options_stack.add_named(&rpg_soundboard_box, Some("rpg-soundboard"));

    let kenney_box = GtkBox::new(Orientation::Vertical, 6);
    kenney_box.append(&source_note(
        "Downloads Kenney's 50-file RPG Audio pack directly from Kenney, keeps the ZIP locally, extracts it, and imports supported audio. License: CC0.",
    ));
    options_stack.add_named(&kenney_box, Some("kenney"));

    let public_clips_box = GtkBox::new(Orientation::Vertical, 6);
    public_clips_box.append(&source_note(
        "Browse the source's validated public full-audio links. The optional filter is matched against sound titles before downloading. Source metadata is saved beside every audio file.",
    ));
    let public_clip_query = Entry::builder()
        .placeholder_text("Optional title filter, e.g. bruh, laugh, reaction")
        .hexpand(true)
        .build();
    public_clips_box.append(&public_clip_query);
    let public_limit_row = GtkBox::new(Orientation::Horizontal, 8);
    let public_limit_label = Label::new(Some("Maximum sounds"));
    public_limit_label.set_xalign(0.0);
    public_limit_label.set_hexpand(true);
    public_limit_row.append(&public_limit_label);
    let public_limit_adjustment = Adjustment::new(25.0, 1.0, 250.0, 1.0, 25.0, 0.0);
    let public_clip_limit = SpinButton::new(Some(&public_limit_adjustment), 1.0, 0);
    public_limit_row.append(&public_clip_limit);
    public_clips_box.append(&public_limit_row);

    let public_browse = Button::with_label("Browse / Search");
    public_clips_box.append(&public_browse);

    let public_results_box = GtkBox::new(Orientation::Vertical, 4);
    let public_results_scroll = ScrolledWindow::builder()
        .child(&public_results_box)
        .min_content_height(150)
        .max_content_height(240)
        .vexpand(true)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .build();
    public_clips_box.append(&public_results_scroll);

    let public_results: Rc<
        RefCell<Vec<(public_clip_sites::PublicClip, Rc<Cell<bool>>)>>,
    > = Rc::new(RefCell::new(Vec::new()));

    options_stack.add_named(&public_clips_box, Some("public-clips"));

    let ambient_box = GtkBox::new(Orientation::Vertical, 6);
    ambient_box.append(&source_note(
        "Curated D&D/fantasy atmospheres whose pages expose a Download audio action. License/source notes are stored beside the local files.",
    ));
    let ambient_labels = online_audio::AMBIENT_MIXER_CHOICES
        .iter()
        .map(|(_, label, _)| *label)
        .collect::<Vec<_>>();
    let ambient_choice = DropDown::from_strings(&ambient_labels);
    ambient_choice.set_selected(0);
    ambient_choice.set_hexpand(true);
    ambient_box.append(&ambient_choice);
    options_stack.add_named(&ambient_box, Some("ambient-mixer"));

    options_stack.set_visible_child_name("myinstants");
    content.append(&options_stack);

    {
        let options_stack = options_stack.clone();
        let public_results_box = public_results_box.clone();
        let public_results = Rc::clone(&public_results);
        source.connect_selected_notify(move |source| {
            let name = match source.selected() {
                SOURCE_TABLETOP_AUDIO => "tabletop",
                SOURCE_OPENGAMEART => "opengameart",
                SOURCE_FREESOUND => "freesound",
                SOURCE_RPG_SOUNDBOARD => "rpg-soundboard",
                SOURCE_AMBIENT_MIXER => "ambient-mixer",
                SOURCE_KENNEY => "kenney",
                SOURCE_SOUND_BUTTONS_COM
                | SOURCE_MOVIE_SOUND_CLIPS
                | SOURCE_MY_INSTANTS_COM => "public-clips",
                _ => "myinstants",
            };
            options_stack.set_visible_child_name(name);
            public_results.borrow_mut().clear();
            while let Some(child) = public_results_box.first_child() {
                public_results_box.remove(&child);
            }
        });
    }

    {
        let source = source.clone();
        let query = public_clip_query.clone();
        let limit = public_clip_limit.clone();
        let results_box = public_results_box.clone();
        let results_state = Rc::clone(&public_results);
        let window = window.clone();
        public_browse.connect_clicked(move |button| {
            let public_source = match source.selected() {
                SOURCE_SOUND_BUTTONS_COM => PublicClipSource::SoundButtonsCom,
                SOURCE_MOVIE_SOUND_CLIPS => PublicClipSource::MovieSoundClips,
                SOURCE_MY_INSTANTS_COM => PublicClipSource::MyInstantsCom,
                _ => return,
            };
            button.set_sensitive(false);
            button.set_label("Browsing…");
            let query = query.text().to_string();
            let limit = limit.value_as_int().max(1) as usize;
            let (progress_tx, _progress_rx) = mpsc::channel::<myinstants::DownloadProgress>();
            let cancelled = AtomicBool::new(false);
            let button_done = button.clone();
            let results_box_done = results_box.clone();
            let results_state_done = Rc::clone(&results_state);
            let window_done = window.clone();

            let _ = commands::dispatch_async_result(
                "browse_public_sound_source",
                move || public_clip_sites::browse(
                    public_source,
                    &query,
                    limit,
                    &progress_tx,
                    &cancelled,
                ),
                move |result| {
                    button_done.set_sensitive(true);
                    button_done.set_label("Browse / Search");
                    while let Some(child) = results_box_done.first_child() {
                        results_box_done.remove(&child);
                    }
                    results_state_done.borrow_mut().clear();

                    match result {
                        Ok(clips) => {
                            for clip in clips {
                                let selected = Rc::new(Cell::new(true));
                                let row = GtkBox::new(Orientation::Horizontal, 6);
                                let check = CheckButton::builder()
                                    .label(&clip.title)
                                    .active(true)
                                    .hexpand(true)
                                    .halign(Align::Fill)
                                    .build();
                                {
                                    let selected = Rc::clone(&selected);
                                    check.connect_toggled(move |check| {
                                        selected.set(check.is_active());
                                    });
                                }
                                row.append(&check);

                                let preview = Button::with_label("Preview");
                                {
                                    let media_url = clip
                                        .preview_url
                                        .clone()
                                        .unwrap_or_else(|| clip.media_url.clone());
                                    let window = window_done.clone();
                                    preview.connect_clicked(move |_| {
                                        gtk4::UriLauncher::new(&media_url).launch(
                                            Some(&window),
                                            None::<&gtk4::gio::Cancellable>,
                                            |result| {
                                                if let Err(error) = result {
                                                    log::warn!(
                                                        "Could not open sound preview: {error}"
                                                    );
                                                }
                                            },
                                        );
                                    });
                                }
                                row.append(&preview);
                                results_box_done.append(&row);
                                results_state_done
                                    .borrow_mut()
                                    .push((clip, selected));
                            }
                        }
                        Err(error) => {
                            let label = Label::new(Some(&format!(
                                "Browse failed: {error}"
                            )));
                            label.set_wrap(true);
                            label.set_xalign(0.0);
                            results_box_done.append(&label);
                        }
                    }
                },
            );
        });
    }

    let configured_directory = {
        let config = state.config.lock();
        config
            .settings
            .online_download_directory
            .clone()
            .or_else(|| config.settings.myinstants_download_directory.clone())
    };
    let selected_directory = Rc::new(RefCell::new(myinstants::download_directory(
        configured_directory.as_deref(),
    )));

    let directory_label = Label::new(Some("Main Download Folder"));
    directory_label.set_xalign(0.0);
    directory_label.add_css_class("heading");
    content.append(&directory_label);

    let directory_row = GtkBox::new(Orientation::Horizontal, 8);
    let storage = Label::new(None);
    update_storage_label(&storage, &selected_directory.borrow());
    storage.set_wrap(true);
    storage.set_xalign(0.0);
    storage.set_hexpand(true);
    storage.add_css_class("dim-label");
    storage.set_selectable(true);
    directory_row.append(&storage);

    let use_default_directory = Button::with_label("Use Default");
    directory_row.append(&use_default_directory);
    let choose_directory = Button::with_label("Choose…");
    directory_row.append(&choose_directory);
    content.append(&directory_row);

    let progress = ProgressBar::new();
    progress.set_show_text(false);
    content.append(&progress);

    let status = Label::new(Some("Ready"));
    status.set_wrap(true);
    status.set_xalign(0.0);
    status.add_css_class("dim-label");
    content.append(&status);

    let buttons = GtkBox::new(Orientation::Horizontal, 8);
    buttons.set_halign(Align::End);
    let close = Button::with_label("Close");
    let cancel = Button::with_label("Cancel");
    cancel.set_sensitive(false);
    let start = Button::with_label("Download & Import");
    start.add_css_class("suggested-action");
    buttons.append(&close);
    buttons.append(&cancel);
    buttons.append(&start);
    content.append(&buttons);

    window.set_child(Some(&content));

    {
        let window = window.clone();
        let state = Arc::clone(&state);
        let selected_directory = Rc::clone(&selected_directory);
        let storage = storage.clone();
        choose_directory.connect_clicked(move |_| {
            let dialog = gtk4::FileDialog::builder()
                .title("Select Main Online Sound Download Folder")
                .build();
            let state = Arc::clone(&state);
            let selected_directory = Rc::clone(&selected_directory);
            let storage = storage.clone();
            dialog.select_folder(
                Some(&window),
                gtk4::gio::Cancellable::NONE,
                move |result| {
                    let Ok(folder) = result else {
                        return;
                    };
                    let Some(path) = folder.path() else {
                        crate::ui_event_bridge::post_toast(
                            "The selected folder is not available as a local path".to_string(),
                        );
                        return;
                    };
                    let configured = path.to_string_lossy().into_owned();
                    match commands::set_online_download_directory(
                        Some(configured),
                        Arc::clone(&state.config),
                    ) {
                        Ok(()) => {
                            *selected_directory.borrow_mut() = path.clone();
                            update_storage_label(&storage, &path);
                            crate::ui_event_bridge::post_toast(
                                "Online sound download folder updated".to_string(),
                            );
                        }
                        Err(error) => {
                            log::warn!("Could not save online sound download folder: {error}");
                            crate::ui_event_bridge::post_toast(format!(
                                "Could not save online sound download folder: {error}"
                            ));
                        }
                    }
                },
            );
        });
    }

    {
        let state = Arc::clone(&state);
        let selected_directory = Rc::clone(&selected_directory);
        let storage = storage.clone();
        use_default_directory.connect_clicked(move |_| {
            match commands::set_online_download_directory(None, Arc::clone(&state.config)) {
                Ok(()) => {
                    let path = myinstants::default_download_directory();
                    *selected_directory.borrow_mut() = path.clone();
                    update_storage_label(&storage, &path);
                    crate::ui_event_bridge::post_toast(
                        "Online sound download folder reset to default".to_string(),
                    );
                }
                Err(error) => {
                    log::warn!("Could not reset online sound download folder: {error}");
                    crate::ui_event_bridge::post_toast(format!(
                        "Could not reset online sound download folder: {error}"
                    ));
                }
            }
        });
    }

    let active_cancel: Rc<RefCell<Option<Arc<AtomicBool>>>> = Rc::new(RefCell::new(None));

    {
        let window = window.clone();
        close.connect_clicked(move |_| window.close());
    }

    {
        let active_cancel = Rc::clone(&active_cancel);
        let status = status.clone();
        cancel.connect_clicked(move |button| {
            if let Some(cancelled) = active_cancel.borrow().as_ref() {
                cancelled.store(true, Ordering::Relaxed);
                button.set_sensitive(false);
                status.set_label("Cancelling after the current network request…");
            }
        });
    }

    {
        let active_cancel = Rc::clone(&active_cancel);
        window.connect_close_request(move |_| {
            if let Some(cancelled) = active_cancel.borrow().as_ref() {
                cancelled.store(true, Ordering::Relaxed);
            }
            glib::Propagation::Proceed
        });
    }

    {
        let state = Arc::clone(&state);
        let sound_list = sound_list.clone();
        let source = source.clone();
        let options_stack = options_stack.clone();
        let country = country.clone();
        let tabletop_collection = tabletop_collection.clone();
        let opengameart_pack = opengameart_pack.clone();
        let freesound_query = freesound_query.clone();
        let freesound_token = freesound_token.clone();
        let freesound_limit = freesound_limit.clone();
        let ambient_choice = ambient_choice.clone();
        let public_results = Rc::clone(&public_results);
        let progress = progress.clone();
        let status = status.clone();
        let start_button = start.clone();
        let cancel_button = cancel.clone();
        let close_button = close.clone();
        let choose_directory_button = choose_directory.clone();
        let use_default_directory_button = use_default_directory.clone();
        let selected_directory = Rc::clone(&selected_directory);
        let active_cancel = Rc::clone(&active_cancel);

        start.connect_clicked(move |_| {
            let source_index = source.selected();
            let source_name = SOURCE_LABELS
                .get(source_index as usize)
                .copied()
                .unwrap_or("Online source")
                .to_string();
            let output_dir = selected_directory.borrow().clone();
            let tab_id = match sound_list.navigation_context().scope {
                crate::library_store::LibraryScope::ManualTab(tab_id) => Some(tab_id),
                crate::library_store::LibraryScope::General
                | crate::library_store::LibraryScope::Folder { .. } => None,
            };

            let myinstants_selection = {
                let selected = country.selected() as usize;
                if selected == 0 {
                    myinstants::ALL_ENGLISH_ID.to_string()
                } else {
                    myinstants::COUNTRY_CHOICES
                        .get(selected - 1)
                        .map(|(code, _)| (*code).to_string())
                        .unwrap_or_else(|| "us".to_string())
                }
            };
            let tabletop_selection = TabletopCollection::from_index(tabletop_collection.selected());
            let opengameart_selection = online_audio::OPENGAMEART_PACKS
                .get(opengameart_pack.selected() as usize)
                .map(|(id, _)| (*id).to_string())
                .unwrap_or_else(|| "all".to_string());
            let freesound_search = freesound_query.text().to_string();
            let freesound_oauth = freesound_token.text().to_string();
            let freesound_count = freesound_limit.value_as_int().max(1) as usize;
            let ambient_selection = online_audio::AMBIENT_MIXER_CHOICES
                .get(ambient_choice.selected() as usize)
                .map(|(id, _, _)| (*id).to_string())
                .unwrap_or_else(|| "all".to_string());
            let selected_public_clips = if matches!(
                source_index,
                SOURCE_SOUND_BUTTONS_COM | SOURCE_MOVIE_SOUND_CLIPS | SOURCE_MY_INSTANTS_COM
            ) {
                public_results
                    .borrow()
                    .iter()
                    .filter(|(_, selected)| selected.get())
                    .map(|(clip, _)| clip.clone())
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            if matches!(
                source_index,
                SOURCE_SOUND_BUTTONS_COM | SOURCE_MOVIE_SOUND_CLIPS | SOURCE_MY_INSTANTS_COM
            ) && selected_public_clips.is_empty()
            {
                status.set_label(
                    "Browse this source first, then select at least one sound to download.",
                );
                return;
            }

            let cancelled = Arc::new(AtomicBool::new(false));
            *active_cancel.borrow_mut() = Some(Arc::clone(&cancelled));

            source.set_sensitive(false);
            options_stack.set_sensitive(false);
            start_button.set_sensitive(false);
            cancel_button.set_sensitive(true);
            close_button.set_sensitive(false);
            choose_directory_button.set_sensitive(false);
            use_default_directory_button.set_sensitive(false);
            progress.set_fraction(0.0);
            status.set_label(&format!("Starting {source_name} download…"));

            let (progress_tx, progress_rx) = mpsc::channel::<myinstants::DownloadProgress>();
            {
                let progress = progress.clone();
                let status = status.clone();
                glib::timeout_add_local(Duration::from_millis(100), move || {
                    let mut disconnected = false;
                    loop {
                        match progress_rx.try_recv() {
                            Ok(update) => {
                                status.set_label(&update.message);
                                match update.total {
                                    Some(total) if total > 0 => {
                                        let fraction = update.completed as f64 / total as f64;
                                        progress.set_fraction(fraction.clamp(0.0, 1.0));
                                    }
                                    _ => progress.pulse(),
                                }
                            }
                            Err(mpsc::TryRecvError::Empty) => break,
                            Err(mpsc::TryRecvError::Disconnected) => {
                                disconnected = true;
                                break;
                            }
                        }
                    }
                    if disconnected {
                        glib::ControlFlow::Break
                    } else {
                        glib::ControlFlow::Continue
                    }
                });
            }

            let state_done = Arc::clone(&state);
            let sound_list_done = sound_list.clone();
            let source_done = source.clone();
            let options_stack_done = options_stack.clone();
            let progress_done = progress.clone();
            let status_done = status.clone();
            let start_done = start_button.clone();
            let cancel_done = cancel_button.clone();
            let close_done = close_button.clone();
            let choose_directory_done = choose_directory_button.clone();
            let use_default_directory_done = use_default_directory_button.clone();
            let active_cancel_done = Rc::clone(&active_cancel);
            let source_name_done = source_name.clone();

            let download_root_for_import = output_dir.clone();
            let dispatch = commands::dispatch_async_result(
                "online_sound_download",
                move || -> Result<DownloadReport, String> {
                    match source_index {
                        SOURCE_TABLETOP_AUDIO => online_audio::download_tabletop_audio(
                            tabletop_selection,
                            &output_dir,
                            progress_tx,
                            cancelled,
                        )
                        .map_err(|error| error.to_string()),
                        SOURCE_OPENGAMEART => online_audio::download_opengameart(
                            &opengameart_selection,
                            &output_dir,
                            progress_tx,
                            cancelled,
                        )
                        .map_err(|error| error.to_string()),
                        SOURCE_FREESOUND => online_audio::download_freesound_originals(
                            &freesound_search,
                            &freesound_oauth,
                            freesound_count,
                            &output_dir,
                            progress_tx,
                            cancelled,
                        )
                        .map_err(|error| error.to_string()),
                        SOURCE_RPG_SOUNDBOARD => online_audio::download_rpg_soundboard_pack(
                            &output_dir,
                            progress_tx,
                            cancelled,
                        )
                        .map_err(|error| error.to_string()),
                        SOURCE_AMBIENT_MIXER => online_audio::download_ambient_mixer(
                            &ambient_selection,
                            &output_dir,
                            progress_tx,
                            cancelled,
                        )
                        .map_err(|error| error.to_string()),
                        SOURCE_KENNEY => online_audio::download_kenney_rpg_audio(
                            &output_dir,
                            progress_tx,
                            cancelled,
                        )
                        .map_err(|error| error.to_string()),
                        SOURCE_SOUND_BUTTONS_COM
                        | SOURCE_MOVIE_SOUND_CLIPS
                        | SOURCE_MY_INSTANTS_COM => {
                            let public_source = match source_index {
                                SOURCE_SOUND_BUTTONS_COM => PublicClipSource::SoundButtonsCom,
                                SOURCE_MOVIE_SOUND_CLIPS => PublicClipSource::MovieSoundClips,
                                _ => PublicClipSource::MyInstantsCom,
                            };
                            public_clip_sites::download_selected(
                                public_source,
                                &selected_public_clips,
                                &output_dir,
                                progress_tx,
                                cancelled,
                            )
                            .map_err(|error| error.to_string())
                        }
                        _ => myinstants::download(
                            &myinstants_selection,
                            &output_dir,
                            progress_tx,
                            cancelled,
                        )
                        .map_err(|error| error.to_string()),
                    }
                },
                move |result| {
                    active_cancel_done.borrow_mut().take();
                    cancel_done.set_sensitive(false);
                    source_done.set_sensitive(true);
                    options_stack_done.set_sensitive(true);
                    choose_directory_done.set_sensitive(true);
                    use_default_directory_done.set_sensitive(true);

                    let report = match result {
                        Ok(report) => report,
                        Err(error) => {
                            log::warn!("{source_name_done} download failed: {error}");
                            status_done
                                .set_label(&format!("{source_name_done} download failed: {error}"));
                            start_done.set_sensitive(true);
                            close_done.set_sensitive(true);
                            crate::ui_event_bridge::post_toast(format!(
                                "{source_name_done} download failed"
                            ));
                            return;
                        }
                    };

                    if report.cancelled {
                        status_done.set_label(
                            "Download cancelled. Completed local files were kept and can be reused next time.",
                        );
                        start_done.set_sensitive(true);
                        close_done.set_sensitive(true);
                        crate::ui_event_bridge::post_toast(
                            "Online sound download cancelled".to_string(),
                        );
                        return;
                    }

                    if report.paths.is_empty() {
                        status_done.set_label(
                            "Download finished, but no supported audio files were available to import. Any downloaded archives/files were kept locally.",
                        );
                        start_done.set_sensitive(true);
                        close_done.set_sensitive(true);
                        return;
                    }

                    progress_done.set_fraction(1.0);
                    status_done.set_label("Download complete. Importing sounds into the soundboard…");
                    import_downloaded_sounds(
                        report,
                        tab_id,
                        download_root_for_import,
                        source_name_done,
                        state_done,
                        sound_list_done,
                        status_done,
                        start_done,
                        close_done,
                    );
                },
            );

            if let Err(error) = dispatch {
                active_cancel.borrow_mut().take();
                cancel_button.set_sensitive(false);
                source.set_sensitive(true);
                options_stack.set_sensitive(true);
                start_button.set_sensitive(true);
                close_button.set_sensitive(true);
                choose_directory_button.set_sensitive(true);
                use_default_directory_button.set_sensitive(true);
                status.set_label(&format!("Could not start online downloader: {error}"));
            }
        });
    }

    window.present();
}

fn source_note(text: &str) -> Label {
    let label = Label::new(Some(text));
    label.set_wrap(true);
    label.set_xalign(0.0);
    label.add_css_class("dim-label");
    label
}

fn update_storage_label(label: &Label, path: &Path) {
    label.set_label(&format!(
        "{}\nEach source downloads into its own subfolder under this location.",
        path.display()
    ));
}

#[allow(clippy::too_many_arguments)]
fn import_downloaded_sounds(
    report: DownloadReport,
    tab_id: Option<String>,
    download_root: std::path::PathBuf,
    source_name: String,
    state: Arc<AppState>,
    sound_list: SoundList,
    status: Label,
    start: Button,
    close: Button,
) {
    let DownloadReport {
        paths,
        path_migrations,
        downloaded,
        reused,
        failed,
        cancelled: _,
    } = report;

    let worker_config = Arc::clone(&state.config);
    let worker_library = state.library.clone();
    let worker_coords = state.loudness_coordinators.clone();
    let worker_projection = state.hotkey_projection.clone();

    let sound_list_done = sound_list;
    let status_done = status.clone();
    let start_done = start.clone();
    let close_done = close.clone();
    let source_name_done = source_name.clone();

    let dispatch = commands::dispatch_async_result(
        "online_sound_import",
        move || {
            let migrated = commands::migrate_sound_paths_with_store(
                path_migrations,
                Arc::clone(&worker_config),
                worker_library.clone(),
                &worker_coords,
            )?;
            let imported = commands::import_files_to_tab_with_store(
                paths,
                tab_id,
                Arc::clone(&worker_config),
                worker_library.clone(),
                &worker_coords,
            )?;

            commands::add_sound_folder_with_store(
                download_root.to_string_lossy().into_owned(),
                worker_library.clone(),
            )?;
            let refreshed = commands::refresh_sounds_with_store(
                worker_config,
                worker_library,
                worker_projection,
                &worker_coords,
            )?;

            Ok::<(usize, usize, usize), commands::CommandError>((
                migrated,
                imported,
                refreshed.refreshed,
            ))
        },
        move |result| {
            start_done.set_sensitive(true);
            close_done.set_sensitive(true);

            match result {
                Ok((migrated, imported, refreshed)) => {
                    if migrated > 0 || imported > 0 || refreshed > 0 {
                        sound_list_done.refresh_from_state();
                    }
                    let migrated_note = if migrated == 0 {
                        String::new()
                    } else {
                        format!(", {migrated} renamed")
                    };
                    let failed_note = if failed == 0 {
                        String::new()
                    } else {
                        format!(", {failed} failed")
                    };
                    let message = format!(
                        "{source_name_done}: {downloaded} downloaded, {reused} reused, {imported} added, source folders refreshed{migrated_note}{failed_note}"
                    );
                    status_done.set_label(&message);
                    crate::ui_event_bridge::post_toast(message);
                }
                Err(error) => {
                    log::warn!("{source_name_done} import failed: {error}");
                    status_done.set_label(&format!(
                        "Files were kept locally, but importing them into Linux Soundboard failed: {error}"
                    ));
                    crate::ui_event_bridge::post_toast(
                        "Online files downloaded; import failed".to_string(),
                    );
                }
            }
        },
    );

    if let Err(error) = dispatch {
        start.set_sensitive(true);
        close.set_sensitive(true);
        status.set_label(&format!(
            "Files were kept locally, but the import could not be started: {error}"
        ));
    }
}
