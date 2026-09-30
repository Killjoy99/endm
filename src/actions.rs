//! The single place where the menus, the toolbar, the list view and the dialogs
//! change state. Every UI element talks to the engine through this type.

use dioxus::prelude::*;

use crate::engine::{DownloadRequest, DownloadSpec, EngineCommand};
use crate::types::{
    Category, Dialog, DownloadStatus, DownloadStore, MenuId, MenuPopup, Schedule, Settings,
    SortKey, SortState, Status, ToolbarStyle,
};
use crate::util::file_name_of_path;

/// Handle to the shared store and to the engine coroutine.
#[derive(Clone, Copy)]
pub struct AppActions {
    pub store: DownloadStore,
    pub tx: Coroutine<EngineCommand>,
}

/// Grab the store and the engine handle inside a component.
pub fn use_actions() -> AppActions {
    AppActions {
        store: use_context(),
        tx: use_coroutine_handle(),
    }
}

impl AppActions {
    // ---------- queries ----------

    pub fn download(&self, id: usize) -> Option<DownloadStatus> {
        self.store
            .downloads
            .read()
            .iter()
            .find(|download| download.id == id)
            .cloned()
    }

    pub fn selected_id(&self) -> Option<usize> {
        *self.store.selected.read()
    }

    pub fn selected_download(&self) -> Option<DownloadStatus> {
        let id = self.selected_id()?;
        self.download(id)
    }

    pub fn settings(&self) -> Settings {
        self.store.settings.read().clone()
    }

    /// Downloads that belong to `category`, sorted the way the list view wants them.
    pub fn rows(&self, category: Category) -> Vec<DownloadStatus> {
        let sort = *self.store.sort.read();
        let mut rows: Vec<DownloadStatus> = self
            .store
            .downloads
            .read()
            .iter()
            .filter(|download| category.matches(download))
            .cloned()
            .collect();

        rows.sort_by(|a, b| {
            let ordering = match sort.key {
                SortKey::Queue => a.in_queue.cmp(&b.in_queue),
                SortKey::FileName => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                SortKey::Size => a.total.cmp(&b.total),
                SortKey::Status => a.status.rank().cmp(&b.status.rank()),
                SortKey::TimeLeft => a
                    .eta_seconds()
                    .partial_cmp(&b.eta_seconds())
                    .unwrap_or(std::cmp::Ordering::Equal),
                SortKey::TransferRate => a
                    .rate
                    .partial_cmp(&b.rate)
                    .unwrap_or(std::cmp::Ordering::Equal),
                SortKey::Description => a
                    .description
                    .to_lowercase()
                    .cmp(&b.description.to_lowercase()),
            };

            if sort.ascending {
                ordering
            } else {
                ordering.reverse()
            }
        });

        rows
    }

    pub fn count_where(&self, predicate: impl Fn(&DownloadStatus) -> bool) -> usize {
        self.store
            .downloads
            .read()
            .iter()
            .filter(|download| predicate(download))
            .count()
    }

    pub fn any_running(&self) -> bool {
        self.count_where(|download| download.status.is_running()) > 0
    }

    pub fn any_queued(&self) -> bool {
        self.count_where(|download| matches!(download.status, Status::Queued)) > 0
    }

    pub fn any_complete(&self) -> bool {
        self.count_where(|download| matches!(download.status, Status::Complete)) > 0
    }

    pub fn total_rate(&self) -> f64 {
        self.store
            .downloads
            .read()
            .iter()
            .map(|download| download.rate)
            .sum()
    }

    // ---------- "Downloads" commands ----------

    pub fn add_download(&self, request: DownloadRequest, start_now: bool) {
        let mut next_id = self.store.next_id;
        let id = *next_id.read();
        *next_id.write() = id + 1;

        let row = DownloadStatus {
            id,
            name: file_name_of_path(&request.path),
            url: request.url.clone(),
            path: request.path.clone(),
            description: request.description.clone(),
            in_queue: !start_now,
            downloaded: 0,
            total: None,
            rate: 0.0,
            status: if start_now {
                Status::Connecting
            } else {
                Status::Queued
            },
        };

        let mut downloads = self.store.downloads;
        downloads.write().push(row);
        let mut selected = self.store.selected;
        selected.set(Some(id));
        self.tx.send(EngineCommand::Add {
            spec: DownloadSpec {
                id,
                request,
                resume_from: 0,
            },
            start_now,
        });
    }

    /// "Resume" — continue a stopped, failed or queued download.
    pub fn start(&self, id: usize) {
        self.tx.send(EngineCommand::Start(id));

        let waiting = matches!(
            self.download(id).map(|download| download.status),
            Some(Status::Paused) | Some(Status::Error(_)) | Some(Status::Queued)
        );
        if waiting {
            self.set_status(id, Status::Connecting);
        }
    }

    /// "Stop" — pause a single transfer.
    pub fn pause(&self, id: usize) {
        self.tx.send(EngineCommand::Pause(id));
        self.set_status(id, Status::Paused);
    }

