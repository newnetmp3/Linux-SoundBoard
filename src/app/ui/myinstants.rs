use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use gtk4::prelude::*;
use gtk4::{
    Adjustment, Align, Box as GtkBox, Button, DropDown, Entry, Label, Orientation, PasswordEntry,
    ProgressBar, SpinButton, Stack, Window,
};

use crate::app_state::AppState;
use crate::commands;
use crate::myinstants::{self, DownloadReport};
use crate::online_audio::{self, TabletopCollection};

use super::sound_list::SoundList;

const SOURCE_MYINSTANTS: u32 = 0;
const SOURCE_TABLETOP_AUDIO: u32 = 1;
const SOURCE_OPENGAMEART: u32 = 2;
const SOURCE_FREESOUND: u32 = 3;
const SOURCE_RPG_SOUNDBOARD: u32 = 4;
const SOURCE_AMBIENT_MIXER: u32 = 5;
const SOURCE_KENNEY: u32 = 6;

const SOURCE_LABELS: &[&str] = &[
    "MyInstants",
    "Tabletop Audio — D&D / fantasy ambience",
    "OpenGameArt — fantasy/RPG sound packs",
    "Freesound — original files only",
    "RPG Soundboard — free Medieval Fantasy pack",
    "Ambient Mixer — D&D / fantasy atmospheres",
    "Kenney — RPG Audio (50 CC0 sounds)",
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

    let intro = Label::new(Some(
        "Download local copies from online sound libraries and import supported audio directly into the selected soundboard tab.",
    ));
    intro.set_wrap(true);
    intro.set_xalign(0.0);
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
        "Country index downloads use a headless Chromium session to perform MyInstants' real infinite scroll. Existing files are reused, interrupted downloads resume, and filenames follow the visible sound name. Requires Chromium + chromedriver.",
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
        source.connect_selected_notify(move |source| {
            let name = match source.selected() {
                SOURCE_TABLETOP_AUDIO => "tabletop",
                SOURCE_OPENGAMEART => "opengameart",
                SOURCE_FREESOUND => "freesound",
                SOURCE_RPG_SOUNDBOARD => "rpg-soundboard",
                SOURCE_AMBIENT_MIXER => "ambient-mixer",
                SOURCE_KENNEY => "kenney",
                _ => "myinstants",
            };
            options_stack.set_visible_child_name(name);
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
    source_name: String,
    state: Arc<AppState>,
    sound_list: SoundList,
    status: Label,
    start: Button,
    close: Button,
) {
    let DownloadReport {
        paths,
        downloaded,
        reused,
        failed,
        cancelled: _,
    } = report;

    let sound_list_done = sound_list;
    let status_done = status.clone();
    let start_done = start.clone();
    let close_done = close.clone();
    let source_name_done = source_name.clone();

    let dispatch = commands::import_files_to_tab_with_store_async(
        paths,
        tab_id,
        Arc::clone(&state.config),
        state.library.clone(),
        state.loudness_coordinators.clone(),
        move |result| {
            start_done.set_sensitive(true);
            close_done.set_sensitive(true);

            match result {
                Ok(imported) => {
                    if imported > 0 {
                        sound_list_done.refresh_from_state();
                    }
                    let failed_note = if failed == 0 {
                        String::new()
                    } else {
                        format!(", {failed} failed")
                    };
                    let message = format!(
                        "{source_name_done}: {downloaded} downloaded, {reused} reused, {imported} added{failed_note}"
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
