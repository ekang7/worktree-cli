use crate::env_files::run_copy_env_hook_quiet;
use crate::error::{Result, WorktreeError};
use crate::hooks::HookManager;
use crate::status::{fetch_git_status, fetch_pr_status, gh_available, GitStatus, PrStatus, StatusUpdate};
use crate::terminal::{get_open_iterm_tab_names, TerminalManager};
use crate::worktree::{Worktree, WorktreeManager};
use crossterm::{
    cursor::Show,
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame, Terminal,
};
use std::io;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

enum AppMode {
    List,
    CreateNew,
    ConfirmDelete,
    StatusDetail,
}

enum CreateField {
    BranchName,
    BaseBranch,
}

pub struct App {
    worktrees: Vec<Worktree>,
    list_state: ListState,
    mode: AppMode,
    should_quit: bool,
    // Status display fields
    git_statuses: Vec<Option<GitStatus>>,
    pr_statuses: Vec<Option<PrStatus>>,
    status_rx: Option<Receiver<StatusUpdate>>,
    status_tx: mpsc::Sender<StatusUpdate>,
    // Open tab tracking
    open_tabs: Vec<bool>,
    // Create mode fields
    branch_name: String,
    available_branches: Vec<String>,
    selected_branch_idx: usize,
    default_branch_idx: usize,
    cursor_position: usize,
    active_field: CreateField,
    // Delete mode fields
    delete_worktree_idx: Option<usize>,
    delete_force: bool,
    // Repository info
    repo_root: PathBuf,
    // UI state
    needs_clear: bool,
    // Time tracking for periodic refreshes
    last_tabs_refresh: Instant,
    last_status_refresh: Instant,
    // Claude Code options
    skip_permissions: bool,
}

impl App {
    pub fn new(
        worktrees: Vec<Worktree>,
        available_branches: Vec<String>,
        default_branch_idx: usize,
        repo_root: PathBuf,
        skip_permissions: bool,
    ) -> Self {
        let mut list_state = ListState::default();
        if !worktrees.is_empty() {
            list_state.select(Some(0));
        }

        // Initialize status arrays with None (loading state)
        let git_statuses = vec![None; worktrees.len()];
        let pr_statuses = vec![None; worktrees.len()];

        // Check which worktrees have open iTerm2 tabs
        let open_tab_names = get_open_iterm_tab_names();
        let open_tabs = worktrees
            .iter()
            .map(|wt| {
                let dirname = wt.path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                let expected_name = format!("{} - {}", dirname, wt.branch);
                open_tab_names.iter().any(|name| name.contains(&expected_name))
            })
            .collect();

        // Set up channel for status updates
        let (tx, rx) = mpsc::channel();

        // Check gh availability once before spawning threads
        let gh_ok = gh_available();

        // Spawn one thread per worktree for parallel status fetching
        for (index, wt) in worktrees.iter().enumerate() {
            let tx = tx.clone();
            let path = wt.path.clone();
            let branch = wt.branch.clone();
            let repo_root_for_thread = repo_root.clone();

            std::thread::spawn(move || {
                // Fetch git status
                if let Ok(status) = fetch_git_status(&path) {
                    let _ = tx.send(StatusUpdate::GitStatus { index, status });
                }

                // Fetch PR status if gh is available
                if gh_ok {
                    if let Ok(status) = fetch_pr_status(&branch, &repo_root_for_thread) {
                        let _ = tx.send(StatusUpdate::PrStatus { index, status });
                    }
                }
            });
        }

        Self {
            worktrees,
            list_state,
            mode: AppMode::List,
            should_quit: false,
            git_statuses,
            pr_statuses,
            status_rx: Some(rx),
            status_tx: tx,
            open_tabs,
            branch_name: String::new(),
            available_branches,
            selected_branch_idx: default_branch_idx,
            default_branch_idx,
            cursor_position: 0,
            active_field: CreateField::BranchName,
            delete_worktree_idx: None,
            delete_force: false,
            repo_root,
            needs_clear: false,
            last_tabs_refresh: Instant::now(),
            last_status_refresh: Instant::now(),
            skip_permissions,
        }
    }

