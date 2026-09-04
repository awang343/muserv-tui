use super::*;

impl App {
    pub(super) fn handle_songs_key(&mut self, key: KeyEvent) -> Result<()> {
        if self.show_tags {
            return self.handle_tags_key(key);
        }
        if self.show_details {
            if matches!(key.code, KeyCode::Char('i')) {
                self.show_details = false;
            }
            return Ok(());
        }
        if matches!(key.code, KeyCode::Esc) {
            self.apply_filter("");
            return Ok(());
        }
        self.handle_tracks_key(key)
    }
    pub(super) fn handle_tracks_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.move_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_selection(-1),
            KeyCode::PageDown => self.move_selection(10),
            KeyCode::PageUp => self.move_selection(-10),
            KeyCode::Char('g') | KeyCode::Home => {
                if !self.filtered.is_empty() {
                    self.list_state.select(Some(0));
                }
            }
            KeyCode::Char('G') | KeyCode::End => {
                if !self.filtered.is_empty() {
                    self.list_state.select(Some(self.filtered.len() - 1));
                }
            }
            KeyCode::Enter => self.play_selected()?,
            KeyCode::Char('a') => self.enqueue_selected()?,
            KeyCode::Char('E') => self.enqueue_all_filtered()?,
            KeyCode::Char('A') => {
                if let Some(t) = self.selected_track().map(|t| t.id) {
                    if self.playlists.is_empty() {
                        self.status_msg = "no playlists — create one in the Playlists tab".into();
                    } else {
                        let containing = self.playlists_containing(t);
                        self.mode = Mode::PickPlaylist {
                            index: 0,
                            track_id: t,
                            containing,
                        };
                    }
                }
            }
            KeyCode::Char('/') => self.mode = Mode::Filter(String::new()),
            KeyCode::Char('T') => self.mode = Mode::TagSearch(String::new()),
            KeyCode::Char('t') => self.open_tags_popup(),
            KeyCode::Char('i') => self.open_details_popup(),
            KeyCode::Char('s') => {
                let index = SortKey::ALL
                    .iter()
                    .position(|k| *k == self.sort_key)
                    .unwrap_or(0);
                self.mode = Mode::SortPicker { index };
            }
            KeyCode::Char('o') => {
                if let Some(id) = self.selected_track().map(|t| t.id) {
                    self.start_download(id);
                }
            }
            _ => {}
        }
        Ok(())
    }
    pub(super) fn apply_filter(&mut self, q: &str) {
        let needle = q.to_lowercase();
        if needle.is_empty() {
            self.filtered = (0..self.tracks.len()).collect();
        } else {
            self.filtered = self
                .tracks
                .iter()
                .enumerate()
                .filter(|(_, t)| {
                    let hay = format!(
                        "{} {} {}",
                        t.display_artist(),
                        t.display_title(),
                        t.display_album()
                    )
                    .to_lowercase();
                    hay.contains(&needle)
                })
                .map(|(i, _)| i)
                .collect();
        }
        self.sort_filtered();
        let sel = if self.filtered.is_empty() {
            None
        } else {
            Some(0)
        };
        self.list_state.select(sel);
    }
    pub(super) fn sort_filtered(&mut self) {
        let tracks = &self.tracks;
        let key = self.sort_key;
        self.filtered
            .sort_by(|&a, &b| cmp_tracks(&tracks[a], &tracks[b], key));
    }
    pub(super) fn apply_sort(&mut self, key: SortKey) {
        let prev_id = self.selected_track().map(|t| t.id);
        self.sort_key = key;
        self.sort_filtered();
        let new_sel = match prev_id {
            Some(id) => self.filtered.iter().position(|&i| self.tracks[i].id == id),
            None => (!self.filtered.is_empty()).then_some(0),
        };
        self.list_state.select(new_sel.or({
            if self.filtered.is_empty() {
                None
            } else {
                Some(0)
            }
        }));
        self.status_msg = format!("sort: {}", key.short_label());
    }
    pub(super) fn move_selection(&mut self, delta: i32) {
        if self.filtered.is_empty() {
            return;
        }
        let cur = self.list_state.selected().unwrap_or(0) as i32;
        let len = self.filtered.len() as i32;
        let next = (cur + delta).clamp(0, len - 1);
        if next as usize != self.list_state.selected().unwrap_or(usize::MAX) {
            self.list_state.select(Some(next as usize));
        }
    }
    pub(super) fn selected_track(&self) -> Option<&Track> {
        let i = self.list_state.selected()?;
        let idx = *self.filtered.get(i)?;
        self.tracks.get(idx)
    }
    pub(super) fn run_tag_search(&mut self, query: &str) {
        let q = query.trim();
        if q.is_empty() {
            self.apply_filter("");
            return;
        }
        let Some(lib) = self.library_id() else {
            self.status_msg = "no library selected".into();
            return;
        };
        match self.client.search(lib, q) {
            Ok(hits) => {
                let by_id: std::collections::HashMap<i64, usize> = self
                    .tracks
                    .iter()
                    .enumerate()
                    .map(|(i, t)| (t.id, i))
                    .collect();
                self.filtered = hits
                    .iter()
                    .filter_map(|t| by_id.get(&t.id).copied())
                    .collect();
                self.sort_filtered();
                self.list_state.select(if self.filtered.is_empty() {
                    None
                } else {
                    Some(0)
                });
                self.status_msg = format!("tag search '{q}': {} hits", self.filtered.len());
            }
            Err(e) => self.status_msg = format!("search failed: {e}"),
        }
    }
}

fn cmp_tracks(a: &Track, b: &Track, key: SortKey) -> std::cmp::Ordering {
    let s = |x: &str| x.to_lowercase();
    match key {
        SortKey::Default => s(a.display_artist())
            .cmp(&s(b.display_artist()))
            .then_with(|| s(a.display_title()).cmp(&s(b.display_title()))),
        SortKey::Title => s(a.display_title()).cmp(&s(b.display_title())),
        SortKey::Artist => s(a.display_artist())
            .cmp(&s(b.display_artist()))
            .then_with(|| s(a.display_album()).cmp(&s(b.display_album())))
            .then(a.disc_no.unwrap_or(0).cmp(&b.disc_no.unwrap_or(0)))
            .then(a.track_no.unwrap_or(0).cmp(&b.track_no.unwrap_or(0))),
        SortKey::Album => s(a.display_album())
            .cmp(&s(b.display_album()))
            .then(a.disc_no.unwrap_or(0).cmp(&b.disc_no.unwrap_or(0)))
            .then(a.track_no.unwrap_or(0).cmp(&b.track_no.unwrap_or(0))),
        SortKey::Duration => a
            .duration_ms
            .unwrap_or(i64::MAX)
            .cmp(&b.duration_ms.unwrap_or(i64::MAX)),
        // Year and AddedAt are "newest first": descending.
        SortKey::Year => b
            .year
            .unwrap_or(i64::MIN)
            .cmp(&a.year.unwrap_or(i64::MIN))
            .then_with(|| s(a.display_album()).cmp(&s(b.display_album())))
            .then(a.disc_no.unwrap_or(0).cmp(&b.disc_no.unwrap_or(0)))
            .then(a.track_no.unwrap_or(0).cmp(&b.track_no.unwrap_or(0))),
        SortKey::AddedAt => b.added_at.cmp(&a.added_at),
    }
}
