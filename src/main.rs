use anyhow::{Context, Result};
use clap::Parser;
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io;

mod api;
mod app;
mod db;
mod downloads;
mod mpv;
mod settings;
mod storage;

#[derive(Parser, Debug)]
#[command(name = "mutui", about = "TUI client for Muserv")]
struct Cli {
    /// Server base URL (overrides settings file).
    #[arg(short, long, env = "MUSIC_LIB_URL")]
    server: Option<String>,

    /// Username (overrides settings file).
    #[arg(short, long, env = "MUSIC_LIB_USERNAME")]
    username: Option<String>,

    /// Token (overrides settings file).
    #[arg(short, long, env = "MUSIC_LIB_TOKEN")]
    token: Option<String>,

    /// Library to select on startup (by name).
    #[arg(short, long, env = "MUSIC_LIB_LIBRARY")]
    library: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let mut s = settings::Settings::load();
    if let Some(url) = cli.server {
        s.server_url = url;
    }
    if let Some(username) = cli.username {
        s.username = username;
    }
    if let Some(tok) = cli.token {
        s.token = tok;
    }
    if let Some(lib) = cli.library {
        s.selected_library = lib;
    }
    if s.server_url.is_empty() {
        s.server_url = "http://127.0.0.1:7700".into();
    }

    let credentials = if s.username.is_empty() || s.token.is_empty() {
        None
    } else {
        Some((s.username.clone(), s.token.clone()))
    };
    let client = api::Client::new(s.server_url.clone(), credentials);

    let mut headers = Vec::new();
    if let Some(auth) = client.auth_header_value() {
        headers.push(format!("Authorization: {auth}"));
    }
    let mpv = mpv::Mpv::spawn(&headers).context("spawning mpv")?;

    // App::new shows cached libraries/tracks/playlists instantly and fetches
    // fresh data from the server on a background thread.
    let mut app = app::App::new(client, mpv, s).context("initializing app")?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = app.run(&mut terminal);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}
