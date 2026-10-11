//! Background model downloads.
//!
//! One worker thread owns a queue of [`DownloadJob`]s. The GUI never blocks:
//! it pushes commands into a channel and drains [`DownloadEvent`]s once per
//! frame.

use std::collections::VecDeque;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::models::{hf_url, ModelDef};
use crate::paths;

/// Per-model download status, as shown in the Models tab.
#[derive(Debug, Clone, PartialEq)]
pub enum State {
    Missing,
    Queued { position: usize },
    Running {
        bytes_done: u64,
        bytes_total: u64,
        current_file: String,
        started: Instant,
    },
    Done,
    Cancelled,
    Failed(String),
}

impl State {
    pub fn is_active(&self) -> bool {
        matches!(self, State::Queued { .. } | State::Running { .. })
    }

    /// 0.0 ..= 1.0, `None` when the total size is not known yet.
    pub fn fraction(&self) -> Option<f32> {
        match self {
            State::Running { bytes_done, bytes_total, .. } if *bytes_total > 0 => {
                Some((*bytes_done as f32 / *bytes_total as f32).clamp(0.0, 1.0))
            }
            _ => None,
        }
    }
}

/// A model that should be downloaded.
#[derive(Debug, Clone)]
pub struct DownloadJob {
    pub model_id: String,
    pub repo: String,
    pub revision: String,
    pub files: Vec<String>,
}

impl DownloadJob {
    pub fn from_model(model: &ModelDef) -> Option<Self> {
        match &model.source {
            crate::models::ModelSource::HuggingFace { repo, revision, files } => Some(Self {
                model_id: model.id.clone(),
                repo: repo.clone(),
                revision: revision.clone(),
                files: files.clone(),
            }),
            // API models have nothing to download.
            crate::models::ModelSource::Api { .. } => None,
        }
    }
}

/// Things the GUI can ask the worker to do.
#[derive(Debug)]
pub enum Command {
    Enqueue(Vec<DownloadJob>),
}

/// Things the worker tells the GUI about.
#[derive(Debug, Clone)]
pub enum Event {
    Started {
        model_id: String,
    },
    Total {
        model_id: String,
        bytes: u64,
    },
    Progress {
        model_id: String,
        bytes_done: u64,
        current_file: String,
    },
    /// One file of a multi-file model landed on disk.
    FileDone {
        #[allow(dead_code, reason = "kept for per-file progress reporting")]
        model_id: String,
        #[allow(dead_code, reason = "useful for future per-file UI")]
        file: String,
    },
    Finished {
        model_id: String,
    },
    Cancelled {
        model_id: String,
    },
    Failed {
        model_id: String,
        error: String,
    },
}

/// Handle held by the GUI.
pub struct Downloader {
    events: Receiver<Event>,
    commands: Sender<Command>,
    cancel: Arc<AtomicBool>,
}

impl Downloader {
    pub fn new() -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel::<Command>();
        let (evt_tx, evt_rx) = mpsc::channel::<Event>();
        let cancel = Arc::new(AtomicBool::new(false));

        let worker_cancel = Arc::clone(&cancel);
        thread::Builder::new()
            .name("projectrem-downloads".into())
            .spawn(move || {
                let client = match reqwest::blocking::Client::builder()
                    .user_agent(concat!("ProjectRem/", env!("PROJECTREM_VERSION")))
                    .connect_timeout(Duration::from_secs(30))
                    .build()
                {
                    Ok(client) => client,
                    Err(err) => {
                        eprintln!("[downloader] could not create HTTP client: {err}");
                        return;
                    }
                };
                worker_loop(cmd_rx, evt_tx, worker_cancel, client);
            })
            .ok();

        Self { events: evt_rx, commands: cmd_tx, cancel }
    }

    /// Queue one model.
    pub fn enqueue(&self, model: &ModelDef) {
        if let Some(job) = DownloadJob::from_model(model) {
            let _ = self.commands.send(Command::Enqueue(vec![job]));
        }
    }

    /// Queue every model that is not downloaded yet.
    pub fn enqueue_missing(&self, catalog: &[ModelDef]) -> usize {
        let jobs: Vec<DownloadJob> =
            catalog.iter().filter(|m| !m.downloaded()).filter_map(DownloadJob::from_model).collect();
        let count = jobs.len();
        if count > 0 {
            let _ = self.commands.send(Command::Enqueue(jobs));
        }
        count
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Drain everything the worker has reported since the last frame.
    pub fn poll(&self) -> Vec<Event> {
        let mut out = Vec::new();
        while let Ok(event) = self.events.try_recv() {
            out.push(event);
        }
        out
    }
}

impl Drop for Downloader {
    fn drop(&mut self) {
        // Ask the worker to stop. It notices the flag between chunks and exits
        // once the channel closes (which happens as soon as `commands` is
        // dropped right after this). We deliberately do not `join()` here: the
        // worker may be blocked on network I/O and blocking the UI thread on
        // shutdown would be worse than letting it finish on its own.
        self.cancel.store(true, Ordering::Relaxed);
    }
}

fn worker_loop(
    cmd_rx: Receiver<Command>,
    evt_tx: Sender<Event>,
    cancel: Arc<AtomicBool>,
    client: reqwest::blocking::Client,
) {
    let mut queue: VecDeque<DownloadJob> = VecDeque::new();

    loop {
        let command = if queue.is_empty() {
            // Nothing to do: sleep until the GUI sends us something.
            match cmd_rx.recv() {
                Ok(command) => Some(command),
                Err(_) => break, // GUI is gone
            }
        } else {
            match cmd_rx.try_recv() {
                Ok(command) => Some(command),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => break,
            }
        };

        match command {
            Some(Command::Enqueue(jobs)) => queue.extend(jobs),
            None => {}
        }

        if cancel.swap(false, Ordering::Relaxed) {
            queue.clear();
            continue;
        }

        if let Some(job) = queue.pop_front() {
            run_job(&job, &client, &evt_tx, &cancel);
        }
    }
}

