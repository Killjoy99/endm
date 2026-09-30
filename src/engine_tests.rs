//! Hermetic end-to-end tests for the download engine.
//!
//! Each test starts a tiny HTTP/1.1 server inside the test process that
//! streams a generated payload — with `Range` support — so every path of
//! [`download_worker`] is exercised without touching the network.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_channel::mpsc;
use futures_util::StreamExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;

use crate::engine::{
    download_worker, DownloadControl, DownloadEvent, DownloadRequest, DownloadSpec,
};

/// What a test server does while answering one request.
#[derive(Clone, Copy)]
enum ServerMode {
    /// Write the payload in pieces of `chunk_size` bytes (`0` = all at once),
    /// optionally sleeping between two pieces.
    Stream {
        chunk_size: usize,
        delay: Option<Duration>,
    },
    /// Send the headers and the first piece, then fall silent: the worker's
    /// idle watchdog has to notice.
    StallAfterFirstChunk,
    /// Read the request and never write a single byte of the response.
    StallBeforeHeaders,
}

/// A running test server plus the `Range` offsets it has been asked for.
struct TestServer {
    addr: SocketAddr,
    ranges: Arc<Mutex<Vec<u64>>>,
}

impl TestServer {
    fn url(&self, file: &str) -> String {
        format!("http://{}/{}", self.addr, file)
    }

    /// Every `Range` request the server answered with a `206`, in order.
    fn ranges_seen(&self) -> Vec<u64> {
        self.ranges.lock().expect("ranges lock").clone()
    }
}

async fn start_server(payload: Vec<u8>, mode: ServerMode) -> TestServer {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local addr");
    let ranges: Arc<Mutex<Vec<u64>>> = Arc::default();
    let payload = Arc::new(payload);

    let task_ranges = ranges.clone();
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let payload = payload.clone();
            let ranges = task_ranges.clone();
            tokio::spawn(handle_connection(stream, payload, ranges, mode));
        }
    });

    TestServer { addr, ranges }
}

async fn handle_connection(
    mut stream: TcpStream,
    payload: Arc<Vec<u8>>,
    ranges: Arc<Mutex<Vec<u64>>>,
    mode: ServerMode,
) {
    if let ServerMode::StallBeforeHeaders = mode {
        tokio::time::sleep(Duration::from_secs(120)).await;
        return;
    }

    // Read the request head; a GET has no body.
    let mut head = Vec::new();
    let mut buffer = [0u8; 1024];
    loop {
        let Ok(read) = stream.read(&mut buffer).await else {
            return;
        };
        if read == 0 {
            return;
        }
        head.extend_from_slice(&buffer[..read]);
        if head.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    let head = String::from_utf8_lossy(&head).to_ascii_lowercase();
    let offset = head
        .lines()
        .find_map(|line| line.strip_prefix("range: bytes="))
        .and_then(|range| range.trim_end_matches('-').parse::<u64>().ok())
        .unwrap_or(0);

    let len = payload.len() as u64;
    if offset > 0 {
        ranges
            .lock()
            .expect("ranges lock")
            .push(offset.min(len));
    }

    // The client already has the whole file.
    if offset >= len && offset > 0 {
        let head = format!(
            "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{len}\r\n\
             Content-Length: 0\r\nConnection: close\r\n\r\n"
        );
        let _ = stream.write_all(head.as_bytes()).await;
        return;
    }

    let (status, extra) = if offset > 0 {
        let end = len - 1;
        (
            "206 Partial Content",
            format!("Content-Range: bytes {offset}-{end}/{len}\r\n"),
        )
    } else {
        ("200 OK", String::new())
    };
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/octet-stream\r\n\
         Content-Length: {}\r\n{extra}Connection: close\r\n\r\n",
        len - offset
    );
    if stream.write_all(head.as_bytes()).await.is_err() {
        return;
    }

    let body = &payload[offset as usize..];
    match mode {
        ServerMode::StallBeforeHeaders => {}
        ServerMode::StallAfterFirstChunk => {
            let first = body.get(..8 * 1024).unwrap_or(body);
            let _ = stream.write_all(first).await;
            let _ = stream.flush().await;
            tokio::time::sleep(Duration::from_secs(120)).await;
        }
        ServerMode::Stream { chunk_size, delay } => {
            if chunk_size == 0 {
                let _ = stream.write_all(body).await;
                let _ = stream.flush().await;
            } else {
                for chunk in body.chunks(chunk_size) {
                    if stream.write_all(chunk).await.is_err() {
                        return;
                    }
                    let _ = stream.flush().await;
                    if let Some(delay) = delay {
                        tokio::time::sleep(delay).await;
                    }
                }
            }
        }
    }
}

