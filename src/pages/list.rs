//! The details view of the main window: the list of downloads with the columns
//! IDM shows, plus selection, sorting and the pop-up menu.

use dioxus::prelude::*;

use crate::actions::use_actions;
use crate::types::{Category, Dialog, DownloadStatus, MenuPopup, SortKey, Status};
use crate::util::{format_eta, format_rate, format_size};

/// The columns of the list view, in the order IDM shows them.
const COLUMNS: [(SortKey, &str, &str); 7] = [
    (SortKey::Queue, "Q", "idm-col--q"),
    (SortKey::FileName, "File Name", "idm-col--name"),
    (SortKey::Size, "Size", "idm-col--size"),
    (SortKey::Status, "Status", "idm-col--status"),
    (SortKey::TimeLeft, "Time Left", "idm-col--time"),
    (SortKey::TransferRate, "Transfer Rate", "idm-col--rate"),
    (SortKey::Description, "Description", "idm-col--description"),
];

/// The details view that fills the main window.
#[component]
pub fn DownloadList() -> Element {
    let actions = use_actions();
    let category = *actions.store.category.read();
    let rows = actions.rows(category);

    rsx! {
        div { class: "idm-list",
            div { class: "idm-list__header",
                for (key, label, class_name) in COLUMNS {
                    ColumnHeader { sort_key: key, label, class_name }
                }
            }
            div { class: "idm-list__body",
                if rows.is_empty() {
                    EmptyState { category }
                }
                for download in rows {
                    DownloadRow { download }
                }
            }
        }
    }
}

/// One clickable column header with the sort arrow.
#[component]
fn ColumnHeader(sort_key: SortKey, label: String, class_name: String) -> Element {
    let actions = use_actions();
    let sort = *actions.store.sort.read();
    let active = sort.key == sort_key;
    let arrow = if !active {
        ""
    } else if sort.ascending {
        "▲"
    } else {
        "▼"
    };
    let class = format!("idm-list__col {class_name}");

    rsx! {
        button {
            class: "{class}",
            onclick: move |_| actions.sort_by(sort_key),
            span { class: "idm-list__col-label", "{label}" }
            span { class: "idm-list__col-arrow", "{arrow}" }
        }
    }
}

/// Shown while the current category has nothing in it.
#[component]
fn EmptyState(category: Category) -> Element {
    let actions = use_actions();
    let empty = actions.count_where(|_| true) == 0;
    let message = if empty {
        "There are no downloads yet. Use the \"Add URL\" button to add a new download."
    } else {
        "There are no downloads in this category yet."
    };
    let hint = format!("Category: {}", category.label());

    rsx! {
        div { class: "idm-empty",
            p { class: "idm-empty__message", "{message}" }
            p { class: "idm-empty__hint", "{hint}" }
            button {
                class: "idm-button",
                onclick: move |_| actions.open(Dialog::AddUrl),
                "Add URL"
            }
        }
    }
}

/// One row of the details view.
#[component]
fn DownloadRow(download: DownloadStatus) -> Element {
    let actions = use_actions();
    let id = download.id;
    let selected = actions.selected_id() == Some(id);
    let class = if selected {
        "idm-row idm-row--selected"
    } else {
        "idm-row"
    };

    let name = download.name.clone();
    let has_progress = download.total.is_some() && !matches!(download.status, Status::Queued);
    let bar_style = format!("width: {:.1}%;", download.percent());
    let percent_text = format!("{:.0}%", download.percent());
    let size_text = match download.total {
        Some(total) => format_size(total),
        None => format_size(download.downloaded),
    };
    let status_text = download.status.label();
    let eta_text = match download.eta_seconds() {
        Some(seconds) if download.status.is_running() => format_eta(seconds),
        _ => String::new(),
    };
    let rate_text = format_rate(download.rate);
    let description = if download.description.is_empty() {
        download.url.clone()
    } else {
        download.description.clone()
    };
    let queue_text = if download.in_queue { "Yes" } else { "" };

    rsx! {
        div {
            class: "{class}",
            onclick: move |_| actions.select(Some(id)),
            ondoubleclick: move |_| actions.open(Dialog::Properties(id)),
            oncontextmenu: move |evt| {
                evt.prevent_default();
                actions.select(Some(id));
                let point = evt.client_coordinates();
                actions.open_menu(MenuPopup::Context { x: point.x, y: point.y });
            },
            div { class: "idm-cell idm-col--q", "{queue_text}" }
            div { class: "idm-cell idm-col--name", title: "{name}", span { "{name}" } }
            div { class: "idm-cell idm-col--size", "{size_text}" }
            div { class: "idm-cell idm-col--status",
                div { class: "idm-status-line",
                    span { class: "idm-status-line__text", "{status_text}" }
                    if has_progress {
                        span { class: "idm-status-line__percent", "{percent_text}" }
                    }
                }
                if has_progress {
                    div { class: "idm-progress",
                        div { class: "idm-progress__bar", style: "{bar_style}" }
                    }
                }
            }
            div { class: "idm-cell idm-col--time", "{eta_text}" }
            div { class: "idm-cell idm-col--rate", "{rate_text}" }
            div { class: "idm-cell idm-col--description", title: "{description}", span { "{description}" } }
        }
    }
}
