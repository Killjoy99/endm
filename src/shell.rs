//! The IDM style main window: menu bar, toolbar, "Categories" tree, status bar and
//! the pop-up menu of the downloads list.

use dioxus::prelude::*;
use dioxus::router::components::Outlet;
use dioxus::router::Navigator;

use crate::actions::{use_actions, AppActions, MenuAction};
use crate::app::Route;
use crate::dialogs::DialogHost;
use crate::icons;
use crate::types::{Category, MenuId, MenuPopup, Status, ToolbarStyle};
use crate::util::{format_rate, format_size};

/// The buttons of the main toolbar, in the order IDM shows them.
const TOOLBAR_BUTTONS: [MenuAction; 11] = [
    MenuAction::AddUrl,
    MenuAction::Resume,
    MenuAction::Stop,
    MenuAction::StopAll,
    MenuAction::Delete,
    MenuAction::DeleteCompleted,
    MenuAction::Options,
    MenuAction::Scheduler,
    MenuAction::StartQueue,
    MenuAction::StopQueue,
    MenuAction::TellAFriend,
];

/// The main window of the application.
#[component]
pub fn Shell() -> Element {
    let actions = use_actions();
    let settings = actions.settings();
    let body_class = if settings.show_categories {
        "idm-body"
    } else {
        "idm-body idm-body--wide"
    };

    rsx! {
        div { class: "idm-app",
            MenuBar {}
            Toolbar {}
            div { class: "{body_class}",
                if settings.show_categories {
                    CategoriesPanel {}
                }
                main { class: "idm-listview", Outlet::<Route> {} }
            }
            if settings.show_status_bar {
                StatusBar {}
            }
            MenuBackdrop {}
            ContextMenu {}
            DialogHost {}
        }
    }
}

/// Runs an action, switching to the "Options" window when needed.
fn run_action(action: MenuAction, actions: &AppActions, navigator: &Navigator) {
    if action == MenuAction::Options {
        let _ = navigator.push(Route::Options {});
    } else {
        action.perform(actions);
    }
    actions.close_menu();
}

#[component]
fn Toolbar() -> Element {
    let actions = use_actions();
    let class = format!("idm-toolbar {}", actions.settings().toolbar_style.class());

    rsx! {
        div { class: "{class}",
            for action in TOOLBAR_BUTTONS {
                ToolButton { action }
            }
        }
    }
}

#[component]
fn ToolButton(action: MenuAction) -> Element {
    let actions = use_actions();
    let navigator = use_navigator();
    let label = action.label();
    let disabled = action.is_disabled(&actions);
    let small = actions.settings().toolbar_style.is_small();

    rsx! {
        button {
            class: "idm-toolbutt",
            disabled,
            title: "{label}",
            onclick: move |_| run_action(action, &actions, &navigator),
            span { class: "idm-toolbutt__icon", {icons::tool_icon(action)} }
            if !small {
                span { class: "idm-toolbutt__label", "{label}" }
            }
        }
    }
}

#[component]
fn MenuBar() -> Element {
    let actions = use_actions();
    let open = *actions.store.menu.read();

    rsx! {
        div { class: "idm-menubar",
            for menu in MenuId::ALL {
                div {
                    class: if open == MenuPopup::Bar(menu) { "idm-menu idm-menu--open" } else { "idm-menu" },
                    button {
                        class: "idm-menu__title",
                        onclick: move |_| actions.toggle_menu(menu),
                        "{menu.label()}"
                    }
                    if open == MenuPopup::Bar(menu) {
                        div { class: "idm-menu__dropdown", MenuContent { menu } }
                    }
                }
            }
        }
    }
}

