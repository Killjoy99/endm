use dioxus::prelude::*;
use dioxus::router::components::Router;
use futures_channel::mpsc;
use futures_util::StreamExt;
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use tokio::sync::Semaphore;

use crate::engine::{
    download_worker, DownloadControl, DownloadEvent, DownloadRequest, DownloadSpec, EngineCommand,
    DEFAULT_CONCURRENCY,
};
use crate::pages::{Completed, Home, Options};
use crate::shell::Shell;
use crate::types::{
    Category, Dialog, DownloadStatus, DownloadStore, MenuPopup, Settings, SortState, Status,
};

/// Every window of the application. The main window is wrapped in [`Shell`], the
/// "Options" dialog is a window of its own — just like in IDM.
#[derive(Clone, Debug, PartialEq, Routable)]
#[rustfmt::skip]
pub enum Route {
    #[layout(Shell)]
        #[route("/")]
        Home {},
        #[route("/completed")]
        Completed {},
    #[end_layout]
    #[route("/settings")]
    Options {},
}

#[component]
pub fn App() -> Element {
    let store = DownloadStore {
        downloads: use_signal(Vec::<DownloadStatus>::new),
        selected: use_signal(|| None::<usize>),
        category: use_signal(|| Category::All),
        sort: use_signal(SortState::default),
        settings: use_signal(Settings::default),
        dialog: use_signal(|| Dialog::None),
        menu: use_signal(|| MenuPopup::None),
        schedule: use_signal(|| None),
        next_id: use_signal(|| 0usize),
    };

    use_context_provider(|| store);

    let downloads = store.downloads;
    use_coroutine(move |mut rx: UnboundedReceiver<EngineCommand>| {
        let mut downloads = downloads;
        async move {
            let (event_tx, mut event_rx) = mpsc::unbounded::<DownloadEvent>();
            let semaphore = Arc::new(Semaphore::new(DEFAULT_CONCURRENCY));
            let mut concurrent = DEFAULT_CONCURRENCY;

            // Live workers, the original requests and the download queue.
            let mut controls: HashMap<usize, Arc<DownloadControl>> = HashMap::new();
            let mut requests: HashMap<usize, DownloadRequest> = HashMap::new();
            let mut queue: BTreeSet<usize> = BTreeSet::new();
            // Set by "Start queue": from then on every queued download — even
            // one added later with "Download Later" — runs on its own.
            let mut queue_running = false;

            loop {
                tokio::select! {
                    Ok(command) = rx.recv() => match command {
                        EngineCommand::Add { spec, start_now } => {
                            requests.insert(spec.id, spec.request.clone());
                            if start_now {
                                spawn_worker(spec, &semaphore, &event_tx, &mut controls);
                            } else {
                                queue.insert(spec.id);
                                if queue_running {
                                    start_download(
                                        spec.id,
                                        &mut controls,
                                        &requests,
                                        &semaphore,
                                        &event_tx,
                                    );
                                }
                            }
                        }
                        EngineCommand::Start(id) => {
                            start_download(id, &mut controls, &requests, &semaphore, &event_tx);
                        }
                        EngineCommand::Pause(id) => {
                            if let Some(control) = controls.get(&id) {
                                control.pause();
                            }
                        }
                        EngineCommand::Cancel(id) => {
                            if let Some(control) = controls.remove(&id) {
                                control.cancel();
                            }
                            requests.remove(&id);
                            queue.remove(&id);
                        }
                        EngineCommand::StartQueue => {
                            queue_running = true;
                            for id in queue.iter().copied().collect::<Vec<usize>>() {
                                start_download(id, &mut controls, &requests, &semaphore, &event_tx);
                            }
                        }
                        EngineCommand::StopQueue => {
                            queue_running = false;
                            for id in queue.iter() {
                                if let Some(control) = controls.get(id) {
                                    control.pause();
                                }
                            }
                        }
                        EngineCommand::SetConcurrency(wanted) => {
                            let wanted = wanted.max(1);
                            if wanted > concurrent {
                                semaphore.add_permits(wanted - concurrent);
                            } else if wanted < concurrent {
                                semaphore.forget_permits(concurrent - wanted);
                            }
                            concurrent = wanted;
                        }
                    },
                    Some(event) = event_rx.next() => match event {
                        DownloadEvent::Started(id) => {
                            update_row(&mut downloads, id, |row| {
                                row.status = Status::Downloading;
                                row.in_queue = false;
                            });
                        }
                        DownloadEvent::Progress { id, downloaded, total, rate } => {
                            update_row(&mut downloads, id, |row| {
                                row.downloaded = downloaded;
                                row.total = total;
                                row.rate = rate;
                                if !matches!(row.status, Status::Paused) {
                                    row.status = Status::Downloading;
                                }
                            });
                        }
                        DownloadEvent::Paused(id) => {
                            update_row(&mut downloads, id, |row| {
                                row.status = Status::Paused;
                                row.rate = 0.0;
                            });
                        }
                        DownloadEvent::Finished(id) => {
                            controls.remove(&id);
                            requests.remove(&id);
                            queue.remove(&id);
                            update_row(&mut downloads, id, |row| {
                                row.status = Status::Complete;
                                row.rate = 0.0;
                                if let Some(total) = row.total {
                                    row.downloaded = total;
                                } else {
                                    // The server never announced a size, so what
                                    // arrived *is* the whole file.
                                    row.total = Some(row.downloaded);
                                }
                            });
                        }
                        DownloadEvent::Error(id, message) => {
                            controls.remove(&id);
                            update_row(&mut downloads, id, |row| {
                                row.status = Status::Error(message);
                                row.rate = 0.0;
                            });
                        }
                    },
                    else => break,
                }
            }
        }
    });

    rsx! {
        document::Stylesheet { href: asset!("/assets/styles.css") }
        document::Link {
            rel: "stylesheet",
            href: "https://fonts.googleapis.com/css2?family=Orbitron:wght@500;600;700&family=Roboto:wght@300;400;500;700&display=swap",
        }
        Router::<Route> {}
    }
}