    /// Spawn background threads to fetch git and PR statuses for all worktrees
    fn spawn_status_fetchers(&mut self) {
        self.last_status_refresh = Instant::now();
        let gh_ok = gh_available();

        for (index, wt) in self.worktrees.iter().enumerate() {
            let tx = self.status_tx.clone();
            let path = wt.path.clone();
            let branch = wt.branch.clone();
            let repo_root = self.repo_root.clone();

            std::thread::spawn(move || {
                if let Ok(status) = fetch_git_status(&path) {
                    let _ = tx.send(StatusUpdate::GitStatus { index, status });
                }

                if gh_ok {
                    if let Ok(status) = fetch_pr_status(&branch, &repo_root) {
                        let _ = tx.send(StatusUpdate::PrStatus { index, status });
                    }
                }
            });
        }
    }

    /// Refresh the worktree list after deletion
    fn refresh_worktrees(&mut self, manager: &WorktreeManager) {
        if let Ok(worktrees) = manager.list_worktrees() {
            self.worktrees = worktrees;
            self.git_statuses = vec![None; self.worktrees.len()];
            self.pr_statuses = vec![None; self.worktrees.len()];

            // Refresh open tabs tracking
            let open_tab_names = get_open_iterm_tab_names();
            self.open_tabs = self
                .worktrees
                .iter()
                .map(|wt| {
                    let dirname = wt.path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    let expected_name = format!("{} - {}", dirname, wt.branch);
                    open_tab_names.iter().any(|name| name.contains(&expected_name))
                })
                .collect();

            // Adjust selection if needed
            if let Some(selected) = self.list_state.selected() {
                if selected >= self.worktrees.len() {
                    self.list_state.select(self.worktrees.len().checked_sub(1));
                }
            } else if !self.worktrees.is_empty() {
                self.list_state.select(Some(0));
            }

            // Re-spawn status fetchers
            self.spawn_status_fetchers();
        }
    }

    /// Poll for status updates from background thread
    fn poll_status_updates(&mut self) {
        if let Some(ref rx) = self.status_rx {
            // Drain all available updates
            while let Ok(update) = rx.try_recv() {
                match update {
                    StatusUpdate::GitStatus { index, status } => {
                        if index < self.git_statuses.len() {
                            self.git_statuses[index] = Some(status);
                        }
                    }
                    StatusUpdate::PrStatus { index, status } => {
                        if index < self.pr_statuses.len() {
                            self.pr_statuses[index] = status;
                        }
                    }
                }
            }
        }

        // Refresh open tabs periodically (every 2 seconds)
        if self.last_tabs_refresh.elapsed() > Duration::from_secs(2) {
            self.refresh_open_tabs();
        }

        // Refresh git/PR statuses periodically (every 5 seconds)
        if self.last_status_refresh.elapsed() > Duration::from_secs(5) {
            self.spawn_status_fetchers();
        }
    }

    /// Refresh open tabs tracking
    fn refresh_open_tabs(&mut self) {
        let open_tab_names = get_open_iterm_tab_names();
        self.open_tabs = self
            .worktrees
            .iter()
            .map(|wt| {
                let dirname = wt.path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                let expected_name = format!("{} - {}", dirname, wt.branch);
                open_tab_names.iter().any(|name| name.contains(&expected_name))
            })
            .collect();
        self.last_tabs_refresh = Instant::now();
    }

    /// Open terminal for the selected worktree
    fn open_terminal_for_worktree(&mut self, index: usize) {
        if let Some(worktree) = self.worktrees.get(index) {
            let terminal = TerminalManager::with_options(self.skip_permissions);
            let _ = terminal.open_terminal_with_panes(worktree);
            // Refresh open tabs immediately after opening
            self.refresh_open_tabs();
        }
    }

    /// Create a new worktree and open terminal for it
    fn create_and_open_worktree(&mut self, manager: &WorktreeManager) {
        let branch_name = self.branch_name.clone();
        let base_branch = self.get_selected_base_branch().to_string();

        // Create the worktree
        match manager.create_worktree(&branch_name, &base_branch, None, true) {
            Ok(worktree_path) => {
                // Run hooks (quiet mode to avoid TUI pollution)
                let hook_manager = HookManager::new(&self.repo_root);
                let _ = run_copy_env_hook_quiet(&self.repo_root, &worktree_path);
                let _ = hook_manager.run_hooks_quiet("post-create", &worktree_path, &branch_name);

                // Open terminal (quiet mode to avoid TUI pollution)
                let terminal = TerminalManager::with_options(self.skip_permissions);
                let _ = terminal.open_terminal_with_panes_for_path_quiet(&worktree_path, &branch_name);

                // Refresh worktree list and open tabs
                self.refresh_worktrees(manager);
            }
            Err(_) => {
                // TODO: Show error in TUI
            }
        }

        // Clear input and return to list mode
        self.branch_name.clear();
        self.cursor_position = 0;
        self.mode = AppMode::List;
    }

