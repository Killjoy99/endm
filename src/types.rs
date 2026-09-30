use dioxus::prelude::*;

use crate::engine::DEFAULT_CONCURRENCY;
use crate::util::extension_of;

/// The value shown in the "Status" column of the list view.
#[derive(Clone, PartialEq, Debug)]
pub enum Status {
    Queued,
    Connecting,
    Downloading,
    Paused,
    Complete,
    Error(String),
}

impl Status {
    pub fn label(&self) -> &'static str {
        match self {
            Status::Queued => "Queued",
            Status::Connecting => "Connecting...",
            Status::Downloading => "Downloading...",
            Status::Paused => "Paused",
            Status::Complete => "Complete",
            Status::Error(_) => "Error",
        }
    }

    /// Small sort key so that the "Status" column sorts meaningfully.
    pub fn rank(&self) -> u8 {
        match self {
            Status::Error(_) => 0,
            Status::Paused => 1,
            Status::Queued => 2,
            Status::Connecting => 3,
            Status::Downloading => 4,
            Status::Complete => 5,
        }
    }

    pub fn is_running(&self) -> bool {
        matches!(self, Status::Connecting | Status::Downloading)
    }
}

/// One row of the downloads list view.
#[derive(Clone, PartialEq, Debug)]
pub struct DownloadStatus {
    pub id: usize,
    pub url: String,
    pub name: String,
    pub path: String,
    pub description: String,
    /// The "Q" column — the download belongs to the download queue.
    pub in_queue: bool,
    pub downloaded: u64,
    pub total: Option<u64>,
    /// Bytes per second, sampled by the worker.
    pub rate: f64,
    pub status: Status,
}

impl DownloadStatus {
    pub fn percent(&self) -> f64 {
        match self.total {
            Some(total) if total > 0 => {
                (self.downloaded as f64 / total as f64 * 100.0).clamp(0.0, 100.0)
            }
            _ => 0.0,
        }
    }

    /// Seconds until the download is finished, when it can be estimated.
    pub fn eta_seconds(&self) -> Option<f64> {
        let total = self.total?;
        if self.rate <= 0.0 || self.downloaded >= total {
            return None;
        }
        Some((total - self.downloaded) as f64 / self.rate)
    }

    pub fn category(&self) -> Category {
        Category::of_extension(&extension_of(&self.name))
    }
}

/// The nodes of the "Categories" tree on the left hand side of the window.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Category {
    All,
    Unfinished,
    Finished,
    Video,
    Music,
    Documents,
    Programs,
    Compressed,
    Other,
}

impl Category {
    /// Everything the tree displays, in the order IDM shows it.
    pub const TREE: [Category; 9] = [
        Category::All,
        Category::Unfinished,
        Category::Finished,
        Category::Video,
        Category::Music,
        Category::Documents,
        Category::Programs,
        Category::Compressed,
        Category::Other,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Category::All => "All Downloads",
            Category::Unfinished => "Unfinished",
            Category::Finished => "Finished",
            Category::Video => "Video",
            Category::Music => "Music",
            Category::Documents => "Documents",
            Category::Programs => "Programs",
            Category::Compressed => "Compressed",
            Category::Other => "Other",
        }
    }

    /// The file types IDM assigns to this category ("Options -> File Types").
    pub fn extensions(&self) -> &'static [&'static str] {
        match self {
            Category::Video => &[
                "mp4", "mkv", "avi", "mov", "webm", "flv", "wmv", "m4v", "mpg", "mpeg", "ts",
                "3gp",
            ],
            Category::Music => &[
                "mp3", "wav", "flac", "aac", "ogg", "oga", "m4a", "wma", "opus", "mid",
            ],
            Category::Documents => &[
                "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "txt", "rtf", "odt", "ods",
                "csv", "epub", "md",
            ],
            Category::Programs => &[
                "exe", "msi", "apk", "dmg", "pkg", "deb", "rpm", "appimage", "jar", "bat", "cmd",
            ],
            Category::Compressed => &[
                "zip", "rar", "7z", "tar", "gz", "bz2", "xz", "zst", "iso", "cab",
            ],
            Category::All | Category::Unfinished | Category::Finished | Category::Other => &[],
        }
    }

    /// Categories that own a list of file types (used by "Options -> File Types").
    pub const TYPES: [Category; 5] = [
        Category::Video,
        Category::Music,
        Category::Documents,
        Category::Programs,
        Category::Compressed,
    ];

    pub fn of_extension(extension: &str) -> Category {
        let extension = extension.to_ascii_lowercase();
        for category in Category::TYPES {
            if category.extensions().contains(&extension.as_str()) {
                return category;
            }
        }
        Category::Other
    }

    pub fn matches(&self, download: &DownloadStatus) -> bool {
        match self {
            Category::All => true,
            Category::Unfinished => !matches!(download.status, Status::Complete),
            Category::Finished => matches!(download.status, Status::Complete),
            Category::Other => download.category() == Category::Other,
            category => download.category() == *category,
        }
    }
}

/// The columns of the list view that can be sorted.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortKey {
    Queue,
    FileName,
    Size,
    Status,
    TimeLeft,
    TransferRate,
    Description,
}

/// Which column the list view is sorted by, and in which direction.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SortState {
    pub key: SortKey,
    pub ascending: bool,
}