/// Starts (or resumes) a download the engine already knows about.
fn start_download(
    id: usize,
    controls: &mut HashMap<usize, Arc<DownloadControl>>,
    requests: &HashMap<usize, DownloadRequest>,
    semaphore: &Arc<Semaphore>,
    event_tx: &mpsc::UnboundedSender<DownloadEvent>,
) {
    if let Some(control) = controls.get(&id) {
        // The worker is still parked on a pause, so it can simply be woken up.
        control.resume();
    } else if let Some(request) = requests.get(&id).cloned() {
        spawn_worker(
            DownloadSpec {
                id,
                request,
                resume_from: 0,
            },
            semaphore,
            event_tx,
            controls,
        );
    }
}

/// Starts a worker for `spec`, remembering how to pause and cancel it.
fn spawn_worker(
    mut spec: DownloadSpec,
    semaphore: &Arc<Semaphore>,
    event_tx: &mpsc::UnboundedSender<DownloadEvent>,
    controls: &mut HashMap<usize, Arc<DownloadControl>>,
) {
    // Anything that is already on disk is resumed with a Range request.
    spec.resume_from = std::fs::metadata(&spec.request.path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);

    let control = Arc::new(DownloadControl::default());
    controls.insert(spec.id, control.clone());
    tokio::spawn(download_worker(
        spec,
        control,
        semaphore.clone(),
        event_tx.clone(),
    ));
}

/// Applies `update` to the row with `id`, if it is still in the list.
fn update_row(
    downloads: &mut Signal<Vec<DownloadStatus>>,
    id: usize,
    update: impl FnOnce(&mut DownloadStatus),
) {
    if let Some(row) = downloads.write().iter_mut().find(|row| row.id == id) {
        update(row);
    }
}
