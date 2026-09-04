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
            KeyCode::Esc => {
                if self.downloads_select.in_range() {
                    self.downloads_select.end_range();
                } else if self.downloads_select.count() > 0 {
                    self.downloads_select.clear();
                }
            }
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
            KeyCode::Char('x') => {
                if let Some(idx) = self.downloads_state.selected() {
                    if let Some(&id) = self.downloads_sorted_ids().get(idx) {
                        self.downloads_select.toggle(id);
                    }
                }
            }
            KeyCode::Char('V') => {
                if self.downloads_select.in_range() {
                    self.downloads_select.end_range();
                } else if let Some(idx) = self.downloads_state.selected() {
                    if let Some(&id) = self.downloads_sorted_ids().get(idx) {
                        self.downloads_select.start_range(idx, id);
                    }
                }
            }
            KeyCode::Enter | KeyCode::Char('r') => {
                if self.downloads_select.count() > 0 {
                    let ids: Vec<i64> = self.downloads_select.ids.iter().copied().collect();
                    let mut count = 0;
                    for id in ids {
                        if matches!(
                            self.downloads.get(&id).map(|s| s.status),
                            Some(DownloadStatus::Failed)
                        ) {
                            self.retry_download(id);
                            count += 1;
                        }
                    }
                    self.downloads_select.clear();
                    self.status_msg = format!("retrying {count} download(s)");
                } else if let Some(idx) = self.downloads_state.selected() {
                    self.retry_download_at(idx);
                }
            }
            KeyCode::Backspace | KeyCode::Delete => {
                if self.downloads_select.count() > 0 {
                    let ids: Vec<i64> = self.downloads_select.ids.iter().copied().collect();
                    let mut removed = 0;
                    let mut skipped = 0;
                    for id in ids {
                        if self.remove_download(id) {
                            removed += 1;
                        } else {
                            skipped += 1;
                        }
                    }
                    self.downloads_select.clear();
                    self.status_msg = if skipped > 0 {
                        format!("removed {removed}, skipped {skipped} in-progress")
                    } else {
                        format!("removed {removed} download(s)")
                    };
                    let new_len = self.downloads.len();
                    let cur = self.downloads_state.selected().unwrap_or(0);
                    self.downloads_state
                        .select((new_len > 0).then_some(cur.min(new_len - 1)));
                } else if let Some(idx) = self.downloads_state.selected() {
                    self.remove_download_at(idx);
                    let new_len = self.downloads.len();
                    self.downloads_state
                        .select((new_len > 0).then_some(idx.min(new_len - 1)));
                }
            }
            _ => {}
        }
        if self.downloads_select.in_range() {
            if let Some(idx) = self.downloads_state.selected() {
                let ids = self.downloads_sorted_ids();
                self.downloads_select.extend_range(idx, &ids);
            }
        }
        Ok(())
    }
    pub(super) fn download_songs_selection(&mut self) {
        if self.songs_select.count() == 0 {
            if let Some(id) = self.selected_track().map(|t| t.id) {
                self.start_download(id);
            }
            return;
        }
        let ids: Vec<i64> = self
            .filtered
            .iter()
            .map(|&i| self.tracks[i].id)
            .filter(|id| self.songs_select.is_selected(*id))
            .collect();
        let count = ids.len();
        for id in ids {
            self.start_download(id);
        }
        self.status_msg = format!("queued {count} track(s) for download");
        self.songs_select.clear();
    }
    pub(super) fn download_playlist_selection(&mut self) {
        if self.playlist_select.count() == 0 {
            if let Some(idx) = self.playlist_tracks_state.selected() {
                if let Some(track_id) = self.playlist_tracks.get(idx).map(|pt| pt.track_id) {
                    self.start_download(track_id);
                }
            }
            return;
        }
        let ids: Vec<i64> = self
            .playlist_tracks
            .iter()
            .filter(|pt| self.playlist_select.is_selected(pt.position))
            .map(|pt| pt.track_id)
            .collect();
        let count = ids.len();
        for id in ids {
            self.start_download(id);
        }
        self.status_msg = format!("queued {count} track(s) for download");
        self.playlist_select.clear();
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
    pub(super) fn retry_download(&mut self, id: i64) {
        let Some(st) = self.downloads.get(&id) else {
            return;
        };
        if st.status != DownloadStatus::Failed {
            return;
        }
        self.start_download(id);
    }
    pub(super) fn retry_download_at(&mut self, index: usize) {
        let ids = self.downloads_sorted_ids();
        if let Some(&id) = ids.get(index) {
            self.retry_download(id);
        }
    }
    pub(super) fn remove_download(&mut self, id: i64) -> bool {
        let Some(lib) = self.library_id else {
            return false;
        };
        let Some(st) = self.downloads.get(&id) else {
            return false;
        };
        if matches!(
            st.status,
            DownloadStatus::Queued | DownloadStatus::Downloading
        ) {
            return false;
        }
        if let Some(path) = &st.local_path {
            let _ = std::fs::remove_file(path);
        }
        if let Err(e) = self.db.delete_download(lib, id) {
            self.status_msg = format!("failed to remove download: {e}");
            return false;
        }
        self.downloads.remove(&id);
        true
    }
    pub(super) fn remove_download_at(&mut self, index: usize) {
        let ids = self.downloads_sorted_ids();
        let Some(&id) = ids.get(index) else { return };
        if self.remove_download(id) {
            self.status_msg = "download removed".into();
        } else if self.downloads.contains_key(&id) {
            self.status_msg = "cannot remove: download in progress".into();
        }
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
    pub(super) fn enter_uploads_tab(&mut self) {
        if self.downloader_job_id.is_some() {
            self.upload_stage = UploadStage::Job;
            return;
        }
        self.refresh_uploader_scripts();
    }
    pub(super) fn refresh_uploader_scripts(&mut self) {
        let Some(lib) = self.library_id() else {
            self.status_msg = "no library selected".into();
            self.upload_stage = UploadStage::List { index: 0 };
            return;
        };
        match self.client.list_downloaders(lib) {
            Ok(scripts) => {
                self.downloader_scripts = scripts;
            }
            Err(e) => {
                self.status_msg = format!("downloaders: {e}");
            }
        }
        self.upload_stage = UploadStage::List { index: 0 };
    }
    pub(super) fn handle_uploads_key(&mut self, key: KeyEvent) -> Result<()> {
        match self.upload_stage.clone() {
            UploadStage::List { mut index } => {
                let len = self.downloader_scripts.len();
                match key.code {
                    KeyCode::Char('j') | KeyCode::Down => {
                        if len > 0 {
                            index = (index + 1).min(len - 1);
                        }
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        index = index.saturating_sub(1);
                    }
                    KeyCode::Char('r') => {
                        self.refresh_uploader_scripts();
                        return Ok(());
                    }
                    KeyCode::Enter => {
                        if let Some(script) =
                            self.downloader_scripts.get(index).map(|d| d.name.clone())
                        {
                            self.upload_stage = UploadStage::Input {
                                script,
                                buf: String::new(),
                            };
                        }
                        return Ok(());
                    }
                    _ => {}
                }
                self.upload_stage = UploadStage::List { index };
            }
            UploadStage::Input { script, mut buf } => match key.code {
                KeyCode::Esc => {
                    self.upload_stage = UploadStage::List { index: 0 };
                }
                KeyCode::Enter => {
                    self.commit_run_downloader(&script, &buf);
                }
                KeyCode::Backspace => {
                    buf.pop();
                    self.upload_stage = UploadStage::Input { script, buf };
                }
                KeyCode::Char(c) => {
                    buf.push(c);
                    self.upload_stage = UploadStage::Input { script, buf };
                }
                _ => {
                    self.upload_stage = UploadStage::Input { script, buf };
                }
            },
            UploadStage::Job => {
                if matches!(key.code, KeyCode::Esc) {
                    self.downloader_polling = false;
                    self.downloader_job = None;
                    self.downloader_job_id = None;
                    self.downloader_script_name = None;
                    self.refresh_uploader_scripts();
                }
            }
        }
        Ok(())
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
            self.upload_stage = UploadStage::List { index: 0 };
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
                self.upload_stage = UploadStage::Job;
            }
            Err(e) => {
                self.status_msg = format!("downloader run failed: {e}");
                self.upload_stage = UploadStage::Input {
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
