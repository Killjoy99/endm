//! The modal dialogs of the main window: "Add URL", "Download File Info",
//! "Properties", "Scheduler", "About" and the delete confirmation.

use std::time::Duration;

use dioxus::prelude::*;

use crate::actions::{use_actions, AppActions};
use crate::engine::DownloadRequest;
use crate::types::{Category, Dialog, Schedule, ScheduleAction, Status};
use crate::util::{
    extension_of, file_name_from_url, file_name_of_path, format_eta, format_rate, format_size,
    seconds_until, unique_file_name, with_credentials,
};

/// Renders whichever dialog the store asks for.
#[component]
pub fn DialogHost() -> Element {
    let dialog = use_actions().store.dialog.read().clone();

    match dialog {
        Dialog::None => rsx! {},
        Dialog::AddUrl => rsx! {
            ModalFrame { title: "Add URL", width: 460, AddUrlDialog {} }
        },
        Dialog::FileInfo {
            address,
            user_name,
            password,
        } => rsx! {
            ModalFrame { title: "Download File Info", width: 520,
                FileInfoDialog { address, user_name, password }
            }
        },
        Dialog::Properties(id) => rsx! {
            ModalFrame { title: "Properties", width: 540, PropertiesDialog { id } }
        },
        Dialog::Scheduler => rsx! {
            ModalFrame { title: "Scheduler", width: 460, SchedulerDialog {} }
        },
        Dialog::About => rsx! {
            ModalFrame { title: "About ENDM", width: 420, AboutDialog {} }
        },
        Dialog::ConfirmDelete(id) => rsx! {
            ModalFrame { title: "Confirm", width: 420, ConfirmDeleteDialog { id } }
        },
    }
}

/// A modal window with a title bar, in the style of a classic Windows dialog.
#[component]
fn ModalFrame(title: String, width: u32, children: Element) -> Element {
    rsx! {
        div { class: "idm-modal-backdrop" }
        div { class: "idm-modal", style: "width: {width}px;",
            div { class: "idm-modal__titlebar",
                span { class: "idm-modal__title", "{title}" }
            }
            div { class: "idm-modal__body", {children} }
        }
    }
}

/// The "Add URL" dialog: a web address plus optional server authorization.
#[component]
fn AddUrlDialog() -> Element {
    let actions = use_actions();
    let mut address = use_signal(String::new);
    let mut use_authorization = use_signal(|| false);
    let mut user_name = use_signal(String::new);
    let mut password = use_signal(String::new);
    let error = use_signal(|| Option::<String>::None);

    let authorize = use_authorization();
    let error_text = error.read().clone();

    rsx! {
        div { class: "idm-form",
            label { class: "idm-label", "URL:" }
            input {
                class: "idm-input",
                r#type: "text",
                placeholder: "http://www.example.com/file.zip",
                value: "{address}",
                autofocus: true,
                oninput: move |evt| address.set(evt.value()),
                onkeydown: move |evt| {
                    if evt.key() == Key::Enter {
                        submit(actions, address, use_authorization, user_name, password, error);
                    }
                },
            }
            label { class: "idm-check",
                input {
                    r#type: "checkbox",
                    checked: authorize,
                    onchange: move |evt| use_authorization.set(evt.checked()),
                }
                "Use authorization"
            }
            if authorize {
                div { class: "idm-form__row",
                    div { class: "idm-form__col",
                        label { class: "idm-label", "User Name:" }
                        input {
                            class: "idm-input",
                            r#type: "text",
                            value: "{user_name}",
                            oninput: move |evt| user_name.set(evt.value()),
                        }
                    }
                    div { class: "idm-form__col",
                        label { class: "idm-label", "Password:" }
                        input {
                            class: "idm-input",
                            r#type: "password",
                            value: "{password}",
                            oninput: move |evt| password.set(evt.value()),
                        }
                    }
                }
            }
            if let Some(error_text) = error_text {
                p { class: "idm-error", "{error_text}" }
            }
            div { class: "idm-form__buttons",
                button {
                    class: "idm-button idm-button--default",
                    onclick: move |_| {
                        submit(actions, address, use_authorization, user_name, password, error);
                    },
                    "OK"
                }
                button {
                    class: "idm-button",
                    onclick: move |_| actions.close_dialog(),
                    "Cancel"
                }
            }
        }
    }
}

