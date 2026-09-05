use super::tags::track_id_from_url;
use super::*;

impl App {
    pub(super) fn render(&mut self, f: &mut Frame) {
        let footer_height = if matches!(self.mode, Mode::Normal) {
            3
        } else {
            4
        };
        let outer = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(footer_height),
            ])
            .split(f.area());

        self.render_tabs(f, outer[0]);
        match self.tab {
            Tab::Songs => self.render_songs(f, outer[1]),
            Tab::Playlists => self.render_playlists(f, outer[1]),
            Tab::Queue => self.render_queue(f, outer[1]),
            Tab::Downloads => self.render_downloads(f, outer[1]),
            Tab::Uploads => self.render_uploads(f, outer[1]),
            Tab::Settings => self.render_settings(f, outer[1]),
        }
        self.render_footer(f, outer[2]);

        // Modal overlays on top of everything.
        if self.tab == Tab::Songs && self.show_tags {
            self.render_tags_overlay(f);
        }
        if self.tab == Tab::Songs && self.show_details {
            self.render_details_overlay(f);
        }
        if self.show_help {
            self.render_help_overlay(f);
        }
        if let Mode::PickPlaylist { .. } = &self.mode {
            self.render_pick_playlist_overlay(f);
        }
        if let Mode::PickLibrary { .. } = &self.mode {
            self.render_pick_library_overlay(f);
        }
        if let Mode::SortPicker { .. } = &self.mode {
            self.render_sort_picker_overlay(f);
        }
    }
    pub(super) fn render_downloads(&mut self, f: &mut Frame, area: Rect) {
        let ids = self.downloads_sorted_ids();
        let downloaded_sizes: HashMap<i64, u64> = ids
            .iter()
            .filter_map(|id| {
                let st = &self.downloads[id];
                if st.status != DownloadStatus::Downloaded {
                    return None;
                }
                let bytes = st
                    .local_path
                    .as_ref()
                    .and_then(|p| std::fs::metadata(p).ok())
                    .map(|m| m.len())?;
                Some((*id, bytes))
            })
            .collect();
        let total_bytes: u64 = downloaded_sizes.values().sum();

        let sel_count = self.downloads_select.count();
        let mut title = format!(" Downloads ({})", self.downloads.len());
        if total_bytes > 0 {
            title = format!(
                "{title} — {} downloaded ({})",
                downloaded_sizes.len(),
                fmt_bytes(total_bytes)
            );
        }
        if sel_count > 0 {
            title = format!("{title}  [{sel_count} selected]");
        }
        title.push(' ');
        let block = Block::default().borders(Borders::ALL).title(title);

        if ids.is_empty() {
            let text = Paragraph::new("no downloads").block(block);
            f.render_widget(text, area);
            return;
        }

        let items: Vec<ListItem> = ids
            .iter()
            .map(|id| {
                let st = &self.downloads[id];
                let label = self
                    .tracks
                    .iter()
                    .find(|t| t.id == *id)
                    .map(|t| format!("{} — {}", t.display_artist(), t.display_title()))
                    .unwrap_or_else(|| format!("track #{id}"));
                let status = match st.status {
                    DownloadStatus::Queued => "queued".to_string(),
                    DownloadStatus::Downloading if st.total > 0 => {
                        format!("downloading {}%", (st.bytes * 100 / st.total).clamp(0, 100))
                    }
                    DownloadStatus::Downloading => "downloading".to_string(),
                    DownloadStatus::Downloaded => match downloaded_sizes.get(id) {
                        Some(bytes) => format!("downloaded ({})", fmt_bytes(*bytes)),
                        None => "downloaded".to_string(),
                    },
                    DownloadStatus::Failed => "failed".to_string(),
                };
                let marker = if self.downloads_select.is_selected(*id) {
                    "✔ "
                } else {
                    "  "
                };
                ListItem::new(Line::from(Span::raw(format!(
                    "{marker}{label}  [{status}]"
                ))))
            })
            .collect();

        let len = ids.len();
        match (self.downloads_state.selected(), len) {
            (_, 0) => self.downloads_state.select(None),
            (None, _) => self.downloads_state.select(Some(0)),
            (Some(i), n) if i >= n => self.downloads_state.select(Some(n - 1)),
            _ => {}
        }

        let list = List::new(items)
            .block(block)
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        f.render_stateful_widget(list, area, &mut self.downloads_state);
    }
    pub(super) fn render_uploads(&mut self, f: &mut Frame, area: Rect) {
        match self.upload_stage.clone() {
            UploadStage::List { index } => {
                let block = Block::default()
                    .borders(Borders::ALL)
                    .title(" Uploads — pick a script  (j/k, ⏎ select, r refresh) ");

                if self.downloader_scripts.is_empty() {
                    let text = Paragraph::new("no downloader scripts configured").block(block);
                    f.render_widget(text, area);
                    return;
                }

                let items: Vec<ListItem> = self
                    .downloader_scripts
                    .iter()
                    .enumerate()
                    .map(|(i, d)| {
                        let is_cursor = i == index;
                        let prefix = if is_cursor { "> " } else { "  " };
                        let style = if is_cursor {
                            Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
                        } else {
                            Style::default()
                        };
                        ListItem::new(Line::from(Span::styled(
                            format!("{prefix}{}", d.name),
                            style,
                        )))
                    })
                    .collect();
                let list = List::new(items).block(block);
                f.render_widget(list, area);
            }
            UploadStage::Input { script, buf } => {
                let block = Block::default().borders(Borders::ALL).title(format!(
                    " Run {script} — URL(s), comma/space/newline separated "
                ));
                let lines = vec![
                    Line::from(vec![Span::raw(buf.clone()), Span::raw("_")]),
                    Line::raw(""),
                    Line::from(Span::styled(
                        "⏎ run   Esc back",
                        Style::default().add_modifier(Modifier::DIM),
                    )),
                ];
                f.render_widget(Paragraph::new(lines).block(block), area);
            }
            UploadStage::Job => {
                let script = self.downloader_script_name.as_deref().unwrap_or("?");
                let status = self
                    .downloader_job
                    .as_ref()
                    .map(|j| j.status.as_str())
                    .unwrap_or("starting…");
                let block = Block::default().borders(Borders::ALL).title(format!(
                    " Uploads — {script} ({status})  (Esc to dismiss) "
                ));
                let inner = block.inner(area);
                f.render_widget(block, area);

                let mut lines: Vec<Line<'static>> = Vec::new();
                if let Some(job) = &self.downloader_job {
                    let extra = if job.summary.is_some() { 2 } else { 0 };
                    let max_log = (inner.height as usize).saturating_sub(extra).max(1);
                    let start = job.log.len().saturating_sub(max_log);
                    for l in &job.log[start..] {
                        lines.push(Line::raw(l.clone()));
                    }
                    if let Some(s) = &job.summary {
                        lines.push(Line::raw(""));
                        lines.push(Line::from(Span::styled(
                            format!(
                                "import: scanned={} +{} dup={} fail={}",
                                s.scanned, s.imported, s.duplicates, s.failed
                            ),
                            Style::default()
                                .fg(Color::Green)
                                .add_modifier(Modifier::BOLD),
                        )));
                    } else if job.status == "failed" {
                        lines.push(Line::from(Span::styled(
                            "job failed",
                            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                        )));
                    }
                } else {
                    lines.push(Line::raw("starting…"));
                }
                f.render_widget(Paragraph::new(lines), inner);
            }
        }
    }
    pub(super) fn render_pick_library_overlay(&self, f: &mut Frame) {
        let Mode::PickLibrary { index } = self.mode else {
            return;
        };
        let area = centered_rect(60, 60, f.area());
        f.render_widget(ratatui::widgets::Clear, area);

        let items: Vec<ListItem> = self
            .libraries
            .iter()
            .enumerate()
            .map(|(i, l)| {
                let is_cursor = i == index;
                let is_active = self.library_id == Some(l.id);
                let prefix = if is_cursor { "> " } else { "  " };
                let marker = if is_active { " *" } else { "" };
                let style = if is_cursor {
                    Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
                } else if is_active {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default()
                };
                ListItem::new(Line::from(Span::styled(
                    format!("{prefix}{}{marker}  {}", l.name, l.path),
                    style,
                )))
            })
            .collect();
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" Library  (j/k, ⏎ confirm, Esc cancel) ");
        let list = List::new(items).block(block);
        f.render_widget(list, area);
    }
    pub(super) fn render_sort_picker_overlay(&self, f: &mut Frame) {
        let Mode::SortPicker { index } = self.mode else {
            return;
        };
        let area = centered_rect(50, 50, f.area());
        f.render_widget(ratatui::widgets::Clear, area);

        let items: Vec<ListItem> = SortKey::ALL
            .iter()
            .enumerate()
            .map(|(i, k)| {
                let is_cursor = i == index;
                let is_active = *k == self.sort_key;
                let prefix = if is_cursor { "> " } else { "  " };
                let marker = if is_active { " *" } else { "" };
                let style = if is_cursor {
                    Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
                } else if is_active {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default()
                };
                ListItem::new(Line::from(Span::styled(
                    format!("{prefix}{}{marker}", k.label()),
                    style,
                )))
            })
            .collect();
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" Sort by  (j/k, ⏎ confirm, Esc cancel) ");
        let list = List::new(items).block(block);
        f.render_widget(list, area);
    }
    pub(super) fn render_help_overlay(&self, f: &mut Frame) {
        let area = centered_rect(70, 80, f.area());
        f.render_widget(ratatui::widgets::Clear, area);

        let header = |s: &'static str| {
            Line::from(Span::styled(
                s,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ))
        };
        let item = |k: &str, d: &str| {
            Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{:<16}", k), Style::default().fg(Color::Green)),
                Span::raw(d.to_string()),
            ])
        };

        let mut lines: Vec<Line<'static>> = vec![
            header("Global"),
            item("q / Ctrl+C", "quit"),
            item("space", "play / pause"),
            item("n / p", "next / previous"),
            item("← / →", "seek ±5s  (Shift: ±30s)"),
            item("S", "shuffle queue"),
            item("R", "cycle repeat (off/all/one)"),
            item("Ctrl+R", "refresh library from server"),
            item("?", "toggle this help"),
            item("1-6", "switch tabs"),
            Line::raw(""),
        ];

        match self.tab {
            Tab::Songs => {
                if self.show_tags {
                    lines.push(header("Songs — tags popup"));
                    lines.push(item("j/k", "navigate tags"));
                    lines.push(item("g / G", "top / bottom"));
                    lines.push(item("a", "add tag"));
                    lines.push(item("d", "remove user tag"));
                    lines.push(item("t / Esc", "close tags popup"));
                } else if self.show_details {
                    lines.push(header("Songs — details popup"));
                    lines.push(item("i / Esc", "close details popup"));
                } else {
                    lines.push(header("Songs"));
                    lines.push(item("j/k, PgUp/Dn", "move selection"));
                    lines.push(item("g / G", "top / bottom"));
                    lines.push(item("x", "toggle mark"));
                    lines.push(item("V", "start/end range select"));
                    lines.push(item("⏎", "play selected"));
                    lines.push(item("a", "queue selected / marked"));
                    lines.push(item("E", "queue all (filtered)"));
                    lines.push(item("A", "add to playlist"));
                    lines.push(item("t", "show tags for selected"));
                    lines.push(item("i", "show details for selected"));
                    lines.push(item("/", "filter"));
                    lines.push(item("T", "tag search"));
                    lines.push(item("s", "sort by…"));
                    lines.push(item("Esc", "clear marks / filter"));
                    lines.push(item("o", "download selected / marked"));
                }
            }
            Tab::Playlists => match self.playlists_focus {
                PlaylistsFocus::List => {
                    lines.push(header("Playlists"));
                    lines.push(item("j/k", "navigate"));
                    lines.push(item("P", "play playlist"));
                    lines.push(item("N", "new playlist"));
                    lines.push(item("r", "rename"));
                    lines.push(item("D", "delete"));
                    lines.push(item("⇥", "open / tracks pane"));
                }
                PlaylistsFocus::Tracks => {
                    lines.push(header("Playlist tracks"));
                    lines.push(item("j/k", "navigate"));
                    lines.push(item("x", "toggle mark"));
                    lines.push(item("V", "start/end range select"));
                    lines.push(item("J / K", "reorder down / up"));
                    lines.push(item("⏎", "play from here"));
                    lines.push(item("a", "queue selected / marked"));
                    lines.push(item("d", "remove selected / marked"));
                    lines.push(item("o", "download selected / marked"));
                    lines.push(item("O", "download whole playlist"));
                    lines.push(item("⇥ / Esc", "clear marks / back to playlists"));
                }
            },
            Tab::Queue => {
                lines.push(header("Queue"));
                lines.push(item("j/k", "navigate"));
                lines.push(item("g / G", "top / bottom"));
                lines.push(item("J / K", "reorder down / up"));
                lines.push(item("⏎", "jump to track"));
                lines.push(item("d", "remove from queue"));
            }
            Tab::Downloads => {
                lines.push(header("Downloads"));
                lines.push(item("j/k, g/G", "navigate"));
                lines.push(item("x", "toggle mark"));
                lines.push(item("V", "start/end range select"));
                lines.push(item("⏎ / r", "retry selected / marked failed"));
                lines.push(item("⌫ / Del", "remove selected / marked"));
                lines.push(item("Esc", "clear marks"));
            }
            Tab::Uploads => match &self.upload_stage {
                UploadStage::List { .. } => {
                    lines.push(header("Uploads"));
                    lines.push(item("j/k", "navigate scripts"));
                    lines.push(item("⏎", "select script"));
                    lines.push(item("r", "refresh script list"));
                }
                UploadStage::Input { .. } => {
                    lines.push(header("Uploads — enter URLs"));
                    lines.push(item("⏎", "run downloader"));
                    lines.push(item("Esc", "back to script list"));
                }
                UploadStage::Job => {
                    lines.push(header("Uploads — job log"));
                    lines.push(item("Esc", "dismiss (keeps running in background)"));
                }
            },
            Tab::Settings => {
                lines.push(header("Settings"));
                lines.push(item("j/k", "navigate fields"));
                lines.push(item("⏎ / e", "edit field"));
                lines.push(item("s", "save changes"));
                lines.push(item("r / Esc", "revert"));
            }
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" Help (? to close) ");
        let p = Paragraph::new(lines).block(block);
        f.render_widget(p, area);
    }
    pub(super) fn render_playlists(&mut self, f: &mut Frame, area: Rect) {
        let split = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
            .split(area);

        let items: Vec<ListItem> = self
            .playlists
            .iter()
            .map(|p| {
                let line = Line::from(vec![
                    Span::styled(
                        p.name.clone(),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("  ({})", p.track_count),
                        Style::default().add_modifier(Modifier::DIM),
                    ),
                ]);
                ListItem::new(line)
            })
            .collect();
        let title = format!(" Playlists ({}) ", self.playlists.len());
        let block = pane_block(title, self.playlists_focus == PlaylistsFocus::List);
        let mut list = List::new(items).block(block);
        if self.playlists_focus == PlaylistsFocus::List {
            list = list.highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        }
        f.render_stateful_widget(list, split[0], &mut self.playlists_state);

        let pl_name = self.selected_playlist_name().unwrap_or_default();
        let track_items: Vec<ListItem> = self
            .playlist_tracks
            .iter()
            .map(|pt| {
                let marker = if self.playlist_select.is_selected(pt.position) {
                    "✔ "
                } else {
                    "  "
                };
                let line = Line::from(vec![
                    Span::raw(marker),
                    Span::styled(
                        format!("{:>3}. ", pt.position + 1),
                        Style::default().add_modifier(Modifier::DIM),
                    ),
                    Span::styled(
                        pt.display_artist().to_string(),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::raw("  "),
                    Span::raw(pt.display_title().to_string()),
                ]);
                ListItem::new(line)
            })
            .collect();
        let sel_count = self.playlist_select.count();
        let title = if pl_name.is_empty() {
            " Tracks ".to_string()
        } else if sel_count > 0 {
            format!(
                " {} ({})  [{sel_count} selected] ",
                pl_name,
                self.playlist_tracks.len()
            )
        } else {
            format!(" {} ({}) ", pl_name, self.playlist_tracks.len())
        };
        let block = pane_block(title, self.playlists_focus == PlaylistsFocus::Tracks);
        let mut list = List::new(track_items).block(block);
        if self.playlists_focus == PlaylistsFocus::Tracks {
            list = list.highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        }
        f.render_stateful_widget(list, split[1], &mut self.playlist_tracks_state);
    }
    pub(super) fn render_pick_playlist_overlay(&self, f: &mut Frame) {
        let Mode::PickPlaylist {
            index,
            ref containing,
            ..
        } = self.mode
        else {
            return;
        };
        let area = centered_rect(60, 60, f.area());
        f.render_widget(ratatui::widgets::Clear, area);

        let items: Vec<ListItem> = self
            .playlists
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let prefix = if i == index { "> " } else { "  " };
                let member = containing.contains(&p.id);
                let marker = if member { "✓ " } else { "  " };
                let style = if i == index {
                    Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
                } else if member {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default()
                };
                ListItem::new(Line::from(Span::styled(
                    format!("{prefix}{marker}{}  ({})", p.name, p.track_count),
                    style,
                )))
            })
            .collect();
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" add to playlist  (j/k, ⏎ confirm, Esc cancel) ");
        let list = List::new(items).block(block);
        f.render_widget(list, area);
    }
    pub(super) fn render_songs(&mut self, f: &mut Frame, area: Rect) {
        self.render_list(f, area);
    }
    pub(super) fn render_tabs(&self, f: &mut Frame, area: Rect) {
        let mut spans: Vec<Span<'static>> = vec![Span::raw(" ")];
        for (i, t) in Tab::ALL.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw("   "));
            }
            let is_active = self.tab == *t;
            let style = if is_active {
                Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
            } else {
                Style::default().add_modifier(Modifier::DIM)
            };
            spans.push(Span::styled(format!(" {} {} ", i + 1, t.label()), style));
        }
        let lib = self
            .current_library_name()
            .unwrap_or_else(|| "(no library)".into());
        let row = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Min(1),
                Constraint::Length(lib.chars().count() as u16 + 6),
            ])
            .split(area);
        f.render_widget(Paragraph::new(Line::from(spans)), row[0]);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!(" [{lib}] "),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )))
            .alignment(Alignment::Right),
            row[1],
        );
    }
    pub(super) fn render_settings(&self, f: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::ALL).title(" Settings ");
        let inner = block.inner(area);
        f.render_widget(block, area);

        let mut lines: Vec<Line<'static>> = vec![Line::raw("")];

        for field in SettingsField::ALL {
            let val: String = match field {
                SettingsField::ServerUrl => self.settings.server_url.clone(),
                SettingsField::Username => self.settings.username.clone(),
                SettingsField::Token => self.settings.token.clone(),
                SettingsField::Library => self
                    .current_library_name()
                    .unwrap_or_else(|| "(none)".into()),
            };
            let is_selected = self.settings_field == field;
            let is_editing = matches!(&self.mode, Mode::EditSetting(f, _) if *f == field);

            let prefix = if is_selected { "> " } else { "  " };
            let label_style = if is_selected {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let val_text = if let Mode::EditSetting(f, buf) = &self.mode {
                if *f == field {
                    format!("{buf}_")
                } else {
                    val
                }
            } else {
                val
            };
            let val_style = if is_editing {
                Style::default().fg(Color::Yellow)
            } else if is_selected {
                Style::default()
            } else {
                Style::default().add_modifier(Modifier::DIM)
            };

            lines.push(Line::from(vec![
                Span::raw(prefix.to_string()),
                Span::styled(format!("{:<12}", field.label()), label_style),
                Span::raw("  "),
                Span::styled(val_text, val_style),
            ]));
            lines.push(Line::raw(""));
        }

        let dirty = self.settings.server_url != self.saved_settings.server_url
            || self.settings.username != self.saved_settings.username
            || self.settings.token != self.saved_settings.token;
        if dirty {
            lines.push(Line::from(Span::styled(
                "  unsaved changes — 's' save, 'r'/Esc revert",
                Style::default().fg(Color::Yellow),
            )));
        } else {
            lines.push(Line::from(Span::styled(
                format!(
                    "  config: {}",
                    crate::settings::Settings::config_path().display()
                ),
                Style::default().add_modifier(Modifier::DIM),
            )));
        }

        let p = Paragraph::new(lines);
        f.render_widget(p, inner);
    }
    pub(super) fn render_queue(&mut self, f: &mut Frame, area: Rect) {
        let snap = self.mpv.snapshot();
        let items: Vec<ListItem> = snap
            .playlist
            .iter()
            .map(|entry| {
                let id = track_id_from_url(&entry.url);
                let track = id.and_then(|i| self.tracks.iter().find(|t| t.id == i));
                let label = match track {
                    Some(t) => format!("{} — {}", t.display_artist(), t.display_title()),
                    None => entry.url.clone(),
                };
                let mark = if entry.current {
                    if snap.paused {
                        "‖ "
                    } else {
                        "▶ "
                    }
                } else {
                    "  "
                };
                let style = if entry.current {
                    Style::default().add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(Line::from(Span::styled(format!("{mark}{label}"), style)))
            })
            .collect();
        // Keep selection in range.
        let len = snap.playlist.len();
        match (self.queue_state.selected(), len) {
            (_, 0) => self.queue_state.select(None),
            (None, _) => self.queue_state.select(Some(0)),
            (Some(i), n) if i >= n => self.queue_state.select(Some(n - 1)),
            _ => {}
        }

        let title = format!(" Queue ({}) ", len);
        let block = Block::default().borders(Borders::ALL).title(title);
        let list = List::new(items)
            .block(block)
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        f.render_stateful_widget(list, area, &mut self.queue_state);
    }
    pub(super) fn render_list(&mut self, f: &mut Frame, area: Rect) {
        let snap = self.mpv.snapshot();
        let now_track_id = snap
            .current_path
            .as_deref()
            .and_then(|p| self.now_playing_id(p));
        let items: Vec<ListItem> = self
            .filtered
            .iter()
            .map(|&i| {
                let t = &self.tracks[i];
                let mark = if Some(t.id) == now_track_id {
                    if snap.paused {
                        "‖ "
                    } else {
                        "▶ "
                    }
                } else {
                    "  "
                };
                let dl_glyph = match self.downloads.get(&t.id) {
                    Some(st) => match st.status {
                        DownloadStatus::Queued => "⬇".to_string(),
                        DownloadStatus::Downloading if st.total > 0 => {
                            format!("↓{}%", (st.bytes * 100 / st.total).clamp(0, 100))
                        }
                        DownloadStatus::Downloading => "↓".to_string(),
                        DownloadStatus::Downloaded => "✓".to_string(),
                        DownloadStatus::Failed => "✗".to_string(),
                    },
                    None => String::new(),
                };
                let sel_marker = if self.songs_select.is_selected(t.id) {
                    "✔ "
                } else {
                    "  "
                };
                let line = Line::from(vec![
                    Span::raw(sel_marker),
                    Span::raw(mark),
                    Span::styled(
                        t.display_artist().to_string(),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::raw("  "),
                    Span::raw(t.display_title().to_string()),
                    Span::raw("  "),
                    Span::raw(dl_glyph),
                ]);
                ListItem::new(line)
            })
            .collect();

        let sel_count = self.songs_select.count();
        let title = if sel_count > 0 {
            format!(
                " muserv — {} / {}  [{sel_count} selected] ",
                self.filtered.len(),
                self.tracks.len()
            )
        } else {
            format!(" muserv — {} / {} ", self.filtered.len(), self.tracks.len())
        };
        let block = pane_block(title, true);
        let list = List::new(items)
            .block(block)
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
            .highlight_symbol("");

        f.render_stateful_widget(list, area, &mut self.list_state);
    }
    pub(super) fn render_tags_overlay(&mut self, f: &mut Frame) {
        let area = centered_rect(60, 70, f.area());
        f.render_widget(ratatui::widgets::Clear, area);

        let items: Vec<ListItem> = self
            .current_tags
            .iter()
            .map(|t| {
                ListItem::new(Line::from(vec![Span::styled(
                    t.display(),
                    Style::default().fg(Color::Green),
                )]))
            })
            .collect();

        let track_label = self
            .selected_track()
            .map(|t| format!("{} — {}", t.display_artist(), t.display_title()))
            .unwrap_or_else(|| "no track".into());
        let title = format!(" Tags — {} ({}) ", track_label, self.current_tags.len());
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(title);
        if self.tags_state.selected().is_none() && !self.current_tags.is_empty() {
            self.tags_state.select(Some(0));
        }
        let list = List::new(items)
            .block(block)
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        f.render_stateful_widget(list, area, &mut self.tags_state);
    }
    pub(super) fn render_details_overlay(&self, f: &mut Frame) {
        let Some(t) = self.selected_track() else {
            return;
        };
        let area = centered_rect(70, 70, f.area());
        f.render_widget(ratatui::widgets::Clear, area);

        let dash = "—".to_string();
        let duration = t
            .duration_ms
            .map(|ms| fmt_time(ms as f64 / 1000.0))
            .unwrap_or_else(|| dash.clone());
        let track_no = match (t.track_no, t.disc_no) {
            (Some(tn), Some(dn)) => format!("{dn}.{tn}"),
            (Some(tn), None) => tn.to_string(),
            (None, Some(dn)) => format!("disc {dn}"),
            _ => dash.clone(),
        };
        let bitrate = t
            .bitrate
            .map(|b| format!("{} kbps", b / 1000))
            .unwrap_or_else(|| dash.clone());
        let sample_rate = t
            .sample_rate
            .map(|s| format!("{:.1} kHz", s as f64 / 1000.0))
            .unwrap_or_else(|| dash.clone());
        let channels = t
            .channels
            .map(|c| c.to_string())
            .unwrap_or_else(|| dash.clone());
        let year = t
            .year
            .map(|y| y.to_string())
            .unwrap_or_else(|| dash.clone());
        let format = t
            .original_filename
            .as_deref()
            .and_then(|p| std::path::Path::new(p).extension())
            .and_then(|e| e.to_str())
            .map(|s| s.to_uppercase())
            .unwrap_or_else(|| dash.clone());
        let added = if t.added_at > 0 {
            fmt_unix_utc(t.added_at)
        } else {
            dash.clone()
        };

        let rows: Vec<(&str, String)> = vec![
            ("Title", t.title.clone().unwrap_or_else(|| dash.clone())),
            ("Artist", t.artist.clone().unwrap_or_else(|| dash.clone())),
            (
                "Album artist",
                t.album_artist.clone().unwrap_or_else(|| dash.clone()),
            ),
            ("Album", t.album.clone().unwrap_or_else(|| dash.clone())),
            ("Year", year),
            ("Track", track_no),
            ("Duration", duration),
            ("Format", format),
            ("Bitrate", bitrate),
            ("Sample rate", sample_rate),
            ("Channels", channels),
            ("Added", added),
            ("Track ID", t.id.to_string()),
            (
                "File",
                t.original_filename.clone().unwrap_or_else(|| dash.clone()),
            ),
        ];

        let label_w = rows
            .iter()
            .map(|(k, _)| k.chars().count())
            .max()
            .unwrap_or(0);
        let mut lines: Vec<Line<'static>> = Vec::with_capacity(rows.len() + 1);
        lines.push(Line::raw(""));
        for (k, v) in rows {
            lines.push(Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    format!("{:<width$}  ", k, width = label_w),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(v),
            ]));
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" Song details  (i / Esc to close) ");
        f.render_widget(Paragraph::new(lines).block(block), area);
    }
    pub(super) fn render_footer(&self, f: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::ALL);
        let inner = block.inner(area);
        f.render_widget(block, area);

        let normal = matches!(self.mode, Mode::Normal)
            && !(self.tab == Tab::Uploads && matches!(self.upload_stage, UploadStage::Input { .. }));
        let constraints: &[Constraint] = if normal {
            &[Constraint::Length(1)]
        } else {
            &[Constraint::Length(1), Constraint::Length(1)]
        };
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(inner);

        let status_width = self.status_msg.chars().count() as u16;
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(1), Constraint::Length(status_width)])
            .split(rows[0]);
        f.render_widget(Paragraph::new(self.now_playing_line()), cols[0]);
        if status_width > 0 {
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    self.status_msg.clone(),
                    Style::default().fg(Color::Yellow),
                )))
                .alignment(Alignment::Right),
                cols[1],
            );
        }

        if !normal {
            f.render_widget(Paragraph::new(self.prompt_or_hints_line()), rows[1]);
        }
    }
    pub(super) fn now_playing_line(&self) -> Line<'static> {
        let snap = self.mpv.snapshot();
        let nothing = snap.idle_active || snap.current_path.is_none();
        if nothing {
            return Line::from(Span::styled(
                "■ stopped",
                Style::default().add_modifier(Modifier::BOLD),
            ));
        }
        let glyph = if snap.paused { "‖" } else { "▶" };
        let url = snap.current_path.as_deref().unwrap_or("");
        let track = track_id_from_url(url).and_then(|id| self.tracks.iter().find(|t| t.id == id));
        let label = match track {
            Some(t) => format!("{} — {}", t.display_artist(), t.display_title()),
            None => url.to_string(),
        };
        let time = format!(
            "   {} / {}",
            snap.time_pos.map(fmt_time).unwrap_or_else(|| "—".into()),
            snap.duration.map(fmt_time).unwrap_or_else(|| "—".into()),
        );
        let repeat = self.repeat.label();
        let mut spans = vec![
            Span::styled(
                format!("{glyph} "),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(label),
            Span::styled(time, Style::default().add_modifier(Modifier::DIM)),
        ];
        if !repeat.is_empty() {
            spans.push(Span::styled(
                format!("   {repeat}"),
                Style::default().fg(Color::Cyan),
            ));
        }
        Line::from(spans)
    }
    pub(super) fn prompt_or_hints_line(&self) -> Line<'static> {
        match &self.mode {
            Mode::Filter(buf) => Line::from(vec![
                Span::styled("/", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(buf.clone()),
                Span::raw("_"),
            ]),
            Mode::TagSearch(buf) => Line::from(vec![
                Span::styled(
                    "T",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" tag: "),
                Span::raw(buf.clone()),
                Span::raw("_"),
            ]),
            Mode::AddTag(buf) => Line::from(vec![
                Span::styled("+tag ", Style::default().fg(Color::Green)),
                Span::raw(buf.clone()),
                Span::raw("_"),
            ]),
            Mode::EditSetting(field, _) => Line::from(vec![
                Span::styled(
                    format!("editing {}: ", field.label()),
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled(
                    "⏎ commit  Esc cancel",
                    Style::default().add_modifier(Modifier::DIM),
                ),
            ]),
            Mode::NewPlaylist(buf) => Line::from(vec![
                Span::styled("new playlist ", Style::default().fg(Color::Green)),
                Span::raw(buf.clone()),
                Span::raw("_"),
            ]),
            Mode::RenamePlaylist(_, buf) => Line::from(vec![
                Span::styled("rename ", Style::default().fg(Color::Yellow)),
                Span::raw(buf.clone()),
                Span::raw("_"),
            ]),
            Mode::PickPlaylist { .. } => Line::from(Span::styled(
                "pick playlist (j/k, ⏎ confirm, Esc cancel)",
                Style::default().fg(Color::Cyan),
            )),
            Mode::PickLibrary { .. } => Line::from(Span::styled(
                "pick library (j/k, ⏎ confirm, Esc cancel)",
                Style::default().fg(Color::Cyan),
            )),
            Mode::SortPicker { .. } => Line::from(Span::styled(
                "sort songs (j/k, ⏎ confirm, Esc cancel)",
                Style::default().fg(Color::Cyan),
            )),
            Mode::Normal => {
                if self.tab == Tab::Uploads {
                    if let UploadStage::Input { script, buf } = &self.upload_stage {
                        return Line::from(vec![
                            Span::styled(
                                format!("run {script}: "),
                                Style::default().fg(Color::Green),
                            ),
                            Span::raw(buf.clone()),
                            Span::raw("_"),
                        ]);
                    }
                }
                Line::raw("")
            }
        }
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_y = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_y[1])[1]
}

fn pane_block(title: String, focused: bool) -> Block<'static> {
    let border_style = if focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(title)
}

fn fmt_unix_utc(ts: i64) -> String {
    let days = ts.div_euclid(86400);
    let secs = ts.rem_euclid(86400);
    let hh = secs / 3600;
    let mm = (secs / 60) % 60;

    // Howard Hinnant's days-from-civil, inverted.
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    if m <= 2 {
        y += 1;
    }
    format!("{y:04}-{m:02}-{d:02} {hh:02}:{mm:02}")
}

fn fmt_bytes(bytes: u64) -> String {
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    let bytes = bytes as f64;
    if bytes >= GB {
        format!("{:.2} GB", bytes / GB)
    } else {
        format!("{:.1} MB", bytes / MB)
    }
}

fn fmt_time(secs: f64) -> String {
    let total = secs.max(0.0) as u64;
    let m = total / 60;
    let s = total % 60;
    if m >= 60 {
        let h = m / 60;
        let m = m % 60;
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}
