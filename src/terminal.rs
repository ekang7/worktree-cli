use crate::error::{Result, WorktreeError};
use crate::worktree::Worktree;
use colored::Colorize;
use std::env;
use std::path::Path;
use std::process::Command;

pub enum TerminalType {
    ITerm2,
    TerminalApp,
    Unknown,
}

pub struct TerminalManager {
    terminal_type: TerminalType,
    skip_permissions: bool,
}

impl TerminalManager {
    pub fn detect() -> Self {
        Self::with_options(false)
    }

    pub fn with_options(skip_permissions: bool) -> Self {
        let terminal_type = match env::var("TERM_PROGRAM") {
            Ok(term) if term == "iTerm.app" => TerminalType::ITerm2,
            Ok(term) if term == "Apple_Terminal" => TerminalType::TerminalApp,
            _ => TerminalType::Unknown,
        };

        Self { terminal_type, skip_permissions }
    }

    fn claude_args(&self) -> &'static str {
        if self.skip_permissions {
            "claude --dangerously-skip-permissions"
        } else {
            "claude"
        }
    }

    /// Check if an iTerm2 tab already exists for this worktree and switch to it if found
    fn find_existing_iterm_tab(&self, worktree_path: &Path, branch: &str) -> Option<()> {
        let dirname = worktree_path.file_name()?.to_str()?;
        let tab_name = format!("{} - {}", dirname, branch);

        let script = format!(
            r#"
tell application "iTerm2"
    repeat with w in windows
        tell w
            repeat with t in tabs
                tell t
                    repeat with s in sessions
                        if name of s contains "{tab_name}" then
                            select t
                            tell application "iTerm2" to activate
                            return "found"
                        end if
                    end repeat
                end tell
            end repeat
        end tell
    end repeat
end tell
return "not_found"
"#,
            tab_name = tab_name
        );

        let output = Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output()
            .ok()?;

        let result = String::from_utf8_lossy(&output.stdout);
        if result.trim() == "found" {
            Some(())
        } else {
            None
        }
    }

    pub fn launch_claude(&self, worktree_path: &Path) -> Result<()> {
        // Check if Claude CLI is available
        let claude_check = Command::new("which")
            .arg("claude")
            .output()
            .map_err(|e| WorktreeError::Terminal(format!("Failed to check for Claude CLI: {}", e)))?;

        if !claude_check.status.success() {
            println!("{}", "Claude CLI not found!".bright_red());
            println!("To install Claude CLI, visit: https://claude.com/download");
            return Err(WorktreeError::Terminal("Claude CLI not installed".to_string()));
        }

        println!(
            "{}",
            format!("Launching Claude Code in {}...", worktree_path.display()).bright_blue()
        );

        let mut cmd = Command::new("claude");
        if self.skip_permissions {
            cmd.arg("--dangerously-skip-permissions");
        }
        let output = cmd
            .current_dir(worktree_path)
            .spawn()
            .map_err(|e| WorktreeError::Terminal(format!("Failed to launch Claude: {}", e)))?;

        println!("{}", format!("Claude Code launched (PID: {:?})", output.id()).bright_green());
        Ok(())
    }

    pub fn open_terminal_with_panes(&self, worktree: &Worktree) -> Result<()> {
        self.open_terminal_with_panes_for_path(&worktree.path, &worktree.branch)
    }

    pub fn open_terminal_with_panes_for_path(&self, worktree_path: &Path, branch_name: &str) -> Result<()> {
        match self.terminal_type {
            TerminalType::ITerm2 => {
                // Check if tab already exists for this worktree
                if self.find_existing_iterm_tab(worktree_path, branch_name).is_some() {
                    return Ok(()); // Tab found and activated
                }
                self.open_iterm2_panes_for_path(worktree_path, branch_name)
            }
            TerminalType::TerminalApp => self.open_terminal_app_panes(worktree_path),
            TerminalType::Unknown => {
                println!("{}", "Terminal type not detected. Opening Claude Code in current terminal...".yellow());
                self.launch_claude(worktree_path)
            }
        }
    }

    /// Open terminal with panes silently (for TUI mode where stdout is captured)
    pub fn open_terminal_with_panes_for_path_quiet(&self, worktree_path: &Path, branch_name: &str) -> Result<()> {
        match self.terminal_type {
            TerminalType::ITerm2 => {
                // Check if tab already exists for this worktree
                if self.find_existing_iterm_tab(worktree_path, branch_name).is_some() {
                    return Ok(()); // Tab found and activated
                }
                self.open_iterm2_panes_for_path(worktree_path, branch_name)
            }
            TerminalType::TerminalApp => self.open_terminal_app_panes(worktree_path),
            TerminalType::Unknown => {
                // In quiet mode, just skip if terminal type is unknown
                // The TUI is managing the terminal, so we can't launch Claude in it
                Ok(())
            }
        }
    }

    fn open_iterm2_panes_for_path(&self, worktree_path: &Path, branch_name: &str) -> Result<()> {
        let path_str = worktree_path.to_str().ok_or_else(|| {
            WorktreeError::InvalidPath("Invalid UTF-8 in path".to_string())
        })?;

        // Extract directory name from path
        let dir_name = worktree_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("worktree");

        // Tab title format matches bash script: "dirname - branch" (no brackets)
        let tab_name = format!("{} - {}", dir_name, branch_name);

        // AppleScript to create split panes with tab color
        // Matches the bash script's iTerm setup
        // Note: \e]1;...\a sets tab title, \e]6;1;bg;...;\a sets tab color
        let claude_cmd = self.claude_args();
        let script = format!(
            r#"
tell application "iTerm2"
    activate
    if (count of windows) = 0 then
        create window with default profile
    end if
    tell current window
        create tab with default profile
        tell current tab
            tell first session
                set name to "{tab_name}"
                write text "cd '{path}' && printf '\\033]1;{tab_name}\\007' && printf '\\033]6;1;bg;red;brightness;60\\007' && printf '\\033]6;1;bg;green;brightness;80\\007' && printf '\\033]6;1;bg;blue;brightness;180\\007'"
                split vertically with default profile
            end tell
            tell first session
                write text "{claude_cmd}"
            end tell
            tell last session
                set name to "{tab_name}"
                write text "cd '{path}' && printf '\\033]1;{tab_name}\\007' && printf '\\033]6;1;bg;red;brightness;60\\007' && printf '\\033]6;1;bg;green;brightness;80\\007' && printf '\\033]6;1;bg;blue;brightness;180\\007'"
            end tell
        end tell
    end tell
end tell
"#,
            tab_name = tab_name,
            path = path_str,
            claude_cmd = claude_cmd
        );

        let output = Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output()
            .map_err(|e| WorktreeError::Terminal(format!("Failed to execute AppleScript: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(WorktreeError::Terminal(format!(
                "AppleScript failed: {}",
                stderr
            )));
        }

        Ok(())
    }

    fn open_terminal_app_panes(&self, worktree_path: &Path) -> Result<()> {
        let path_str = worktree_path.to_str().ok_or_else(|| {
            WorktreeError::InvalidPath("Invalid UTF-8 in path".to_string())
        })?;

        // Terminal.app doesn't support splitting panes via AppleScript
        // So we'll open two separate tabs instead
        let claude_cmd = self.claude_args();
        let script = format!(
            r#"
tell application "Terminal"
    activate
    do script "cd '{path}' && {claude_cmd}"
    tell application "System Events" to keystroke "t" using command down
    delay 0.5
    do script "cd '{path}'" in front window
end tell
"#,
            path = path_str,
            claude_cmd = claude_cmd
        );

        let output = Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output()
            .map_err(|e| WorktreeError::Terminal(format!("Failed to execute AppleScript: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(WorktreeError::Terminal(format!(
                "AppleScript failed: {}",
                stderr
            )));
        }

        Ok(())
    }

    pub fn launch_claude_in_new_tab(&self, worktree_path: &Path) -> Result<()> {
        let path_str = worktree_path.to_str().ok_or_else(|| {
            WorktreeError::InvalidPath("Invalid UTF-8 in path".to_string())
        })?;

        let claude_cmd = self.claude_args();
        match self.terminal_type {
            TerminalType::ITerm2 => {
                let script = format!(
                    r#"
tell application "iTerm"
    activate
    tell current window
        create tab with default profile
        tell current session
            write text "cd '{path}' && {claude_cmd}"
        end tell
    end tell
end tell
"#,
                    path = path_str,
                    claude_cmd = claude_cmd
                );

                let output = Command::new("osascript")
                    .arg("-e")
                    .arg(&script)
                    .output()
                    .map_err(|e| WorktreeError::Terminal(format!("Failed to execute AppleScript: {}", e)))?;

                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    return Err(WorktreeError::Terminal(format!(
                        "AppleScript failed: {}",
                        stderr
                    )));
                }
            }
            TerminalType::TerminalApp => {
                let script = format!(
                    r#"
tell application "Terminal"
    activate
    tell application "System Events"
        keystroke "t" using command down
    end tell
    delay 0.5
    do script "cd '{path}' && {claude_cmd}" in front window
end tell
"#,
                    path = path_str,
                    claude_cmd = claude_cmd
                );

                let output = Command::new("osascript")
                    .arg("-e")
                    .arg(&script)
                    .output()
                    .map_err(|e| WorktreeError::Terminal(format!("Failed to execute AppleScript: {}", e)))?;

                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    return Err(WorktreeError::Terminal(format!(
                        "AppleScript failed: {}",
                        stderr
                    )));
                }
            }
            TerminalType::Unknown => {
                println!("{}", "To launch Claude Code in the worktree, run:".bright_blue());
                println!("  cd {} && {}", path_str, claude_cmd);
            }
        }

        Ok(())
    }

    pub fn open_shell(&self, worktree_path: &Path) -> Result<()> {
        let path_str = worktree_path.to_str().ok_or_else(|| {
            WorktreeError::InvalidPath("Invalid UTF-8 in path".to_string())
        })?;

        match self.terminal_type {
            TerminalType::ITerm2 => {
                let script = format!(
                    r#"
tell application "iTerm"
    activate
    tell current window
        create tab with default profile
        tell current session
            write text "cd '{}'"
        end tell
    end tell
end tell
"#,
                    path_str
                );

                let output = Command::new("osascript")
                    .arg("-e")
                    .arg(&script)
                    .output()
                    .map_err(|e| WorktreeError::Terminal(format!("Failed to execute AppleScript: {}", e)))?;

                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    return Err(WorktreeError::Terminal(format!(
                        "AppleScript failed: {}",
                        stderr
                    )));
                }
            }
            TerminalType::TerminalApp => {
                let script = format!(
                    r#"
tell application "Terminal"
    activate
    tell application "System Events"
        keystroke "t" using command down
    end tell
    delay 0.5
    do script "cd '{}'" in front window
end tell
"#,
                    path_str
                );

                let output = Command::new("osascript")
                    .arg("-e")
                    .arg(&script)
                    .output()
                    .map_err(|e| WorktreeError::Terminal(format!("Failed to execute AppleScript: {}", e)))?;

                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    return Err(WorktreeError::Terminal(format!(
                        "AppleScript failed: {}",
                        stderr
                    )));
                }
            }
            TerminalType::Unknown => {
                println!("{}", "To open in the worktree, run:".bright_blue());
                println!("  cd {}", path_str);
            }
        }

        Ok(())
    }
}

