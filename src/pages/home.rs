use dioxus::prelude::*;

use crate::pages::DownloadList;

/// The main window: the download list of the selected category.
#[component]
pub fn Home() -> Element {
    rsx! {
        DownloadList {}
    }
}
