use super::*;

impl App {
    pub(super) fn open_tags_popup(&mut self) {
        if self.selected_track().is_none() {
            self.status_msg = "no track selected".into();
            return;
        }
        self.refresh_tags();
        self.show_tags = true;
    }
    pub(super) fn open_details_popup(&mut self) {
        if self.selected_track().is_none() {
            self.status_msg = "no track selected".into();
            return;
        }
        self.show_details = true;
    }
    pub(super) fn handle_tags_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.move_tag_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_tag_selection(-1),
            KeyCode::Char('g') | KeyCode::Home => {
                if !self.current_tags.is_empty() {
                    self.tags_state.select(Some(0));
                }
            }
            KeyCode::Char('G') | KeyCode::End => {
                if !self.current_tags.is_empty() {
                    self.tags_state.select(Some(self.current_tags.len() - 1));
                }
            }
            KeyCode::Char('a') => self.mode = Mode::AddTag(String::new()),
            KeyCode::Char('d') => self.delete_selected_tag()?,
            KeyCode::Char('t') => self.show_tags = false,
            _ => {}
        }
        Ok(())
    }
    pub(super) fn move_tag_selection(&mut self, delta: i32) {
        if self.current_tags.is_empty() {
            return;
        }
        let cur = self.tags_state.selected().unwrap_or(0) as i32;
        let len = self.current_tags.len() as i32;
        let next = (cur + delta).clamp(0, len - 1);
        self.tags_state.select(Some(next as usize));
    }
    pub(super) fn delete_selected_tag(&mut self) -> Result<()> {
        let Some(idx) = self.tags_state.selected() else {
            return Ok(());
        };
        let Some(tag) = self.current_tags.get(idx).cloned() else {
            return Ok(());
        };
        let Some(track_id) = self.selected_track().map(|t| t.id) else {
            return Ok(());
        };
        let Some(lib) = self.library_id() else {
            self.status_msg = "no library selected".into();
            return Ok(());
        };
        match self.client.remove_user_tag(lib, track_id, tag.tag_id) {
            Ok(()) => {
                self.status_msg = format!("removed {}", tag.display());
                self.current_tags_for = None;
                self.refresh_tags();
                if self.tags_state.selected().unwrap_or(0) >= self.current_tags.len() {
                    let new = self.current_tags.len().checked_sub(1);
                    self.tags_state.select(new);
                }
            }
            Err(e) => self.status_msg = format!("remove tag failed: {e}"),
        }
        Ok(())
    }
    pub(super) fn commit_add_tag(&mut self, raw: &str) -> Result<()> {
        let Some((ns, val)) = parse_tag_input(raw) else {
            self.status_msg = "tag input: '<ns>:<val>' or '<val>'".into();
            return Ok(());
        };
        let Some(track) = self.selected_track().map(|t| t.id) else {
            return Ok(());
        };
        let Some(lib) = self.library_id() else {
            self.status_msg = "no library selected".into();
            return Ok(());
        };
        match self.client.add_user_tag(lib, track, &ns, &val) {
            Ok(_) => {
                self.status_msg = format!("added {}", fmt_tag(&ns, &val));
                self.current_tags_for = None; // force refresh
                self.refresh_tags();
            }
            Err(e) => self.status_msg = format!("add tag failed: {e}"),
        }
        Ok(())
    }
}

fn parse_tag_input(s: &str) -> Option<(String, String)> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let (ns, val) = match s.split_once(':') {
        Some((n, v)) => (n.trim(), v.trim()),
        None => ("", s),
    };
    if val.is_empty() {
        return None;
    }
    Some((ns.to_string(), val.to_string()))
}

fn fmt_tag(ns: &str, val: &str) -> String {
    if ns.is_empty() {
        format!(":{val}")
    } else {
        format!("{ns}:{val}")
    }
}

pub(super) fn track_id_from_url(url: &str) -> Option<i64> {
    let mut segs = url.split('/');
    while let Some(s) = segs.next() {
        if s == "tracks" {
            return segs.next().and_then(|n| n.parse().ok());
        }
    }
    None
}
