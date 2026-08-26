//! PTY output path: coalescing and transport.
//!
//! Every PTY reader (local `RawLocalPty`, daemon-backed `RawRemotePty`) pushes
//! its chunks into an [`OutputBatcher`] instead of emitting them one by one.
//! A dedicated emitter thread drains the batcher — at most one frontend
//! message per [`BATCH_WINDOW`] / [`BATCH_MAX_BYTES`] — and hands each batch
//! to the [`PtyOutputSink`]: a raw Tauri IPC channel (`Channel<Vec<u8>>`,
//! binary, no base64, registered once by the frontend at startup) with the
//! JSON `pty-output` event as the fallback while no channel is registered.
//!
//! Contract (see `docs/performance.md`):
//! - the first chunk of a batch is never delayed by more than `BATCH_WINDOW`;
//! - a batch never exceeds `BATCH_MAX_BYTES` unless a single read did;
//! - `Exit` is delivered strictly after every byte read before it;
//! - a batch always belongs to ONE tab (each PTY owns its own batcher).

use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use parking_lot::Mutex;
use serde::Serialize;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Emitter, Manager};

/// Longest a byte waits in the batcher before it is shipped.
pub const BATCH_WINDOW: Duration = Duration::from_millis(8);
/// Ship early once this many bytes are queued.
pub const BATCH_MAX_BYTES: usize = 64 * 1024;

/// Message from a reader thread to its emitter.
#[derive(Debug)]
pub enum OutMsg {
    Data(Vec<u8>),
    /// The process ended (`None` = read error / detached). Ordered after
    /// every `Data` sent before it.
    Exit(Option<i32>),
}

/// What the emitter gets from [`OutputBatcher::next`].
#[derive(Debug, PartialEq, Eq)]
pub enum Batch {
    Data(Vec<u8>),
    Exit(Option<i32>),
    /// The reader hung up and nothing is left.
    Closed,
}

/// Receives [`OutMsg`]s and turns them into bounded batches.
pub struct OutputBatcher {
    rx: Receiver<OutMsg>,
    window: Duration,
    max_bytes: usize,
    /// An `Exit` that arrived while a batch was being filled — handed out on
    /// the next call so it stays ordered after the data.
    pending_exit: Option<Option<i32>>,
}

impl OutputBatcher {
    pub fn new(rx: Receiver<OutMsg>, window: Duration, max_bytes: usize) -> Self {
        Self {
            rx,
            window,
            max_bytes,
            pending_exit: None,
        }
    }

