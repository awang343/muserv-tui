use std::path::PathBuf;

pub fn cache_dir() -> PathBuf {
    let base = std::env::var("XDG_CACHE_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home).join(".cache")
        });
    base.join("muserv")
}

pub fn db_path() -> PathBuf {
    cache_dir().join("metadata.db")
}

pub fn tracks_dir() -> PathBuf {
    cache_dir().join("tracks")
}

pub fn tmp_dir() -> PathBuf {
    tracks_dir().join("tmp")
}

pub fn ensure_dirs() -> std::io::Result<()> {
    std::fs::create_dir_all(tmp_dir())
}