/// One entry of a drop down menu.
#[component]
fn MenuEntry(action: MenuAction) -> Element {
    let actions = use_actions();
    let navigator = use_navigator();
    let disabled = action.is_disabled(&actions);
    let checked = action.is_checked(&actions);
    let label = action.menu_label();
    let shortcut = action.shortcut();

    rsx! {
        button {
            class: "idm-menu__item",
            disabled,
            onclick: move |_| run_action(action, &actions, &navigator),
            if checked {
                span { class: "idm-menu__check", "✔" }
            } else {
                span { class: "idm-menu__check" }
            }
            span { class: "idm-menu__text", "{label}" }
            if let Some(shortcut) = shortcut {
                span { class: "idm-menu__shortcut", "{shortcut}" }
            }
        }
    }
}

/// The items of a top level menu.
#[component]
fn MenuContent(menu: MenuId) -> Element {
    match menu {
        MenuId::Tasks => rsx! {
            MenuEntry { action: MenuAction::AddUrl }
            div { class: "idm-menu__sep" }
            MenuEntry { action: MenuAction::StartQueue }
            MenuEntry { action: MenuAction::StopQueue }
            MenuEntry { action: MenuAction::StopAll }
            div { class: "idm-menu__sep" }
            MenuEntry { action: MenuAction::Scheduler }
            MenuEntry { action: MenuAction::TurnOffScheduler }
        },
        MenuId::Downloads => rsx! {
            MenuEntry { action: MenuAction::Resume }
            MenuEntry { action: MenuAction::Stop }
            MenuEntry { action: MenuAction::StopAll }
            div { class: "idm-menu__sep" }
            MenuEntry { action: MenuAction::Delete }
            MenuEntry { action: MenuAction::DeleteCompleted }
            div { class: "idm-menu__sep" }
            MenuEntry { action: MenuAction::Properties }
        },
        MenuId::View => rsx! {
            MenuEntry { action: MenuAction::ShowCategories }
            MenuEntry { action: MenuAction::ShowStatusBar }
            div { class: "idm-menu__sep" }
            div { class: "idm-menu__group", "Toolbar look" }
            MenuEntry { action: MenuAction::ToolbarStyle(ToolbarStyle::Large3D) }
            MenuEntry { action: MenuAction::ToolbarStyle(ToolbarStyle::Small3D) }
            MenuEntry { action: MenuAction::ToolbarStyle(ToolbarStyle::LargeNeon) }
            MenuEntry { action: MenuAction::ToolbarStyle(ToolbarStyle::SmallNeon) }
        },
        MenuId::Options => rsx! {
            MenuEntry { action: MenuAction::Options }
            div { class: "idm-menu__sep" }
            MenuEntry { action: MenuAction::Scheduler }
            MenuEntry { action: MenuAction::TurnOffScheduler }
        },
        MenuId::Help => rsx! {
            MenuEntry { action: MenuAction::TellAFriend }
            div { class: "idm-menu__sep" }
            MenuEntry { action: MenuAction::About }
        },
    }
}

/// Invisible layer that closes an open menu when the user clicks anywhere else.
#[component]
fn MenuBackdrop() -> Element {
    let actions = use_actions();
    let open = *actions.store.menu.read() != MenuPopup::None;

    rsx! {
        if open {
            div {
                class: "idm-menu-backdrop",
                onclick: move |_| actions.close_menu(),
                oncontextmenu: move |evt| {
                    evt.prevent_default();
                    actions.close_menu();
                },
            }
        }
    }
}

/// The pop-up menu IDM shows when a download is right clicked.
#[component]
fn ContextMenu() -> Element {
    let actions = use_actions();
    let popup = *actions.store.menu.read();

    match popup {
        MenuPopup::Context { x, y } => rsx! {
            div {
                class: "idm-menu__dropdown idm-menu__dropdown--popup",
                style: "left: {x}px; top: {y}px;",
                MenuEntry { action: MenuAction::AddUrl }
                div { class: "idm-menu__sep" }
                MenuEntry { action: MenuAction::Resume }
                MenuEntry { action: MenuAction::Stop }
                div { class: "idm-menu__sep" }
                MenuEntry { action: MenuAction::Delete }
                MenuEntry { action: MenuAction::Properties }
            }
        },
        _ => rsx! {},
    }
}

