use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph},
    Frame, Terminal,
};
use std::{
    io::{self, Stdout},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Debug, Clone, PartialEq)]
pub enum ProgressState {
    Pending,
    Downloading,
    Installing,
    Removing,
    Completed,
    Failed,
}

#[derive(Debug, Clone)]
pub struct PackageProgress {
    pub name: String,
    pub state: ProgressState,
    pub progress: u16, // 0-100
    pub message: String,
}

impl PackageProgress {
    pub fn new(name: String) -> Self {
        Self {
            name,
            state: ProgressState::Pending,
            progress: 0,
            message: String::new(),
        }
    }

    pub fn state_color(&self) -> Color {
        match self.state {
            ProgressState::Pending => Color::Gray,
            ProgressState::Downloading => Color::Yellow,
            ProgressState::Installing => Color::Blue,
            ProgressState::Removing => Color::Magenta,
            ProgressState::Completed => Color::Green,
            ProgressState::Failed => Color::Red,
        }
    }

    pub fn state_label(&self) -> &str {
        match self.state {
            ProgressState::Pending => "pending",
            ProgressState::Downloading => "downloading",
            ProgressState::Installing => "installing",
            ProgressState::Removing => "removing",
            ProgressState::Completed => "completed",
            ProgressState::Failed => "failed",
        }
    }
}

pub struct ProgressTracker {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    packages: Arc<Mutex<Vec<PackageProgress>>>,
    total_packages: usize,
    show_summary: bool,
    autoquit_secs: Option<u64>,
    summary_countdown_secs: Option<u64>,
}

impl ProgressTracker {
    pub fn new(package_names: Vec<String>, autoquit_secs: Option<u64>) -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        let packages: Vec<PackageProgress> = package_names
            .into_iter()
            .map(PackageProgress::new)
            .collect();

        let total_packages = packages.len();