    /// Block until something is available, then keep pulling until the
    /// window since the FIRST chunk elapses, the byte cap is hit, an `Exit`
    /// shows up, or the sender is gone.
    pub fn next(&mut self) -> Batch {
        if let Some(code) = self.pending_exit.take() {
            return Batch::Exit(code);
        }
        let mut buf = match self.rx.recv() {
            Ok(OutMsg::Data(d)) => d,
            Ok(OutMsg::Exit(code)) => return Batch::Exit(code),
            Err(_) => return Batch::Closed,
        };
        let deadline = Instant::now() + self.window;
        while buf.len() < self.max_bytes {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            match self.rx.recv_timeout(deadline - now) {
                Ok(OutMsg::Data(d)) => buf.extend_from_slice(&d),
                Ok(OutMsg::Exit(code)) => {
                    self.pending_exit = Some(code);
                    break;
                }
                Err(RecvTimeoutError::Timeout) | Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        Batch::Data(buf)
    }
}

/// Create the reader → emitter pair with the production limits.
pub fn output_channel() -> (Sender<OutMsg>, OutputBatcher) {
    let (tx, rx) = channel();
    (tx, OutputBatcher::new(rx, BATCH_WINDOW, BATCH_MAX_BYTES))
}

/// Wire format of one raw-channel message: `len(tab_id) as u8`, the tab id
/// bytes, then the PTY bytes. Mirrored by `frontend/src/pty-frame.ts`.
pub fn encode_frame(tab_id: &str, data: &[u8]) -> Vec<u8> {
    let id = tab_id.as_bytes();
    debug_assert!(id.len() <= u8::MAX as usize, "tab ids are UUIDs");
    let mut frame = Vec::with_capacity(1 + id.len() + data.len());
    frame.push(id.len() as u8);
    frame.extend_from_slice(id);
    frame.extend_from_slice(data);
    frame
}

/// Event fallback payload (`pty-output`), base64 for JSON transport.
#[derive(Serialize, Clone)]
struct PtyOutputPayload {
    tab_id: String,
    data: String,
}

/// Tauri-managed state: the frontend's raw output channel, once registered.
#[derive(Default)]
pub struct PtyOutputSink {
    channel: Mutex<Option<Channel<InvokeResponseBody>>>,
}

impl PtyOutputSink {
    pub fn set_channel(&self, channel: Channel<InvokeResponseBody>) {
        *self.channel.lock() = Some(channel);
    }

    /// Ship one batch for `tab_id`: raw channel when registered, JSON event
    /// otherwise (or when the channel send fails — e.g. the webview is gone).
    pub fn send(&self, app: &AppHandle, tab_id: &str, data: &[u8]) {
        let sent = {
            let guard = self.channel.lock();
            match guard.as_ref() {
                Some(ch) => ch
                    .send(InvokeResponseBody::Raw(encode_frame(tab_id, data)))
                    .is_ok(),
                None => false,
            }
        };
        if !sent {
            let _ = app.emit(
                "pty-output",
                PtyOutputPayload {
                    tab_id: tab_id.to_string(),
                    data: BASE64.encode(data),
                },
            );
        }
    }
}

/// Send a batch through the app's sink (event fallback when the sink state
/// is not managed — only in tests).
pub fn emit_output(app: &AppHandle, tab_id: &str, data: &[u8]) {
    match app.try_state::<PtyOutputSink>() {
        Some(sink) => sink.send(app, tab_id, data),
        None => {
            let _ = app.emit(
                "pty-output",
                PtyOutputPayload {
                    tab_id: tab_id.to_string(),
                    data: BASE64.encode(data),
                },
            );
        }
    }
}

/// Spawn the emitter thread for one PTY: drains `batcher`, ships each batch
/// via [`emit_output`] and finally the `pty-exit` event (after all output).
pub fn spawn_emitter(
    app: AppHandle,
    tab_id: String,
    mut batcher: OutputBatcher,
    on_exit: impl Fn(&AppHandle, &str, Option<i32>) + Send + 'static,
) -> std::io::Result<std::thread::JoinHandle<()>> {
    std::thread::Builder::new()
        .name("pty-output-emitter".into())
        .spawn(move || {
            loop {
                match batcher.next() {
                    Batch::Data(b) => emit_output(&app, &tab_id, &b),
                    Batch::Exit(code) => on_exit(&app, &tab_id, code),
                    Batch::Closed => break,
                }
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    fn batcher(rx: Receiver<OutMsg>) -> OutputBatcher {
        OutputBatcher::new(rx, BATCH_WINDOW, BATCH_MAX_BYTES)
    }

    /// Fake reader: pushes `n` chunks of `size` bytes as fast as it can.
    fn fake_reader(tx: Sender<OutMsg>, n: usize, size: usize) -> thread::JoinHandle<()> {
        thread::spawn(move || {
            for i in 0..n {
                tx.send(OutMsg::Data(vec![(i % 251) as u8; size])).unwrap();
            }
            tx.send(OutMsg::Exit(Some(0))).unwrap();
        })
    }

    fn drain(mut b: OutputBatcher) -> (Vec<Vec<u8>>, Vec<Option<i32>>) {
        let mut data = Vec::new();
        let mut exits = Vec::new();
        loop {
            match b.next() {
                Batch::Data(d) => data.push(d),
                Batch::Exit(c) => exits.push(c),
                Batch::Closed => return (data, exits),
            }
        }
    }

    #[test]
    fn many_small_reads_coalesce_into_few_batches() {
        // 2000 × 32 B = 64 KB in one go: bounded by the window, not by
        // the read count.
        let (tx, rx) = channel();
        let reader = fake_reader(tx, 2000, 32);
        let (batches, exits) = drain(batcher(rx));
        reader.join().unwrap();
        let total: usize = batches.iter().map(Vec::len).sum();
        assert_eq!(total, 2000 * 32);
        assert!(
            batches.len() <= 40,
            "2000 reads became {} batches (expected ≤ 40)",
            batches.len()
        );
        assert_eq!(exits, vec![Some(0)]);
    }

    #[test]
    fn batches_never_exceed_the_byte_cap_unless_one_read_did() {
        let (tx, rx) = channel();
        let reader = fake_reader(tx, 100, 16 * 1024); // 1.6 MB
        let (batches, _) = drain(batcher(rx));
        reader.join().unwrap();
        let total: usize = batches.iter().map(Vec::len).sum();
        assert_eq!(total, 100 * 16 * 1024);
        for b in &batches {
            assert!(
                b.len() <= BATCH_MAX_BYTES + 16 * 1024,
                "batch of {} bytes",
                b.len()
            );
        }
        // 1.6 MB / 64 KB = at least 25 batches, whatever the timing.
        assert!(batches.len() >= 25, "{} batches", batches.len());
    }

    #[test]
    fn oversized_single_read_is_one_batch() {
        let (tx, rx) = channel();
        tx.send(OutMsg::Data(vec![7u8; 200 * 1024])).unwrap();
        drop(tx);
        let (batches, exits) = drain(batcher(rx));
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].len(), 200 * 1024);
        assert!(exits.is_empty());
    }

    #[test]
    fn bytes_are_delivered_in_order_and_exit_last() {
        let (tx, rx) = channel();
        let reader = thread::spawn(move || {
            for i in 0..5000u32 {
                tx.send(OutMsg::Data(i.to_le_bytes().to_vec())).unwrap();
            }
            tx.send(OutMsg::Exit(None)).unwrap();
            // Nothing after Exit must be observable by ordering anyway.
        });
        let mut b = batcher(rx);
        let mut all = Vec::new();
        let mut exit_seen = false;
        loop {
            match b.next() {
                Batch::Data(d) => {
                    assert!(!exit_seen, "data after exit");
                    all.extend_from_slice(&d);
                }
                Batch::Exit(c) => {
                    assert_eq!(c, None);
                    exit_seen = true;
                }
                Batch::Closed => break,
            }
        }
        reader.join().unwrap();
        assert!(exit_seen);
        assert_eq!(all.len(), 5000 * 4);
        for (i, w) in all.chunks(4).enumerate() {
            assert_eq!(u32::from_le_bytes([w[0], w[1], w[2], w[3]]), i as u32);
        }
    }

    #[test]
    fn first_chunk_latency_is_bounded_by_the_window() {
        let (tx, rx) = channel();
        let mut b = batcher(rx);
        tx.send(OutMsg::Data(b"hi".to_vec())).unwrap();
        let t = Instant::now();
        assert_eq!(b.next(), Batch::Data(b"hi".to_vec()));
        // Generous bound: the window plus scheduler slack.
        assert!(t.elapsed() < BATCH_WINDOW * 10, "took {:?}", t.elapsed());
    }

    #[test]
    fn frame_layout_matches_the_frontend_decoder() {
        let f = encode_frame("abc", b"xyz");
        assert_eq!(f, vec![3, b'a', b'b', b'c', b'x', b'y', b'z']);
    }

    /// Micro-benchmark for `docs/performance.md`: 50 MB streamed as 16 KB
    /// reads (what the PTY reader hands us). Run with
    /// `cargo test --release -p piki-desktop bench_ -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn bench_coalescer_50mb() {
        const READ: usize = 16 * 1024;
        const TOTAL: usize = 50 * 1024 * 1024;
        let reads = TOTAL / READ;
        let (tx, rx) = channel();
        let t = Instant::now();
        let reader = fake_reader(tx, reads, READ);
        let (batches, _) = drain(batcher(rx));
        reader.join().unwrap();
        let el = t.elapsed();
        let frames_per_read = batches.len() as f64 / reads as f64;
        println!(
            "coalescer: {reads} reads of {READ} B → {} batches ({:.2} frames/read, mean {} B) in {el:?}; \
             per-read path would have been {reads} IPC messages",
            batches.len(),
            frames_per_read,
            TOTAL / batches.len().max(1)
        );
        assert!(batches.len() < reads);
    }
}
