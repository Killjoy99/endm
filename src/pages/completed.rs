use dioxus::prelude::*;

use crate::actions::use_actions;
use crate::pages::DownloadList;
use crate::types::Category;

/// The "Finished" category of the main window, reached through its own route.
#[component]
pub fn Completed() -> Element {
    let actions = use_actions();

    use_effect(move || {
        actions.select_category(Category::Finished);
    });

    rsx! {
        DownloadList {}
    }
}