        Ok(Self {
            terminal,
            packages: Arc::new(Mutex::new(packages)),
            total_packages,
            show_summary: false,
            autoquit_secs,
            summary_countdown_secs: None,
        })
    }

    pub fn get_packages(&self) -> Arc<Mutex<Vec<PackageProgress>>> {
        Arc::clone(&self.packages)
    }

    #[allow(dead_code)]
    pub fn update_package(
        &self,
        index: usize,
        state: ProgressState,
        progress: u16,
        message: String,
    ) {
        if let Ok(mut packages) = self.packages.lock() {
            if let Some(package) = packages.get_mut(index) {
                package.state = state;
                package.progress = progress;
                package.message = message;
            }
        }
    }

    pub fn draw(&mut self) -> io::Result<()> {
        let packages = Arc::clone(&self.packages);
        let total_packages = self.total_packages;
        let show_summary = self.show_summary;
        let countdown = self.summary_countdown_secs;

        self.terminal.draw(|f| {
            if show_summary {
                Self::render_summary_static(f, &packages, total_packages, countdown);
            } else {
                Self::render_ui_static(f, &packages, total_packages);
            }
        })?;
        Ok(())
    }

    fn render_summary_static(
        f: &mut Frame,
        packages_arc: &Arc<Mutex<Vec<PackageProgress>>>,
        total_packages: usize,
        countdown_secs: Option<u64>,
    ) {
        let packages = packages_arc.lock().unwrap();

        let completed = packages
            .iter()
            .filter(|p| p.state == ProgressState::Completed)
            .count();
        let failed = packages
            .iter()
            .filter(|p| p.state == ProgressState::Failed)
            .count();

        // Main layout
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(2)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(5),
                Constraint::Min(10),
                Constraint::Length(3),
            ])
            .split(f.area());

        let (top, middle, bottom) = crate::utilities::brew_common::header_lines("Summary");
        let title = Paragraph::new(vec![
            Line::from(vec![Span::styled(
                top,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(vec![Span::styled(
                middle,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(vec![Span::styled(
                bottom,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]),
        ]);
        f.render_widget(title, chunks[0]);

        // Stats
        let stats_text = vec![
            Line::from(vec![
                Span::styled("Total: ", Style::default().fg(Color::White)),
                Span::styled(
                    format!("{}", total_packages),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("Completed: ", Style::default().fg(Color::White)),
                Span::styled(
                    format!("{}", completed),
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("Failed: ", Style::default().fg(Color::White)),
                Span::styled(
                    format!("{}", failed),
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
            ]),
        ];
        let stats = Paragraph::new(stats_text)
            .block(Block::default().title("Results").borders(Borders::ALL));
        f.render_widget(stats, chunks[1]);

        // Package list
        let available_height = chunks[2].height.saturating_sub(2);
        let packages_per_screen = (available_height / 2).max(1) as usize;

        let visible_packages: Vec<_> = packages.iter().take(packages_per_screen).collect();

        let package_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                visible_packages
                    .iter()
                    .map(|_| Constraint::Length(2))
                    .collect::<Vec<_>>(),
            )
            .split(chunks[2]);

        for (i, package) in visible_packages.iter().enumerate() {
            if i < package_chunks.len() {
                let status_icon = match package.state {
                    ProgressState::Completed => "✓",
                    ProgressState::Failed => "✗",
                    _ => "•",
                };

                let line = Line::from(vec![
                    Span::styled(
                        format!(" {} ", status_icon),
                        Style::default()
                            .fg(package.state_color())
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(&package.name, Style::default().fg(Color::White)),
                ]);

                let para = Paragraph::new(line).block(Block::default().borders(Borders::BOTTOM));
                f.render_widget(para, package_chunks[i]);
            }
        }

        // Footer: show countdown when autoquit is set, else "Press q or ESC to exit"
        let footer_line = match countdown_secs {
            Some(n) => Line::from(vec![
                Span::styled("Quitting in ", Style::default().fg(Color::Gray)),
                Span::styled(
                    format!("{}", n),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" second(s)...", Style::default().fg(Color::Gray)),
            ]),
            None => Line::from(vec![
                Span::styled("Press ", Style::default().fg(Color::Gray)),
                Span::styled(
                    "q",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" or ", Style::default().fg(Color::Gray)),
                Span::styled(
                    "ESC",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                Span::styled(" to exit", Style::default().fg(Color::Gray)),
            ]),
        };
        let footer = Paragraph::new(footer_line).block(Block::default().borders(Borders::ALL));
        f.render_widget(footer, chunks[3]);
    }

    fn render_ui_static(
        f: &mut Frame,
        packages_arc: &Arc<Mutex<Vec<PackageProgress>>>,
        total_packages: usize,
    ) {
        let packages = packages_arc.lock().unwrap();
        let completed = packages
            .iter()
            .filter(|p| p.state == ProgressState::Completed)
            .count();

        // Main layout
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(2)
            .constraints([
                Constraint::Length(3), // Title
                Constraint::Length(3), // Overall progress
                Constraint::Min(10),   // Package list
                Constraint::Length(3), // Footer
            ])
            .split(f.area());
        let (top, middle, bottom) =
            crate::utilities::brew_common::header_lines("Brew Recipe Install Manager");
        let title = Paragraph::new(vec![
            Line::from(vec![Span::styled(
                top,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(vec![Span::styled(
                middle,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(vec![Span::styled(
                bottom,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]),
        ]);
        f.render_widget(title, chunks[0]);

        // Overall progress
        let overall_progress = (completed as f64 / total_packages as f64 * 100.0) as u16;
        let progress_label = format!("{}/{} packages", completed, total_packages);
        let overall_gauge = Gauge::default()
            .block(Block::default().title("Progress").borders(Borders::ALL))
            .gauge_style(
                Style::default()
                    .fg(Color::Yellow)
                    .bg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            )
            .percent(overall_progress)
            .label(progress_label);
        f.render_widget(overall_gauge, chunks[1]);

        // Package list
        Self::render_package_list_static(f, chunks[2], &packages);

        // Footer
        let footer = Paragraph::new(Line::from(vec![
            Span::styled("Press ", Style::default().fg(Color::Gray)),
            Span::styled(
                "q",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " to quit (after completion) or ",
                Style::default().fg(Color::Gray),
            ),
            Span::styled(
                "ESC",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" to force quit", Style::default().fg(Color::Gray)),
        ]))
        .block(Block::default().borders(Borders::ALL));
        f.render_widget(footer, chunks[3]);
    }

    fn render_package_list_static(f: &mut Frame, area: Rect, packages: &[PackageProgress]) {
        let available_height = area.height.saturating_sub(2);
        let packages_per_screen = (available_height / 3).max(1) as usize;
        let first_active = packages
            .iter()
            .position(|p| p.state != ProgressState::Completed)
            .unwrap_or(0);

        let start_idx = first_active
            .saturating_sub(1)
            .min(packages.len().saturating_sub(packages_per_screen));
        let visible_packages: Vec<_> = packages
            .iter()
            .skip(start_idx)
            .take(packages_per_screen)
            .collect();

        let package_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                visible_packages
                    .iter()
                    .map(|_| Constraint::Length(3))
                    .collect::<Vec<_>>(),
            )
            .split(area);

        for (i, package) in visible_packages.iter().enumerate() {
            if i < package_chunks.len() {
                Self::render_package_static(f, package_chunks[i], package);
            }
        }
    }

    fn render_package_static(f: &mut Frame, area: Rect, package: &PackageProgress) {
        let gauge = Gauge::default()
            .block(
                Block::default()
                    .title(package.name.clone())
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(package.state_color())),
            )
            .gauge_style(Style::default().fg(package.state_color()).bg(Color::Black))
            .percent(package.progress)
            .label(if package.message.is_empty() {
                format!("{}%", package.progress)
            } else {
                package.message.clone()
            });
        f.render_widget(gauge, area);
    }

    pub fn run_with_updates<F>(&mut self, update_fn: F) -> io::Result<bool>
    where
        F: FnMut() -> bool,
    {
        self.run_with_updates_internal(update_fn, true)
    }

    pub fn run_without_summary<F>(&mut self, update_fn: F) -> io::Result<bool>
    where
        F: FnMut() -> bool,
    {
        self.run_with_updates_internal(update_fn, false)
    }

    fn run_with_updates_internal<F>(
        &mut self,
        mut update_fn: F,
        show_summary_at_end: bool,
    ) -> io::Result<bool>
    where
        F: FnMut() -> bool,
    {
        let mut user_cancelled = false;

        loop {
            self.draw()?;

            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Char('q') => {
                            if let Ok(packages) = self.packages.lock() {
                                let all_done = packages.iter().all(|p| {
                                    p.state == ProgressState::Completed
                                        || p.state == ProgressState::Failed
                                });
                                if all_done {
                                    user_cancelled = false;
                                    break;
                                }
                            }
                        }
                        KeyCode::Esc => {
                            user_cancelled = true;
                            break;
                        }
                        _ => {}
                    }
                }
            }

            if update_fn() {
                if show_summary_at_end {
                    self.show_summary = true;
                    let autoquit = self.autoquit_secs;
                    let mut remaining_secs = autoquit.unwrap_or(0);
                    let mut last_tick = Instant::now();
                    self.summary_countdown_secs = autoquit;
                    self.draw()?;

                    loop {
                        if event::poll(Duration::from_millis(100))? {
                            if let Event::Key(key) = event::read()? {
                                match key.code {
                                    KeyCode::Char('q') | KeyCode::Esc => break,
                                    _ => {}
                                }
                            }
                        }
                        if self.autoquit_secs.is_some()
                            && last_tick.elapsed() >= Duration::from_secs(1)
                        {
                            last_tick = Instant::now();
                            remaining_secs = remaining_secs.saturating_sub(1);
                            self.summary_countdown_secs = Some(remaining_secs);
                            if remaining_secs == 0 {
                                break;
                            }
                        }
                        self.draw()?;
                    }
                }
                break;
            }

            std::thread::sleep(Duration::from_millis(100));
        }

        Ok(!user_cancelled)
    }
}

impl Drop for ProgressTracker {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_progress_new_starts_pending() {
        let p = PackageProgress::new("wget".to_string());
        assert_eq!(p.state, ProgressState::Pending);
        assert_eq!(p.progress, 0);
        assert_eq!(p.name, "wget");
    }

    #[test]
    fn package_progress_completed_state_label() {
        let mut p = PackageProgress::new("wget".to_string());
        p.state = ProgressState::Completed;
        p.progress = 100;
        assert_eq!(p.state_label(), "completed");
    }
}
