mod actions;
mod app;
mod dialogs;
mod engine;
#[cfg(test)]
mod engine_tests;
mod icons;
mod pages;
mod shell;
mod types;
mod util;

use app::App;

fn main() {
    dioxus::launch(App);
}