use super::*;

impl App {
    pub(super) fn handle_settings_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                let idx = SettingsField::ALL
                    .iter()
                    .position(|f| *f == self.settings_field)
                    .unwrap_or(0);
                let next = (idx + 1).min(SettingsField::ALL.len() - 1);
                self.settings_field = SettingsField::ALL[next];
            }
            KeyCode::Char('k') | KeyCode::Up => {
                let idx = SettingsField::ALL
                    .iter()
                    .position(|f| *f == self.settings_field)
                    .unwrap_or(0);
                let next = idx.saturating_sub(1);
                self.settings_field = SettingsField::ALL[next];
            }
            KeyCode::Enter | KeyCode::Char('e') => match self.settings_field {
                SettingsField::ServerUrl => {
                    self.mode =
                        Mode::EditSetting(self.settings_field, self.settings.server_url.clone());
                }
                SettingsField::Username => {
                    self.mode =
                        Mode::EditSetting(self.settings_field, self.settings.username.clone());
                }
                SettingsField::Token => {
                    self.mode =
                        Mode::EditSetting(self.settings_field, self.settings.token.clone());
                }
                SettingsField::Library => self.open_library_picker(),
            },
            KeyCode::Char('s') => self.save_and_apply_settings()?,
            KeyCode::Char('r') | KeyCode::Esc => {
                self.settings = self.saved_settings.clone();
                self.status_msg = "settings reverted".into();
            }
            _ => {}
        }
        Ok(())
    }
    pub(super) fn save_and_apply_settings(&mut self) -> Result<()> {
        if let Err(e) = self.settings.save() {
            self.status_msg = format!("save failed: {e}");
            return Ok(());
        }
        self.saved_settings = self.settings.clone();

        let credentials = if self.settings.username.is_empty() || self.settings.token.is_empty() {
            None
        } else {
            Some((self.settings.username.clone(), self.settings.token.clone()))
        };
        self.client = api::Client::new(self.settings.server_url.clone(), credentials);

        let mut headers = Vec::new();
        if let Some(auth) = self.client.auth_header_value() {
            headers.push(format!("Authorization: {auth}"));
        }
        let _ = self.mpv.set_http_headers(&headers);

        // Re-fetch libraries against the (possibly new) server, then load tracks.
        match self.client.list_libraries() {
            Ok(libs) => {
                self.libraries = libs;
                let prev_name = self.settings.selected_library.clone();
                let pick = self
                    .libraries
                    .iter()
                    .find(|l| l.name == prev_name)
                    .or_else(|| self.libraries.first())
                    .cloned();
                self.library_id = pick.as_ref().map(|l| l.id);
                if let Some(ref l) = pick {
                    self.settings.selected_library = l.name.clone();
                    self.saved_settings.selected_library = l.name.clone();
                }
            }
            Err(e) => {
                self.libraries.clear();
                self.library_id = None;
                self.status_msg = format!("saved, but libraries fetch failed: {e}");
                return Ok(());
            }
        }

        let Some(lib) = self.library_id() else {
            self.tracks.clear();
            self.filtered.clear();
            self.list_state.select(None);
            self.current_tags.clear();
            self.current_tags_for = None;
            self.status_msg = "saved — server has no libraries".into();
            return Ok(());
        };

        match self.client.list_tracks(lib) {
            Ok(t) => {
                self.tracks = t;
                self.filtered = (0..self.tracks.len()).collect();
                self.sort_filtered();
                self.list_state.select(if self.filtered.is_empty() {
                    None
                } else {
                    Some(0)
                });
                self.current_tags.clear();
                self.current_tags_for = None;
                self.refresh_playlists();
                self.status_msg = format!("saved & reloaded ({} tracks)", self.tracks.len());
            }
            Err(e) => {
                self.tracks.clear();
                self.filtered.clear();
                self.list_state.select(None);
                self.current_tags.clear();
                self.current_tags_for = None;
                self.status_msg = format!("saved, but connect failed: {e}");
            }
        }
        Ok(())
    }
}