    /// "Stop All" — pause every running transfer.
    pub fn stop_all(&self) {
        let running: Vec<usize> = self
            .store
            .downloads
            .read()
            .iter()
            .filter(|download| download.status.is_running())
            .map(|download| download.id)
            .collect();

        for id in running {
            self.tx.send(EngineCommand::Pause(id));
            self.set_status(id, Status::Paused);
        }
    }

    pub fn start_queue(&self) {
        self.tx.send(EngineCommand::StartQueue);
    }

    pub fn stop_queue(&self) {
        self.tx.send(EngineCommand::StopQueue);
    }

    /// "Delete" — drops one download from the list, asking for confirmation first
    /// while the download is not finished.
    pub fn delete(&self, id: usize) {
        let settings = self.settings();
        let finished = matches!(
            self.download(id).map(|download| download.status),
            Some(Status::Complete)
        );

        if settings.confirm_delete && !finished {
            self.open(Dialog::ConfirmDelete(id));
            return;
        }

        self.remove(id);
    }

    pub fn remove(&self, id: usize) {
        self.tx.send(EngineCommand::Cancel(id));
        let mut downloads = self.store.downloads;
        downloads
            .write()
            .retain(|download| download.id != id);

        let mut selected = self.store.selected;
        let was_selected = *selected.read() == Some(id);
        if was_selected {
            selected.set(None);
        }
    }

    /// "Delete Completed" — drops every finished download from the list.
    pub fn delete_completed(&self) {
        let finished: Vec<usize> = self
            .store
            .downloads
            .read()
            .iter()
            .filter(|download| matches!(download.status, Status::Complete))
            .map(|download| download.id)
            .collect();

        for id in finished {
            self.remove(id);
        }
    }

    pub fn set_description(&self, id: usize, description: String) {
        let mut downloads = self.store.downloads;
        let mut rows = downloads.write();
        if let Some(row) = rows.iter_mut().find(|row| row.id == id) {
            row.description = description;
        }
    }

    // ---------- window state ----------

    pub fn open(&self, dialog: Dialog) {
        let mut target = self.store.dialog;
        target.set(dialog);
    }

    pub fn close_dialog(&self) {
        let mut target = self.store.dialog;
        target.set(Dialog::None);
    }

    pub fn select(&self, id: Option<usize>) {
        let mut selected = self.store.selected;
        selected.set(id);
    }

    pub fn select_category(&self, category: Category) {
        let mut current = self.store.category;
        current.set(category);
    }

    /// Click on a column header: the same column flips the sort direction.
    pub fn sort_by(&self, key: SortKey) {
        let mut sort = self.store.sort;
        let current = *sort.read();
        sort.set(if current.key == key {
            SortState {
                key,
                ascending: !current.ascending,
            }
        } else {
            SortState {
                key,
                ascending: true,
            }
        });
    }

    pub fn save_settings(&self, settings: Settings) {
        let concurrent = settings.max_concurrent;
        let mut target = self.store.settings;
        target.set(settings);
        self.tx.send(EngineCommand::SetConcurrency(concurrent));
    }

    pub fn set_toolbar_style(&self, style: ToolbarStyle) {
        let mut target = self.store.settings;
        target.write().toolbar_style = style;
    }

    pub fn toggle_categories(&self) {
        let mut settings = self.store.settings;
        let shown = !settings.read().show_categories;
        settings.write().show_categories = shown;
    }

    pub fn toggle_status_bar(&self) {
        let mut settings = self.store.settings;
        let shown = !settings.read().show_status_bar;
        settings.write().show_status_bar = shown;
    }

    pub fn set_status(&self, id: usize, status: Status) {
        let mut downloads = self.store.downloads;
        let mut rows = downloads.write();
        if let Some(row) = rows.iter_mut().find(|row| row.id == id) {
            if !status.is_running() {
                row.rate = 0.0;
            }
            row.status = status;
        }
    }

    pub fn set_schedule(&self, schedule: Option<Schedule>) {
        let mut target = self.store.schedule;
        target.set(schedule);
    }

    // ---------- menus ----------

    /// Click on a menu bar title: opens it, or closes it when it is already open.
    pub fn toggle_menu(&self, menu: MenuId) {
        let mut popup = self.store.menu;
        let current = *popup.read();
        popup.set(if current == MenuPopup::Bar(menu) {
            MenuPopup::None
        } else {
            MenuPopup::Bar(menu)
        });
    }

    pub fn open_menu(&self, popup: MenuPopup) {
        let mut target = self.store.menu;
        target.set(popup);
    }

    pub fn close_menu(&self) {
        let mut target = self.store.menu;
        target.set(MenuPopup::None);
    }
}

/// Every action the menus, the toolbar and the pop-up menu can run.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MenuAction {
    AddUrl,
    Resume,
    Stop,
    StopAll,
    StartQueue,
    StopQueue,
    Delete,
    DeleteCompleted,
    Properties,
    Scheduler,
    TurnOffScheduler,
    Options,
    About,
    TellAFriend,
    ShowCategories,
    ShowStatusBar,
    ToolbarStyle(ToolbarStyle),
}

