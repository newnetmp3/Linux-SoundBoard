use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use gtk4::prelude::*;
use gtk4::{
    Align, Box as GtkBox, Button, DropDown, Label, Orientation, ProgressBar, Window,
};

use crate::app_state::AppState;
use crate::commands;
use crate::myinstants::{self, DownloadReport};

use super::sound_list::SoundList;

pub(super) fn show_downloader(parent: &gtk4::Window, state: Arc<AppState>, sound_list: SoundList) {
    let window = Window::builder()
        .title("MyInstants Downloader")
        .transient_for(parent)
        .modal(true)
        .default_width(540)
        .default_height(320)
        .resizable(false)
        .build();

    let content = GtkBox::new(Orientation::Vertical, 14);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    content.set_margin_start(18);
    content.set_margin_end(18);

    let intro = Label::new(Some(
        "Download sounds from MyInstants and add them directly to the selected soundboard tab. Folder views import into General. Existing downloads and duplicate MyInstants audio are reused automatically.",
    ));
    intro.set_wrap(true);
    intro.set_xalign(0.0);
    content.append(&intro);

    let country_label = Label::new(Some("Index"));
    country_label.set_xalign(0.0);
    country_label.add_css_class("heading");
    content.append(&country_label);

    let mut country_labels = vec!["All English-speaking indexes"];
    country_labels.extend(myinstants::COUNTRY_CHOICES.iter().map(|(_, label)| *label));
    let country = DropDown::from_strings(&country_labels);
    country.set_selected(1);
    country.set_hexpand(true);
    content.append(&country);

    let directory = myinstants::download_directory();
    let storage = Label::new(Some(&format!(
        "Downloads are kept in {} so interrupted runs can resume without starting over.",
        directory.display()
    )));
    storage.set_wrap(true);
    storage.set_xalign(0.0);
    storage.add_css_class("dim-label");
    storage.set_selectable(true);
    content.append(&storage);

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
        let country = country.clone();
        let progress = progress.clone();
        let status = status.clone();
        let start_button = start.clone();
        let cancel_button = cancel.clone();
        let close_button = close.clone();
        let active_cancel = Rc::clone(&active_cancel);

        start.connect_clicked(move |_| {
            let selected = country.selected() as usize;
            let selection = if selected == 0 {
                myinstants::ALL_ENGLISH_ID
            } else {
                myinstants::COUNTRY_CHOICES
                    .get(selected - 1)
                    .map(|(code, _)| *code)
                    .unwrap_or("us")
            };

            let tab_id = match sound_list.navigation_context().scope {
                crate::library_store::LibraryScope::ManualTab(tab_id) => Some(tab_id),
                crate::library_store::LibraryScope::General
                | crate::library_store::LibraryScope::Folder { .. } => None,
            };
            let cancelled = Arc::new(AtomicBool::new(false));
            *active_cancel.borrow_mut() = Some(Arc::clone(&cancelled));

            country.set_sensitive(false);
            start_button.set_sensitive(false);
            cancel_button.set_sensitive(true);
            close_button.set_sensitive(false);
            progress.set_fraction(0.0);
            status.set_label("Starting MyInstants download…");

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
            let country_done = country.clone();
            let progress_done = progress.clone();
            let status_done = status.clone();
            let start_done = start_button.clone();
            let cancel_done = cancel_button.clone();
            let close_done = close_button.clone();
            let active_cancel_done = Rc::clone(&active_cancel);
            let selection = selection.to_string();

            let dispatch = commands::dispatch_async_result(
                "myinstants_download",
                move || myinstants::download(&selection, progress_tx, cancelled),
                move |result| {
                    active_cancel_done.borrow_mut().take();
                    cancel_done.set_sensitive(false);

                    let report = match result {
                        Ok(report) => report,
                        Err(error) => {
                            log::warn!("MyInstants download failed: {error}");
                            status_done.set_label(&format!("MyInstants download failed: {error}"));
                            country_done.set_sensitive(true);
                            start_done.set_sensitive(true);
                            close_done.set_sensitive(true);
                            crate::ui_event_bridge::post_toast(
                                "MyInstants download failed".to_string(),
                            );
                            return;
                        }
                    };

                    if report.cancelled {
                        status_done.set_label(
                            "Download cancelled. Completed files were kept and will be reused next time.",
                        );
                        country_done.set_sensitive(true);
                        start_done.set_sensitive(true);
                        close_done.set_sensitive(true);
                        crate::ui_event_bridge::post_toast(
                            "MyInstants download cancelled".to_string(),
                        );
                        return;
                    }

                    if report.paths.is_empty() {
                        status_done.set_label("No downloadable MyInstants sounds were found.");
                        country_done.set_sensitive(true);
                        start_done.set_sensitive(true);
                        close_done.set_sensitive(true);
                        return;
                    }

                    progress_done.set_fraction(1.0);
                    status_done
                        .set_label("Download complete. Importing sounds into the soundboard…");
                    import_downloaded_sounds(
                        report,
                        tab_id.clone(),
                        Arc::clone(&state_done),
                        sound_list_done.clone(),
                        country_done.clone(),
                        status_done.clone(),
                        start_done.clone(),
                        close_done.clone(),
                    );
                },
            );

            if let Err(error) = dispatch {
                active_cancel.borrow_mut().take();
                cancel_button.set_sensitive(false);
                country.set_sensitive(true);
                start_button.set_sensitive(true);
                close_button.set_sensitive(true);
                status.set_label(&format!("Could not start MyInstants downloader: {error}"));
            }
        });
    }

    window.present();
}

#[allow(clippy::too_many_arguments)]
fn import_downloaded_sounds(
    report: DownloadReport,
    tab_id: Option<String>,
    state: Arc<AppState>,
    sound_list: SoundList,
    country: DropDown,
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

    let sound_list_done = sound_list.clone();
    let status_done = status.clone();
    let country_done = country.clone();
    let start_done = start.clone();
    let close_done = close.clone();

    let dispatch = commands::import_files_to_tab_with_store_async(
        paths,
        tab_id,
        Arc::clone(&state.config),
        state.library.clone(),
        state.loudness_coordinators.clone(),
        move |result| {
            country_done.set_sensitive(true);
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
                        "MyInstants complete: {downloaded} downloaded, {reused} reused, {imported} added{failed_note}"
                    );
                    status_done.set_label(&message);
                    crate::ui_event_bridge::post_toast(message);
                }
                Err(error) => {
                    log::warn!("MyInstants import failed: {error}");
                    status_done.set_label(&format!(
                        "Sounds downloaded, but importing them into Linux Soundboard failed: {error}"
                    ));
                    crate::ui_event_bridge::post_toast(
                        "MyInstants files downloaded; import failed".to_string(),
                    );
                }
            }
        },
    );

    if let Err(error) = dispatch {
        country.set_sensitive(true);
        start.set_sensitive(true);
        close.set_sensitive(true);
        status.set_label(&format!(
            "Sounds downloaded, but the import could not be started: {error}"
        ));
    }
}
