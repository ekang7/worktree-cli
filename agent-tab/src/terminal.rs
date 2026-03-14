use std::env;
use std::process::Command;

use anyhow::{Context, Result};

use crate::color::TabColor;

/// Struct-based input for opening a tab.
pub struct OpenTabRequest<'a> {
    pub path: &'a str,
    pub tab_name: &'a str,
    pub agent_cmd: Option<&'a str>,
    pub color: &'a TabColor,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TerminalType {
    Tmux,
    ITerm2,
    TerminalApp,
    Unknown,
}

impl std::fmt::Display for TerminalType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tmux => write!(f, "tmux"),
            Self::ITerm2 => write!(f, "iTerm2"),
            Self::TerminalApp => write!(f, "Terminal.app"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

pub fn detect_terminal() -> TerminalType {
    if env::var("TMUX").is_ok() {
        return TerminalType::Tmux;
    }
    match env::var("TERM_PROGRAM").as_deref() {
        Ok("iTerm.app") => TerminalType::ITerm2,
        Ok("Apple_Terminal") => TerminalType::TerminalApp,
        _ => TerminalType::Unknown,
    }
}

pub struct TerminalManager {
    terminal_type: TerminalType,
}

impl TerminalManager {
    pub fn detect() -> Self {
        Self {
            terminal_type: detect_terminal(),
        }
    }

    pub fn terminal_type(&self) -> TerminalType {
        self.terminal_type
    }

    pub fn open_tab(&self, req: &OpenTabRequest) -> Result<()> {
        // Check for existing tab with the same name
        let existing = self.list_open_tabs();
        if existing.iter().any(|n| n == req.tab_name) {
            anyhow::bail!("Tab '{}' already exists. Close it first or use a different name.", req.tab_name);
        }

        match self.terminal_type {
            TerminalType::ITerm2 => self.open_tab_iterm2(req.path, req.tab_name, req.agent_cmd, req.color),
            TerminalType::Tmux => self.open_tab_tmux(req.path, req.tab_name, req.agent_cmd, req.color),
            TerminalType::TerminalApp => self.open_tab_terminal_app(req.path, req.tab_name, req.agent_cmd),
            TerminalType::Unknown => {
                self.print_manual_instructions(req.path, req.tab_name, req.agent_cmd);
                Ok(())
            }
        }
    }

    pub fn close_tab(&self, tab_name: &str) -> Result<bool> {
        match self.terminal_type {
            TerminalType::ITerm2 => self.close_tab_iterm2(tab_name),
            TerminalType::Tmux => self.close_tab_tmux(tab_name),
            TerminalType::TerminalApp => {
                eprintln!("Warning: Closing tabs is not reliably supported in Terminal.app");
                Ok(false)
            }
            TerminalType::Unknown => {
                eprintln!("Warning: Cannot close tabs in unknown terminal");
                Ok(false)
            }
        }
    }

    pub fn list_open_tabs(&self) -> Vec<String> {
        match self.terminal_type {
            TerminalType::ITerm2 => self.list_tabs_iterm2(),
            TerminalType::Tmux => self.list_tabs_tmux(),
            TerminalType::TerminalApp => {
                eprintln!("Warning: Listing tabs is not reliably supported in Terminal.app");
                Vec::new()
            }
            TerminalType::Unknown => Vec::new(),
        }
    }

    // ── iTerm2 backend ───────────────────────────────────────────────

    fn open_tab_iterm2(
        &self,
        path: &str,
        tab_name: &str,
        agent_cmd: Option<&str>,
        color: &TabColor,
    ) -> Result<()> {
        // Build escape sequence strings for the shell command
        // \033]1;name\007 sets tab title, color escapes set tab background
        let title_esc = format!(
            "printf '\\\\033]1;{}\\\\007'",
            tab_name.replace('\'', "'\\''"),
        );
        let color_esc = format!(
            "printf '\\\\033]6;1;bg;red;brightness;{}\\\\007' && printf '\\\\033]6;1;bg;green;brightness;{}\\\\007' && printf '\\\\033]6;1;bg;blue;brightness;{}\\\\007'",
            color.r, color.g, color.b,
        );
        let setup_cmd = format!(
            "cd '{}' && {} && {}",
            path.replace('\'', "'\\''"),
            title_esc,
            color_esc,
        );

        let script = if let Some(cmd) = agent_cmd {
            format!(
                r#"
tell application "iTerm2"
    activate
    if (count of windows) = 0 then
        create window with default profile
    end if
    tell current window
        set originalTab to current tab
        create tab with default profile
        tell current tab
            tell first session
                set name to "{tab_name}"
                write text "{setup_cmd} && {agent_cmd}"
                split vertically with default profile
            end tell
            tell last session
                set name to "{tab_name}"
                write text "{setup_cmd}"
            end tell
        end tell
        select originalTab
    end tell
end tell
"#,
                tab_name = tab_name.replace('"', "\\\""),
                setup_cmd = setup_cmd.replace('"', "\\\""),
                agent_cmd = cmd.replace('"', "\\\""),
            )
        } else {
            format!(
                r#"
tell application "iTerm2"
    activate
    if (count of windows) = 0 then
        create window with default profile
    end if
    tell current window
        set originalTab to current tab
        create tab with default profile
        tell current tab
            tell first session
                set name to "{tab_name}"
                write text "{setup_cmd}"
            end tell
        end tell
        select originalTab
    end tell
end tell
"#,
                tab_name = tab_name.replace('"', "\\\""),
                setup_cmd = setup_cmd.replace('"', "\\\""),
            )
        };

        let output = Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output()
            .context("Failed to run osascript for iTerm2")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("AppleScript failed: {}", stderr);
        }

        Ok(())
    }

    fn close_tab_iterm2(&self, tab_name: &str) -> Result<bool> {
        let script = format!(
            r#"
            tell application "iTerm2"
                repeat with w in windows
                    repeat with t in tabs of w
                        repeat with s in sessions of t
                            if name of s is "{name}" then
                                close t
                                return true
                            end if
                        end repeat
                    end repeat
                end repeat
            end tell
            return false
            "#,
            name = tab_name.replace('"', "\\\""),
        );

        let output = Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output()
            .context("Failed to run osascript for iTerm2")?;

        let result = String::from_utf8_lossy(&output.stdout);
        Ok(result.trim() == "true")
    }

    fn list_tabs_iterm2(&self) -> Vec<String> {
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

        let output = Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output();

        match output {
            Ok(o) if o.status.success() => {
                // AppleScript returns comma-separated list: "item1, item2, item3"
                let result = String::from_utf8_lossy(&o.stdout);
                result
                    .trim()
                    .split(", ")
                    .map(|s| s.to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            }
            _ => Vec::new(),
        }
    }

    // ── tmux backend ─────────────────────────────────────────────────

    fn open_tab_tmux(
        &self,
        path: &str,
        tab_name: &str,
        agent_cmd: Option<&str>,
        color: &TabColor,
    ) -> Result<()> {
        // Create new window without switching focus (-d)
        let status = Command::new("tmux")
            .args(["new-window", "-d", "-n", tab_name, "-c", path])
            .status()
            .context("Failed to create tmux window")?;

        if !status.success() {
            anyhow::bail!("tmux new-window failed");
        }

        // Set window style with color
        let style = format!("window-status-current-style=bg={}", color.tmux_style());
        let _ = Command::new("tmux")
            .args(["set-window-option", "-t", tab_name, &style])
            .status();

        if let Some(cmd) = agent_cmd {
            // Send agent command to the first pane
            Command::new("tmux")
                .args(["send-keys", "-t", tab_name, cmd, "Enter"])
                .status()
                .context("Failed to send agent command")?;

            // Split horizontally for a shell pane
            Command::new("tmux")
                .args(["split-window", "-h", "-t", tab_name, "-c", path])
                .status()
                .context("Failed to split tmux window")?;

            // Select the agent pane (left)
            Command::new("tmux")
                .args(["select-pane", "-t", &format!("{}:0", tab_name)])
                .status()
                .context("Failed to select tmux pane")?;
        }

        Ok(())
    }

    fn close_tab_tmux(&self, tab_name: &str) -> Result<bool> {
        let status = Command::new("tmux")
            .args(["kill-window", "-t", tab_name])
            .status()
            .context("Failed to kill tmux window")?;

        Ok(status.success())
    }

    fn list_tabs_tmux(&self) -> Vec<String> {
        let output = Command::new("tmux")
            .args(["list-windows", "-F", "#{window_name}"])
            .output();

        match output {
            Ok(o) if o.status.success() => {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .filter(|l| !l.is_empty())
                    .map(|l| l.to_string())
                    .collect()
            }
            _ => Vec::new(),
        }
    }

    // ── Terminal.app backend ─────────────────────────────────────────

    fn open_tab_terminal_app(
        &self,
        path: &str,
        tab_name: &str,
        agent_cmd: Option<&str>,
    ) -> Result<()> {
        // Open a tab with the agent
        let agent_script = if let Some(cmd) = agent_cmd {
            format!(
                r#"
                tell application "Terminal"
                    activate
                    do script "cd '{path}' && {cmd}"
                    set custom title of front window to "{tab_name}"
                end tell
                "#,
                path = path.replace('\'', "'\\''"),
                cmd = cmd.replace('\\', "\\\\").replace('"', "\\\""),
                tab_name = tab_name.replace('"', "\\\""),
            )
        } else {
            format!(
                r#"
                tell application "Terminal"
                    activate
                    do script "cd '{path}'"
                    set custom title of front window to "{tab_name}"
                end tell
                "#,
                path = path.replace('\'', "'\\''"),
                tab_name = tab_name.replace('"', "\\\""),
            )
        };

        Command::new("osascript")
            .arg("-e")
            .arg(&agent_script)
            .output()
            .context("Failed to run osascript for Terminal.app")?;

        // Open a second tab with just a shell if we have an agent
        if agent_cmd.is_some() {
            let shell_script = format!(
                r#"
                tell application "Terminal"
                    activate
                    do script "cd '{path}'"
                end tell
                "#,
                path = path.replace('\'', "'\\''"),
            );

            Command::new("osascript")
                .arg("-e")
                .arg(&shell_script)
                .output()
                .context("Failed to open shell tab in Terminal.app")?;
        }

        Ok(())
    }

    // ── Unknown fallback ─────────────────────────────────────────────

    fn print_manual_instructions(
        &self,
        path: &str,
        tab_name: &str,
        agent_cmd: Option<&str>,
    ) {
        eprintln!("Unknown terminal. Please manually:");
        eprintln!("  1. Open a new tab named '{}'", tab_name);
        eprintln!("  2. cd to: {}", path);
        if let Some(cmd) = agent_cmd {
            eprintln!("  3. Run: {}", cmd);
        }
    }
}
