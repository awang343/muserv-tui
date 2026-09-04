use super::*;

impl App {
    pub(super) fn handle_playlists_key(&mut self, key: KeyEvent) -> Result<()> {
        if matches!(key.code, KeyCode::Tab) {
            self.playlists_focus = match self.playlists_focus {
                PlaylistsFocus::List => PlaylistsFocus::Tracks,
                PlaylistsFocus::Tracks => PlaylistsFocus::List,
            };
            return Ok(());
        }
        if matches!(key.code, KeyCode::Esc) && self.playlists_focus == PlaylistsFocus::Tracks {
            if self.playlist_select.in_range() {
                self.playlist_select.end_range();
                return Ok(());
            }
            if self.playlist_select.count() > 0 {
                self.playlist_select.clear();
                return Ok(());
            }
            self.playlists_focus = PlaylistsFocus::List;
            return Ok(());
        }
        match self.playlists_focus {
            PlaylistsFocus::List => self.handle_playlists_list_key(key),
            PlaylistsFocus::Tracks => self.handle_playlists_tracks_key(key),
        }
    }
    pub(super) fn handle_playlists_list_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.move_playlist_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_playlist_selection(-1),
            KeyCode::Char('g') | KeyCode::Home => {
                if !self.playlists.is_empty() {
                    self.playlists_state.select(Some(0));
                    self.refresh_playlist_tracks();
                }
            }
            KeyCode::Char('G') | KeyCode::End => {
                if !self.playlists.is_empty() {
                    self.playlists_state.select(Some(self.playlists.len() - 1));
                    self.refresh_playlist_tracks();
                }
            }
            KeyCode::Char('N') => self.mode = Mode::NewPlaylist(String::new()),
            KeyCode::Char('r') => {
                if let (Some(id), Some(name)) =
                    (self.selected_playlist_id(), self.selected_playlist_name())
                {
                    self.mode = Mode::RenamePlaylist(id, name);
                }
            }
            KeyCode::Char('D') => self.delete_selected_playlist()?,
            KeyCode::Char('P') => self.play_selected_playlist(0)?,
            _ => {}
        }
        Ok(())
    }
    pub(super) fn handle_playlists_tracks_key(&mut self, key: KeyEvent) -> Result<()> {
        let len = self.playlist_tracks.len();
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if len > 0 {
                    let cur = self.playlist_tracks_state.selected().unwrap_or(0);
                    self.playlist_tracks_state
                        .select(Some((cur + 1).min(len - 1)));
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if len > 0 {
                    let cur = self.playlist_tracks_state.selected().unwrap_or(0);
                    self.playlist_tracks_state
                        .select(Some(cur.saturating_sub(1)));
                }
            }
            KeyCode::Char('g') | KeyCode::Home => {
                if len > 0 {
                    self.playlist_tracks_state.select(Some(0));
                }
            }
            KeyCode::Char('G') | KeyCode::End => {
                if len > 0 {
                    self.playlist_tracks_state.select(Some(len - 1));
                }
            }
            KeyCode::Char('x') => {
                if let Some(idx) = self.playlist_tracks_state.selected() {
                    if let Some(pt) = self.playlist_tracks.get(idx) {
                        self.playlist_select.toggle(pt.position);
                    }
                }
            }
            KeyCode::Char('V') => {
                if self.playlist_select.in_range() {
                    self.playlist_select.end_range();
                } else if let Some(idx) = self.playlist_tracks_state.selected() {
                    if let Some(pt) = self.playlist_tracks.get(idx) {
                        self.playlist_select.start_range(idx, pt.position);
                    }
                }
            }
            KeyCode::Enter => {
                if let Some(idx) = self.playlist_tracks_state.selected() {
                    self.play_selected_playlist(idx)?;
                }
            }
            KeyCode::Char('a') => self.enqueue_playlist_selection()?,
            KeyCode::Char('d') => self.remove_playlist_selection()?,
            KeyCode::Char('J') => self.move_playlist_track(1)?,
            KeyCode::Char('K') => self.move_playlist_track(-1)?,
            KeyCode::Char('o') => self.download_playlist_selection(),
            KeyCode::Char('O') => self.download_selected_playlist(),
            _ => {}
        }
        if self.playlist_select.in_range() {
            if let Some(idx) = self.playlist_tracks_state.selected() {
                let ids: Vec<i64> = self.playlist_tracks.iter().map(|pt| pt.position).collect();
                self.playlist_select.extend_range(idx, &ids);
            }
        }
        Ok(())
    }
    pub(super) fn enqueue_playlist_selection(&mut self) -> Result<()> {
        if self.playlist_select.count() == 0 {
            if let Some(idx) = self.playlist_tracks_state.selected() {
                if let Some(pt) = self.playlist_tracks.get(idx) {
                    let Some(lib) = self.library_id() else {
                        self.status_msg = "no library selected".into();
                        return Ok(());
                    };
                    let url = self.resolve_track_url(lib, pt.track_id);
                    self.mpv.enqueue(&url)?;
                    self.status_msg =
                        format!("queued: {} — {}", pt.display_artist(), pt.display_title());
                }
            }
            return Ok(());
        }
        let Some(lib) = self.library_id() else {
            self.status_msg = "no library selected".into();
            return Ok(());
        };
        let ids: Vec<i64> = self
            .playlist_tracks
            .iter()
            .filter(|pt| self.playlist_select.is_selected(pt.position))
            .map(|pt| pt.track_id)
            .collect();
        let count = ids.len();
        for id in ids {
            let url = self.resolve_track_url(lib, id);
            self.mpv.enqueue(&url)?;
        }
        self.status_msg = format!("queued {count} tracks");
        self.playlist_select.clear();
        Ok(())
    }
    pub(super) fn remove_playlist_selection(&mut self) -> Result<()> {
        if self.playlist_select.count() == 0 {
            return self.remove_selected_playlist_track();
        }
        let Some(pid) = self.selected_playlist_id() else {
            return Ok(());
        };
        let Some(lib) = self.library_id() else {
            self.status_msg = "no library selected".into();
            return Ok(());
        };
        let track_ids: Vec<i64> = self
            .playlist_tracks
            .iter()
            .filter(|pt| self.playlist_select.is_selected(pt.position))
            .map(|pt| pt.track_id)
            .collect();
        let total = track_ids.len();
        let mut removed = 0;
        for tid in track_ids {
            if self.client.remove_from_playlist(lib, pid, tid).is_ok() {
                removed += 1;
            }
        }
        self.playlist_select.clear();
        self.status_msg = format!("removed {removed}/{total} from playlist");
        self.refresh_playlist_tracks();
        self.refresh_playlists();
        Ok(())
    }
    pub(super) fn move_playlist_track(&mut self, delta: i32) -> Result<()> {
        let Some(pid) = self.selected_playlist_id() else {
            return Ok(());
        };
        let Some(idx) = self.playlist_tracks_state.selected() else {
            return Ok(());
        };
        let len = self.playlist_tracks.len() as i32;
        let new_idx = idx as i32 + delta;
        if new_idx < 0 || new_idx >= len {
            return Ok(());
        }
        let new_idx = new_idx as usize;

        self.playlist_tracks.swap(idx, new_idx);
        for (i, pt) in self.playlist_tracks.iter_mut().enumerate() {
            pt.position = i as i64;
        }
        self.playlist_tracks_state.select(Some(new_idx));
        self.playlist_select.clear();

        let track_ids: Vec<i64> = self.playlist_tracks.iter().map(|p| p.track_id).collect();
        let Some(lib) = self.library_id() else {
            self.status_msg = "no library selected".into();
            return Ok(());
        };
        if let Err(e) = self.client.set_playlist_tracks(lib, pid, &track_ids) {
            self.status_msg = format!("reorder failed: {e}");
            self.refresh_playlist_tracks();
        }
        Ok(())
    }
    pub(super) fn move_playlist_selection(&mut self, delta: i32) {
        if self.playlists.is_empty() {
            return;
        }
        let cur = self.playlists_state.selected().unwrap_or(0) as i32;
        let len = self.playlists.len() as i32;
        let next = (cur + delta).clamp(0, len - 1);
        if next as usize != self.playlists_state.selected().unwrap_or(usize::MAX) {
            self.playlists_state.select(Some(next as usize));
            self.refresh_playlist_tracks();
        }
    }
    pub(super) fn commit_new_playlist(&mut self, raw: &str) {
        let name = raw.trim();
        if name.is_empty() {
            self.status_msg = "playlist name is empty".into();
            return;
        }
        let Some(lib) = self.library_id() else {
            self.status_msg = "no library selected".into();
            return;
        };
        match self.client.create_playlist(lib, name) {
            Ok(p) => {
                self.status_msg = format!("created playlist {}", p.name);
                self.refresh_playlists();
                if let Some(i) = self.playlists.iter().position(|x| x.id == p.id) {
                    self.playlists_state.select(Some(i));
                    self.refresh_playlist_tracks();
                }
            }
            Err(e) => self.status_msg = format!("create failed: {e}"),
        }
    }
    pub(super) fn commit_rename_playlist(&mut self, id: i64, raw: &str) {
        let name = raw.trim();
        if name.is_empty() {
            self.status_msg = "name is empty".into();
            return;
        }
        let Some(lib) = self.library_id() else {
            self.status_msg = "no library selected".into();
            return;
        };
        match self.client.rename_playlist(lib, id, name) {
            Ok(_) => {
                self.status_msg = "renamed".into();
                self.refresh_playlists();
            }
            Err(e) => self.status_msg = format!("rename failed: {e}"),
        }
    }
    pub(super) fn delete_selected_playlist(&mut self) -> Result<()> {
        let Some(id) = self.selected_playlist_id() else {
            return Ok(());
        };
        let name = self.selected_playlist_name().unwrap_or_default();
        let Some(lib) = self.library_id() else {
            self.status_msg = "no library selected".into();
            return Ok(());
        };
        match self.client.delete_playlist(lib, id) {
            Ok(()) => {
                self.status_msg = format!("deleted {name}");
                self.playlists_state.select(None);
                self.playlist_tracks_for = None;
                self.refresh_playlists();
            }
            Err(e) => self.status_msg = format!("delete failed: {e}"),
        }
        Ok(())
    }
    pub(super) fn play_selected_playlist(&mut self, start_index: usize) -> Result<()> {
        let Some(_) = self.selected_playlist_id() else {
            return Ok(());
        };
        if self.playlist_tracks.is_empty() {
            self.status_msg = "playlist is empty".into();
            return Ok(());
        }
        let Some(lib) = self.library_id() else {
            self.status_msg = "no library selected".into();
            return Ok(());
        };
        let start = start_index.min(self.playlist_tracks.len().saturating_sub(1));
        let first = &self.playlist_tracks[start];
        let url = self.resolve_track_url(lib, first.track_id);
        self.mpv.load(&url)?;
        self.mpv.set_pause(false)?;
        for pt in &self.playlist_tracks[start + 1..] {
            let u = self.resolve_track_url(lib, pt.track_id);
            self.mpv.enqueue(&u)?;
        }
        let name = self.selected_playlist_name().unwrap_or_default();
        self.status_msg = format!(
            "playing {name} from #{} ({} tracks)",
            start + 1,
            self.playlist_tracks.len() - start
        );
        Ok(())
    }
    pub(super) fn remove_selected_playlist_track(&mut self) -> Result<()> {
        let Some(pid) = self.selected_playlist_id() else {
            return Ok(());
        };
        let Some(idx) = self.playlist_tracks_state.selected() else {
            return Ok(());
        };
        let Some(pt) = self.playlist_tracks.get(idx).cloned() else {
            return Ok(());
        };
        let Some(lib) = self.library_id() else {
            self.status_msg = "no library selected".into();
            return Ok(());
        };
        match self.client.remove_from_playlist(lib, pid, pt.track_id) {
            Ok(()) => {
                self.status_msg = "removed from playlist".into();
                self.refresh_playlist_tracks();
                if let Some(sel) = self.playlist_tracks_state.selected() {
                    if sel >= self.playlist_tracks.len() {
                        let new = self.playlist_tracks.len().checked_sub(1);
                        self.playlist_tracks_state.select(new);
                    }
                }
                self.refresh_playlists();
            }
            Err(e) => self.status_msg = format!("remove failed: {e}"),
        }
        Ok(())
    }
    pub(super) fn selected_playlist_id(&self) -> Option<i64> {
        let i = self.playlists_state.selected()?;
        self.playlists.get(i).map(|p| p.id)
    }
    pub(super) fn selected_playlist_name(&self) -> Option<String> {
        let i = self.playlists_state.selected()?;
        self.playlists.get(i).map(|p| p.name.clone())
    }
    pub(super) fn playlists_containing(&self, track_id: i64) -> Vec<i64> {
        let Some(lib) = self.library_id() else {
            return Vec::new();
        };
        self.client
            .playlists_containing_track(lib, track_id)
            .unwrap_or_default()
    }
}
