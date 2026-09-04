use super::*;
use std::sync::mpsc;
use std::thread;

pub(super) enum StartupSync {
    Libraries(Result<Vec<Library>, String>),
    Tracks {
        library_id: i64,
        result: Result<Vec<Track>, String>,
    },
    Playlists {
        library_id: i64,
        result: Result<Vec<Playlist>, String>,
    },
    PlaylistTracks {
        library_id: i64,
        playlist_id: i64,
        result: Result<Vec<PlaylistTrack>, String>,
    },
}

impl App {
    // Fetches libraries -> tracks -> playlists -> first playlist's tracks on a
    // background thread, streaming each stage back as soon as it lands so the
    // UI (already showing cached data) can update progressively instead of
    // blocking startup on a chain of network round-trips.
    pub(super) fn spawn_startup_sync(
        client: Client,
        preferred_library: String,
    ) -> mpsc::Receiver<StartupSync> {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let libraries_result = client.list_libraries().map_err(|e| e.to_string());
            let selected = libraries_result
                .as_ref()
                .ok()
                .and_then(|libs| {
                    libs.iter()
                        .find(|l| l.name == preferred_library)
                        .or_else(|| libs.first())
                })
                .cloned();
            if tx.send(StartupSync::Libraries(libraries_result)).is_err() {
                return;
            }
            let Some(lib) = selected else { return };

            let tracks_result = client.list_tracks(lib.id).map_err(|e| e.to_string());
            if tx
                .send(StartupSync::Tracks {
                    library_id: lib.id,
                    result: tracks_result,
                })
                .is_err()
            {
                return;
            }

            let playlists_result = client.list_playlists(lib.id).map_err(|e| e.to_string());
            let first_playlist = playlists_result
                .as_ref()
                .ok()
                .and_then(|pls| pls.first().cloned());
            if tx
                .send(StartupSync::Playlists {
                    library_id: lib.id,
                    result: playlists_result,
                })
                .is_err()
            {
                return;
            }
            if let Some(pl) = first_playlist {
                let pt_result = client
                    .get_playlist_tracks(lib.id, pl.id)
                    .map_err(|e| e.to_string());
                let _ = tx.send(StartupSync::PlaylistTracks {
                    library_id: lib.id,
                    playlist_id: pl.id,
                    result: pt_result,
                });
            }
        });
        rx
    }

    pub(super) fn drain_startup_sync(&mut self) {
        let Some(rx) = self.startup_rx.take() else {
            return;
        };
        let mut disconnected = false;
        loop {
            match rx.try_recv() {
                Ok(msg) => self.apply_startup_sync(msg),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }
        if !disconnected {
            self.startup_rx = Some(rx);
        }
    }

    fn apply_startup_sync(&mut self, msg: StartupSync) {
        match msg {
            StartupSync::Libraries(Ok(libs)) => {
                if self.library_id.is_none() {
                    let lib = libs
                        .iter()
                        .find(|l| l.name == self.settings.selected_library)
                        .or_else(|| libs.first())
                        .cloned();
                    if let Some(l) = lib {
                        self.library_id = Some(l.id);
                        self.settings.selected_library = l.name.clone();
                        let _ = self.settings.save();
                        self.saved_settings = self.settings.clone();
                        self.tracks = self.db.tracks_for_library(l.id).unwrap_or_default();
                        self.apply_filter("");
                        self.playlists = self.db.playlists_for_library(l.id).unwrap_or_default();
                        self.playlists_state
                            .select((!self.playlists.is_empty()).then_some(0));
                        self.reload_downloads_for_current_library();
                    }
                }
                self.libraries = libs;
                if let Err(e) = self.db.replace_libraries(&self.libraries) {
                    self.status_msg = format!("failed to cache libraries: {e}");
                }
            }
            StartupSync::Libraries(Err(e)) => {
                if self.libraries.is_empty() {
                    self.status_msg = format!("offline — no cached libraries ({e})");
                }
            }
            StartupSync::Tracks { library_id, result } => {
                if self.library_id != Some(library_id) {
                    return;
                }
                match result {
                    Ok(tracks) => {
                        if let Err(e) = self.db.replace_tracks(library_id, &tracks) {
                            self.status_msg = format!("failed to cache tracks: {e}");
                        }
                        self.tracks = tracks;
                        self.apply_filter("");
                        self.status_msg = format!("{} tracks", self.tracks.len());
                    }
                    Err(e) => {
                        if self.tracks.is_empty() {
                            self.status_msg = format!("offline — {e}");
                        }
                    }
                }
            }
            StartupSync::Playlists { library_id, result } => {
                if self.library_id != Some(library_id) {
                    return;
                }
                if let Ok(pls) = result {
                    if let Err(e) = self.db.replace_playlists(library_id, &pls) {
                        self.status_msg = format!("failed to cache playlists: {e}");
                    }
                    let cur = self
                        .selected_playlist_id()
                        .or_else(|| pls.first().map(|p| p.id));
                    self.playlists = pls;
                    let new_sel = cur
                        .and_then(|id| self.playlists.iter().position(|p| p.id == id))
                        .or((!self.playlists.is_empty()).then_some(0));
                    self.playlists_state.select(new_sel);
                }
            }
            StartupSync::PlaylistTracks {
                library_id,
                playlist_id,
                result,
            } => {
                if self.library_id != Some(library_id)
                    || self.selected_playlist_id() != Some(playlist_id)
                {
                    return;
                }
                if let Ok(tracks) = result {
                    if let Err(e) = self.db.replace_playlist_tracks(library_id, playlist_id, &tracks)
                    {
                        self.status_msg = format!("failed to cache playlist tracks: {e}");
                    }
                    self.playlist_tracks = tracks;
                    self.playlist_tracks_for = Some(playlist_id);
                    self.playlist_tracks_state
                        .select((!self.playlist_tracks.is_empty()).then_some(0));
                }
            }
        }
    }
}