impl Default for SortState {
    fn default() -> Self {
        Self {
            key: SortKey::FileName,
            ascending: true,
        }
    }
}

/// The four toolbar looks IDM ships with.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToolbarStyle {
    Large3D,
    Small3D,
    LargeNeon,
    SmallNeon,
}

impl ToolbarStyle {
    pub const ALL: [ToolbarStyle; 4] = [
        ToolbarStyle::Large3D,
        ToolbarStyle::Small3D,
        ToolbarStyle::LargeNeon,
        ToolbarStyle::SmallNeon,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            ToolbarStyle::Large3D => "Large buttons",
            ToolbarStyle::Small3D => "Small buttons",
            ToolbarStyle::LargeNeon => "Neon buttons (large)",
            ToolbarStyle::SmallNeon => "Neon buttons (small)",
        }
    }

    /// CSS modifier used by the toolbar.
    pub fn class(&self) -> &'static str {
        match self {
            ToolbarStyle::Large3D => "idm-toolbar--large-3d",
            ToolbarStyle::Small3D => "idm-toolbar--small-3d",
            ToolbarStyle::LargeNeon => "idm-toolbar--large-neon",
            ToolbarStyle::SmallNeon => "idm-toolbar--small-neon",
        }
    }

    pub fn is_small(&self) -> bool {
        matches!(self, ToolbarStyle::Small3D | ToolbarStyle::SmallNeon)
    }
}

/// What the scheduler does when its time is reached.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ScheduleAction {
    StartQueue,
    StopQueue,
    StopAll,
}

impl ScheduleAction {
    pub const ALL: [ScheduleAction; 3] = [
        ScheduleAction::StartQueue,
        ScheduleAction::StopQueue,
        ScheduleAction::StopAll,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            ScheduleAction::StartQueue => "Start queue",
            ScheduleAction::StopQueue => "Stop queue",
            ScheduleAction::StopAll => "Stop all downloads",
        }
    }
}

/// A pending scheduler task.
#[derive(Clone, PartialEq, Debug)]
pub struct Schedule {
    pub action: ScheduleAction,
    pub hour: u32,
    pub minute: u32,
}

impl Schedule {
    pub fn time_label(&self) -> String {
        format!("{:02}:{:02}", self.hour, self.minute)
    }
}

/// Everything the "Options" dialog can change.
#[derive(Clone, PartialEq, Debug)]
pub struct Settings {
    /// "Save To" tab — default directory for new downloads.
    pub default_dir: String,
    /// "Connection" tab — user agent used for downloads added by hand.
    pub user_agent: String,
    /// "Connection" tab — connect / response / idle-data timeout in seconds.
    pub timeout_secs: u64,
    /// "Downloads" tab — maximum number of simultaneous downloads.
    pub max_concurrent: usize,
    /// "General" tab — ask before dropping a download from the list.
    pub confirm_delete: bool,
    /// "General" tab — show the "Categories" tree.
    pub show_categories: bool,
    /// "General" tab — show the status bar.
    pub show_status_bar: bool,
    /// "General" tab — "Toolbar look".
    pub toolbar_style: ToolbarStyle,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            default_dir: "./downloads".to_string(),
            user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) ENDM/0.1".to_string(),
            timeout_secs: 60,
            max_concurrent: DEFAULT_CONCURRENCY,
            confirm_delete: true,
            show_categories: true,
            show_status_bar: true,
            toolbar_style: ToolbarStyle::Large3D,
        }
    }
}

/// The top level menus of the menu bar.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MenuId {
    Tasks,
    Downloads,
    View,
    Options,
    Help,
}

impl MenuId {
    pub const ALL: [MenuId; 5] = [
        MenuId::Tasks,
        MenuId::Downloads,
        MenuId::View,
        MenuId::Options,
        MenuId::Help,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            MenuId::Tasks => "Tasks",
            MenuId::Downloads => "Downloads",
            MenuId::View => "View",
            MenuId::Options => "Options",
            MenuId::Help => "Help",
        }
    }
}

/// Which menu is currently dropped down, if any.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum MenuPopup {
    None,
    /// A menu of the menu bar is open.
    Bar(MenuId),
    /// The pop-up menu (right click in the list view) is open at these coordinates.
    Context { x: f64, y: f64 },
}

/// The modal dialogs of the main window.
#[derive(Clone, PartialEq, Debug)]
pub enum Dialog {
    None,
    /// Step 1 of "Add URL": asks for the web address.
    AddUrl,
    /// Step 2: the "Download File Info" dialog.
    FileInfo {
        address: String,
        user_name: String,
        password: String,
    },
    /// "Properties" of a single download.
    Properties(usize),
    Scheduler,
    About,
    ConfirmDelete(usize),
}

/// Application state shared through the context API.
#[derive(Clone, Copy)]
pub struct DownloadStore {
    pub downloads: Signal<Vec<DownloadStatus>>,
    pub selected: Signal<Option<usize>>,
    pub category: Signal<Category>,
    pub sort: Signal<SortState>,
    pub settings: Signal<Settings>,
    pub dialog: Signal<Dialog>,
    pub menu: Signal<MenuPopup>,
    pub schedule: Signal<Option<Schedule>>,
    pub next_id: Signal<usize>,
}