use super::tags::track_id_from_url;
use super::*;

impl App {
    pub(super) fn sync_initial_tracks_cache(&mut self, fetch_failed: bool) {
        let Some(lib) = self.library_id else { return };
        if fetch_failed {
            match self.db.tracks_for_library(lib) {
                Ok(cached) if !cached.is_empty() => {
                    self.tracks = cached;
                    self.apply_filter("");
                    self.status_msg = "offline — showing cached library".into();
                }
                Ok(_) => {}
                Err(e) => {
                    self.status_msg = format!("offline — cache read failed: {e}");
                }
            }
        } else if let Err(e) = self.db.replace_tracks(lib, &self.tracks) {
            self.status_msg = format!("failed to cache tracks: {e}");
        }
    }
    pub(super) fn reload_downloads_for_current_library(&mut self) {
        self.downloads.clear();
        let Some(lib) = self.library_id else { return };
        match self.db.downloads_for_library(lib) {
            Ok(rows) => {
                for row in rows {
                    self.downloads
                        .insert(row.track_id, DownloadState::from(row));
                }
            }
            Err(e) => {
                self.status_msg = format!("failed to load downloads: {e}");
            }
        }
    }
    pub(super) fn refresh_libraries_list(&mut self) {
        match self.client.list_libraries() {
            Ok(libs) => {
                let prev_id = self.library_id;
                self.libraries = libs;
                // Keep current selection if still present; otherwise pick first.
                let new = prev_id
                    .filter(|id| self.libraries.iter().any(|l| l.id == *id))
                    .or_else(|| self.libraries.first().map(|l| l.id));
                if let Some(id) = new {
                    if prev_id != Some(id) {
                        self.switch_library(id);
                    } else {
                        // Even on the same id, name may have changed; sync settings.
                        if let Some(n) = self.current_library_name() {
                            self.settings.selected_library = n;
                        }
                    }
                } else {
                    self.library_id = None;
                    self.tracks.clear();
                    self.apply_filter("");
                }
            }
            Err(e) => {
                self.status_msg = format!("libraries: {e}");
            }
        }
    }
    pub(super) fn refresh_playlists(&mut self) {
        let Some(lib) = self.library_id() else {
            self.playlists.clear();
            self.playlists_state.select(None);
            self.playlist_tracks.clear();
            self.playlist_tracks_state.select(None);
            self.playlist_tracks_for = None;
            return;
        };
        match self.client.list_playlists(lib) {
            Ok(pls) => {
                if let Err(e) = self.db.replace_playlists(lib, &pls) {
                    self.status_msg = format!("failed to cache playlists: {e}");
                }
                let cur = self
                    .selected_playlist_id()
                    .or_else(|| pls.first().map(|p| p.id));
                self.playlists = pls;
                let new_sel = cur
                    .and_then(|id| self.playlists.iter().position(|p| p.id == id))
                    .or({
                        if self.playlists.is_empty() {
                            None
                        } else {
                            Some(0)
                        }
                    });
                self.playlists_state.select(new_sel);
                self.refresh_playlist_tracks();
            }
            Err(e) => match self.db.playlists_for_library(lib) {
                Ok(cached) if !cached.is_empty() => {
                    let cur = self
                        .selected_playlist_id()
                        .or_else(|| cached.first().map(|p| p.id));
                    self.playlists = cached;
                    let new_sel = cur
                        .and_then(|id| self.playlists.iter().position(|p| p.id == id))
                        .or(if self.playlists.is_empty() {
                            None
                        } else {
                            Some(0)
                        });
                    self.playlists_state.select(new_sel);
                    self.status_msg = "offline — showing cached playlists".into();
                    self.refresh_playlist_tracks();
                }
                _ => self.status_msg = format!("playlists: {e}"),
            },
        }
    }
    pub(super) fn refresh_playlist_tracks(&mut self) {
        let Some(id) = self.selected_playlist_id() else {
            self.playlist_tracks.clear();
            self.playlist_tracks_for = None;
            self.playlist_tracks_state.select(None);
            return;
        };
        let Some(lib) = self.library_id() else {
            return;
        };
        match self.client.get_playlist_tracks(lib, id) {
            Ok(tracks) => {
                if let Err(e) = self.db.replace_playlist_tracks(lib, id, &tracks) {
                    self.status_msg = format!("failed to cache playlist tracks: {e}");
                }
                self.playlist_tracks = tracks;
                self.playlist_tracks_for = Some(id);
                let sel = if self.playlist_tracks.is_empty() {
                    None
                } else {
                    Some(0)
                };
                self.playlist_tracks_state.select(sel);
            }
            Err(e) => match self.db.playlist_tracks_for(lib, id) {
                Ok(cached) if !cached.is_empty() => {
                    self.playlist_tracks = cached;
                    self.playlist_tracks_for = Some(id);
                    self.playlist_tracks_state.select(Some(0));
                    self.status_msg = "offline — showing cached playlist tracks".into();
                }
                _ => {
                    self.playlist_tracks.clear();
                    self.playlist_tracks_for = Some(id);
                    self.playlist_tracks_state.select(None);
                    self.status_msg = format!("playlist tracks: {e}");
                }
            },
        }
    }
    pub(super) fn manual_refresh(&mut self) {
        let Some(lib) = self.library_id() else {
            self.status_msg = "no library selected".into();
            return;
        };
        match self.client.list_tracks(lib) {
            Ok(tracks) => {
                if let Err(e) = self.db.replace_tracks(lib, &tracks) {
                    self.status_msg = format!("failed to cache tracks: {e}");
                }
                self.tracks = tracks;
                self.apply_filter("");
                self.status_msg = format!("refreshed: {} tracks", self.tracks.len());
            }
            Err(e) => {
                self.status_msg = format!("refresh failed (offline?): {e}");
            }
        }
        self.refresh_playlists();
    }
    pub(super) fn refresh_tags(&mut self) {
        let Some(t) = self.selected_track().map(|t| t.id) else {
            self.current_tags.clear();
            self.current_tags_for = None;
            return;
        };
        if self.current_tags_for == Some(t) {
            return;
        }
        let Some(lib) = self.library_id() else {
            self.current_tags.clear();
            self.current_tags_for = Some(t);
            return;
        };
        match self.client.list_track_tags(lib, t) {
            Ok(tags) => {
                self.current_tags = tags;
                self.current_tags_for = Some(t);
            }
            Err(e) => {
                self.status_msg = format!("tag fetch failed: {e}");
                self.current_tags.clear();
                self.current_tags_for = Some(t);
            }
        }
        self.clamp_tag_selection();
    }
    pub(super) fn clamp_tag_selection(&mut self) {
        let len = self.current_tags.len();
        let new = match (self.tags_state.selected(), len) {
            (_, 0) => None,
            (None, _) => Some(0),
            (Some(i), n) if i >= n => Some(n - 1),
            (Some(i), _) => Some(i),
        };
        self.tags_state.select(new);
    }
    pub(super) fn now_playing_id(&self, path: &str) -> Option<i64> {
        if let Some(id) = track_id_from_url(path) {
            return Some(id);
        }
        let stripped = path.strip_prefix("file://").unwrap_or(path);
        self.downloads
            .iter()
            .find(|(_, st)| st.local_path.as_deref() == Some(stripped))
            .map(|(id, _)| *id)
    }
    pub(super) fn library_id(&self) -> Option<i64> {
        self.library_id
    }
    pub(super) fn current_library_name(&self) -> Option<String> {
        let id = self.library_id?;
        self.libraries
            .iter()
            .find(|l| l.id == id)
            .map(|l| l.name.clone())
    }
    pub(super) fn switch_library(&mut self, new_id: i64) {
        if self.library_id == Some(new_id) {
            return;
        }
        self.library_id = Some(new_id);
        let name = self
            .libraries
            .iter()
            .find(|l| l.id == new_id)
            .map(|l| l.name.clone())
            .unwrap_or_default();
        self.settings.selected_library = name.clone();
        let _ = self.settings.save();
        self.saved_settings = self.settings.clone();
        match self.client.list_tracks(new_id) {
            Ok(t) => {
                if let Err(e) = self.db.replace_tracks(new_id, &t) {
                    self.status_msg = format!("failed to cache tracks: {e}");
                }
                self.tracks = t;
                self.status_msg = format!("library: {name} ({} tracks)", self.tracks.len());
            }
            Err(e) => match self.db.tracks_for_library(new_id) {
                Ok(cached) if !cached.is_empty() => {
                    self.tracks = cached;
                    self.status_msg = format!("offline — showing cached {name}");
                }
                _ => {
                    self.tracks.clear();
                    self.status_msg = format!("library {name}: {e}");
                }
            },
        }
        self.apply_filter("");
        self.current_tags.clear();
        self.current_tags_for = None;
        self.playlist_tracks.clear();
        self.playlist_tracks_for = None;
        self.playlist_tracks_state.select(None);
        self.playlists_state.select(None);
        self.refresh_playlists();
        self.reload_downloads_for_current_library();
    }
    pub(super) fn open_library_picker(&mut self) {
        // Always re-fetch first so the picker reflects current server state.
        self.refresh_libraries_list();
        if self.libraries.is_empty() {
            self.status_msg = "no libraries on this server".into();
            return;
        }
        let cur = self
            .library_id
            .and_then(|id| self.libraries.iter().position(|l| l.id == id))
            .unwrap_or(0);
        self.mode = Mode::PickLibrary { index: cur };
    }
}