/// Query iTerm2 for all open tab/session names (used by TUI for indicators)
pub fn get_open_iterm_tab_names() -> Vec<String> {
    let script = r#"
tell application "iTerm2"
    set tabNames to {}
    repeat with w in windows
        tell w
            repeat with t in tabs
                tell t
                    repeat with s in sessions
                        set end of tabNames to name of s
                    end repeat
                end tell
            end repeat
        end tell
    end repeat
    return tabNames
end tell
"#;

    let output = match Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
    {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };

    if !output.status.success() {
        return Vec::new();
    }

    // Parse AppleScript list output: "item1, item2, item3"
    let result = String::from_utf8_lossy(&output.stdout);
    result
        .trim()
        .split(", ")
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Close an iTerm2 tab associated with a worktree
/// Returns true if tab was found and closed, false otherwise
pub fn close_iterm_tab_for_worktree(worktree_path: &Path, branch: &str) -> bool {
    let dirname = match worktree_path.file_name().and_then(|n| n.to_str()) {
        Some(name) => name,
        None => return false,
    };

    let tab_name = format!("{} - {}", dirname, branch);

    let script = format!(
        r#"
tell application "iTerm2"
    repeat with w in windows
        tell w
            repeat with t in tabs
                tell t
                    repeat with s in sessions
                        if name of s contains "{tab_name}" then
                            close t
                            return "closed"
                        end if
                    end repeat
                end tell
            end repeat
        end tell
    end repeat
end tell
return "not_found"
"#,
        tab_name = tab_name
    );

    let output = match Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
    {
        Ok(o) => o,
        Err(_) => return false,
    };

    if !output.status.success() {
        return false;
    }

    String::from_utf8_lossy(&output.stdout).trim() == "closed"
}
