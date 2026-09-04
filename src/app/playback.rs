use super::*;

impl App {
    pub(super) fn handle_queue_key(&mut self, key: KeyEvent) -> Result<()> {
        let len = self.mpv.snapshot().playlist.len();
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if len > 0 {
                    let cur = self.queue_state.selected().unwrap_or(0);
                    self.queue_state.select(Some((cur + 1).min(len - 1)));
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if len > 0 {
                    let cur = self.queue_state.selected().unwrap_or(0);
                    self.queue_state.select(Some(cur.saturating_sub(1)));
                }
            }
            KeyCode::Char('g') | KeyCode::Home => {
                if len > 0 {
                    self.queue_state.select(Some(0));
                }
            }
            KeyCode::Char('G') | KeyCode::End => {
                if len > 0 {
                    self.queue_state.select(Some(len - 1));
                }
            }
            KeyCode::Enter => {
                if let Some(idx) = self.queue_state.selected() {
                    self.mpv.playlist_play_index(idx as i64)?;
                }
            }
            KeyCode::Char('d') => {
                if let Some(idx) = self.queue_state.selected() {
                    self.mpv.playlist_remove_index(idx as i64)?;
                }
            }
            KeyCode::Char('J') => self.move_queue_item(1)?,
            KeyCode::Char('K') => self.move_queue_item(-1)?,
            _ => {}
        }
        Ok(())
    }
    pub(super) fn move_queue_item(&mut self, delta: i32) -> Result<()> {
        let len = self.mpv.snapshot().playlist.len() as i32;
        if len < 2 {
            return Ok(());
        }
        let Some(cur) = self.queue_state.selected() else {
            return Ok(());
        };
        let cur = cur as i32;
        let new = cur + delta;
        if new < 0 || new >= len {
            return Ok(());
        }
        // mpv playlist-move quirk: when src < dst, the moved entry ends up at dst-1.
        // So bump dst by one for downward moves to land exactly at `new`.
        let dst = if delta > 0 { new + 1 } else { new };
        self.mpv.playlist_move(cur as i64, dst as i64)?;
        self.queue_state.select(Some(new as usize));
        Ok(())
    }
    pub(super) fn play_selected(&mut self) -> Result<()> {
        let Some(track) = self.selected_track().cloned() else {
            return Ok(());
        };
        let Some(lib) = self.library_id() else {
            self.status_msg = "no library selected".into();
            return Ok(());
        };
        let url = self.resolve_track_url(lib, track.id);
        self.mpv.load(&url)?;
        self.mpv.set_pause(false)?;
        Ok(())
    }
    pub(super) fn enqueue_songs_selection(&mut self) -> Result<()> {
        if self.songs_select.count() == 0 {
            return self.enqueue_selected();
        }
        let Some(lib) = self.library_id() else {
            self.status_msg = "no library selected".into();
            return Ok(());
        };
        let ids: Vec<i64> = self
            .filtered
            .iter()
            .map(|&i| self.tracks[i].id)
            .filter(|id| self.songs_select.is_selected(*id))
            .collect();
        let count = ids.len();
        for id in ids {
            let url = self.resolve_track_url(lib, id);
            self.mpv.enqueue(&url)?;
        }
        self.status_msg = format!("queued {count} tracks");
        self.songs_select.clear();
        Ok(())
    }
    pub(super) fn enqueue_selected(&mut self) -> Result<()> {
        let Some(track) = self.selected_track().cloned() else {
            return Ok(());
        };
        let Some(lib) = self.library_id() else {
            self.status_msg = "no library selected".into();
            return Ok(());
        };
        let url = self.resolve_track_url(lib, track.id);
        self.mpv.enqueue(&url)?;
        self.status_msg = format!(
            "queued: {} — {}",
            track.display_artist(),
            track.display_title()
        );
        Ok(())
    }
    pub(super) fn enqueue_all_filtered(&mut self) -> Result<()> {
        if self.filtered.is_empty() {
            self.status_msg = "nothing to queue".into();
            return Ok(());
        }
        let Some(lib) = self.library_id() else {
            self.status_msg = "no library selected".into();
            return Ok(());
        };
        let ids: Vec<i64> = self.filtered.iter().map(|&i| self.tracks[i].id).collect();
        for id in &ids {
            let url = self.resolve_track_url(lib, *id);
            self.mpv.enqueue(&url)?;
        }
        self.status_msg = format!("queued {} tracks", ids.len());
        Ok(())
    }
    pub(super) fn toggle_pause(&mut self) -> Result<()> {
        let snap = self.mpv.snapshot();
        if snap.idle_active || snap.current_path.is_none() {
            return Ok(());
        }
        self.mpv.set_pause(!snap.paused)
    }
}
