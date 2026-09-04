use crate::api::{self, Client, Library, Playlist, PlaylistTrack, Track, TrackTag};
use crate::db::{Db, DownloadRow, DownloadStatus};
use crate::downloads::{DownloadEvent, DownloadJob, DownloadManager};
use crate::mpv::Mpv;
use crate::settings::Settings;
use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
struct DownloadState {
    status: DownloadStatus,
    bytes: i64,
    total: i64,
    local_path: Option<String>,
    #[allow(dead_code)]
    error: Option<String>,
}

impl From<DownloadRow> for DownloadState {
    fn from(row: DownloadRow) -> Self {
        Self {
            status: row.status,
            bytes: row.bytes_downloaded,
            total: row.total_bytes,
            local_path: row.local_path,
            error: row.error,
        }
    }
}

#[derive(Debug, Clone)]
enum Mode {
    Normal,
    Filter(String),
    TagSearch(String),
    AddTag(String),
    EditSetting(SettingsField, String),
    NewPlaylist(String),
    RenamePlaylist(i64, String),
    PickPlaylist {
        index: usize,
        track_id: i64,
        containing: Vec<i64>,
    },
    PickLibrary {
        index: usize,
    },
    SortPicker {
        index: usize,
    },
    DownloaderList {
        index: usize,
    },
    DownloaderInput {
        script: String,
        buf: String,
    },
    DownloaderJob,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsField {
    ServerUrl,
    AuthToken,
    Library,
}

impl SettingsField {
    const ALL: [SettingsField; 3] = [
        SettingsField::ServerUrl,
        SettingsField::AuthToken,
        SettingsField::Library,
    ];
    fn label(&self) -> &'static str {
        match self {
            SettingsField::ServerUrl => "Server URL",
            SettingsField::AuthToken => "Auth Token",
            SettingsField::Library => "Library",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlaylistsFocus {
    List,
    Tracks,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RepeatMode {
    Off,
    All,
    One,
}

impl RepeatMode {
    fn cycle(self) -> Self {
        match self {
            RepeatMode::Off => RepeatMode::All,
            RepeatMode::All => RepeatMode::One,
            RepeatMode::One => RepeatMode::Off,
        }
    }
    fn label(self) -> &'static str {
        match self {
            RepeatMode::Off => "",
            RepeatMode::All => "↻",
            RepeatMode::One => "↻¹",
        }
    }
    fn status_label(self) -> &'static str {
        match self {
            RepeatMode::Off => "repeat off",
            RepeatMode::All => "repeat all",
            RepeatMode::One => "repeat one",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortKey {
    Default,
    Title,
    Artist,
    Album,
    Duration,
    Year,
    AddedAt,
}

impl SortKey {
    const ALL: [SortKey; 7] = [
        SortKey::Default,
        SortKey::Title,
        SortKey::Artist,
        SortKey::Album,
        SortKey::Duration,
        SortKey::Year,
        SortKey::AddedAt,
    ];

    fn label(self) -> &'static str {
        match self {
            SortKey::Default => "Default (artist / title)",
            SortKey::Title => "Title (A→Z)",
            SortKey::Artist => "Artist (A→Z)",
            SortKey::Album => "Album (A→Z)",
            SortKey::Duration => "Duration (shortest first)",
            SortKey::Year => "Year (newest first)",
            SortKey::AddedAt => "Date added (newest first)",
        }
    }

    fn short_label(self) -> &'static str {
        match self {
            SortKey::Default => "default",
            SortKey::Title => "title",
            SortKey::Artist => "artist",
            SortKey::Album => "album",
            SortKey::Duration => "duration",
            SortKey::Year => "year",
            SortKey::AddedAt => "added",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Songs,
    Playlists,
    Queue,
    Downloads,
    Settings,
}

impl Tab {
    const ALL: [Tab; 5] = [
        Tab::Songs,
        Tab::Playlists,
        Tab::Queue,
        Tab::Downloads,
        Tab::Settings,
    ];

    fn label(&self) -> &'static str {
        match self {
            Tab::Songs => "Songs",
            Tab::Playlists => "Playlists",
            Tab::Queue => "Queue",
            Tab::Downloads => "Downloads",
            Tab::Settings => "Settings",
        }
    }

    fn from_digit(c: char) -> Option<Tab> {
        match c {
            '1' => Some(Tab::Songs),
            '2' => Some(Tab::Playlists),
            '3' => Some(Tab::Queue),
            '4' => Some(Tab::Downloads),
            '5' => Some(Tab::Settings),
            _ => None,
        }
    }
}

pub struct App {
    client: Client,
    mpv: Mpv,
    tracks: Vec<Track>,
    filtered: Vec<usize>,
    list_state: ListState,
    tags_state: ListState,
    queue_state: ListState,
    downloads_state: ListState,
    tab: Tab,
    mode: Mode,
    settings: Settings,
    saved_settings: Settings,
    settings_field: SettingsField,
    current_tags: Vec<TrackTag>,
    playlists: Vec<Playlist>,
    playlists_state: ListState,
    playlists_focus: PlaylistsFocus,
    playlist_tracks: Vec<PlaylistTrack>,
    playlist_tracks_state: ListState,
    playlist_tracks_for: Option<i64>,
    current_tags_for: Option<i64>,
    libraries: Vec<Library>,
    library_id: Option<i64>,
    repeat: RepeatMode,
    sort_key: SortKey,
    status_msg: String,
    show_help: bool,
    show_tags: bool,
    show_details: bool,
    should_quit: bool,
    downloader_scripts: Vec<api::DownloaderInfo>,
    downloader_script_name: Option<String>,
    downloader_job_id: Option<String>,
    downloader_job: Option<api::DownloaderJob>,
    downloader_polling: bool,
    last_downloader_poll: Instant,
    db: Db,
    downloads: HashMap<i64, DownloadState>,
    download_mgr: DownloadManager,
}

mod data;
mod downloads;
mod input;
mod playback;
mod playlists;
mod render;
mod settings;
mod tags;
mod tracks;

impl App {
    pub fn new(
        client: Client,
        mpv: Mpv,
        tracks: Vec<Track>,
        tracks_fetch_failed: bool,
        settings: Settings,
        libraries: Vec<Library>,
        library_id: Option<i64>,
    ) -> Result<Self> {
        let db = Db::open().context("opening local download database")?;
        let download_mgr = DownloadManager::new(client.agent());
        let filtered: Vec<usize> = (0..tracks.len()).collect();
        let mut list_state = ListState::default();
        if !filtered.is_empty() {
            list_state.select(Some(0));
        }
        let mut app = Self {
            client,
            mpv,
            tracks,
            filtered,
            list_state,
            tags_state: ListState::default(),
            queue_state: ListState::default(),
            downloads_state: ListState::default(),
            tab: Tab::Songs,
            mode: Mode::Normal,
            saved_settings: settings.clone(),
            settings,
            settings_field: SettingsField::ServerUrl,
            current_tags: Vec::new(),
            current_tags_for: None,
            playlists: Vec::new(),
            playlists_state: ListState::default(),
            playlists_focus: PlaylistsFocus::List,
            playlist_tracks: Vec::new(),
            playlist_tracks_state: ListState::default(),
            playlist_tracks_for: None,
            libraries,
            library_id,
            repeat: RepeatMode::Off,
            sort_key: SortKey::Default,
            status_msg: String::new(),
            show_help: false,
            show_tags: false,
            show_details: false,
            should_quit: false,
            downloader_scripts: Vec::new(),
            downloader_script_name: None,
            downloader_job_id: None,
            downloader_job: None,
            downloader_polling: false,
            last_downloader_poll: Instant::now(),
            db,
            downloads: HashMap::new(),
            download_mgr,
        };
        app.sort_filtered();
        app.sync_initial_tracks_cache(tracks_fetch_failed);
        app.refresh_playlists();
        app.reload_downloads_for_current_library();
        Ok(app)
    }

    pub fn run<B>(&mut self, terminal: &mut ratatui::Terminal<B>) -> Result<()>
    where
        B: ratatui::backend::Backend,
        <B as ratatui::backend::Backend>::Error: std::error::Error + Send + Sync + 'static,
    {
        while !self.should_quit {
            terminal.draw(|f| self.render(f))?;
            if event::poll(Duration::from_millis(200))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        if let Err(e) = self.handle_key(key) {
                            self.status_msg = format!("{e:#}");
                        }
                    }
                }
            }
            self.poll_downloader_if_due();
            self.drain_download_events();
        }
        Ok(())
    }
}
