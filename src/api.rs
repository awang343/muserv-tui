use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use ureq::http::Response;
use ureq::typestate::{WithBody, WithoutBody};
use ureq::{Body, RequestBuilder};

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Library {
    pub id: i64,
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Track {
    pub id: i64,
    #[serde(default)]
    pub hash: Option<String>,
    #[serde(default)]
    pub original_filename: Option<String>,
    pub title: Option<String>,
    pub album: Option<String>,
    pub artist: Option<String>,
    pub album_artist: Option<String>,
    pub track_no: Option<i64>,
    pub disc_no: Option<i64>,
    pub duration_ms: Option<i64>,
    pub year: Option<i64>,
    #[serde(default)]
    pub bitrate: Option<i64>,
    #[serde(default)]
    pub sample_rate: Option<i64>,
    #[serde(default)]
    pub channels: Option<i64>,
    #[serde(default)]
    pub added_at: i64,
}

impl Track {
    pub fn display_title(&self) -> &str {
        self.title
            .as_deref()
            .or(self.original_filename.as_deref())
            .unwrap_or("(untitled)")
    }
    pub fn display_artist(&self) -> &str {
        self.artist
            .as_deref()
            .or(self.album_artist.as_deref())
            .unwrap_or("—")
    }
    pub fn display_album(&self) -> &str {
        self.album.as_deref().unwrap_or("—")
    }
}

#[derive(Clone)]
pub struct Client {
    base: String,
    token: Option<String>,
    agent: ureq::Agent,
}

impl Client {
    pub fn new(base: String, token: Option<String>) -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(10)))
            .http_status_as_error(false)
            .build();
        let agent: ureq::Agent = config.into();
        Self {
            base: base.trim_end_matches('/').to_string(),
            token,
            agent,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    fn auth_header(&self) -> Option<String> {
        self.token.as_ref().map(|t| format!("Bearer {t}"))
    }

    pub fn agent(&self) -> ureq::Agent {
        self.agent.clone()
    }

    pub fn auth_header_value(&self) -> Option<String> {
        self.auth_header()
    }

    fn get(&self, path: &str) -> RequestBuilder<WithoutBody> {
        let mut r = self.agent.get(self.url(path));
        if let Some(a) = self.auth_header() {
            r = r.header("Authorization", a);
        }
        r
    }

    fn delete(&self, path: &str) -> RequestBuilder<WithoutBody> {
        let mut r = self.agent.delete(self.url(path));
        if let Some(a) = self.auth_header() {
            r = r.header("Authorization", a);
        }
        r
    }

    fn post(&self, path: &str) -> RequestBuilder<WithBody> {
        let mut r = self.agent.post(self.url(path));
        if let Some(a) = self.auth_header() {
            r = r.header("Authorization", a);
        }
        r
    }

    fn put(&self, path: &str) -> RequestBuilder<WithBody> {
        let mut r = self.agent.put(self.url(path));
        if let Some(a) = self.auth_header() {
            r = r.header("Authorization", a);
        }
        r
    }

    fn patch(&self, path: &str) -> RequestBuilder<WithBody> {
        let mut r = self.agent.patch(self.url(path));
        if let Some(a) = self.auth_header() {
            r = r.header("Authorization", a);
        }
        r
    }

    pub fn list_libraries(&self) -> Result<Vec<Library>> {
        let resp = self
            .get("/api/libraries")
            .call()
            .context("GET /api/libraries")?;
        decode_json(resp, "decode libraries")
    }

    pub fn search(&self, library_id: i64, query: &str) -> Result<Vec<Track>> {
        let encoded = percent_encode(query);
        let resp = self
            .get(&format!("/api/libraries/{library_id}/search?q={encoded}"))
            .call()
            .context("GET search")?;
        decode_json(resp, "decode search")
    }

    pub fn list_tracks(&self, library_id: i64) -> Result<Vec<Track>> {
        let mut out = Vec::new();
        let limit = 1000i64;
        let mut offset = 0i64;
        loop {
            let resp = self
                .get(&format!(
                    "/api/libraries/{library_id}/tracks?limit={limit}&offset={offset}"
                ))
                .call()
                .context("GET tracks")?;
            let chunk: Vec<Track> = decode_json(resp, "decode tracks")?;
            let n = chunk.len() as i64;
            out.extend(chunk);
            if n < limit {
                break;
            }
            offset += n;
        }
        Ok(out)
    }

    pub fn stream_url(&self, library_id: i64, track_id: i64) -> String {
        format!(
            "{}/api/libraries/{}/tracks/{}/stream",
            self.base, library_id, track_id
        )
    }

    pub fn list_track_tags(&self, library_id: i64, track_id: i64) -> Result<Vec<TrackTag>> {
        let resp = self
            .get(&format!(
                "/api/libraries/{library_id}/tracks/{track_id}/tags"
            ))
            .call()
            .context("GET tags")?;
        decode_json(resp, "decode tags")
    }

    pub fn add_user_tag(
        &self,
        library_id: i64,
        track_id: i64,
        namespace: &str,
        value: &str,
    ) -> Result<AddedTag> {
        #[derive(Serialize)]
        struct Body<'a> {
            namespace: &'a str,
            value: &'a str,
        }
        let resp = self
            .post(&format!(
                "/api/libraries/{library_id}/tracks/{track_id}/tags"
            ))
            .send_json(Body { namespace, value })
            .context("POST tag")?;
        decode_json(resp, "decode added tag")
    }

    pub fn remove_user_tag(&self, library_id: i64, track_id: i64, tag_id: i64) -> Result<()> {
        let resp = self
            .delete(&format!(
                "/api/libraries/{library_id}/tracks/{track_id}/tags/{tag_id}"
            ))
            .call()
            .context("DELETE tag")?;
        ensure_ok(resp)
    }

    pub fn list_playlists(&self, library_id: i64) -> Result<Vec<Playlist>> {
        let resp = self
            .get(&format!("/api/libraries/{library_id}/playlists"))
            .call()
            .context("GET playlists")?;
        decode_json(resp, "decode playlists")
    }

    pub fn create_playlist(&self, library_id: i64, name: &str) -> Result<Playlist> {
        #[derive(Serialize)]
        struct Body<'a> {
            name: &'a str,
        }
        let resp = self
            .post(&format!("/api/libraries/{library_id}/playlists"))
            .send_json(Body { name })
            .context("POST playlist")?;
        decode_json(resp, "decode playlist")
    }

    pub fn rename_playlist(&self, library_id: i64, id: i64, name: &str) -> Result<Playlist> {
        #[derive(Serialize)]
        struct Body<'a> {
            name: &'a str,
        }
        let resp = self
            .patch(&format!("/api/libraries/{library_id}/playlists/{id}"))
            .send_json(Body { name })
            .context("PATCH playlist")?;
        decode_json(resp, "decode playlist")
    }

    pub fn delete_playlist(&self, library_id: i64, id: i64) -> Result<()> {
        let resp = self
            .delete(&format!("/api/libraries/{library_id}/playlists/{id}"))
            .call()
            .context("DELETE playlist")?;
        ensure_ok(resp)
    }

    pub fn get_playlist_tracks(&self, library_id: i64, id: i64) -> Result<Vec<PlaylistTrack>> {
        let resp = self
            .get(&format!(
                "/api/libraries/{library_id}/playlists/{id}/tracks"
            ))
            .call()
            .context("GET playlist tracks")?;
        decode_json(resp, "decode playlist tracks")
    }

    pub fn playlists_containing_track(&self, library_id: i64, track_id: i64) -> Result<Vec<i64>> {
        let resp = self
            .get(&format!(
                "/api/libraries/{library_id}/tracks/{track_id}/playlists"
            ))
            .call()
            .context("GET track playlists")?;
        decode_json(resp, "decode track playlists")
    }

    pub fn add_to_playlist(&self, library_id: i64, playlist_id: i64, track_id: i64) -> Result<()> {
        #[derive(Serialize)]
        struct Body {
            track_id: i64,
        }
        let resp = self
            .post(&format!(
                "/api/libraries/{library_id}/playlists/{playlist_id}/tracks"
            ))
            .send_json(Body { track_id })
            .context("POST playlist track")?;
        ensure_ok(resp)
    }

    pub fn set_playlist_tracks(
        &self,
        library_id: i64,
        playlist_id: i64,
        track_ids: &[i64],
    ) -> Result<()> {
        #[derive(Serialize)]
        struct Body<'a> {
            track_ids: &'a [i64],
        }
        let resp = self
            .put(&format!(
                "/api/libraries/{library_id}/playlists/{playlist_id}/tracks"
            ))
            .send_json(Body { track_ids })
            .context("PUT playlist tracks")?;
        ensure_ok(resp)
    }

    pub fn remove_from_playlist(
        &self,
        library_id: i64,
        playlist_id: i64,
        track_id: i64,
    ) -> Result<()> {
        let resp = self
            .delete(&format!(
                "/api/libraries/{library_id}/playlists/{playlist_id}/tracks/{track_id}"
            ))
            .call()
            .context("DELETE playlist track")?;
        ensure_ok(resp)
    }

    pub fn list_downloaders(&self, library_id: i64) -> Result<Vec<DownloaderInfo>> {
        let resp = self
            .get(&format!("/api/libraries/{library_id}/downloaders"))
            .call()
            .context("GET downloaders")?;
        decode_json(resp, "decode downloaders")
    }

    pub fn run_downloader(&self, library_id: i64, name: &str, urls: &[String]) -> Result<String> {
        #[derive(Serialize)]
        struct Body<'a> {
            urls: &'a [String],
        }
        #[derive(Deserialize)]
        struct RunResponse {
            job_id: String,
        }
        let resp = self
            .post(&format!(
                "/api/libraries/{library_id}/downloaders/{name}/run"
            ))
            .send_json(Body { urls })
            .context("POST downloader run")?;
        let r: RunResponse = decode_json(resp, "decode run response")?;
        Ok(r.job_id)
    }

    pub fn downloader_job_status(&self, library_id: i64, job_id: &str) -> Result<DownloaderJob> {
        let resp = self
            .get(&format!(
                "/api/libraries/{library_id}/downloaders/jobs/{job_id}"
            ))
            .call()
            .context("GET downloader job")?;
        decode_json(resp, "decode downloader job")
    }
}

