//! Bridge to the Python sidecar.
//!
//! The sidecar is a long-lived `python sidecar/main.py` process that speaks
//! newline-delimited JSON on stdin/stdout. The GUI owns it and calls into it
//! for the things egui cannot do by itself (enumerating audio devices, for
//! example).
//!
//! If Python is missing the GUI still works; it just reports the sidecar as
//! unavailable.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Stopped,
    Starting,
    Running,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub channels: u32,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct AudioDevices {
    #[serde(default)]
    pub inputs: Vec<DeviceInfo>,
    #[serde(default)]
    pub outputs: Vec<DeviceInfo>,
}

pub struct Sidecar {
    status: Arc<Mutex<Status>>,
    lines: Option<Receiver<Value>>,
    stdin: Option<Arc<Mutex<ChildStdin>>>,
    child: Option<Child>,
    next_id: AtomicU64,
}

impl Sidecar {
    pub fn new() -> Self {
        Self {
            status: Arc::new(Mutex::new(Status::Stopped)),
            lines: None,
            stdin: None,
            child: None,
            next_id: AtomicU64::new(1),
        }
    }

    pub fn status(&self) -> Status {
        *self.status.lock().unwrap()
    }

    /// Start the process if it is not running yet.
    pub fn ensure_started(&mut self) -> Status {
        match self.status() {
            Status::Running => return Status::Running,
            Status::Starting => return Status::Starting,
            _ => {}
        }

        *self.status.lock().unwrap() = Status::Starting;

        let script = crate::paths::sidecar_dir().join("main.py");
        if !script.is_file() {
            eprintln!("[sidecar] not found at {}", script.display());
            *self.status.lock().unwrap() = Status::Failed;
            return Status::Failed;
        }

        let python = python_command();
        let mut command = Command::new(&python.program);
        command
            .args(python.prefix_args)
            .arg(&script)
            .arg("--version")
            .arg(crate::version::VERSION)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());

        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(err) => {
                eprintln!("[sidecar] could not run {}: {err}", python.program);
                *self.status.lock().unwrap() = Status::Failed;
                return Status::Failed;
            }
        };

        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");

        let (tx, rx) = mpsc::channel::<Value>();
        let status = Arc::clone(&self.status);
        thread::Builder::new()
            .name("projectrem-sidecar-reader".into())
            .spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    match serde_json::from_str::<Value>(trimmed) {
                        Ok(value) => {
                            if value.get("event").is_some() {
                                // Asynchronous notification, nothing to reply to.
                                continue;
                            }
                            if tx.send(value).is_err() {
                                break;
                            }
                        }
                        Err(err) => eprintln!("[sidecar] bad JSON line ({err}): {trimmed}"),
                    }
                }
                *status.lock().unwrap() = Status::Stopped;
            })
            .ok();

        self.stdin = Some(Arc::new(Mutex::new(stdin)));
        self.lines = Some(rx);
        self.child = Some(child);

        *self.status.lock().unwrap() = Status::Running;
        Status::Running
    }

    pub fn stop(&mut self) {
        if let Some(stdin) = &self.stdin {
            let _ = send_line(stdin, &json!({ "cmd": "shutdown" }));
        }
        self.stdin = None;
        self.lines = None;
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        *self.status.lock().unwrap() = Status::Stopped;
    }

    /// Fire a request without waiting for the answer.
    fn request(&self, payload: Value) -> bool {
        let Some(stdin) = &self.stdin else { return false };
        send_line(stdin, &payload).is_ok()
    }

    /// Send a request and return the matching reply, waiting up to `timeout`.
    ///
    /// Requests are tagged with an id so late replies can never be mistaken
    /// for the answer to a newer request.
    pub fn call(&self, cmd: &str, extra: Value, timeout: std::time::Duration) -> Result<Value, String> {
        if self.status() != Status::Running {
            return Err("sidecar is not running".into());
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let mut payload = extra;
        if !payload.is_object() {
            payload = json!({});
        }
        payload["cmd"] = json!(cmd);
        payload["id"] = json!(id);

        if !self.request(payload) {
            return Err("could not write to the sidecar".into());
        }

        let Some(rx) = &self.lines else { return Err("no reply channel".into()) };
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err(format!("timed out waiting for \"{cmd}\""));
            }
            match rx.recv_timeout(remaining) {
                Ok(value) => {
                    if value.get("id").and_then(Value::as_u64) == Some(id) {
                        return Ok(value);
                    }
                    // Reply for a different request, keep waiting.
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    return Err(format!("timed out waiting for \"{cmd}\""));
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("sidecar closed the connection".into());
                }
            }
        }
    }

    #[allow(dead_code, reason = "used by diagnostics")]
    pub fn ping(&self) -> Result<Value, String> {
        self.call("ping", json!({}), std::time::Duration::from_secs(5))
    }

    /// Ask the sidecar which audio devices exist.
    ///
    /// Falls back to a device-less list when the sidecar cannot help, so the
    /// Settings tab always has something sensible to show.
    pub fn audio_devices(&self) -> AudioDevices {
        let result = self.call("audio_devices", json!({}), std::time::Duration::from_secs(8));
        match result {
            Ok(value) => serde_json::from_value(value).unwrap_or_default(),
            Err(_) => AudioDevices::default(),
        }
    }
}

impl Drop for Sidecar {
    fn drop(&mut self) {
        self.stop();
    }
}

struct PythonCommand {
    program: String,
    prefix_args: Vec<String>,
}

/// `PROJECTREM_PYTHON` wins, then `python`, then the `py` launcher.
fn python_command() -> PythonCommand {
    if let Ok(custom) = std::env::var("PROJECTREM_PYTHON") {
        if !custom.trim().is_empty() {
            return PythonCommand { program: custom, prefix_args: Vec::new() };
        }
    }
    if which("python") {
        return PythonCommand { program: "python".into(), prefix_args: Vec::new() };
    }
    PythonCommand { program: "py".into(), prefix_args: vec!["-3".into()] }
}

fn which(program: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else { return false };
    std::env::split_paths(&path).any(|dir| {
        let candidate = dir.join(program);
        candidate.is_file() || candidate.with_extension("exe").is_file()
    })
}

fn send_line(stdin: &Arc<Mutex<ChildStdin>>, payload: &Value) -> std::io::Result<()> {
    let mut line = serde_json::to_string(payload)?;
    line.push('\n');
    let mut guard = match stdin.lock() {
        Ok(guard) => guard,
        Err(_) => return Err(std::io::Error::other("the sidecar stdin lock was poisoned")),
    };
    guard.write_all(line.as_bytes())?;
    guard.flush()
}