/// Validates the address and moves on to the "Download File Info" dialog.
fn submit(
    actions: AppActions,
    address: Signal<String>,
    use_authorization: Signal<bool>,
    user_name: Signal<String>,
    password: Signal<String>,
    mut error: Signal<Option<String>>,
) {
    let url = address.read().trim().to_string();

    if !url.starts_with("http://") && !url.starts_with("https://") {
        error.set(Some(
            "The URL is not valid. Only http:// and https:// addresses are supported.".to_string(),
        ));
        return;
    }

    error.set(None);
    let (user_name, password) = if *use_authorization.read() {
        (user_name.read().clone(), password.read().clone())
    } else {
        (String::new(), String::new())
    };

    actions.open(Dialog::FileInfo {
        address: url,
        user_name,
        password,
    });
}

/// The "Download File Info" dialog: where the file is saved and what it is.
#[component]
fn FileInfoDialog(address: String, user_name: String, password: String) -> Element {
    let actions = use_actions();
    let settings = actions.settings();

    let suggested_name = file_name_from_url(&address);
    let taken: Vec<String> = actions
        .store
        .downloads
        .read()
        .iter()
        .map(|download| download.path.clone())
        .collect();
    let suggested_path = unique_file_name(&settings.default_dir, &suggested_name, &taken);

    let mut path = use_signal(|| suggested_path);
    let mut description = use_signal(String::new);
    let mut error = use_signal(|| Option::<String>::None);

    let category = Category::of_extension(&extension_of(&file_name_of_path(&path())));
    let category_text = format!("{} — {}", category.label(), suggested_name);
    let error_text = error.read().clone();

    let start_address = address.clone();
    let start_user = user_name.clone();
    let start_password = password.clone();
    let on_start = move |_| {
        if path().trim().is_empty() {
            error.set(Some("Please specify where the file should be saved.".to_string()));
            return;
        }
        error.set(None);
        add(
            actions,
            &start_address,
            &start_user,
            &start_password,
            &path(),
            &description(),
            true,
        );
    };

    let later_address = address.clone();
    let later_user = user_name.clone();
    let later_password = password.clone();
    let on_later = move |_| {
        if path().trim().is_empty() {
            error.set(Some("Please specify where the file should be saved.".to_string()));
            return;
        }
        error.set(None);
        add(
            actions,
            &later_address,
            &later_user,
            &later_password,
            &path(),
            &description(),
            false,
        );
    };

    rsx! {
        div { class: "idm-form",
            label { class: "idm-label", "Address:" }
            input { class: "idm-input", r#type: "text", value: "{address}", readonly: true, }
            div { class: "idm-form__row",
                div { class: "idm-form__col idm-form__col--grow",
                    label { class: "idm-label", "Save As:" }
                    input {
                        class: "idm-input",
                        r#type: "text",
                        value: "{path}",
                        autofocus: true,
                        oninput: move |evt| path.set(evt.value()),
                    }
                }
                div { class: "idm-form__col",
                    label { class: "idm-label", "Category:" }
                    input { class: "idm-input", r#type: "text", value: "{category_text}", readonly: true, }
                }
            }
            label { class: "idm-label", "Description:" }
            input {
                class: "idm-input",
                r#type: "text",
                value: "{description}",
                oninput: move |evt| description.set(evt.value()),
            }
            if let Some(error_text) = error_text {
                p { class: "idm-error", "{error_text}" }
            }
            div { class: "idm-form__buttons",
                button { class: "idm-button idm-button--default", onclick: on_start, "Start Download" }
                button { class: "idm-button", onclick: on_later, "Download Later" }
                button { class: "idm-button", onclick: move |_| actions.close_dialog(), "Cancel" }
            }
        }
    }
}

/// Adds a download to the list, then closes the dialog.
fn add(
    actions: AppActions,
    address: &str,
    user_name: &str,
    password: &str,
    path: &str,
    description: &str,
    start_now: bool,
) {
    let settings = actions.settings();
    let request = DownloadRequest {
        url: with_credentials(address, user_name, password),
        path: path.to_string(),
        description: description.to_string(),
        user_agent: settings.user_agent.clone(),
        timeout_secs: settings.timeout_secs,
    };

    actions.add_download(request, start_now);
    actions.close_dialog();
}

/// "Properties" of the selected download.
#[component]
fn PropertiesDialog(id: usize) -> Element {
    let actions = use_actions();
    let download = actions.download(id);
    let initial_description = download
        .as_ref()
        .map(|download| download.description.clone())
        .unwrap_or_default();
    let mut description = use_signal(|| initial_description);

    let Some(download) = download else {
        return rsx! {
            div { class: "idm-form",
                p { class: "idm-error", "This download is no longer in the list." }
                div { class: "idm-form__buttons",
                    button { class: "idm-button", onclick: move |_| actions.close_dialog(), "Close" }
                }
            }
        };
    };

    let status = match &download.status {
        Status::Error(message) => format!("Error — {message}"),
        other => other.label().to_string(),
    };
    let size = match download.total {
        Some(total) => format!(
            "{} of {}",
            format_size(download.downloaded),
            format_size(total)
        ),
        None => format_size(download.downloaded),
    };
    let eta = match download.eta_seconds() {
        Some(seconds) => format_eta(seconds),
        None => "Unknown".to_string(),
    };

    rsx! {
        div { class: "idm-form",
            div { class: "idm-props",
                PropRow { label: "File Name:", value: download.name.clone() }
                PropRow { label: "Address:", value: download.url.clone() }
                PropRow { label: "Save As:", value: download.path.clone() }
                PropRow { label: "Size:", value: size }
                PropRow { label: "Status:", value: status }
                PropRow { label: "Transfer Rate:", value: format_rate(download.rate) }
                PropRow { label: "Time Left:", value: eta }
            }
            label { class: "idm-label", "Description:" }
            input {
                class: "idm-input",
                r#type: "text",
                value: "{description}",
                oninput: move |evt| description.set(evt.value()),
            }
            div { class: "idm-form__buttons",
                button {
                    class: "idm-button idm-button--default",
                    onclick: move |_| {
                        actions.set_description(id, description());
                        actions.close_dialog();
                    },
                    "OK"
                }
                button { class: "idm-button", onclick: move |_| actions.close_dialog(), "Cancel" }
            }
        }
    }
}

/// One `label: value` line of the "Properties" dialog.
#[component]
fn PropRow(label: String, value: String) -> Element {
    rsx! {
        div { class: "idm-props__row",
            span { class: "idm-props__label", "{label}" }
            span { class: "idm-props__value", "{value}" }
        }
    }
}

/// The "Scheduler" dialog: start or stop the queue at a given time of the day.
#[component]
fn SchedulerDialog() -> Element {
    let actions = use_actions();
    let existing = actions.store.schedule.read().clone();

    let mut enabled = use_signal(|| existing.is_some());
    let mut action = use_signal(|| {
        existing
            .as_ref()
            .map(|schedule| schedule.action)
            .unwrap_or(ScheduleAction::StartQueue)
    });
    let mut time = use_signal(|| {
        existing
            .as_ref()
            .map(|schedule| schedule.time_label())
            .unwrap_or_else(|| "23:00".to_string())
    });
    let mut error = use_signal(|| Option::<String>::None);

    let enabled_now = enabled();
    let selected_action = action();
    let time_value = time();
    let error_text = error.read().clone();

    let on_ok = move |_| {
        if !enabled() {
            actions.set_schedule(None);
            actions.close_dialog();
            return;
        }

        let Some((hour, minute)) = parse_time(&time()) else {
            error.set(Some(
                "Please enter the time of day as hh:mm, for example 23:30.".to_string(),
            ));
            return;
        };

        error.set(None);
        let chosen = action();
        let delay = seconds_until(hour, minute);

        spawn(async move {
            tokio::time::sleep(Duration::from_secs(delay)).await;
            match chosen {
                ScheduleAction::StartQueue => actions.start_queue(),
                ScheduleAction::StopQueue => actions.stop_queue(),
                ScheduleAction::StopAll => actions.stop_all(),
            }
            actions.set_schedule(None);
        });

        actions.set_schedule(Some(Schedule {
            action: chosen,
            hour,
            minute,
        }));
        actions.close_dialog();
    };

    rsx! {
        div { class: "idm-form",
            label { class: "idm-check",
                input {
                    r#type: "checkbox",
                    checked: enabled_now,
                    onchange: move |evt| enabled.set(evt.checked()),
                }
                "Turn on the scheduler"
            }
            div { class: "idm-form__row",
                div { class: "idm-form__col idm-form__col--grow",
                    label { class: "idm-label", "Action:" }
                    select {
                        class: "idm-select",
                        onchange: move |evt| {
                            if let Ok(index) = evt.value().parse::<usize>() {
                                if let Some(choice) = ScheduleAction::ALL.get(index) {
                                    action.set(*choice);
                                }
                            }
                        },
                        for (index, choice) in ScheduleAction::ALL.iter().enumerate() {
                            option { value: "{index}", selected: *choice == selected_action, "{choice.label()}" }
                        }
                    }
                }
                div { class: "idm-form__col",
                    label { class: "idm-label", "Time (hh:mm):" }
                    input {
                        class: "idm-input",
                        r#type: "text",
                        value: "{time_value}",
                        oninput: move |evt| time.set(evt.value()),
                    }
                }
            }
            if let Some(error_text) = error_text {
                p { class: "idm-error", "{error_text}" }
            }
            div { class: "idm-form__buttons",
                button { class: "idm-button idm-button--default", onclick: on_ok, "OK" }
                button {
                    class: "idm-button",
                    onclick: move |_| {
                        actions.set_schedule(None);
                        actions.close_dialog();
                    },
                    "Turn off"
                }
                button { class: "idm-button", onclick: move |_| actions.close_dialog(), "Close" }
            }
        }
    }
}

/// `"23:30"` -> `(23, 30)`.
fn parse_time(value: &str) -> Option<(u32, u32)> {
    let (hour, minute) = value.trim().split_once(':')?;
    let hour: u32 = hour.trim().parse().ok()?;
    let minute: u32 = minute.trim().parse().ok()?;

    if hour < 24 && minute < 60 {
        Some((hour, minute))
    } else {
        None
    }
}

/// The "About" box, also used by the "Tell A Friend" button.
#[component]
fn AboutDialog() -> Element {
    let actions = use_actions();

    rsx! {
        div { class: "idm-form idm-about",
            h2 { class: "idm-about__title", "ENDM — Download Manager" }
            p { class: "idm-about__text",
                "A download manager with the look of Internet Download Manager, built with Rust and Dioxus."
            }
            p { class: "idm-about__text", "Version 0.1.0" }
            p { class: "idm-about__link", "https://github.com/Killjoy99/endm" }
            div { class: "idm-form__buttons",
                button { class: "idm-button idm-button--default", onclick: move |_| actions.close_dialog(), "Close" }
            }
        }
    }
}

/// Asks for confirmation before a download is dropped from the list.
#[component]
fn ConfirmDeleteDialog(id: usize) -> Element {
    let actions = use_actions();
    let name = actions
        .download(id)
        .map(|download| download.name)
        .unwrap_or_else(|| format!("download {id}"));
    let message = format!("Do you want to delete \"{name}\" from the list of downloads?");

    rsx! {
        div { class: "idm-form",
            p { class: "idm-confirm__text", "{message}" }
            p { class: "idm-confirm__hint", "The file that was already written is left on disk." }
            div { class: "idm-form__buttons",
                button {
                    class: "idm-button idm-button--default",
                    onclick: move |_| {
                        actions.remove(id);
                        actions.close_dialog();
                    },
                    "Yes"
                }
                button { class: "idm-button", onclick: move |_| actions.close_dialog(), "No" }
            }
        }
    }
}
