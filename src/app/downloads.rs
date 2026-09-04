use super::*;

impl App {
    pub(super) fn handle_downloads_key(&mut self, key: KeyEvent) -> Result<()> {
        let len = self.downloads.len();
        match (self.downloads_state.selected(), len) {
            (_, 0) => self.downloads_state.select(None),
            (None, _) => self.downloads_state.select(Some(0)),
            (Some(i), n) if i >= n => self.downloads_state.select(Some(n - 1)),
            _ => {}
        }
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if len > 0 {
                    let cur = self.downloads_state.selected().unwrap_or(0);
                    self.downloads_state.select(Some((cur + 1).min(len - 1)));
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if len > 0 {
                    let cur = self.downloads_state.selected().unwrap_or(0);
                    self.downloads_state.select(Some(cur.saturating_sub(1)));
                }
            }
            KeyCode::Char('g') | KeyCode::Home => {
                if len > 0 {
                    self.downloads_state.select(Some(0));
                }
            }
            KeyCode::Char('G') | KeyCode::End => {
                if len > 0 {
                    self.downloads_state.select(Some(len - 1));
                }
            }
            KeyCode::Enter | KeyCode::Char('r') => {
                if let Some(idx) = self.downloads_state.selected() {
                    self.retry_download_at(idx);
                }
            }
            KeyCode::Backspace | KeyCode::Delete => {
                if let Some(idx) = self.downloads_state.selected() {
                    self.remove_download_at(idx);
                    let new_len = self.downloads.len();
                    if new_len == 0 {
                        self.downloads_state.select(None);
                    } else {
                        self.downloads_state.select(Some(idx.min(new_len - 1)));
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
    pub(super) fn start_download(&mut self, track_id: i64) {
        let Some(lib) = self.library_id else { return };
        if matches!(
            self.downloads.get(&track_id).map(|s| s.status),
            Some(DownloadStatus::Queued)
                | Some(DownloadStatus::Downloading)
                | Some(DownloadStatus::Downloaded)
        ) {
            return;
        }
        let Some(track) = self.tracks.iter().find(|t| t.id == track_id) else {
            return;
        };
        let Some(hash) = track.hash.clone() else {
            self.status_msg = "cannot download: track has no hash".to_string();
            return;
        };
        let url = self.client.stream_url(lib, track_id);
        let auth_header = self.client.auth_header_value();
        let dest_path = crate::storage::tracks_dir().join(&hash);
        let tmp_path = crate::storage::tmp_dir().join(format!("{track_id}-{hash}.part"));

        let row = DownloadRow {
            library_id: lib,
            track_id,
            hash: hash.clone(),
            status: DownloadStatus::Queued,
            bytes_downloaded: 0,
            total_bytes: 0,
            local_path: None,
            error: None,
        };
        if let Err(e) = self.db.upsert_download(&row) {
            self.status_msg = format!("failed to save download state: {e}");
            return;
        }
        self.downloads.insert(track_id, DownloadState::from(row));

        self.download_mgr.enqueue(DownloadJob {
            track_id,
            url,
            auth_header,
            expected_hash: hash,
            dest_path,
            tmp_path,
        });
    }
    pub(super) fn downloads_sorted_ids(&self) -> Vec<i64> {
        let mut ids: Vec<i64> = self.downloads.keys().copied().collect();
        ids.sort_unstable();
        ids
    }
    pub(super) fn retry_download_at(&mut self, index: usize) {
        let ids = self.downloads_sorted_ids();
        let Some(&id) = ids.get(index) else { return };
        let Some(st) = self.downloads.get(&id) else {
            return;
        };
        if st.status != DownloadStatus::Failed {
            return;
        }
        self.start_download(id);
    }
    pub(super) fn remove_download_at(&mut self, index: usize) {
        let ids = self.downloads_sorted_ids();
        let Some(&id) = ids.get(index) else { return };
        let Some(lib) = self.library_id else { return };
        let Some(st) = self.downloads.get(&id) else {
            return;
        };
        if matches!(
            st.status,
            DownloadStatus::Queued | DownloadStatus::Downloading
        ) {
            self.status_msg = "cannot remove: download in progress".into();
            return;
        }
        if let Some(path) = &st.local_path {
            let _ = std::fs::remove_file(path);
        }
        if let Err(e) = self.db.delete_download(lib, id) {
            self.status_msg = format!("failed to remove download: {e}");
            return;
        }
        self.downloads.remove(&id);
        self.status_msg = "download removed".into();
    }
    pub(super) fn drain_download_events(&mut self) {
        while let Some(event) = self.download_mgr.try_recv() {
            let Some(lib) = self.library_id else { continue };
            match event {
                DownloadEvent::Progress {
                    track_id,
                    bytes,
                    total,
                } => {
                    if let Some(st) = self.downloads.get_mut(&track_id) {
                        st.status = DownloadStatus::Downloading;
                        st.bytes = bytes as i64;
                        st.total = total as i64;
                    }
                }
                DownloadEvent::Done {
                    track_id,
                    local_path,
                } => {
                    let hash = self
                        .tracks
                        .iter()
                        .find(|t| t.id == track_id)
                        .and_then(|t| t.hash.clone())
                        .unwrap_or_default();
                    let row = DownloadRow {
                        library_id: lib,
                        track_id,
                        hash,
                        status: DownloadStatus::Downloaded,
                        bytes_downloaded: 0,
                        total_bytes: 0,
                        local_path: Some(local_path.clone()),
                        error: None,
                    };
                    if let Err(e) = self.db.upsert_download(&row) {
                        self.status_msg = format!("failed to save download state: {e}");
                    }
                    self.downloads.insert(track_id, DownloadState::from(row));
                }
                DownloadEvent::Failed { track_id, error } => {
                    let hash = self
                        .tracks
                        .iter()
                        .find(|t| t.id == track_id)
                        .and_then(|t| t.hash.clone())
                        .unwrap_or_default();
                    let row = DownloadRow {
                        library_id: lib,
                        track_id,
                        hash,
                        status: DownloadStatus::Failed,
                        bytes_downloaded: 0,
                        total_bytes: 0,
                        local_path: None,
                        error: Some(error.clone()),
                    };
                    if let Err(e) = self.db.upsert_download(&row) {
                        self.status_msg = format!("failed to save download state: {e}");
                    }
                    self.downloads.insert(track_id, DownloadState::from(row));
                    self.status_msg = format!("download failed: {error}");
                }
            }
        }
    }
    pub(super) fn resolve_track_url(&self, library_id: i64, track_id: i64) -> String {
        if let Some(st) = self.downloads.get(&track_id) {
            if st.status == DownloadStatus::Downloaded {
                if let Some(path) = &st.local_path {
                    if std::path::Path::new(path).exists() {
                        return format!("file://{path}");
                    }
                }
            }
        }
        self.client.stream_url(library_id, track_id)
    }
    pub(super) fn open_downloader_screen(&mut self) {
        let Some(lib) = self.library_id() else {
            self.status_msg = "no library selected".into();
            return;
        };
        match self.client.list_downloaders(lib) {
            Ok(scripts) => {
                self.downloader_scripts = scripts;
                self.mode = Mode::DownloaderList { index: 0 };
            }
            Err(e) => {
                self.status_msg = format!("downloaders: {e}");
            }
        }
    }
    pub(super) fn commit_run_downloader(&mut self, script: &str, raw: &str) {
        let urls: Vec<String> = raw
            .split(|c: char| c == ',' || c == '\n' || c.is_whitespace())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if urls.is_empty() {
            self.status_msg = "enter at least one URL".into();
            return;
        }
        let Some(lib) = self.library_id() else {
            self.status_msg = "no library selected".into();
            self.mode = Mode::Normal;
            return;
        };
        match self.client.run_downloader(lib, script, &urls) {
            Ok(job_id) => {
                self.downloader_job_id = Some(job_id);
                self.downloader_job = None;
                self.downloader_script_name = Some(script.to_string());
                self.downloader_polling = true;
                self.last_downloader_poll = Instant::now();
                self.status_msg = format!("downloader started: {script}");
                self.mode = Mode::DownloaderJob;
            }
            Err(e) => {
                self.status_msg = format!("downloader run failed: {e}");
                self.mode = Mode::DownloaderInput {
                    script: script.to_string(),
                    buf: raw.to_string(),
                };
            }
        }
    }
    pub(super) fn poll_downloader_if_due(&mut self) {
        if !self.downloader_polling {
            return;
        }
        if self.last_downloader_poll.elapsed() < Duration::from_millis(1000) {
            return;
        }
        self.last_downloader_poll = Instant::now();
        let Some(lib) = self.library_id() else {
            self.downloader_polling = false;
            return;
        };
        let Some(job_id) = self.downloader_job_id.clone() else {
            self.downloader_polling = false;
            return;
        };
        let job = match self.client.downloader_job_status(lib, &job_id) {
            Ok(j) => j,
            Err(_) => return,
        };
        if job.is_done() {
            self.downloader_polling = false;
        }
        self.downloader_job = Some(job);
    }
    pub(super) fn download_selected_playlist(&mut self) {
        if self.playlist_tracks.is_empty() {
            self.status_msg = "playlist is empty".into();
            return;
        }
        let track_ids: Vec<i64> = self.playlist_tracks.iter().map(|pt| pt.track_id).collect();
        let name = self.selected_playlist_name().unwrap_or_default();
        let count = track_ids.len();
        for track_id in track_ids {
            self.start_download(track_id);
        }
        self.status_msg = format!("queued {count} track(s) from {name} for download");
    }
}