/// Deterministic content for the payload of a test.
fn payload(len: usize) -> Vec<u8> {
    (0..len).map(|index| (index % 251) as u8).collect()
}

fn request(url: &str, path: &std::path::Path, timeout_secs: u64) -> DownloadRequest {
    DownloadRequest {
        url: url.to_string(),
        path: path.to_string_lossy().into_owned(),
        description: "test download".to_string(),
        user_agent: "ENDM-test/0.1".to_string(),
        timeout_secs,
    }
}

/// A scratch file in the system temp directory, unique per process.
fn temp_path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("endm-tests-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir.join(name)
}

fn spawn_worker(
    spec: DownloadSpec,
    control: Arc<DownloadControl>,
) -> (mpsc::UnboundedReceiver<DownloadEvent>, JoinHandle<()>) {
    spawn_on(spec, control, Arc::new(Semaphore::new(1)))
}

/// Runs a worker on an existing connection-slot semaphore, so that a test can
/// show what happens when several downloads share one slot.
fn spawn_on(
    spec: DownloadSpec,
    control: Arc<DownloadControl>,
    semaphore: Arc<Semaphore>,
) -> (mpsc::UnboundedReceiver<DownloadEvent>, JoinHandle<()>) {
    let (tx, rx) = mpsc::unbounded();
    let handle = tokio::spawn(download_worker(spec, control, semaphore, tx));
    (rx, handle)
}

async fn next_event(
    rx: &mut mpsc::UnboundedReceiver<DownloadEvent>,
    seconds: u64,
) -> DownloadEvent {
    tokio::time::timeout(Duration::from_secs(seconds), rx.next())
        .await
        .expect("an event should arrive in time")
        .expect("the event channel must stay open")
}

/// Drains events until `Finished(id)` shows up and fails on any `Error`.
async fn wait_for_finish(rx: &mut mpsc::UnboundedReceiver<DownloadEvent>, id: usize) {
    loop {
        match next_event(rx, 10).await {
            DownloadEvent::Finished(found) => {
                assert_eq!(found, id, "the expected download finished");
                return;
            }
            DownloadEvent::Error(found, message) => {
                panic!("download {found} failed: {message}");
            }
            _ => {}
        }
    }
}

/// A worker must be gone after its terminal event.
async fn worker_exits(worker: JoinHandle<()>) {
    tokio::time::timeout(Duration::from_secs(5), worker)
        .await
        .expect("the worker task exits")
        .expect("the worker must not panic");
}

