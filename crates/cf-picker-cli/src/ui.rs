use std::{
    io::{self, stdout},
    mem::take,
    process::exit,
    thread::spawn,
    time::Duration,
};

use crossterm::{
    cursor::Show,
    event::{Event, KeyCode, KeyModifiers, poll, read},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgressStage {
    Ping,
    Download,
    Finished,
}

#[derive(Debug, Clone)]
pub enum UiEvent {
    SetStage {
        stage: ProgressStage,
        sub_header: Option<String>,
        total: u64,
    },
    UpdateProgress {
        pos: u64,
        available: Option<usize>,
    },
    AddLog(String),
    AddError(String),
    Finished,
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub header_info: String,
    pub sub_header_info: Option<String>,
    pub stage: ProgressStage,
    pub progress_pos: u64,
    pub progress_len: u64,
    pub available_count: usize,
    pub logs: Vec<String>,
    pub errors: Vec<String>,
}

impl AppState {
    pub fn new(header_info: String) -> Self {
        Self {
            header_info,
            sub_header_info: None,
            stage: ProgressStage::Ping,
            progress_pos: 0,
            progress_len: 0,
            available_count: 0,
            logs: Vec::new(),
            errors: Vec::new(),
        }
    }
}

pub struct TerminalUi {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
    state: AppState,
}

impl TerminalUi {
    pub fn new(header_info: String) -> io::Result<Self> {
        let mut stdout = stdout();
        let _ = enable_raw_mode();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        // 在 raw mode 下监听 Crossterm 键盘事件（特别是 Ctrl+C）
        spawn(move || {
            loop {
                if poll(Duration::from_millis(100)).unwrap_or(false)
                    && let Ok(Event::Key(key)) = read()
                        && key.code == KeyCode::Char('c')
                            && key.modifiers.contains(KeyModifiers::CONTROL)
                        {
                            let mut stdout = io::stdout();
                            let _ = disable_raw_mode();
                            let _ = execute!(stdout, LeaveAlternateScreen);
                            let _ = execute!(stdout, Show);
                            exit(130);
                        }
            }
        });

        Ok(Self {
            terminal,
            state: AppState::new(header_info),
        })
    }

    pub fn handle_event(&mut self, event: UiEvent) {
        match event {
            UiEvent::SetStage {
                stage,
                sub_header,
                total,
            } => {
                self.state.stage = stage;
                if sub_header.is_some() {
                    self.state.sub_header_info = sub_header;
                }
                self.state.progress_len = total;
                self.state.progress_pos = 0;
                self.state.logs.clear();
            }
            UiEvent::UpdateProgress { pos, available } => {
                self.state.progress_pos = pos;
                if let Some(avail) = available {
                    self.state.available_count = avail;
                }
            }
            UiEvent::AddLog(log) => {
                self.state.logs.push(log);
            }
            UiEvent::AddError(err) => {
                self.state.errors.push(err);
            }
            UiEvent::Finished => {
                self.state.stage = ProgressStage::Finished;
                self.state.progress_pos = self.state.progress_len;
            }
        }
    }

    pub fn draw(&mut self) -> io::Result<()> {
        let state = &self.state;

        self.terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(4), // 开始信息 Header
                    Constraint::Length(3), // 进度条 Gauge
                    Constraint::Min(5),    // 日志与错误 Block
                ])
                .split(f.area());

            // 1. Header Block
            let header_text = vec![
                Line::from(vec![
                    Span::styled(
                        "CF-Picker ",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(&state.header_info),
                ]),
                Line::from(vec![if let Some(sub) = &state.sub_header_info {
                    Span::styled(sub.as_str(), Style::default().fg(Color::Yellow))
                } else {
                    Span::raw("")
                }]),
            ];
            let header_p = Paragraph::new(header_text).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" 配置与任务信息 "),
            );
            f.render_widget(header_p, chunks[0]);

            // 2. Progress Gauge
            let ratio = if state.progress_len == 0 {
                0.0
            } else {
                (state.progress_pos as f64 / state.progress_len as f64).clamp(0.0, 1.0)
            };

            let title = match state.stage {
                ProgressStage::Ping => format!(
                    " 延迟测速进度: {}/{} (可用: {}) ",
                    state.progress_pos, state.progress_len, state.available_count
                ),
                ProgressStage::Download => format!(
                    " 下载测速进度: {}/{} ",
                    state.progress_pos, state.progress_len
                ),
                ProgressStage::Finished => " 测速已完成 ".to_string(),
            };

            let gauge = Gauge::default()
                .block(Block::default().borders(Borders::ALL).title(title))
                .gauge_style(
                    Style::default()
                        .fg(Color::Green)
                        .bg(Color::Black)
                        .add_modifier(Modifier::BOLD),
                )
                .ratio(ratio);
            f.render_widget(gauge, chunks[1]);

            // 3. Log & Error Output split (Left: Exec Log, Right: Error Log)
            let bottom_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(chunks[2]);

            // 3a. Execution Log List
            let log_list_height = bottom_chunks[0].height.saturating_sub(2) as usize;
            let log_skip_count = state.logs.len().saturating_sub(log_list_height);

            let log_items: Vec<ListItem> = state
                .logs
                .iter()
                .skip(log_skip_count)
                .map(|log| {
                    ListItem::new(Line::from(vec![Span::styled(
                        log,
                        Style::default().fg(Color::Green),
                    )]))
                })
                .collect();

            let log_list = List::new(log_items).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" 执行日志 ({}) ", state.logs.len())),
            );
            f.render_widget(log_list, bottom_chunks[0]);

            // 3b. Error Output List
            let list_height = bottom_chunks[1].height.saturating_sub(2) as usize;
            let skip_count = state.errors.len().saturating_sub(list_height);

            let items: Vec<ListItem> = state
                .errors
                .iter()
                .skip(skip_count)
                .map(|err| {
                    ListItem::new(Line::from(vec![Span::styled(
                        err,
                        Style::default().fg(Color::Red),
                    )]))
                })
                .collect();

            let list = List::new(items).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" 错误信息日志 ({}) ", state.errors.len())),
            );
            f.render_widget(list, bottom_chunks[1]);
        })?;

        Ok(())
    }

    pub fn take_errors(&mut self) -> Vec<String> {
        take(&mut self.state.errors)
    }

    pub fn cleanup(&mut self) -> io::Result<()> {
        let _ = disable_raw_mode();
        execute!(self.terminal.backend_mut(), LeaveAlternateScreen)?;
        self.terminal.show_cursor()?;
        Ok(())
    }
}

impl Drop for TerminalUi {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}
