//! Download engine: worker tasks, their pause / resume / cancel controls and the
//! events they report back to the UI.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_channel::mpsc;
use futures_util::StreamExt;
use tokio::fs::{File, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::{Notify, Semaphore};

/// How many downloads run at the same time unless the user changes it in
/// "Options" → "Downloads".
pub const DEFAULT_CONCURRENCY: usize = 4;

/// How often a worker reports its progress. One event per chunk would drown
/// the UI on fast connections, so updates are batched to this interval — the
/// very last update before a pause, an error or the end of the file is always
/// sent, whatever the interval says.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(200);

/// A download request coming from the "Download File Info" dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadRequest {
    pub url: String,
    pub path: String,
    pub description: String,
    pub user_agent: String,
    pub timeout_secs: u64,
}

/// A [`DownloadRequest`] together with the identifier the window knows it by.
#[derive(Clone, Debug)]
pub struct DownloadSpec {
    pub id: usize,
    pub request: DownloadRequest,
    /// Bytes that are already on disk; the worker resumes with a Range request.
    pub resume_from: u64,
}

/// Commands the UI sends to the engine coroutine.
pub enum EngineCommand {
    /// Remember the request (so that it can be started later) and start it when
    /// `start_now` is set — otherwise it just waits in the queue.
    Add { spec: DownloadSpec, start_now: bool },
    /// Start or resume a download the engine already knows about.
    Start(usize),
    /// Stop the transfer but keep the partially written file.
    Pause(usize),
    /// Forget the download completely.
    Cancel(usize),
    /// Start every download that is waiting in the queue.
    StartQueue,
    /// Stop every download the queue has started.
    StopQueue,
    /// How many downloads may run at the same time.
    SetConcurrency(usize),
}

/// Events the workers report back to the engine coroutine.
#[derive(Debug)]
pub enum DownloadEvent {
    /// The worker got a connection slot and is about to request the file.
    Started(usize),
    /// A new batch of bytes is on disk. `total` is `None` when the server did
    /// not announce a size.
    Progress {
        id: usize,
        downloaded: u64,
        total: Option<u64>,
        rate: f64,
    },
    /// The download was stopped by the user; the partial file stays on disk.
    Paused(usize),
    /// The file is complete.
    Finished(usize),
    /// The transfer broke down. The partial file stays on disk so that the
    /// download can be resumed.
    Error(usize, String),
}

/// Pause / cancel flags shared between the window and a running worker.
#[derive(Default)]
pub struct DownloadControl {
    paused: AtomicBool,
    cancelled: AtomicBool,
    notify: Notify,
}

