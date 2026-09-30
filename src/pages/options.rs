//! The "Options" window. Like in IDM it is a window of its own with OK and Cancel
//! buttons — nothing is applied before OK is pressed.

use dioxus::prelude::*;

use crate::actions::use_actions;
use crate::app::Route;
use crate::types::{Category, Settings, ToolbarStyle};

/// The tabs of the dialog, in the order IDM lists them.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    General,
    FileTypes,
    Connection,
    SaveTo,
    Downloads,
    Proxy,
    SiteLogins,
    DialUp,
    Sounds,
}

impl Tab {
    const ALL: [Tab; 9] = [
        Tab::General,
        Tab::FileTypes,
        Tab::Connection,
        Tab::SaveTo,
        Tab::Downloads,
        Tab::Proxy,
        Tab::SiteLogins,
        Tab::DialUp,
        Tab::Sounds,
    ];

    fn label(&self) -> &'static str {
        match self {
            Tab::General => "General",
            Tab::FileTypes => "File Types",
            Tab::Connection => "Connection",
            Tab::SaveTo => "Save To",
            Tab::Downloads => "Downloads",
            Tab::Proxy => "Proxy / Socks",
            Tab::SiteLogins => "Site Logins",
            Tab::DialUp => "Dial-Up / VPN",
            Tab::Sounds => "Sounds",
        }
    }
}

#[component]
pub fn Options() -> Element {
    let actions = use_actions();
    let navigator_ok = use_navigator();
    let navigator_cancel = use_navigator();

    // The dialog edits a copy of the settings; OK applies them.
    let draft = use_signal(|| actions.settings());
    let mut tab = use_signal(|| Tab::General);
    let current = tab();

    let content = match current {
        Tab::General => rsx! {
            GeneralTab { draft }
        },
        Tab::FileTypes => rsx! {
            FileTypesTab {}
        },
        Tab::Connection => rsx! {
            ConnectionTab { draft }
        },
        Tab::SaveTo => rsx! {
            SaveToTab { draft }
        },
        Tab::Downloads => rsx! {
            DownloadsTab { draft }
        },
        Tab::Proxy => rsx! {
            UnsupportedTab {
                title: "Proxy / Socks",
                text: "ENDM requests files directly through the system connection, so proxy and SOCKS settings are not available.",
            }
        },
        Tab::SiteLogins => rsx! {
            UnsupportedTab {
                title: "Site Logins",
                text: "Keeping a list of logins for protected sites is not implemented yet. Use \"Add URL\" with \"Use authorization\" instead.",
            }
        },
        Tab::DialUp => rsx! {
            UnsupportedTab {
                title: "Dial-Up / VPN",
                text: "Choosing a dial-up connection for the downloads is not implemented yet.",
            }
        },
        Tab::Sounds => rsx! {
            UnsupportedTab {
                title: "Sounds",
                text: "Playing a sound for download events is not implemented yet.",
            }
        },
    };

    rsx! {
        div { class: "idm-desktop",
            div { class: "idm-window",
                div { class: "idm-window__titlebar",
                    span { class: "idm-window__title", "Options" }
                }
                div { class: "idm-tabs",
                    for item in Tab::ALL {
                        button {
                            class: if current == item { "idm-tab idm-tab--active" } else { "idm-tab" },
                            onclick: move |_| tab.set(item),
                            "{item.label()}"
                        }
                    }
                }
                div { class: "idm-window__body", {content} }
                div { class: "idm-window__buttons",
                    button {
                        class: "idm-button idm-button--default",
                        onclick: move |_| {
                            actions.save_settings(draft());
                            let _ = navigator_ok.push(Route::Home {});
                        },
                        "OK"
                    }
                    button {
                        class: "idm-button",
                        onclick: move |_| {
                            let _ = navigator_cancel.push(Route::Home {});
                        },
                        "Cancel"
                    }
                }
            }
        }
    }
}