    fn get_selected_base_branch(&self) -> &str {
        self.available_branches
            .get(self.selected_branch_idx)
            .map(|s| s.as_str())
            .unwrap_or("main")
    }

    fn handle_list_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.should_quit = true;
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
            KeyCode::Char('n') => {
                self.mode = AppMode::CreateNew;
                self.branch_name.clear();
                self.selected_branch_idx = self.default_branch_idx;
                self.cursor_position = 0;
                self.active_field = CreateField::BranchName;
            }
            KeyCode::Char('d') | KeyCode::Delete => {
                if let Some(selected) = self.list_state.selected() {
                    // Don't allow deleting bare/main worktree
                    if !self.worktrees[selected].is_bare {
                        self.delete_worktree_idx = Some(selected);
                        self.mode = AppMode::ConfirmDelete;
                    }
                }
            }
            KeyCode::Char('s') => {
                if self.list_state.selected().is_some() {
                    self.mode = AppMode::StatusDetail;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.select_next();
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.select_previous();
            }
            KeyCode::Enter => {
                if let Some(selected) = self.list_state.selected() {
                    self.open_terminal_for_worktree(selected);
                }
            }
            _ => {}
        }
    }

    fn handle_delete_input(&mut self, key: KeyEvent, manager: &WorktreeManager) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Enter => {
                // Confirmed deletion (normal)
                self.perform_deletion(manager, false);
            }
            KeyCode::Char('f') => {
                // Force deletion
                self.perform_deletion(manager, true);
            }
            KeyCode::Char('n') | KeyCode::Esc => {
                self.delete_worktree_idx = None;
                self.delete_force = false;
                self.mode = AppMode::List;
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.delete_worktree_idx = None;
                self.delete_force = false;
                self.mode = AppMode::List;
            }
            _ => {}
        }
    }

    /// Perform worktree deletion inline within the TUI
    fn perform_deletion(&mut self, manager: &WorktreeManager, force: bool) {
        if let Some(idx) = self.delete_worktree_idx {
            let worktree = &self.worktrees[idx];
            let path = worktree.path.clone();
            let branch = worktree.branch.clone();

            // Run pre-remove hooks (quiet mode to avoid TUI pollution)
            let hook_manager = HookManager::new(&self.repo_root);
            let _ = hook_manager.run_hooks_quiet("pre-remove", &path, &branch);

            // Attempt to remove the worktree
            if manager.remove_worktree(&path, force).is_ok() {
                // Refresh the worktree list
                self.refresh_worktrees(manager);
            }

            // Clear delete state and return to list mode
            self.delete_worktree_idx = None;
            self.delete_force = false;
            self.mode = AppMode::List;
            self.needs_clear = true; // Force full terminal redraw
        }
    }

    fn handle_status_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('s') | KeyCode::Enter => {
                self.mode = AppMode::List;
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.mode = AppMode::List;
            }
            _ => {}
        }
    }

    fn handle_create_input(&mut self, key: KeyEvent, manager: &WorktreeManager) {
        // BaseBranch is a selector, handle differently
        if matches!(self.active_field, CreateField::BaseBranch) {
            match key.code {
                KeyCode::Esc => {
                    self.mode = AppMode::List;
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.mode = AppMode::List;
                }
                KeyCode::Tab => {
                    self.active_field = CreateField::BranchName;
                    self.update_cursor_position();
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if self.selected_branch_idx > 0 {
                        self.selected_branch_idx -= 1;
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if self.selected_branch_idx < self.available_branches.len().saturating_sub(1) {
                        self.selected_branch_idx += 1;
                    }
                }
                KeyCode::Enter => {
                    if !self.branch_name.is_empty() {
                        self.create_and_open_worktree(manager);
                    }
                }
                _ => {}
            }
            return;
        }

        // BranchName is text input
        // Check for modifier keys first
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('c') => {
                    self.mode = AppMode::List;
                    return;
                }
                KeyCode::Char('a') => {
                    self.cursor_position = 0;
                    return;
                }
                KeyCode::Char('e') => {
                    let len = self.branch_name.len();
                    self.cursor_position = len;
                    return;
                }
                KeyCode::Char('w') => {
                    self.delete_word_backward();
                    return;
                }
                KeyCode::Char('k') => {
                    let pos = self.cursor_position;
                    self.branch_name.truncate(pos);
                    return;
                }
                _ => {}
            }
        }

        if key.modifiers.contains(KeyModifiers::ALT) {
            match key.code {
                KeyCode::Left => {
                    self.move_cursor_left_word();
                    return;
                }
                KeyCode::Right => {
                    self.move_cursor_right_word();
                    return;
                }
                _ => {}
            }
        }

        // Handle regular keys
        match key.code {
            KeyCode::Esc => {
                self.mode = AppMode::List;
            }
            KeyCode::Tab => {
                self.active_field = CreateField::BaseBranch;
            }
            KeyCode::Enter => {
                if !self.branch_name.is_empty() {
                    self.create_and_open_worktree(manager);
                }
            }
            KeyCode::Backspace => {
                if self.cursor_position > 0 {
                    let pos = self.cursor_position;
                    self.branch_name.remove(pos - 1);
                    self.cursor_position -= 1;
                }
            }
            KeyCode::Delete => {
                let pos = self.cursor_position;
                let len = self.branch_name.len();
                if pos < len {
                    self.branch_name.remove(pos);
                }
            }
            KeyCode::Left => {
                if self.cursor_position > 0 {
                    self.cursor_position -= 1;
                }
            }
            KeyCode::Right => {
                let text_len = self.branch_name.len();
                if self.cursor_position < text_len {
                    self.cursor_position += 1;
                }
            }
            KeyCode::Home => {
                self.cursor_position = 0;
            }
            KeyCode::End => {
                let len = self.branch_name.len();
                self.cursor_position = len;
            }
            KeyCode::Char(c) => {
                let pos = self.cursor_position;
                self.branch_name.insert(pos, c);
                self.cursor_position += 1;
            }
            _ => {}
        }
    }

    fn update_cursor_position(&mut self) {
        self.cursor_position = self.branch_name.len();
    }

    fn move_cursor_left_word(&mut self) {
        if self.cursor_position == 0 {
            return;
        }

        let mut pos = self.cursor_position - 1;
        // Skip whitespace
        while pos > 0 && self.branch_name.chars().nth(pos).unwrap().is_whitespace() {
            pos -= 1;
        }
        // Skip word characters
        while pos > 0 && !self.branch_name.chars().nth(pos - 1).unwrap().is_whitespace() {
            pos -= 1;
        }
        self.cursor_position = pos;
    }

    fn move_cursor_right_word(&mut self) {
        let len = self.branch_name.len();
        if self.cursor_position >= len {
            return;
        }

        let mut pos = self.cursor_position;
        // Skip word characters
        while pos < len && !self.branch_name.chars().nth(pos).unwrap().is_whitespace() {
            pos += 1;
        }
        // Skip whitespace
        while pos < len && self.branch_name.chars().nth(pos).unwrap().is_whitespace() {
            pos += 1;
        }
        self.cursor_position = pos;
    }

    fn delete_word_backward(&mut self) {
        if self.cursor_position == 0 {
            return;
        }

        // Calculate positions first without holding a reference
        let original_pos = self.cursor_position;
        let mut pos = self.cursor_position - 1;

        // Skip whitespace
        while pos > 0 && self.branch_name.chars().nth(pos).unwrap().is_whitespace() {
            pos -= 1;
        }
        // Skip word characters
        while pos > 0 && !self.branch_name.chars().nth(pos - 1).unwrap().is_whitespace() {
            pos -= 1;
        }

        // Now mutate
        self.branch_name.drain(pos..original_pos);
        self.cursor_position = pos;
    }

    fn select_next(&mut self) {
        if self.worktrees.is_empty() {
            return;
        }

        let i = match self.list_state.selected() {
            Some(i) => {
                if i >= self.worktrees.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn select_previous(&mut self) {
        if self.worktrees.is_empty() {
            return;
        }

        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.worktrees.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn render_list(&mut self, frame: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = self
            .worktrees
            .iter()
            .enumerate()
            .map(|(i, wt)| {
                let branch = if wt.branch.is_empty() {
                    "(bare)".to_string()
                } else {
                    wt.branch.clone()
                };

                // Open tab indicator
                let open_indicator = if self.open_tabs.get(i).copied().unwrap_or(false) {
                    Span::styled("● ", Style::default().fg(Color::Green))
                } else {
                    Span::raw("  ")
                };

                let mut spans = vec![
                    open_indicator,
                    Span::styled(
                        format!("{:24}", branch),  // Reduced width to accommodate indicator
                        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                    ),
                ];

                // Git status
                let git_status_str = match &self.git_statuses.get(i).and_then(|s| s.as_ref()) {
                    Some(status) => status.format_display(),
                    None => "[...]".to_string(),
                };
                let git_status_style = if git_status_str == "[clean]" {
                    Style::default().fg(Color::Green)
                } else if git_status_str == "[...]" {
                    Style::default().fg(Color::DarkGray)
                } else {
                    Style::default().fg(Color::Yellow)
                };
                spans.push(Span::styled(
                    format!("{:14}", git_status_str),
                    git_status_style,
                ));

                // PR status
                if let Some(Some(pr_status)) = self.pr_statuses.get(i) {
                    let pr_str = pr_status.format_display();
                    let pr_style = match pr_status.state {
                        crate::status::PrState::Draft => Style::default().fg(Color::Gray),
                        crate::status::PrState::Merged => Style::default().fg(Color::Magenta),
                        crate::status::PrState::Closed => Style::default().fg(Color::Red),
                        crate::status::PrState::Open => {
                            if pr_status.ci_passing == Some(true) && pr_status.approved > 0 {
                                Style::default().fg(Color::Green)
                            } else if pr_status.ci_passing == Some(false) || pr_status.changes_requested > 0 {
                                Style::default().fg(Color::Red)
                            } else {
                                Style::default().fg(Color::Blue)
                            }
                        }
                    };
                    spans.push(Span::styled(format!("{:20}", pr_str), pr_style));
                } else {
                    spans.push(Span::raw("                    ")); // 20 spaces placeholder
                }

                // Path
                spans.push(Span::styled(
                    format!(" {}", wt.path.display()),
                    Style::default().fg(Color::DarkGray),
                ));

                let content = Line::from(spans);
                ListItem::new(content)
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Git Worktrees ")
                    .title_style(Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");

        frame.render_stateful_widget(list, area, &mut self.list_state);
    }

    fn render_create_form(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(10),
                Constraint::Min(1),
            ])
            .split(area);

        // Branch name field
        let branch_active = matches!(self.active_field, CreateField::BranchName);
        let branch_style = if branch_active {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::White)
        };

        let branch_input = Paragraph::new(self.branch_name.as_str())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Branch Name ")
                    .title_style(branch_style)
                    .border_style(branch_style),
            )
            .style(Style::default().fg(Color::White));
        frame.render_widget(branch_input, chunks[0]);

        // Set cursor position for branch name field
        if branch_active {
            frame.set_cursor(
                // Account for border (1) + cursor position
                chunks[0].x + 1 + self.cursor_position as u16,
                chunks[0].y + 1, // Account for top border
            );
        }

        // Base branch selector
        let base_active = matches!(self.active_field, CreateField::BaseBranch);
        let base_style = if base_active {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::White)
        };

        let branch_items: Vec<ListItem> = self
            .available_branches
            .iter()
            .map(|branch| ListItem::new(Line::from(branch.as_str())))
            .collect();

        let mut branch_list_state = ListState::default();
        branch_list_state.select(Some(self.selected_branch_idx));

        let branch_list = List::new(branch_items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Base Branch (↑/↓ to select) ")
                    .title_style(base_style)
                    .border_style(base_style),
            )
            .highlight_style(
                Style::default()
                    .bg(if base_active { Color::DarkGray } else { Color::Black })
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(if base_active { "▶ " } else { "  " });

        frame.render_stateful_widget(branch_list, chunks[1], &mut branch_list_state.clone());

        // Help text
        let help_text = Text::from(vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("Tab", Style::default().fg(Color::Cyan)),
                Span::raw(" - Switch fields"),
            ]),
            Line::from(vec![
                Span::styled("Enter", Style::default().fg(Color::Cyan)),
                Span::raw(" - Create worktree"),
            ]),
            Line::from(vec![
                Span::styled("Esc", Style::default().fg(Color::Cyan)),
                Span::raw(" - Cancel"),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Ctrl+A", Style::default().fg(Color::Yellow)),
                Span::raw(" - Go to start  "),
                Span::styled("Ctrl+E", Style::default().fg(Color::Yellow)),
                Span::raw(" - Go to end"),
            ]),
            Line::from(vec![
                Span::styled("Alt+←/→", Style::default().fg(Color::Yellow)),
                Span::raw(" - Move by word  "),
                Span::styled("Ctrl+W", Style::default().fg(Color::Yellow)),
                Span::raw(" - Delete word"),
            ]),
            Line::from(vec![
                Span::styled("Ctrl+K", Style::default().fg(Color::Yellow)),
                Span::raw(" - Delete to end"),
            ]),
        ]);

        let help = Paragraph::new(help_text)
            .block(Block::default().borders(Borders::ALL).title(" Help "))
            .wrap(Wrap { trim: true });
        frame.render_widget(help, chunks[2]);
    }

    fn render_delete_confirm(&self, frame: &mut Frame, area: Rect) {
        // Render the list in the background (dimmed)
        self.render_list_dimmed(frame, area);

        // Render confirmation dialog in center
        let dialog_width = 60;
        let dialog_height = 9;
        let dialog_x = (area.width.saturating_sub(dialog_width)) / 2;
        let dialog_y = (area.height.saturating_sub(dialog_height)) / 2;
        let dialog_area = Rect::new(
            area.x + dialog_x,
            area.y + dialog_y,
            dialog_width.min(area.width),
            dialog_height.min(area.height),
        );

        // Clear the area behind the dialog
        let clear_block = Block::default().style(Style::default().bg(Color::Black));
        frame.render_widget(clear_block, dialog_area);

        if let Some(idx) = self.delete_worktree_idx {
            let wt = &self.worktrees[idx];
            let branch = if wt.branch.is_empty() { "(bare)" } else { &wt.branch };

            // Check if worktree has changes
            let has_changes = self.git_statuses
                .get(idx)
                .and_then(|s| s.as_ref())
                .map(|s| !s.is_clean())
                .unwrap_or(false);

            let mut lines = vec![
                Line::from(""),
                Line::from(vec![
                    Span::raw("Delete worktree "),
                    Span::styled(branch, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                    Span::raw("?"),
                ]),
            ];

            if has_changes {
                lines.push(Line::from(Span::styled(
                    "⚠ Has uncommitted changes",
                    Style::default().fg(Color::Yellow),
                )));
            }

            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("y", Style::default().fg(Color::Green)),
                Span::raw(" Delete  "),
                Span::styled("f", Style::default().fg(Color::Yellow)),
                Span::raw(" Force delete  "),
                Span::styled("n/Esc", Style::default().fg(Color::Red)),
                Span::raw(" Cancel"),
            ]));

            let text = Text::from(lines);

            let dialog = Paragraph::new(text)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Red))
                        .title(" Confirm Delete ")
                        .title_style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                )
                .alignment(ratatui::layout::Alignment::Center);

            frame.render_widget(dialog, dialog_area);
        }
    }

    fn render_list_dimmed(&self, frame: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = self
            .worktrees
            .iter()
            .map(|wt| {
                let branch = if wt.branch.is_empty() {
                    "(bare)".to_string()
                } else {
                    wt.branch.clone()
                };
                ListItem::new(Line::from(Span::styled(
                    format!("{:26} {}", branch, wt.path.display()),
                    Style::default().fg(Color::DarkGray),
                )))
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Git Worktrees ")
                    .border_style(Style::default().fg(Color::DarkGray)),
            );

        frame.render_widget(list, area);
    }

    fn render_status_detail(&self, frame: &mut Frame, area: Rect) {
        let selected_idx = match self.list_state.selected() {
            Some(idx) => idx,
            None => return,
        };

        let wt = &self.worktrees[selected_idx];
        let git_status = self.git_statuses.get(selected_idx).and_then(|s| s.as_ref());
        let pr_status = self.pr_statuses.get(selected_idx).and_then(|s| s.as_ref());

        let branch = if wt.branch.is_empty() { "(bare)" } else { &wt.branch };

        let mut lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("Branch: ", Style::default().fg(Color::Gray)),
                Span::styled(branch, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled("Path:   ", Style::default().fg(Color::Gray)),
                Span::raw(wt.path.display().to_string()),
            ]),
            Line::from(vec![
                Span::styled("Commit: ", Style::default().fg(Color::Gray)),
                Span::styled(&wt.commit[..8.min(wt.commit.len())], Style::default().fg(Color::Yellow)),
            ]),
            Line::from(""),
            Line::from(Span::styled("── Git Status ──", Style::default().fg(Color::Magenta))),
        ];

        if let Some(status) = git_status {
            if status.is_clean() {
                lines.push(Line::from(Span::styled("  Working tree clean", Style::default().fg(Color::Green))));
            } else {
                if status.staged > 0 {
                    lines.push(Line::from(vec![
                        Span::styled("  Staged:    ", Style::default().fg(Color::Gray)),
                        Span::styled(format!("{} files", status.staged), Style::default().fg(Color::Green)),
                    ]));
                }
                if status.unstaged > 0 {
                    lines.push(Line::from(vec![
                        Span::styled("  Modified:  ", Style::default().fg(Color::Gray)),
                        Span::styled(format!("{} files", status.unstaged), Style::default().fg(Color::Yellow)),
                    ]));
                }
                if status.untracked > 0 {
                    lines.push(Line::from(vec![
                        Span::styled("  Untracked: ", Style::default().fg(Color::Gray)),
                        Span::styled(format!("{} files", status.untracked), Style::default().fg(Color::Red)),
                    ]));
                }
            }
        } else {
            lines.push(Line::from(Span::styled("  Loading...", Style::default().fg(Color::DarkGray))));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("── PR Status ──", Style::default().fg(Color::Magenta))));

        if let Some(pr) = pr_status {
            let state_style = match pr.state {
                crate::status::PrState::Open => Style::default().fg(Color::Blue),
                crate::status::PrState::Draft => Style::default().fg(Color::Gray),
                crate::status::PrState::Merged => Style::default().fg(Color::Magenta),
                crate::status::PrState::Closed => Style::default().fg(Color::Red),
            };
            let state_str = match pr.state {
                crate::status::PrState::Open => "Open",
                crate::status::PrState::Draft => "Draft",
                crate::status::PrState::Merged => "Merged",
                crate::status::PrState::Closed => "Closed",
            };

            lines.push(Line::from(vec![
                Span::styled("  PR #", Style::default().fg(Color::Gray)),
                Span::styled(pr.number.to_string(), Style::default().fg(Color::Cyan)),
                Span::raw(" "),
                Span::styled(state_str, state_style),
            ]));

            if matches!(pr.state, crate::status::PrState::Open | crate::status::PrState::Draft) {
                // Reviews
                if pr.approved > 0 || pr.changes_requested > 0 {
                    let mut review_spans = vec![Span::styled("  Reviews:   ", Style::default().fg(Color::Gray))];
                    if pr.approved > 0 {
                        review_spans.push(Span::styled(format!("{} approved", pr.approved), Style::default().fg(Color::Green)));
                    }
                    if pr.approved > 0 && pr.changes_requested > 0 {
                        review_spans.push(Span::raw(", "));
                    }
                    if pr.changes_requested > 0 {
                        review_spans.push(Span::styled(format!("{} changes requested", pr.changes_requested), Style::default().fg(Color::Red)));
                    }
                    lines.push(Line::from(review_spans));
                }

                // CI status
                if let Some(passing) = pr.ci_passing {
                    let ci_style = if passing {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default().fg(Color::Red)
                    };
                    let ci_str = if passing { "Passing" } else { "Failing" };
                    lines.push(Line::from(vec![
                        Span::styled("  CI:        ", Style::default().fg(Color::Gray)),
                        Span::styled(ci_str, ci_style),
                    ]));
                }
            }
        } else {
            lines.push(Line::from(Span::styled("  No PR found", Style::default().fg(Color::DarkGray))));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("Press ", Style::default().fg(Color::DarkGray)),
            Span::styled("Esc", Style::default().fg(Color::Cyan)),
            Span::styled(" to close", Style::default().fg(Color::DarkGray)),
        ]));

        let text = Text::from(lines);
        let detail = Paragraph::new(text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Status Details ")
                    .title_style(Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            )
            .wrap(Wrap { trim: true });

        frame.render_widget(detail, area);
    }

    fn render_help(&self, frame: &mut Frame, area: Rect) {
        let help_text = match self.mode {
            AppMode::List => Text::from(vec![
                Line::from(vec![
                    Span::styled("↑/↓", Style::default().fg(Color::Cyan)),
                    Span::raw(" Navigate  "),
                    Span::styled("Enter", Style::default().fg(Color::Cyan)),
                    Span::raw(" Select  "),
                    Span::styled("s", Style::default().fg(Color::Yellow)),
                    Span::raw(" Status  "),
                    Span::styled("n", Style::default().fg(Color::Green)),
                    Span::raw(" New  "),
                    Span::styled("d", Style::default().fg(Color::Red)),
                    Span::raw(" Delete  "),
                    Span::styled("q", Style::default().fg(Color::DarkGray)),
                    Span::raw(" Quit"),
                ]),
            ]),
            AppMode::CreateNew => Text::from(vec![
                Line::from(vec![
                    Span::styled("Tab", Style::default().fg(Color::Cyan)),
                    Span::raw(" - Switch fields  "),
                    Span::styled("Enter", Style::default().fg(Color::Green)),
                    Span::raw(" - Create  "),
                    Span::styled("Esc", Style::default().fg(Color::Red)),
                    Span::raw(" - Cancel"),
                ]),
            ]),
            AppMode::ConfirmDelete => Text::from(vec![
                Line::from(vec![
                    Span::styled("y/Enter", Style::default().fg(Color::Green)),
                    Span::raw(" - Confirm  "),
                    Span::styled("n/Esc", Style::default().fg(Color::Red)),
                    Span::raw(" - Cancel"),
                ]),
            ]),
            AppMode::StatusDetail => Text::from(vec![
                Line::from(vec![
                    Span::styled("Esc/s/Enter", Style::default().fg(Color::Cyan)),
                    Span::raw(" - Close"),
                ]),
            ]),
        };

        let help = Paragraph::new(help_text).block(Block::default().borders(Borders::ALL));
        frame.render_widget(help, area);
    }
}

