use crate::protocol::{WorkerEvent, WorkerRequest};
use eframe::egui;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub struct WorkerHandle {
    child: Child,
    stdin: BufWriter<ChildStdin>,
    rx: Receiver<WorkerEvent>,
    python_path: PathBuf,
    script_path: PathBuf,
}

impl WorkerHandle {
    pub fn spawn(ctx: egui::Context) -> Result<Self, String> {
        let (python_path, script_path) = locate_backend_paths()?;
        Self::spawn_with_paths(python_path, script_path, ctx)
    }

    fn spawn_with_paths(
        python_path: PathBuf,
        script_path: PathBuf,
        ctx: egui::Context,
    ) -> Result<Self, String> {
        let mut cmd = Command::new(&python_path);
        cmd.arg("-u")
            .arg(&script_path)
            .env("PYTHONUTF8", "1")
            .env("PYTHONIOENCODING", "utf-8")
            .env("HF_HUB_DISABLE_PROGRESS_BARS", "0")
            .env("HF_HUB_DISABLE_SYMLINKS_WARNING", "1")
            .env("HF_HUB_VERBOSITY", "error")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        #[cfg(target_os = "windows")]
        cmd.creation_flags(CREATE_NO_WINDOW);

        let mut child = cmd.spawn().map_err(|e| {
            format!(
                "Could not start Python voice engine ({}): {}",
                python_path.display(),
                e
            )
        })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Failed to open worker stdin".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Failed to open worker stdout".to_string())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "Failed to open worker stderr".to_string())?;

        let (tx, rx): (Sender<WorkerEvent>, Receiver<WorkerEvent>) = mpsc::channel();

        // Reader thread for JSON lines on stdout
        let tx_out = tx.clone();
        let ctx_out = ctx.clone();
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(raw) => {
                        let trimmed = raw.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        match serde_json::from_str::<WorkerEvent>(trimmed) {
                            Ok(evt) => {
                                let _ = tx_out.send(evt);
                            }
                            Err(err) => {
                                let _ = tx_out.send(WorkerEvent::Log(format!(
                                    "[stdout] {trimmed} (parse note: {err})"
                                )));
                            }
                        }
                        ctx_out.request_repaint();
                    }
                    Err(e) => {
                        let _ = tx_out.send(WorkerEvent::Exited(format!(
                            "Voice engine stream closed: {e}"
                        )));
                        ctx_out.request_repaint();
                        break;
                    }
                }
            }
            let _ = tx_out.send(WorkerEvent::Exited(
                "Voice engine process disconnected.".to_string(),
            ));
            ctx_out.request_repaint();
        });

        // Reader thread for stderr logs / download progress
        let tx_err = tx;
        let ctx_err = ctx;
        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                match line {
                    Ok(raw) => {
                        let trimmed = raw.trim();
                        if !trimmed.is_empty() && !is_benign_hf_warning(trimmed) {
                            let _ = tx_err.send(WorkerEvent::Log(trimmed.to_string()));
                            ctx_err.request_repaint();
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        let mut handle = Self {
            child,
            stdin: BufWriter::new(stdin),
            rx,
            python_path,
            script_path,
        };
        handle.send(&WorkerRequest::Ping)?;
        Ok(handle)
    }

    pub fn send(&mut self, req: &WorkerRequest) -> Result<(), String> {
        let json = serde_json::to_string(req).map_err(|e| e.to_string())?;
        self.stdin
            .write_all(json.as_bytes())
            .and_then(|_| self.stdin.write_all(b"\n"))
            .and_then(|_| self.stdin.flush())
            .map_err(|e| format!("Failed to send request to voice engine: {e}"))
    }

    pub fn try_recv_all(&self) -> Vec<WorkerEvent> {
        let mut events = Vec::new();
        while let Ok(evt) = self.rx.try_recv() {
            events.push(evt);
        }
        events
    }

    pub fn restart(&mut self, ctx: egui::Context) -> Result<(), String> {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let fresh = Self::spawn_with_paths(
            self.python_path.clone(),
            self.script_path.clone(),
            ctx,
        )?;
        *self = fresh;
        Ok(())
    }

    pub fn python_path(&self) -> &Path {
        &self.python_path
    }
}

impl Drop for WorkerHandle {
    fn drop(&mut self) {
        let _ = self.send(&WorkerRequest::Shutdown);
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Walk up from current exe and current working directory to locate `.venv` and `worker.py`.
pub fn locate_workspace_root() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        let mut cur = exe.parent().map(Path::to_path_buf);
        for _ in 0..5 {
            if let Some(dir) = cur {
                candidates.push(dir.clone());
                cur = dir.parent().map(Path::to_path_buf);
            }
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        let mut cur = Some(cwd);
        for _ in 0..5 {
            if let Some(dir) = cur {
                candidates.push(dir.clone());
                cur = dir.parent().map(Path::to_path_buf);
            }
        }
    }

    for dir in candidates {
        let has_worker = dir.join("voxa-studio").join("backend").join("worker.py").is_file()
            || dir.join("kitten-tts-gui").join("backend").join("worker.py").is_file()
            || dir.join("backend").join("worker.py").is_file();
        let has_venv = dir.join(".venv").join("Scripts").join("python.exe").is_file()
            || dir.join(".venv").join("bin").join("python").is_file();
        if has_worker && has_venv {
            return Some(dir);
        }
    }
    None
}

fn locate_backend_paths() -> Result<(PathBuf, PathBuf), String> {
    if let Some(root) = locate_workspace_root() {
        let py = if cfg!(target_os = "windows") {
            root.join(".venv").join("Scripts").join("python.exe")
        } else {
            root.join(".venv").join("bin").join("python")
        };
        let worker_voxa = root.join("voxa-studio").join("backend").join("worker.py");
        let worker_legacy = root.join("kitten-tts-gui").join("backend").join("worker.py");
        let worker_alt = root.join("backend").join("worker.py");
        let script = if worker_voxa.is_file() {
            worker_voxa
        } else if worker_legacy.is_file() {
            worker_legacy
        } else {
            worker_alt
        };
        if py.is_file() && script.is_file() {
            return Ok((py, script));
        }
    }

    Err("Could not find `.venv/Scripts/python.exe` and `voxa-studio/backend/worker.py`. Make sure Voxa-Studio.exe is placed inside the Voxa Studio workspace folder.".to_string())
}

pub fn is_benign_hf_warning(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    if lower.contains("unauthenticated requests to the hf hub")
        || lower.contains("please set a hf_token to enable higher rate limits")
        || lower.contains("cache-system uses symlinks by default")
        || lower.contains("hf_hub_disable_symlinks_warning")
        || lower.contains("to support symlinks on windows")
        || lower.contains("enable-your-device-for-development")
        || lower.contains("how-to-cache#limitations")
    {
        return true;
    }
    line.contains("%|") && (line.contains("B/s]") || line.contains("it/s]"))
}