fn run_job(
    job: &DownloadJob,
    client: &reqwest::blocking::Client,
    evt_tx: &Sender<Event>,
    cancel: &Arc<AtomicBool>,
) {
    let _ = evt_tx.send(Event::Started { model_id: job.model_id.clone() });

    let dest_dir = paths::model_dir(&job.model_id);

    // Which files actually still need downloading?
    let pending: Vec<(String, PathBuf)> = job
        .files
        .iter()
        .map(|file| (file.clone(), dest_dir.join(file)))
        .filter(|(_, dest)| !file_present(dest))
        .collect();

    if pending.is_empty() {
        let _ = evt_tx.send(Event::Finished { model_id: job.model_id.clone() });
        return;
    }

    // Pass 1: ask the server how big everything is, so the progress bar is
    // accurate from the first frame instead of creeping backwards.
    let mut total = 0u64;
    for (file, _) in &pending {
        if cancelled(cancel) {
            let _ = evt_tx.send(Event::Cancelled { model_id: job.model_id.clone() });
            return;
        }
        let url = hf_url(&job.repo, &job.revision, file);
        if let Ok(response) = client.head(&url).send() {
            if response.status().is_success() {
                total += response.content_length().unwrap_or(0);
            }
        }
    }
    let _ = evt_tx.send(Event::Total { model_id: job.model_id.clone(), bytes: total });

    // Pass 2: stream the files to disk.
    let mut done = 0u64;
    for (file, dest) in pending {
        if cancelled(cancel) {
            let _ = evt_tx.send(Event::Cancelled { model_id: job.model_id.clone() });
            return;
        }

        let url = hf_url(&job.repo, &job.revision, &file);
        let response = match client.get(&url).send() {
            Ok(response) if response.status().is_success() => response,
            Ok(response) => {
                let _ = evt_tx.send(Event::Failed {
                    model_id: job.model_id.clone(),
                    error: format!("HTTP {} for {file}", response.status()),
                });
                return;
            }
            Err(err) => {
                let _ = evt_tx.send(Event::Failed { model_id: job.model_id.clone(), error: err.to_string() });
                return;
            }
        };
        let mut response = response;

        if let Some(parent) = dest.parent() {
            if let Err(err) = fs::create_dir_all(parent) {
                let _ = evt_tx.send(Event::Failed {
                    model_id: job.model_id.clone(),
                    error: format!("could not create {}: {err}", parent.display()),
                });
                return;
            }
        }

        // `.part` so an interrupted download is never mistaken for a finished one.
        let tmp = dest.with_extension("part");
        let out = match fs::File::create(&tmp) {
            Ok(file) => file,
            Err(err) => {
                let _ = evt_tx.send(Event::Failed {
                    model_id: job.model_id.clone(),
                    error: format!("could not open {}: {err}", tmp.display()),
                });
                return;
            }
        };

        let mut writer = ProgressWriter {
            inner: out,
            written: 0,
            already_done: done,
            model_id: &job.model_id,
            file: &file,
            events: evt_tx,
            cancel,
        };

        // `copy_to` streams the body into our writer, which reports progress
        // for every chunk that lands on disk.
        match response.copy_to(&mut writer) {
            Ok(_) => done += writer.written,
            Err(_err) => {
                drop(writer);
                let _ = fs::remove_file(&tmp);
                let _ = evt_tx.send(Event::Cancelled { model_id: job.model_id.clone() });
                return;
            }
        }
        drop(writer);

        if let Err(err) = fs::rename(&tmp, &dest) {
            let _ = evt_tx.send(Event::Failed {
                model_id: job.model_id.clone(),
                error: format!("could not finalise {}: {err}", dest.display()),
            });
            return;
        }

        let _ = evt_tx.send(Event::FileDone { model_id: job.model_id.clone(), file: file.clone() });
    }

    let _ = evt_tx.send(Event::Finished { model_id: job.model_id.clone() });
}

/// Wraps the destination file and turns every write into a progress event.
struct ProgressWriter<'a> {
    inner: fs::File,
    /// Bytes written by this file so far.
    written: u64,
    /// Bytes already on disk from the previous files of this model.
    already_done: u64,
    model_id: &'a str,
    file: &'a str,
    events: &'a Sender<Event>,
    cancel: &'a AtomicBool,
}

impl Write for ProgressWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "cancelled"));
        }
        self.inner.write_all(buf)?;
        self.written += buf.len() as u64;
        let _ = self.events.send(Event::Progress {
            model_id: self.model_id.to_owned(),
            bytes_done: self.already_done + self.written,
            current_file: self.file.to_owned(),
        });
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

fn cancelled(cancel: &AtomicBool) -> bool {
    cancel.load(Ordering::Relaxed)
}

fn file_present(path: &PathBuf) -> bool {
    std::fs::metadata(path).map(|m| m.len() > 0).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fraction_is_clamped() {
        let state = State::Running {
            bytes_done: 50,
            bytes_total: 100,
            current_file: "a".into(),
            started: Instant::now(),
        };
        assert_eq!(state.fraction(), Some(0.5));

        let over = State::Running {
            bytes_done: 150,
            bytes_total: 100,
            current_file: "a".into(),
            started: Instant::now(),
        };
        assert_eq!(over.fraction(), Some(1.0));

        let unknown = State::Running {
            bytes_done: 10,
            bytes_total: 0,
            current_file: "a".into(),
            started: Instant::now(),
        };
        assert_eq!(unknown.fraction(), None);
    }
}