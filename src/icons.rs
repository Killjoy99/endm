//! Toolbar and tree icons, drawn as inline SVG so that the UI needs no image
//! assets. Every icon uses a 32x32 view box and is sized by CSS.

use dioxus::prelude::*;

use crate::actions::MenuAction;

/// The icon of a toolbar button or menu entry.
pub fn tool_icon(action: MenuAction) -> Element {
    match action {
        MenuAction::AddUrl => rsx! {
            svg { class: "idm-icon", view_box: "0 0 32 32",
                rect { x: "13", y: "4", width: "6", height: "12", fill: "#2f8f2f" }
                polygon { points: "6,15 26,15 16,27", fill: "#2f8f2f" }
                rect { x: "4", y: "28", width: "24", height: "3", fill: "#8a6d1f" }
            }
        },
        MenuAction::Resume => rsx! {
            svg { class: "idm-icon", view_box: "0 0 32 32",
                polygon { points: "9,4 27,16 9,28", fill: "#1f7f1f" }
            }
        },
        MenuAction::Stop => rsx! {
            svg { class: "idm-icon", view_box: "0 0 32 32",
                rect { x: "7", y: "7", width: "18", height: "18", rx: "2", fill: "#c62828" }
                rect { x: "11", y: "11", width: "10", height: "10", fill: "#e9a0a0" }
            }
        },
        MenuAction::StopAll => rsx! {
            svg { class: "idm-icon", view_box: "0 0 32 32",
                rect { x: "3", y: "9", width: "13", height: "14", fill: "#c62828" }
                rect { x: "16", y: "9", width: "13", height: "14", fill: "#8e1616" }
            }
        },
        MenuAction::Delete => rsx! {
            svg { class: "idm-icon", view_box: "0 0 32 32",
                rect { x: "5", y: "13", width: "22", height: "6", transform: "rotate(45 16 16)", fill: "#c62828" }
                rect { x: "5", y: "13", width: "22", height: "6", transform: "rotate(-45 16 16)", fill: "#c62828" }
            }
        },
        MenuAction::DeleteCompleted => rsx! {
            svg { class: "idm-icon", view_box: "0 0 32 32",
                rect { x: "9", y: "11", width: "14", height: "17", fill: "#8e1616" }
                rect { x: "6", y: "7", width: "20", height: "4", fill: "#5c0f0f" }
                rect { x: "13", y: "3", width: "6", height: "4", fill: "#5c0f0f" }
                rect { x: "12", y: "14", width: "2", height: "11", fill: "#d9a0a0" }
                rect { x: "18", y: "14", width: "2", height: "11", fill: "#d9a0a0" }
            }
        },
        MenuAction::Options => rsx! {
            svg { class: "idm-icon", view_box: "0 0 32 32",
                circle { cx: "16", cy: "16", r: "10", fill: "#40639c" }
                rect { x: "14", y: "1", width: "4", height: "7", fill: "#40639c" }
                rect { x: "14", y: "24", width: "4", height: "7", fill: "#40639c" }
                rect { x: "1", y: "14", width: "7", height: "4", fill: "#40639c" }
                rect { x: "24", y: "14", width: "7", height: "4", fill: "#40639c" }
                circle { cx: "16", cy: "16", r: "4", fill: "#ffffff" }
            }
        },
        MenuAction::Scheduler => rsx! {
            svg { class: "idm-icon", view_box: "0 0 32 32",
                circle { cx: "16", cy: "17", r: "11", fill: "#f2f2f2", stroke: "#40639c", stroke_width: "3" }
                rect { x: "14", y: "9", width: "3", height: "9", fill: "#40639c" }
                rect { x: "14", y: "16", width: "8", height: "3", fill: "#40639c" }
            }
        },
        MenuAction::StartQueue => rsx! {
            svg { class: "idm-icon", view_box: "0 0 32 32",
                polygon { points: "4,6 15,13 4,20", fill: "#1f7f1f" }
                rect { x: "18", y: "6", width: "11", height: "3", fill: "#4a4a4a" }
                rect { x: "18", y: "12", width: "11", height: "3", fill: "#4a4a4a" }
                rect { x: "18", y: "18", width: "11", height: "3", fill: "#4a4a4a" }
                rect { x: "4", y: "24", width: "25", height: "3", fill: "#4a4a4a" }
            }
        },
        MenuAction::StopQueue => rsx! {
            svg { class: "idm-icon", view_box: "0 0 32 32",
                rect { x: "3", y: "7", width: "13", height: "13", fill: "#c62828" }
                rect { x: "18", y: "6", width: "11", height: "3", fill: "#4a4a4a" }
                rect { x: "18", y: "12", width: "11", height: "3", fill: "#4a4a4a" }
                rect { x: "18", y: "18", width: "11", height: "3", fill: "#4a4a4a" }
                rect { x: "4", y: "24", width: "25", height: "3", fill: "#4a4a4a" }
            }
        },
        MenuAction::About | MenuAction::TellAFriend => rsx! {
            svg { class: "idm-icon", view_box: "0 0 32 32",
                rect { x: "2", y: "7", width: "28", height: "19", fill: "#f2d17a", stroke: "#8a6d1f", stroke_width: "2" }
                polygon { points: "3,8 16,19 29,8", fill: "#e0b64a", stroke: "#8a6d1f", stroke_width: "2" }
            }
        },
        _ => rsx! {
            svg { class: "idm-icon", view_box: "0 0 32 32",
                rect { x: "6", y: "6", width: "20", height: "20", fill: "#9a9a9a" }
            }
        },
    }
}

/// A folder of the "Categories" tree.
pub fn folder_icon(open: bool) -> Element {
    if open {
        rsx! {
            svg { class: "idm-tree-icon", view_box: "0 0 32 32",
                polygon { points: "2,11 11,11 14,7 28,7 28,13 2,13", fill: "#e0b64a", stroke: "#8a6d1f" }
                polygon { points: "2,11 30,11 25,25 2,25", fill: "#f2d17a", stroke: "#8a6d1f" }
            }
        }
    } else {
        rsx! {
            svg { class: "idm-tree-icon", view_box: "0 0 32 32",
                rect { x: "2", y: "8", width: "28", height: "18", rx: "2", fill: "#f2d17a", stroke: "#8a6d1f" }
                rect { x: "2", y: "8", width: "12", height: "5", fill: "#e0b64a" }
            }
        }
    }
}

/// The little screen shown next to "All Downloads".
pub fn desktop_icon() -> Element {
    rsx! {
        svg { class: "idm-tree-icon", view_box: "0 0 32 32",
            rect { x: "2", y: "6", width: "28", height: "18", fill: "#d8d8d8", stroke: "#7a7a7a" }
            rect { x: "4", y: "8", width: "24", height: "14", fill: "#40639c" }
            polygon { points: "12,24 20,24 23,29 9,29", fill: "#b0b0b0", stroke: "#7a7a7a" }
        }
    }
}

