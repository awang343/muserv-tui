use super::*;

impl App {
    pub(super) fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        match self.mode.clone() {
            Mode::Filter(buf) => {
                self.handle_text_input(key, buf, Mode::Filter, |app, q| {
                    app.apply_filter(&q);
                });
                return Ok(());
            }
            Mode::TagSearch(mut buf) => {
                if matches!(key.code, KeyCode::Esc) {
                    self.mode = Mode::Normal;
                    self.apply_filter("");
                    return Ok(());
                }
                if matches!(key.code, KeyCode::Enter) {
                    let q = buf.clone();
                    self.mode = Mode::Normal;
                    self.run_tag_search(&q);
                    return Ok(());
                }
                match key.code {
                    KeyCode::Backspace => {
                        buf.pop();
                    }
                    KeyCode::Char(c) => {
                        buf.push(c);
                    }
                    _ => {}
                }
                self.mode = Mode::TagSearch(buf);
                return Ok(());
            }
            Mode::AddTag(buf) => {
                if matches!(key.code, KeyCode::Esc) {
                    self.mode = Mode::Normal;
                    return Ok(());
                }
                if matches!(key.code, KeyCode::Enter) {
                    self.mode = Mode::Normal;
                    self.commit_add_tag(&buf)?;
                    return Ok(());
                }
                self.edit_buf(key, buf, Mode::AddTag);
                return Ok(());
            }
            Mode::EditSetting(field, mut buf) => {
                if matches!(key.code, KeyCode::Esc) {
                    self.mode = Mode::Normal;
                    return Ok(());
                }
                if matches!(key.code, KeyCode::Enter) {
                    match field {
                        SettingsField::ServerUrl => self.settings.server_url = buf,
                        SettingsField::AuthToken => self.settings.auth_token = buf,
                        SettingsField::Library => {} // picked via overlay, not text input
                    }
                    self.mode = Mode::Normal;
                    return Ok(());
                }
                match key.code {
                    KeyCode::Backspace => {
                        buf.pop();
                    }
                    KeyCode::Char(c) => {
                        buf.push(c);
                    }
                    _ => {}
                }
                self.mode = Mode::EditSetting(field, buf);
                return Ok(());
            }
            Mode::NewPlaylist(mut buf) => {
                if matches!(key.code, KeyCode::Esc) {
                    self.mode = Mode::Normal;
                    return Ok(());
                }
                if matches!(key.code, KeyCode::Enter) {
                    self.mode = Mode::Normal;
                    self.commit_new_playlist(&buf);
                    return Ok(());
                }
                match key.code {
                    KeyCode::Backspace => {
                        buf.pop();
                    }
                    KeyCode::Char(c) => {
                        buf.push(c);
                    }
                    _ => {}
                }
                self.mode = Mode::NewPlaylist(buf);
                return Ok(());
            }
            Mode::RenamePlaylist(id, mut buf) => {
                if matches!(key.code, KeyCode::Esc) {
                    self.mode = Mode::Normal;
                    return Ok(());
                }
                if matches!(key.code, KeyCode::Enter) {
                    self.mode = Mode::Normal;
                    self.commit_rename_playlist(id, &buf);
                    return Ok(());
                }
                match key.code {
                    KeyCode::Backspace => {
                        buf.pop();
                    }
                    KeyCode::Char(c) => {
                        buf.push(c);
                    }
                    _ => {}
                }
                self.mode = Mode::RenamePlaylist(id, buf);
                return Ok(());
            }
            Mode::PickPlaylist {
                mut index,
                track_ids,
                containing,
            } => {
                let len = self.playlists.len();
                match key.code {
                    KeyCode::Esc => {
                        self.mode = Mode::Normal;
                        return Ok(());
                    }
                    KeyCode::Enter => {
                        self.mode = Mode::Normal;
                        if let Some(p) = self.playlists.get(index) {
                            let pid = p.id;
                            let pname = p.name.clone();
                            let Some(lib) = self.library_id() else {
                                self.status_msg = "no library selected".into();
                                return Ok(());
                            };
                            let total = track_ids.len();
                            let mut added = 0;
                            let mut last_err: Option<String> = None;
                            for tid in &track_ids {
                                match self.client.add_to_playlist(lib, pid, *tid) {
                                    Ok(()) => added += 1,
                                    Err(e) => last_err = Some(e.to_string()),
                                }
                            }
                            self.status_msg = match last_err {
                                Some(e) => format!("added {added}/{total} to {pname}; {e}"),
                                None if total == 1 => format!("added to {pname}"),
                                None => format!("added {added} tracks to {pname}"),
                            };
                            if self.playlist_tracks_for == Some(pid) {
                                self.refresh_playlist_tracks();
                            }
                            self.refresh_playlists();
                            self.songs_select.clear();
                        } else {
                            self.status_msg =
                                "no playlists — create one first (Playlists tab)".into();
                        }
                        return Ok(());
                    }
                    KeyCode::Char('j') | KeyCode::Down => {
                        if len > 0 {
                            index = (index + 1).min(len - 1);
                        }
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        index = index.saturating_sub(1);
                    }
                    _ => {}
                }
                self.mode = Mode::PickPlaylist {
                    index,
                    track_ids,
                    containing,
                };
                return Ok(());
            }
            Mode::PickLibrary { mut index } => {
                let len = self.libraries.len();
                match key.code {
                    KeyCode::Esc => {
                        self.mode = Mode::Normal;
                        return Ok(());
                    }
                    KeyCode::Enter => {
                        let chosen = self.libraries.get(index).map(|l| l.id);
                        self.mode = Mode::Normal;
                        if let Some(id) = chosen {
                            self.switch_library(id);
                        } else {
                            self.status_msg = "no libraries available".into();
                        }
                        return Ok(());
                    }
                    KeyCode::Char('j') | KeyCode::Down => {
                        if len > 0 {
                            index = (index + 1).min(len - 1);
                        }
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        index = index.saturating_sub(1);
                    }
                    _ => {}
                }
                self.mode = Mode::PickLibrary { index };
                return Ok(());
            }
            Mode::SortPicker { mut index } => {
                let len = SortKey::ALL.len();
                match key.code {
                    KeyCode::Esc => {
                        self.mode = Mode::Normal;
                        return Ok(());
                    }
                    KeyCode::Enter => {
                        let chosen = SortKey::ALL.get(index).copied();
                        self.mode = Mode::Normal;
                        if let Some(k) = chosen {
                            self.apply_sort(k);
                        }
                        return Ok(());
                    }
                    KeyCode::Char('j') | KeyCode::Down => {
                        if len > 0 {
                            index = (index + 1).min(len - 1);
                        }
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        index = index.saturating_sub(1);
                    }
                    _ => {}
                }
                self.mode = Mode::SortPicker { index };
                return Ok(());
            }
            Mode::Normal => {}
        }

