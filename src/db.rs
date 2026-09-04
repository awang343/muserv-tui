use crate::api::{Library, Playlist, PlaylistTrack, Track};
use anyhow::{Context, Result};
use rusqlite::{params, Connection};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadStatus {
    Queued,
    Downloading,
    Downloaded,
    Failed,
}

impl DownloadStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            DownloadStatus::Queued => "queued",
            DownloadStatus::Downloading => "downloading",
            DownloadStatus::Downloaded => "downloaded",
            DownloadStatus::Failed => "failed",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "downloading" => DownloadStatus::Downloading,
            "downloaded" => DownloadStatus::Downloaded,
            "failed" => DownloadStatus::Failed,
            _ => DownloadStatus::Queued,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DownloadRow {
    pub library_id: i64,
    pub track_id: i64,
    pub hash: String,
    pub status: DownloadStatus,
    pub bytes_downloaded: i64,
    pub total_bytes: i64,
    pub local_path: Option<String>,
    pub error: Option<String>,
}

pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn open() -> Result<Self> {
        crate::storage::ensure_dirs().context("creating muserv cache dirs")?;
        let path = crate::storage::db_path();
        let conn = Connection::open(&path)
            .with_context(|| format!("opening sqlite db at {}", path.display()))?;
        let db = Self { conn };
        db.init_schema()?;
        Ok(db)
    }

    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS libraries (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                path TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS tracks (
                library_id INTEGER NOT NULL,
                id INTEGER NOT NULL,
                hash TEXT,
                original_filename TEXT,
                title TEXT,
                album TEXT,
                artist TEXT,
                album_artist TEXT,
                track_no INTEGER,
                disc_no INTEGER,
                duration_ms INTEGER,
                year INTEGER,
                bitrate INTEGER,
                sample_rate INTEGER,
                channels INTEGER,
                added_at INTEGER,
                PRIMARY KEY (library_id, id)
            );
            CREATE TABLE IF NOT EXISTS playlists (
                library_id INTEGER NOT NULL,
                id INTEGER NOT NULL,
                name TEXT,
                description TEXT,
                track_count INTEGER,
                created_at INTEGER,
                updated_at INTEGER,
                PRIMARY KEY (library_id, id)
            );
            CREATE TABLE IF NOT EXISTS playlist_tracks (
                library_id INTEGER NOT NULL,
                playlist_id INTEGER NOT NULL,
                track_id INTEGER NOT NULL,
                position INTEGER,
                added_at INTEGER,
                title TEXT,
                album TEXT,
                artist TEXT,
                album_artist TEXT,
                duration_ms INTEGER,
                PRIMARY KEY (library_id, playlist_id, track_id)
            );
            CREATE TABLE IF NOT EXISTS downloads (
                library_id INTEGER NOT NULL,
                track_id INTEGER NOT NULL,
                hash TEXT NOT NULL,
                status TEXT NOT NULL,
                bytes_downloaded INTEGER NOT NULL DEFAULT 0,
                total_bytes INTEGER NOT NULL DEFAULT 0,
                local_path TEXT,
                updated_at TEXT,
                error TEXT,
                PRIMARY KEY (library_id, track_id)
            );
            "#,
        )?;
        Ok(())
    }

    pub fn replace_libraries(&mut self, libraries: &[Library]) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM libraries", [])?;
        {
            let mut stmt =
                tx.prepare("INSERT INTO libraries (id, name, path) VALUES (?1, ?2, ?3)")?;
            for l in libraries {
                stmt.execute(params![l.id, l.name, l.path])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn libraries(&self) -> Result<Vec<Library>> {
        let mut stmt = self.conn.prepare("SELECT id, name, path FROM libraries")?;
        let rows = stmt
            .query_map([], |r| {
                Ok(Library {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    path: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn upsert_download(&self, row: &DownloadRow) -> Result<()> {
        self.conn.execute(
            "INSERT INTO downloads (library_id, track_id, hash, status, bytes_downloaded, total_bytes, local_path, updated_at, error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, datetime('now'), ?8)
             ON CONFLICT(library_id, track_id) DO UPDATE SET
                hash = excluded.hash,
                status = excluded.status,
                bytes_downloaded = excluded.bytes_downloaded,
                total_bytes = excluded.total_bytes,
                local_path = excluded.local_path,
                updated_at = excluded.updated_at,
                error = excluded.error",
            params![
                row.library_id,
                row.track_id,
                row.hash,
                row.status.as_str(),
                row.bytes_downloaded,
                row.total_bytes,
                row.local_path,
                row.error,
            ],
        )?;
        Ok(())
    }

    pub fn delete_download(&self, library_id: i64, track_id: i64) -> Result<()> {
        self.conn.execute(
            "DELETE FROM downloads WHERE library_id = ?1 AND track_id = ?2",
            params![library_id, track_id],
        )?;
        Ok(())
    }

    pub fn downloads_for_library(&self, library_id: i64) -> Result<Vec<DownloadRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT library_id, track_id, hash, status, bytes_downloaded, total_bytes, local_path, error
             FROM downloads WHERE library_id = ?1",
        )?;
        let rows = stmt
            .query_map(params![library_id], |r| {
                Ok(DownloadRow {
                    library_id: r.get(0)?,
                    track_id: r.get(1)?,
                    hash: r.get(2)?,
                    status: DownloadStatus::from_str(&r.get::<_, String>(3)?),
                    bytes_downloaded: r.get(4)?,
                    total_bytes: r.get(5)?,
                    local_path: r.get(6)?,
                    error: r.get(7)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn replace_tracks(&mut self, library_id: i64, tracks: &[Track]) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "DELETE FROM tracks WHERE library_id = ?1",
            params![library_id],
        )?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO tracks (library_id, id, hash, original_filename, title, album, artist, album_artist, track_no, disc_no, duration_ms, year, bitrate, sample_rate, channels, added_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            )?;
            for t in tracks {
                stmt.execute(params![
                    library_id,
                    t.id,
                    t.hash,
                    t.original_filename,
                    t.title,
                    t.album,
                    t.artist,
                    t.album_artist,
                    t.track_no,
                    t.disc_no,
                    t.duration_ms,
                    t.year,
                    t.bitrate,
                    t.sample_rate,
                    t.channels,
                    t.added_at,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn tracks_for_library(&self, library_id: i64) -> Result<Vec<Track>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, hash, original_filename, title, album, artist, album_artist, track_no, disc_no, duration_ms, year, bitrate, sample_rate, channels, added_at
             FROM tracks WHERE library_id = ?1",
        )?;
        let rows = stmt
            .query_map(params![library_id], |r| {
                Ok(Track {
                    id: r.get(0)?,
                    hash: r.get(1)?,
                    original_filename: r.get(2)?,
                    title: r.get(3)?,
                    album: r.get(4)?,
                    artist: r.get(5)?,
                    album_artist: r.get(6)?,
                    track_no: r.get(7)?,
                    disc_no: r.get(8)?,
                    duration_ms: r.get(9)?,
                    year: r.get(10)?,
                    bitrate: r.get(11)?,
                    sample_rate: r.get(12)?,
                    channels: r.get(13)?,
                    added_at: r.get(14)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn replace_playlists(&mut self, library_id: i64, playlists: &[Playlist]) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "DELETE FROM playlists WHERE library_id = ?1",
            params![library_id],
        )?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO playlists (library_id, id, name, description, track_count, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            for p in playlists {
                stmt.execute(params![
                    library_id,
                    p.id,
                    p.name,
                    p.description,
                    p.track_count,
                    p.created_at,
                    p.updated_at,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn playlists_for_library(&self, library_id: i64) -> Result<Vec<Playlist>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, description, track_count, created_at, updated_at
             FROM playlists WHERE library_id = ?1",
        )?;
        let rows = stmt
            .query_map(params![library_id], |r| {
                Ok(Playlist {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    description: r.get(2)?,
                    track_count: r.get(3)?,
                    created_at: r.get(4)?,
                    updated_at: r.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn replace_playlist_tracks(
        &mut self,
        library_id: i64,
        playlist_id: i64,
        tracks: &[PlaylistTrack],
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "DELETE FROM playlist_tracks WHERE library_id = ?1 AND playlist_id = ?2",
            params![library_id, playlist_id],
        )?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO playlist_tracks (library_id, playlist_id, track_id, position, added_at, title, album, artist, album_artist, duration_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            )?;
            for t in tracks {
                stmt.execute(params![
                    library_id,
                    playlist_id,
                    t.track_id,
                    t.position,
                    t.added_at,
                    t.title,
                    t.album,
                    t.artist,
                    t.album_artist,
                    t.duration_ms,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn playlist_tracks_for(
        &self,
        library_id: i64,
        playlist_id: i64,
    ) -> Result<Vec<PlaylistTrack>> {
        let mut stmt = self.conn.prepare(
            "SELECT track_id, position, added_at, title, album, artist, album_artist, duration_ms
             FROM playlist_tracks WHERE library_id = ?1 AND playlist_id = ?2 ORDER BY position",
        )?;
        let rows = stmt
            .query_map(params![library_id, playlist_id], |r| {
                Ok(PlaylistTrack {
                    track_id: r.get(0)?,
                    position: r.get(1)?,
                    added_at: r.get(2)?,
                    title: r.get(3)?,
                    album: r.get(4)?,
                    artist: r.get(5)?,
                    album_artist: r.get(6)?,
                    duration_ms: r.get(7)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}