impl DownloadControl {
    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        // A worker parked on a pause must wake up so that it notices the
        // cancellation, too.
        self.paused.store(false, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Parks the worker until the download is resumed (or cancelled).
    pub async fn wait_until_resumed(&self) {
        while self.is_paused() && !self.is_cancelled() {
            let notified = self.notify.notified();
            tokio::pin!(notified);
            // Register for the wake-up *before* checking the flag again, so a
            // resume that happens in between is never missed.
            notified.as_mut().enable();
            if !self.is_paused() || self.is_cancelled() {
                break;
            }
            notified.await;
        }
    }

    /// Resolves as soon as the download is stopped or cancelled — even when
    /// the worker cannot check the flags itself because it is blocked on
    /// something else: a free connection slot, the response headers of a slow
    /// server or the next chunk of data.
    pub async fn wait_for_interrupt(&self) {
        loop {
            if self.is_paused() || self.is_cancelled() {
                return;
            }
            let notified = self.notify.notified();
            tokio::pin!(notified);
            // Register first, check second — the same dance as above.
            notified.as_mut().enable();
            if self.is_paused() || self.is_cancelled() {
                return;
            }
            notified.await;
        }
    }
}

/// Streams one file to disk, reporting progress through `tx`.
///
/// Every attempt starts with a fresh connection whose `Range` header is built
/// from the bytes that are already on disk, so stopping, failing and resuming
/// never corrupts the file. A stopped download parks *without* holding a
/// connection slot, which keeps the queue moving for everybody else.
pub async fn download_worker(
    spec: DownloadSpec,
    control: Arc<DownloadControl>,
    semaphore: Arc<Semaphore>,
    tx: mpsc::UnboundedSender<DownloadEvent>,
) {
    let id = spec.id;
    // The setting bounds three different waits: the connection setup, the gap
    // until the response headers arrive, and the silence between two chunks.
    let timeout = Duration::from_secs(spec.request.timeout_secs.max(1));

    'attempt: loop {
        // A stopped download parks here — without holding a connection slot.
        control.wait_until_resumed().await;
        if control.is_cancelled() {
            return;
        }

        // One connection slot per transfer, released again whenever we park.
        let _permit = tokio::select! {
            biased;
            _ = control.wait_for_interrupt() => {
                if control.is_cancelled() {
                    return;
                }
                // Stopped while the row still showed "Connecting...": the
                // worker never started, so report the stop ourselves.
                let _ = tx.unbounded_send(DownloadEvent::Paused(id));
                continue 'attempt;
            }
            acquired = semaphore.acquire() => match acquired {
                Ok(permit) => permit,
                Err(_) => return,
            },
        };
        if control.is_cancelled() {
            return;
        }
        if control.is_paused() {
            // A "Stop" arrived while waiting for the slot: give it back.
            continue 'attempt;
        }

        // Make sure the output directory exists — idempotent.
        if let Some(parent) = std::path::Path::new(&spec.request.path).parent() {
            if !parent.as_os_str().is_empty() {
                if let Err(err) = tokio::fs::create_dir_all(parent).await {
                    let _ = tx.unbounded_send(DownloadEvent::Error(id, err.to_string()));
                    return;
                }
            }
        }

        // What is on disk decides the Range header of this attempt.
        let offset = std::fs::metadata(&spec.request.path)
            .map(|metadata| metadata.len())
            .unwrap_or(spec.resume_from);

        // A fresh client per attempt: no pooled sockets that a long pause may
        // have left half-dead.
        let client = match reqwest::Client::builder()
            .connect_timeout(timeout)
            .user_agent(spec.request.user_agent.clone())
            .build()
        {
            Ok(client) => client,
            Err(err) => {
                let _ = tx.unbounded_send(DownloadEvent::Error(id, err.to_string()));
                return;
            }
        };

        let mut request = client.get(&spec.request.url);
        if offset > 0 {
            request = request.header(reqwest::header::RANGE, format!("bytes={offset}-"));
        }

        // Deliberately *no* total request timeout on the client: it would kill
        // every download that runs longer than `timeout`. The wait for the
        // response headers is bounded right below instead.
        let response = tokio::select! {
            biased;
            _ = control.wait_for_interrupt() => {
                if control.is_cancelled() {
                    return;
                }
                let _ = tx.unbounded_send(DownloadEvent::Paused(id));
                continue 'attempt;
            }
            answered = tokio::time::timeout(timeout, request.send()) => match answered {
                Ok(Ok(response)) => response,
                Ok(Err(err)) => {
                    let _ = tx.unbounded_send(DownloadEvent::Error(id, err.to_string()));
                    return;
                }
                Err(_) => {
                    let _ = tx.unbounded_send(DownloadEvent::Error(
                        id,
                        format!(
                            "The server did not respond within {} seconds.",
                            timeout.as_secs()
                        ),
                    ));
                    return;
                }
            },
        };

        let status = response.status();

        // The file on disk already is the whole file — nothing left to send.
        if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE && offset > 0 {
            let _ = tx.unbounded_send(DownloadEvent::Progress {
                id,
                downloaded: offset,
                total: Some(offset),
                rate: 0.0,
            });
            let _ = tx.unbounded_send(DownloadEvent::Finished(id));
            return;
        }

        if !status.is_success() {
            let _ = tx.unbounded_send(DownloadEvent::Error(id, format!("HTTP {status}")));
            return;
        }

        // A server only honours the Range request when it answers with 206.
        let partial = status == reqwest::StatusCode::PARTIAL_CONTENT;
        let mut downloaded = if partial { offset } else { 0 };

        let mut file = if partial {
            match OpenOptions::new()
                .create(true)
                .append(true)
                .open(&spec.request.path)
                .await
            {
                Ok(file) => file,
                Err(err) => {
                    let _ = tx.unbounded_send(DownloadEvent::Error(id, err.to_string()));
                    return;
                }
            }
        } else {
            match File::create(&spec.request.path).await {
                Ok(file) => file,
                Err(err) => {
                    let _ = tx.unbounded_send(DownloadEvent::Error(id, err.to_string()));
                    return;
                }
            }
        };

        let total = if partial {
            response
                .content_length()
                .map(|remaining| remaining + downloaded)
        } else {
            response.content_length()
        };

        let _ = tx.unbounded_send(DownloadEvent::Started(id));

        let stream = response.bytes_stream();
        tokio::pin!(stream);

        let mut rate = 0.0;
        let mut sample_bytes = 0u64;
        let mut sample_start = Instant::now();
        let mut last_progress: Option<Instant> = None;

        loop {
            if control.is_cancelled() {
                return;
            }

            tokio::select! {
                biased;
                _ = control.wait_for_interrupt() => {
                    if control.is_cancelled() {
                        return;
                    }
                    // "Stop": flush what we have, hand the connection slot
                    // back and park. The partial file stays as it is.
                    let _ = file.flush().await;
                    let _ = tx.unbounded_send(DownloadEvent::Progress {
                        id,
                        downloaded,
                        total,
                        rate: 0.0,
                    });
                    let _ = tx.unbounded_send(DownloadEvent::Paused(id));
                    continue 'attempt;
                }
                chunk = stream.next() => match chunk {
                    None => break, // the body ended where the server said it would
                    Some(Err(err)) => {
                        let _ = file.flush().await;
                        let _ = tx.unbounded_send(DownloadEvent::Progress {
                            id,
                            downloaded,
                            total,
                            rate: 0.0,
                        });
                        let _ = tx.unbounded_send(DownloadEvent::Error(id, err.to_string()));
                        return;
                    }
                    Some(Ok(bytes)) => {
                        if let Err(err) = file.write_all(&bytes).await {
                            let _ = tx.unbounded_send(DownloadEvent::Error(id, err.to_string()));
                            return;
                        }
                        downloaded += bytes.len() as u64;
                        sample_bytes += bytes.len() as u64;

                        let sampled = sample_start.elapsed();
                        if sampled >= Duration::from_millis(250) {
                            rate = sample_bytes as f64 / sampled.as_secs_f64();
                            sample_bytes = 0;
                            sample_start = Instant::now();
                        }

                        let now = Instant::now();
                        let due = last_progress
                            .map(|sent| now.duration_since(sent) >= PROGRESS_INTERVAL)
                            .unwrap_or(true);
                        if due {
                            let _ = tx.unbounded_send(DownloadEvent::Progress {
                                id,
                                downloaded,
                                total,
                                rate,
                            });
                            last_progress = Some(now);
                        }
                    }
                },
                _ = tokio::time::sleep(timeout) => {
                    // No data for `timeout` seconds: the connection stalled.
                    let _ = file.flush().await;
                    let _ = tx.unbounded_send(DownloadEvent::Progress {
                        id,
                        downloaded,
                        total,
                        rate: 0.0,
                    });
                    let _ = tx.unbounded_send(DownloadEvent::Error(
                        id,
                        format!(
                            "Connection timed out: no data arrived for {} seconds.",
                            timeout.as_secs()
                        ),
                    ));
                    return;
                }
            }
        }

        // Everything the server promised has arrived.
        if let Err(err) = file.flush().await {
            let _ = tx.unbounded_send(DownloadEvent::Error(id, err.to_string()));
            return;
        }
        let _ = tx.unbounded_send(DownloadEvent::Progress {
            id,
            downloaded,
            total,
            rate: 0.0,
        });
        let _ = tx.unbounded_send(DownloadEvent::Finished(id));
        return;
    }
}