        // While typing a URL in the Uploads tab, every key is text input —
        // bypass tab switching and global shortcuts entirely (same as the
        // other free-text Modes above).
        if self.tab == Tab::Uploads {
            if let UploadStage::Input { .. } = self.upload_stage {
                return self.handle_uploads_key(key);
            }
        }

        // Tab switching by number key (any tab).
        if let KeyCode::Char(c) = key.code {
            if let Some(t) = Tab::from_digit(c) {
                if t == Tab::Uploads && self.tab != Tab::Uploads {
                    self.enter_uploads_tab();
                }
                self.tab = t;
                self.show_tags = false;
                return Ok(());
            }
        }

        // Other global keys (any tab).
        match (key.code, key.modifiers) {
            (KeyCode::Char('q'), _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                self.should_quit = true;
                return Ok(());
            }
            (KeyCode::Char(' '), _) => {
                self.toggle_pause()?;
                return Ok(());
            }
            (KeyCode::Char('n'), _) => {
                self.mpv.next()?;
                return Ok(());
            }
            (KeyCode::Char('p'), _) => {
                self.mpv.prev()?;
                return Ok(());
            }
            (KeyCode::Left, m) => {
                let secs = if m.contains(KeyModifiers::SHIFT) {
                    -30.0
                } else {
                    -5.0
                };
                self.mpv.seek_relative(secs)?;
                return Ok(());
            }
            (KeyCode::Right, m) => {
                let secs = if m.contains(KeyModifiers::SHIFT) {
                    30.0
                } else {
                    5.0
                };
                self.mpv.seek_relative(secs)?;
                return Ok(());
            }
            (KeyCode::Char('?'), _) => {
                self.show_help = !self.show_help;
                return Ok(());
            }
            (KeyCode::Esc, _) if self.show_help => {
                self.show_help = false;
                return Ok(());
            }
            (KeyCode::Esc, _) if self.show_tags => {
                self.show_tags = false;
                return Ok(());
            }
            (KeyCode::Esc, _) if self.show_details => {
                self.show_details = false;
                return Ok(());
            }
            (KeyCode::Char('S'), _) => {
                self.mpv.shuffle()?;
                self.status_msg = "shuffled queue".into();
                return Ok(());
            }
            (KeyCode::Char('R'), _) => {
                self.repeat = self.repeat.cycle();
                let (lp, lf) = match self.repeat {
                    RepeatMode::Off => ("no", "no"),
                    RepeatMode::All => ("inf", "no"),
                    RepeatMode::One => ("no", "inf"),
                };
                self.mpv.set_loop_playlist(lp)?;
                self.mpv.set_loop_file(lf)?;
                self.status_msg = self.repeat.status_label().into();
                return Ok(());
            }
            (KeyCode::Char('r'), KeyModifiers::CONTROL) => {
                self.manual_refresh();
                return Ok(());
            }
            _ => {}
        }

        match self.tab {
            Tab::Songs => self.handle_songs_key(key),
            Tab::Queue => self.handle_queue_key(key),
            Tab::Downloads => self.handle_downloads_key(key),
            Tab::Uploads => self.handle_uploads_key(key),
            Tab::Settings => self.handle_settings_key(key),
            Tab::Playlists => self.handle_playlists_key(key),
        }
    }
    pub(super) fn handle_text_input<F1, F2>(
        &mut self,
        key: KeyEvent,
        mut buf: String,
        wrap: F1,
        mut on_change: F2,
    ) where
        F1: Fn(String) -> Mode,
        F2: FnMut(&mut Self, String),
    {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                on_change(self, String::new());
            }
            KeyCode::Enter => {
                self.mode = Mode::Normal;
            }
            KeyCode::Backspace => {
                buf.pop();
                let q = buf.clone();
                self.mode = wrap(buf);
                on_change(self, q);
            }
            KeyCode::Char(c) => {
                buf.push(c);
                let q = buf.clone();
                self.mode = wrap(buf);
                on_change(self, q);
            }
            _ => {
                self.mode = wrap(buf);
            }
        }
    }
    pub(super) fn edit_buf<F: Fn(String) -> Mode>(
        &mut self,
        key: KeyEvent,
        mut buf: String,
        wrap: F,
    ) {
        match key.code {
            KeyCode::Backspace => {
                buf.pop();
                self.mode = wrap(buf);
            }
            KeyCode::Char(c) => {
                buf.push(c);
                self.mode = wrap(buf);
            }
            _ => self.mode = wrap(buf),
        }
    }
}