/// The "Categories" tree on the left hand side of the window.
#[component]
fn CategoriesPanel() -> Element {
    let mut expanded = use_signal(|| true);
    let is_expanded = expanded();

    rsx! {
        aside { class: "idm-categories",
            div { class: "idm-categories__title", "Categories" }
            div { class: "idm-tree",
                div { class: "idm-tree__row",
                    button {
                        class: "idm-tree__expander",
                        onclick: move |evt| {
                            evt.stop_propagation();
                            let shown = expanded();
                            expanded.set(!shown);
                        },
                        if is_expanded { "−" } else { "+" }
                    }
                    CategoryItem { category: Category::All, root: true }
                }
                if is_expanded {
                    for category in Category::TREE.iter().skip(1) {
                        CategoryItem { category: *category, root: false }
                    }
                }
            }
        }
    }
}

/// One node of the "Categories" tree.
#[component]
fn CategoryItem(category: Category, root: bool) -> Element {
    let actions = use_actions();
    let navigator = use_navigator();
    let current: Route = use_route();
    let on_home = matches!(current, Route::Home {});
    let active = *actions.store.category.read() == category;
    let label = category.label();
    let class = if active {
        "idm-tree__item idm-tree__item--active"
    } else {
        "idm-tree__item"
    };
    let indent = if root {
        "padding-left: 0px;"
    } else {
        "padding-left: 22px;"
    };

    rsx! {
        div {
            class: "{class}",
            style: "{indent}",
            onclick: move |_| {
                actions.select_category(category);
                if !on_home {
                    let _ = navigator.push(Route::Home {});
                }
            },
            if root {
                {icons::desktop_icon()}
            } else {
                {icons::folder_icon(active)}
            }
            span { class: "idm-tree__label", "{label}" }
        }
    }
}

/// The panes at the bottom of the main window.
#[component]
fn StatusBar() -> Element {
    let actions = use_actions();
    let selected = actions.selected_download();
    let running = actions.count_where(|download| download.status.is_running());
    let queued = actions.count_where(|download| matches!(download.status, Status::Queued));
    let failed = actions.count_where(|download| matches!(download.status, Status::Error(_)));
    let total = actions.count_where(|_| true);
    let rate = actions.total_rate();
    let downloaded: u64 = actions
        .store
        .downloads
        .read()
        .iter()
        .map(|download| download.downloaded)
        .sum();

    let message = match &selected {
        Some(download) => format!("{} — {}", download.name, download.status.label()),
        None if running > 0 => format!("Downloading: {running} file(s) in progress"),
        None if queued > 0 => format!("{queued} file(s) are waiting in the download queue"),
        None if failed > 0 => format!("{failed} download(s) finished with an error"),
        None => "Ready".to_string(),
    };
    let rate_text = format!("Transfer rate: {}", format_rate(rate));
    let downloaded_text = format!("Downloaded: {}", format_size(downloaded));
    let total_text = format!("Total files: {total}");
    let schedule_text = actions
        .store
        .schedule
        .read()
        .as_ref()
        .map(|schedule| {
            format!(
                "Scheduler: {} at {}",
                schedule.action.label(),
                schedule.time_label()
            )
        });

    rsx! {
        footer { class: "idm-statusbar",
            span { class: "idm-statusbar__pane idm-statusbar__pane--grow", "{message}" }
            if let Some(schedule_text) = schedule_text {
                span { class: "idm-statusbar__pane", "{schedule_text}" }
            }
            span { class: "idm-statusbar__pane", "{rate_text}" }
            span { class: "idm-statusbar__pane", "{downloaded_text}" }
            span { class: "idm-statusbar__pane", "{total_text}" }
        }
    }
}