impl MenuAction {
    /// The caption used on the toolbar buttons.
    pub fn label(&self) -> &'static str {
        match self {
            MenuAction::AddUrl => "Add URL",
            MenuAction::Resume => "Resume",
            MenuAction::Stop => "Stop",
            MenuAction::StopAll => "Stop All",
            MenuAction::StartQueue => "Start Queue",
            MenuAction::StopQueue => "Stop Queue",
            MenuAction::Delete => "Delete",
            MenuAction::DeleteCompleted => "Delete Completed",
            MenuAction::Properties => "Properties",
            MenuAction::Scheduler => "Scheduler",
            MenuAction::TurnOffScheduler => "Turn off the scheduler",
            MenuAction::Options => "Options",
            MenuAction::About => "About ENDM",
            MenuAction::TellAFriend => "Tell A Friend",
            MenuAction::ShowCategories => "Categories",
            MenuAction::ShowStatusBar => "Status Bar",
            MenuAction::ToolbarStyle(style) => style.label(),
        }
    }

    /// Menu captions are suffixed with "..." when they open a dialog.
    pub fn menu_label(&self) -> String {
        match self {
            MenuAction::AddUrl
            | MenuAction::Properties
            | MenuAction::Scheduler
            | MenuAction::Options
            | MenuAction::About
            | MenuAction::TellAFriend => format!("{}...", self.label()),
            _ => self.label().to_string(),
        }
    }

    /// The key that IDM shows on the right of a menu item.
    pub fn shortcut(&self) -> Option<&'static str> {
        match self {
            MenuAction::AddUrl => Some("Ins"),
            MenuAction::Resume => Some("F2"),
            MenuAction::Stop => Some("F3"),
            MenuAction::StopAll => Some("F4"),
            MenuAction::Delete => Some("Del"),
            MenuAction::DeleteCompleted => Some("Shift+Del"),
            MenuAction::Properties => Some("Alt+Enter"),
            MenuAction::Options => Some("Ctrl+O"),
            _ => None,
        }
    }

    /// Buttons and menu items are greyed out when the action is not possible,
    /// exactly like IDM does it.
    pub fn is_disabled(&self, actions: &AppActions) -> bool {
        match self {
            MenuAction::AddUrl
            | MenuAction::Options
            | MenuAction::About
            | MenuAction::TellAFriend
            | MenuAction::Scheduler
            | MenuAction::ShowCategories
            | MenuAction::ShowStatusBar
            | MenuAction::ToolbarStyle(_) => false,
            MenuAction::Resume => !matches!(
                actions.selected_download().map(|download| download.status),
                Some(Status::Queued) | Some(Status::Paused) | Some(Status::Error(_))
            ),
            MenuAction::Stop => !matches!(
                actions.selected_download().map(|download| download.status),
                Some(Status::Connecting) | Some(Status::Downloading)
            ),
            MenuAction::StopAll | MenuAction::StopQueue => !actions.any_running(),
            MenuAction::StartQueue => !actions.any_queued(),
            MenuAction::Delete | MenuAction::Properties => actions.selected_download().is_none(),
            MenuAction::DeleteCompleted => !actions.any_complete(),
            MenuAction::TurnOffScheduler => actions.store.schedule.read().is_none(),
        }
    }

    /// Items of the "View" menu carry a check mark when they are switched on.
    pub fn is_checked(&self, actions: &AppActions) -> bool {
        let settings = actions.settings();
        match self {
            MenuAction::ShowCategories => settings.show_categories,
            MenuAction::ShowStatusBar => settings.show_status_bar,
            MenuAction::ToolbarStyle(style) => settings.toolbar_style == *style,
            _ => false,
        }
    }

    /// Runs the action. Switching windows is left to the caller, which owns the router.
    pub fn perform(&self, actions: &AppActions) {
        match self {
            MenuAction::AddUrl => actions.open(Dialog::AddUrl),
            MenuAction::Resume => {
                if let Some(id) = actions.selected_id() {
                    actions.start(id);
                }
            }
            MenuAction::Stop => {
                if let Some(id) = actions.selected_id() {
                    actions.pause(id);
                }
            }
            MenuAction::StopAll => actions.stop_all(),
            MenuAction::StartQueue => actions.start_queue(),
            MenuAction::StopQueue => actions.stop_queue(),
            MenuAction::Delete => {
                if let Some(id) = actions.selected_id() {
                    actions.delete(id);
                }
            }
            MenuAction::DeleteCompleted => actions.delete_completed(),
            MenuAction::Properties => {
                if let Some(id) = actions.selected_id() {
                    actions.open(Dialog::Properties(id));
                }
            }
            MenuAction::Scheduler => actions.open(Dialog::Scheduler),
            MenuAction::TurnOffScheduler => actions.set_schedule(None),
            MenuAction::About | MenuAction::TellAFriend => actions.open(Dialog::About),
            MenuAction::ShowCategories => actions.toggle_categories(),
            MenuAction::ShowStatusBar => actions.toggle_status_bar(),
            MenuAction::ToolbarStyle(style) => actions.set_toolbar_style(*style),
            // The caller navigates to the "Options" window.
            MenuAction::Options => {}
        }
    }
}
