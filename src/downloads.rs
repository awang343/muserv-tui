use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::fs::File;
use std::io::{BufWriter, Read, Write as _};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

const NUM_WORKERS: usize = 2;

pub struct DownloadJob {
    pub track_id: i64,
    pub url: String,
    pub auth_header: Option<String>,
    pub expected_hash: String,
    pub dest_path: PathBuf,
    pub tmp_path: PathBuf,
}

pub enum DownloadEvent {
    Progress {
        track_id: i64,
        bytes: u64,
        total: u64,
    },
    Done {
        track_id: i64,
        local_path: String,
    },
    Failed {
        track_id: i64,
        error: String,
    },
}

pub struct DownloadManager {
    job_tx: Sender<DownloadJob>,
    event_rx: Receiver<DownloadEvent>,
}

impl DownloadManager {
    pub fn new(agent: ureq::Agent) -> Self {
        let (job_tx, job_rx) = mpsc::channel::<DownloadJob>();
        let (event_tx, event_rx) = mpsc::channel::<DownloadEvent>();
        let job_rx = Arc::new(Mutex::new(job_rx));

        for _ in 0..NUM_WORKERS {
            let job_rx = job_rx.clone();
            let event_tx = event_tx.clone();
            let agent = agent.clone();
            thread::spawn(move || loop {
                let job = {
                    let rx = match job_rx.lock() {
                        Ok(rx) => rx,
                        Err(_) => break,
                    };
                    match rx.recv() {
                        Ok(job) => job,
                        Err(_) => break,
                    }
                };
                run_job(&agent, job, &event_tx);
            });
        }

        Self { job_tx, event_rx }
    }

    pub fn enqueue(&self, job: DownloadJob) {
        let _ = self.job_tx.send(job);
    }

    pub fn try_recv(&self) -> Option<DownloadEvent> {
        self.event_rx.try_recv().ok()
    }
}

fn run_job(agent: &ureq::Agent, job: DownloadJob, event_tx: &Sender<DownloadEvent>) {
    if let Err(e) = run_job_inner(agent, &job, event_tx) {
        let _ = std::fs::remove_file(&job.tmp_path);
        let _ = event_tx.send(DownloadEvent::Failed {
            track_id: job.track_id,
            error: e.to_string(),
        });
    }
}

fn run_job_inner(
    agent: &ureq::Agent,
    job: &DownloadJob,
    event_tx: &Sender<DownloadEvent>,
) -> Result<()> {
    if let Some(parent) = job.tmp_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut req = agent.get(&job.url);
    if let Some(auth) = &job.auth_header {
        req = req.header("Authorization", auth);
    }
    let mut resp = req.call().context("requesting track stream")?;
    let total = resp.body().content_length().unwrap_or(0);

    let file = File::create(&job.tmp_path)
        .with_context(|| format!("creating {}", job.tmp_path.display()))?;
    let mut writer = BufWriter::new(file);
    let mut hasher = Sha256::new();
    let mut reader = resp.body_mut().as_reader();

    let mut buf = [0u8; 65536];
    let mut downloaded: u64 = 0;
    let mut last_emit = Instant::now();
    loop {
        let n = reader.read(&mut buf).context("reading response body")?;
        if n == 0 {
            break;
        }
        writer
            .write_all(&buf[..n])
            .context("writing to temp file")?;
        hasher.update(&buf[..n]);
        downloaded += n as u64;
        if last_emit.elapsed().as_millis() >= 200 {
            let _ = event_tx.send(DownloadEvent::Progress {
                track_id: job.track_id,
                bytes: downloaded,
                total,
            });
            last_emit = Instant::now();
        }
    }
    writer.flush().context("flushing temp file")?;
    drop(writer);

    let digest = hasher.finalize();
    let actual_hash = hex_encode(&digest);
    if !job.expected_hash.is_empty() && actual_hash != job.expected_hash {
        let _ = std::fs::remove_file(&job.tmp_path);
        anyhow::bail!(
            "hash mismatch: expected {}, got {}",
            job.expected_hash,
            actual_hash
        );
    }

    if let Some(parent) = job.dest_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(&job.tmp_path, &job.dest_path).context("finalizing downloaded file")?;

    let _ = event_tx.send(DownloadEvent::Done {
        track_id: job.track_id,
        local_path: job.dest_path.display().to_string(),
    });
    Ok(())
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{:02x}", b);
    }
    s
}