#[tokio::test]
async fn downloads_the_whole_file() {
    let bytes = payload(96 * 1024);
    let server = start_server(
        bytes.clone(),
        ServerMode::Stream {
            chunk_size: 8 * 1024,
            delay: None,
        },
    )
    .await;

    let path = temp_path("whole-file.bin");
    let control = Arc::new(DownloadControl::default());
    let spec = DownloadSpec {
        id: 1,
        request: request(&server.url("whole-file.bin"), &path, 10),
        resume_from: 0,
    };
    let (mut rx, worker) = spawn_worker(spec, control);

    assert!(matches!(
        next_event(&mut rx, 5).await,
        DownloadEvent::Started(1)
    ));
    wait_for_finish(&mut rx, 1).await;
    worker_exits(worker).await;

    assert_eq!(std::fs::read(&path).expect("file on disk"), bytes);
    // A fresh download never needs a Range request.
    assert!(server.ranges_seen().is_empty());
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn stopping_parks_the_download_and_frees_the_slot() {
    let bytes = payload(256 * 1024);
    let server = start_server(
        bytes.clone(),
        ServerMode::Stream {
            chunk_size: 8 * 1024,
            delay: Some(Duration::from_millis(40)),
        },
    )
    .await;

    let path = temp_path("parked.bin");
    let control = Arc::new(DownloadControl::default());
    let spec = DownloadSpec {
        id: 2,
        request: request(&server.url("parked.bin"), &path, 10),
        resume_from: 0,
    };

    // Both downloads share the single connection slot.
    let semaphore = Arc::new(Semaphore::new(1));
    let (mut rx, worker) = spawn_on(spec, control.clone(), semaphore.clone());

    assert!(matches!(
        next_event(&mut rx, 5).await,
        DownloadEvent::Started(2)
    ));
    // Wait for the first bytes to hit the disk.
    loop {
        match next_event(&mut rx, 5).await {
            DownloadEvent::Progress { id: 2, .. } => break,
            DownloadEvent::Error(_, message) => panic!("download failed: {message}"),
            _ => {}
        }
    }

    control.pause();
    let mut stopped = false;
    while !stopped {
        match next_event(&mut rx, 5).await {
            DownloadEvent::Paused(2) => stopped = true,
            DownloadEvent::Started(2) | DownloadEvent::Progress { id: 2, .. } => {}
            other => panic!("unexpected event while stopping: {other:?}"),
        }
    }

    let partial = std::fs::metadata(&path)
        .expect("the partial file exists")
        .len();
    assert!(
        partial > 0 && partial < bytes.len() as u64,
        "the file must hold exactly the bytes downloaded so far"
    );

    // The slot the parked download held is handed to somebody else: without
    // the fix the second download would wait forever.
    let other_bytes = payload(48 * 1024);
    let other_server = start_server(
        other_bytes.clone(),
        ServerMode::Stream {
            chunk_size: 0,
            delay: None,
        },
    )
    .await;
    let other_path = temp_path("while-parked.bin");
    let other_control = Arc::new(DownloadControl::default());
    let other_spec = DownloadSpec {
        id: 3,
        request: request(&other_server.url("while-parked.bin"), &other_path, 10),
        resume_from: 0,
    };
    let (mut other_rx, other_worker) = spawn_on(other_spec, other_control, semaphore.clone());

    assert!(
        matches!(
            next_event(&mut other_rx, 5).await,
            DownloadEvent::Started(3)
        ),
        "the free connection slot must be handed out while the other download is parked"
    );
    wait_for_finish(&mut other_rx, 3).await;
    worker_exits(other_worker).await;
    assert_eq!(std::fs::read(&other_path).expect("file"), other_bytes);

    // Resuming opens a fresh connection that continues at the bytes on disk.
    control.resume();
    wait_for_finish(&mut rx, 2).await;
    worker_exits(worker).await;

    assert_eq!(std::fs::read(&path).expect("file"), bytes);
    assert_eq!(
        server.ranges_seen(),
        vec![partial],
        "the resume must continue exactly where the file stops on disk"
    );

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&other_path);
}