/// "General" — the look of the window and the delete confirmation.
#[component]
fn GeneralTab(draft: Signal<Settings>) -> Element {
    let settings = draft();

    rsx! {
        div { class: "idm-fieldset",
            div { class: "idm-fieldset__title", "Window" }
            label { class: "idm-check",
                input {
                    r#type: "checkbox",
                    checked: settings.confirm_delete,
                    onchange: move |evt| draft.write().confirm_delete = evt.checked(),
                }
                "Confirm before a download is removed from the list"
            }
            label { class: "idm-check",
                input {
                    r#type: "checkbox",
                    checked: settings.show_categories,
                    onchange: move |evt| draft.write().show_categories = evt.checked(),
                }
                "Show the Categories panel"
            }
            label { class: "idm-check",
                input {
                    r#type: "checkbox",
                    checked: settings.show_status_bar,
                    onchange: move |evt| draft.write().show_status_bar = evt.checked(),
                }
                "Show the status bar"
            }
        }
        div { class: "idm-fieldset",
            div { class: "idm-fieldset__title", "Toolbar look" }
            for style in ToolbarStyle::ALL {
                label { class: "idm-check",
                    input {
                        r#type: "radio",
                        name: "toolbar-look",
                        checked: settings.toolbar_style == style,
                        onchange: move |_| draft.write().toolbar_style = style,
                    }
                    "{style.label()}"
                }
            }
        }
    }
}

/// "File Types" — the extensions ENDM sorts into the categories.
#[component]
fn FileTypesTab() -> Element {
    let rows: Vec<(String, String)> = Category::TYPES
        .iter()
        .map(|category| (category.label().to_string(), category.extensions().join(" ")))
        .collect();

    rsx! {
        div { class: "idm-fieldset",
            div { class: "idm-fieldset__title", "File types taken over from the browser" }
            div { class: "idm-listbox",
                for (name, extensions) in rows {
                    div { class: "idm-listbox__row",
                        span { class: "idm-listbox__key", "{name}" }
                        span { class: "idm-listbox__value", "{extensions}" }
                    }
                }
            }
            p { class: "idm-hint",
                "Files with these extensions are placed in the matching category. Files of any other type end up in \"Other\"."
            }
        }
    }
}

/// "Connection" — user agent and timeout of the requests.
#[component]
fn ConnectionTab(draft: Signal<Settings>) -> Element {
    let settings = draft();

    rsx! {
        div { class: "idm-fieldset",
            div { class: "idm-fieldset__title", "Connection" }
            label { class: "idm-label", "User agent for manually added downloads:" }
            input {
                class: "idm-input",
                r#type: "text",
                value: "{settings.user_agent}",
                oninput: move |evt| draft.write().user_agent = evt.value(),
            }
            label { class: "idm-label", "Timeout (seconds):" }
            input {
                class: "idm-input idm-input--short",
                r#type: "number",
                min: "5",
                max: "600",
                value: "{settings.timeout_secs}",
                oninput: move |evt| {
                    if let Ok(seconds) = evt.value().parse::<u64>() {
                        draft.write().timeout_secs = seconds;
                    }
                },
            }
            p { class: "idm-hint",
                "Every file is fetched with a single connection, so \"Max. connections per file\" is always 1."
            }
        }
    }
}

/// "Save To" — the directory new downloads are saved in.
#[component]
fn SaveToTab(draft: Signal<Settings>) -> Element {
    let settings = draft();

    rsx! {
        div { class: "idm-fieldset",
            div { class: "idm-fieldset__title", "Default download directory" }
            input {
                class: "idm-input",
                r#type: "text",
                value: "{settings.default_dir}",
                oninput: move |evt| draft.write().default_dir = evt.value(),
            }
            p { class: "idm-hint",
                "New downloads start with this directory in their \"Save As\" field. Relative paths are resolved from the working directory."
            }
        }
    }
}

/// "Downloads" — how many files are fetched at the same time.
#[component]
fn DownloadsTab(draft: Signal<Settings>) -> Element {
    let settings = draft();

    rsx! {
        div { class: "idm-fieldset",
            div { class: "idm-fieldset__title", "Simultaneous downloads" }
            label { class: "idm-label", "Max. number of simultaneous downloads:" }
            input {
                class: "idm-input idm-input--short",
                r#type: "number",
                min: "1",
                max: "32",
                value: "{settings.max_concurrent}",
                oninput: move |evt| {
                    if let Ok(count) = evt.value().parse::<usize>() {
                        draft.write().max_concurrent = count;
                    }
                },
            }
            p { class: "idm-hint",
                "Downloads that are waiting in the queue start as soon as a running transfer is finished."
            }
        }
    }
}

/// A tab whose feature does not exist in ENDM yet.
#[component]
fn UnsupportedTab(title: String, text: String) -> Element {
    rsx! {
        div { class: "idm-fieldset",
            div { class: "idm-fieldset__title", "{title}" }
            p { class: "idm-hint", "{text}" }
        }
    }
}