fn decode_json<T: serde::de::DeserializeOwned>(
    mut resp: Response<Body>,
    ctx: &'static str,
) -> Result<T> {
    let status = resp.status();
    if !status.is_success() {
        let msg = resp.body_mut().read_to_string().unwrap_or_default();
        anyhow::bail!("status {}: {msg}", status.as_u16());
    }
    resp.body_mut().read_json::<T>().context(ctx)
}

fn ensure_ok(mut resp: Response<Body>) -> Result<()> {
    let status = resp.status();
    if !status.is_success() {
        let msg = resp.body_mut().read_to_string().unwrap_or_default();
        anyhow::bail!("status {}: {msg}", status.as_u16());
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImportStats {
    pub scanned: u64,
    pub imported: u64,
    pub duplicates: u64,
    pub failed: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DownloaderInfo {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct DownloaderJob {
    pub id: String,
    pub script: String,
    pub urls: Vec<String>,
    pub current_index: Option<usize>,
    pub status: String,
    pub log: Vec<String>,
    pub summary: Option<ImportStats>,
    pub started_at: String,
    pub finished_at: Option<String>,
}

impl DownloaderJob {
    pub fn is_done(&self) -> bool {
        self.status == "completed" || self.status == "failed"
    }
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Playlist {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub track_count: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct PlaylistTrack {
    pub track_id: i64,
    pub position: i64,
    pub added_at: i64,
    pub title: Option<String>,
    pub album: Option<String>,
    pub artist: Option<String>,
    pub album_artist: Option<String>,
    pub duration_ms: Option<i64>,
}

impl PlaylistTrack {
    pub fn display_title(&self) -> &str {
        self.title.as_deref().unwrap_or("(untitled)")
    }
    pub fn display_artist(&self) -> &str {
        self.artist
            .as_deref()
            .or(self.album_artist.as_deref())
            .unwrap_or("—")
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct TrackTag {
    pub tag_id: i64,
    pub namespace: String,
    pub value: String,
}

impl TrackTag {
    pub fn display(&self) -> String {
        if self.namespace.is_empty() {
            format!(":{}", self.value)
        } else {
            format!("{}:{}", self.namespace, self.value)
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct AddedTag {
    pub tag_id: i64,
    pub namespace: String,
    pub value: String,
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