#[tokio::test]
async fn cancel_stops_the_transfer() {
    let bytes = payload(256 * 1024);
    let server = start_server(
        bytes.clone(),
        ServerMode::Stream {
            chunk_size: 8 * 1024,
            delay: Some(Duration::from_millis(40)),
        },
    )
    .await;

    let path = temp_path("cancelled.bin");
    let control = Arc::new(DownloadControl::default());
    let spec = DownloadSpec {
        id: 4,
        request: request(&server.url("cancelled.bin"), &path, 10),
        resume_from: 0,
    };
    let (mut rx, worker) = spawn_worker(spec, control.clone());

    assert!(matches!(
        next_event(&mut rx, 5).await,
        DownloadEvent::Started(4)
    ));
    loop {
        match next_event(&mut rx, 5).await {
            DownloadEvent::Progress { id: 4, .. } => break,
            DownloadEvent::Error(_, message) => panic!("download failed: {message}"),
            _ => {}
        }
    }

    control.cancel();
    worker_exits(worker).await;

    // Nothing may announce a completion after the cancellation.
    match tokio::time::timeout(Duration::from_millis(300), rx.next()).await {
        Err(_) => {}
        Ok(Some(DownloadEvent::Finished(id))) => {
            panic!("the cancelled download {id} must not finish")
        }
        // A closed channel is exactly what a cancelled worker leaves behind:
        // its sender died with the task.
        Ok(_) => {}
    }

    let partial = std::fs::metadata(&path)
        .expect("the partial file exists")
        .len();
    assert!(
        partial < bytes.len() as u64,
        "the file must stay where the cancellation hit it"
    );
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn a_stalled_body_reports_a_timeout() {
    let server = start_server(payload(64 * 1024), ServerMode::StallAfterFirstChunk).await;

    let path = temp_path("stalled-body.bin");
    let control = Arc::new(DownloadControl::default());
    let spec = DownloadSpec {
        id: 5,
        request: request(&server.url("stalled-body.bin"), &path, 1),
        resume_from: 0,
    };
    let (mut rx, worker) = spawn_worker(spec, control);

    assert!(matches!(
        next_event(&mut rx, 5).await,
        DownloadEvent::Started(5)
    ));
    // The server sends one piece and then keeps quiet: the watchdog has to
    // turn the silence into an error instead of waiting forever.
    let mut failed = false;
    while !failed {
        match next_event(&mut rx, 10).await {
            DownloadEvent::Error(5, message) => {
                assert!(message.contains("timed out"), "unexpected error: {message}");
                failed = true;
            }
            DownloadEvent::Finished(id) => panic!("download {id} must not finish"),
            _ => {}
        }
    }
    worker_exits(worker).await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn a_server_that_never_answers_reports_a_timeout() {
    let server = start_server(payload(1024), ServerMode::StallBeforeHeaders).await;

    let path = temp_path("silent-server.bin");
    let control = Arc::new(DownloadControl::default());
    let spec = DownloadSpec {
        id: 6,
        request: request(&server.url("silent-server.bin"), &path, 1),
        resume_from: 0,
    };
    let (mut rx, worker) = spawn_worker(spec, control);

    // The worker never gets as far as `Started`.
    match next_event(&mut rx, 10).await {
        DownloadEvent::Error(6, message) => {
            assert!(
                message.contains("did not respond"),
                "unexpected error: {message}"
            );
        }
        other => panic!("expected an error, got {other:?}"),
    }
    worker_exits(worker).await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn a_partial_file_is_resumed_with_a_range_request() {
    let bytes = payload(60 * 1024);
    let server = start_server(
        bytes.clone(),
        ServerMode::Stream {
            chunk_size: 0,
            delay: None,
        },
    )
    .await;

    // Pretend a previous run of the app already wrote this much.
    let path = temp_path("resume.bin");
    let kept = 20 * 1024;
    std::fs::write(&path, &bytes[..kept]).expect("seed the partial file");

    let control = Arc::new(DownloadControl::default());
    let spec = DownloadSpec {
        id: 7,
        request: request(&server.url("resume.bin"), &path, 10),
        resume_from: kept as u64,
    };
    let (mut rx, worker) = spawn_worker(spec, control);

    assert!(matches!(
        next_event(&mut rx, 5).await,
        DownloadEvent::Started(7)
    ));
    wait_for_finish(&mut rx, 7).await;
    worker_exits(worker).await;

    assert_eq!(
        server.ranges_seen(),
        vec![kept as u64],
        "the bytes already on disk must be skipped with a Range request"
    );
    assert_eq!(std::fs::read(&path).expect("file"), bytes);
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn an_already_complete_file_finishes_without_retransferring() {
    let bytes = payload(16 * 1024);
    let server = start_server(
        bytes.clone(),
        ServerMode::Stream {
            chunk_size: 0,
            delay: None,
        },
    )
    .await;

    let path = temp_path("already-done.bin");
    std::fs::write(&path, &bytes).expect("seed the complete file");

    let control = Arc::new(DownloadControl::default());
    let spec = DownloadSpec {
        id: 8,
        request: request(
            &server.url("already-done.bin"),
            &path,
            10,
        ),
        resume_from: 0,
    };
    let (mut rx, worker) = spawn_worker(spec, control);

    // The server answers 416: there is nothing left to send, so the download
    // completes straight away instead of erroring forever.
    match next_event(&mut rx, 5).await {
        DownloadEvent::Progress {
            id: 8,
            downloaded,
            total,
            ..
        } => {
            assert_eq!(downloaded, bytes.len() as u64);
            assert_eq!(total, Some(bytes.len() as u64));
        }
        other => panic!("expected the final progress, got {other:?}"),
    }
    match next_event(&mut rx, 5).await {
        DownloadEvent::Finished(8) => {}
        other => panic!("expected Finished, got {other:?}"),
    }
    worker_exits(worker).await;

    assert_eq!(server.ranges_seen(), vec![bytes.len() as u64]);
    assert_eq!(std::fs::read(&path).expect("file"), bytes);
    let _ = std::fs::remove_file(&path);
}

/// Not part of the regular run: this one needs a real network connection and
/// a server that speaks TLS, to prove the whole HTTPS path works end to end.
#[tokio::test]
#[ignore = "requires network access"]
async fn downloads_a_real_https_file() {
    let path = temp_path("real-https.bin");
    let control = Arc::new(DownloadControl::default());
    let spec = DownloadSpec {
        id: 9,
        request: request("https://example.com/", &path, 20),
        resume_from: 0,
    };
    let (mut rx, worker) = spawn_worker(spec, control);

    assert!(matches!(
        next_event(&mut rx, 30).await,
        DownloadEvent::Started(9)
    ));
    wait_for_finish(&mut rx, 9).await;
    worker_exits(worker).await;

    let size = std::fs::metadata(&path)
        .expect("the file exists")
        .len();
    assert!(size > 0, "a real download produces bytes");
    let _ = std::fs::remove_file(&path);
}