// Set iTerm2 tab color to red (worktree-cli active indicator)
fn set_iterm_tab_red() {
    use std::io::Write;
    print!("\x1b]6;1;bg;red;brightness;180\x07");
    print!("\x1b]6;1;bg;green;brightness;60\x07");
    print!("\x1b]6;1;bg;blue;brightness;60\x07");
    let _ = io::stdout().flush();
}

// Reset iTerm2 tab color to default
fn reset_iterm_tab_color() {
    use std::io::Write;
    print!("\x1b]6;1;bg;*;default\x07");
    let _ = io::stdout().flush();
}

pub fn run_tui(manager: &WorktreeManager, skip_permissions: bool) -> Result<()> {
    let worktrees = manager.list_worktrees()?;
    let available_branches = manager.list_branches()?;
    let default_branch = manager.get_default_branch()?;
    let repo_root = manager.repo_root().to_path_buf();

    // Find the index of the default branch
    let default_branch_idx = available_branches
        .iter()
        .position(|b| b == &default_branch)
        .unwrap_or(0);

    // Set tab color to red while TUI is active
    set_iterm_tab_red();

    // Setup terminal
    enable_raw_mode().map_err(|e| WorktreeError::Terminal(e.to_string()))?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).map_err(|e| WorktreeError::Terminal(e.to_string()))?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal =
        Terminal::new(backend).map_err(|e| WorktreeError::Terminal(e.to_string()))?;

    let mut app = App::new(worktrees, available_branches, default_branch_idx, repo_root, skip_permissions);

    let result = run_app(&mut terminal, &mut app, manager);

    // Restore terminal
    disable_raw_mode().map_err(|e| WorktreeError::Terminal(e.to_string()))?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, Show)
        .map_err(|e| WorktreeError::Terminal(e.to_string()))?;

    // Reset tab color back to default
    reset_iterm_tab_color();

    // Flush to ensure terminal commands are fully executed
    use std::io::Write;
    io::stdout().flush().map_err(|e| WorktreeError::Terminal(e.to_string()))?;

    result
}

fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    manager: &WorktreeManager,
) -> Result<()> {
    loop {
        // Poll for status updates from background thread
        app.poll_status_updates();

        // Clear terminal if needed (e.g., after deletion dialog)
        if app.needs_clear {
            terminal.clear().map_err(|e| WorktreeError::Terminal(e.to_string()))?;
            app.needs_clear = false;
        }

        terminal
            .draw(|frame| {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Min(0), Constraint::Length(3)])
                    .split(frame.size());

                match app.mode {
                    AppMode::List => app.render_list(frame, chunks[0]),
                    AppMode::CreateNew => app.render_create_form(frame, chunks[0]),
                    AppMode::ConfirmDelete => app.render_delete_confirm(frame, chunks[0]),
                    AppMode::StatusDetail => app.render_status_detail(frame, chunks[0]),
                }

                app.render_help(frame, chunks[1]);
            })
            .map_err(|e| WorktreeError::Terminal(e.to_string()))?;

        // Use poll with timeout instead of blocking read for responsiveness
        if event::poll(Duration::from_millis(100)).map_err(|e| WorktreeError::Terminal(e.to_string()))? {
            if let Event::Key(key) = event::read().map_err(|e| WorktreeError::Terminal(e.to_string()))? {
                match app.mode {
                    AppMode::List => app.handle_list_input(key),
                    AppMode::CreateNew => app.handle_create_input(key, manager),
                    AppMode::ConfirmDelete => app.handle_delete_input(key, manager),
                    AppMode::StatusDetail => app.handle_status_input(key),
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}
