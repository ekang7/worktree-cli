use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

use crate::color::TabColor;
use crate::terminal::TerminalManager;

/// Input config for listing worktrees with tab status.
pub struct ListOptions {
    pub repo_path: Option<String>,
    pub color: String,
}

/// Simple git status counts (staged/unstaged/untracked).
pub struct GitStatus {
    pub staged: usize,
    pub unstaged: usize,
    pub untracked: usize,
}

/// Rich worktree info combining git worktree data with tab state.
pub struct WorktreeInfo {
    pub path: PathBuf,
    pub branch: String,
    pub commit: String,
    pub is_bare: bool,
    pub worktree_name: String,
    pub git_status: GitStatus,
    pub is_tab_opened: bool,
    pub tab_name: Option<String>,
    pub tab_color: Option<TabColor>,
}

/// List all git worktrees enriched with tab status info.
pub fn list_worktrees(tm: &TerminalManager, options: &ListOptions) -> Result<Vec<WorktreeInfo>> {
    let repo_path = options
        .repo_path
        .as_deref()
        .unwrap_or(".");

    // Parse git worktree list --porcelain
    let output = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(repo_path)
        .output()
        .context("Failed to run 'git worktree list --porcelain'")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git worktree list failed: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let entries = parse_worktree_porcelain(&stdout);

    // Get all open tabs once
    let open_tabs = tm.list_open_tabs();

    let mut worktrees = Vec::new();
    for entry in entries {
        let git_status = fetch_git_status(&entry.path).unwrap_or(GitStatus {
            staged: 0,
            unstaged: 0,
            untracked: 0,
        });

        let worktree_name = entry
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        // Match tab by convention: "{dirname} - {branch}" or exact "{dirname}"
        let (is_tab_opened, tab_name, tab_color) =
            find_matching_tab(&open_tabs, &worktree_name, &entry.branch, &options.color);

        worktrees.push(WorktreeInfo {
            path: entry.path,
            branch: entry.branch,
            commit: entry.commit,
            is_bare: entry.is_bare,
            worktree_name,
            git_status,
            is_tab_opened,
            tab_name,
            tab_color,
        });
    }

    Ok(worktrees)
}

struct ParsedEntry {
    path: PathBuf,
    branch: String,
    commit: String,
    is_bare: bool,
}

fn parse_worktree_porcelain(output: &str) -> Vec<ParsedEntry> {
    let mut entries = Vec::new();
    let mut path: Option<PathBuf> = None;
    let mut branch = String::new();
    let mut commit = String::new();
    let mut is_bare = false;

    for line in output.lines() {
        if let Some(p) = line.strip_prefix("worktree ") {
            // If we already have a path, push the previous entry
            if let Some(prev_path) = path.take() {
                entries.push(ParsedEntry {
                    path: prev_path,
                    branch: std::mem::take(&mut branch),
                    commit: std::mem::take(&mut commit),
                    is_bare,
                });
                is_bare = false;
            }
            path = Some(PathBuf::from(p));
        } else if let Some(h) = line.strip_prefix("HEAD ") {
            commit = h.to_string();
        } else if let Some(b) = line.strip_prefix("branch ") {
            // Strip refs/heads/ prefix
            branch = b.strip_prefix("refs/heads/").unwrap_or(b).to_string();
        } else if line == "bare" {
            is_bare = true;
        } else if line == "detached" {
            branch = "(detached)".to_string();
        }
    }

    // Push final entry
    if let Some(p) = path {
        entries.push(ParsedEntry {
            path: p,
            branch,
            commit,
            is_bare,
        });
    }

    entries
}

fn fetch_git_status(path: &Path) -> Result<GitStatus> {
    let output = Command::new("git")
        .args(["status", "--porcelain=v1"])
        .current_dir(path)
        .output()
        .context("Failed to run git status")?;

    if !output.status.success() {
        anyhow::bail!("git status failed");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut staged = 0;
    let mut unstaged = 0;
    let mut untracked = 0;

    for line in stdout.lines() {
        if line.len() < 2 {
            continue;
        }
        let bytes = line.as_bytes();
        let x = bytes[0];
        let y = bytes[1];

        if x == b'?' && y == b'?' {
            untracked += 1;
        } else {
            if x != b' ' && x != b'?' {
                staged += 1;
            }
            if y != b' ' && y != b'?' {
                unstaged += 1;
            }
        }
    }

    Ok(GitStatus {
        staged,
        unstaged,
        untracked,
    })
}

fn find_matching_tab(
    open_tabs: &[String],
    worktree_name: &str,
    branch: &str,
    color_name: &str,
) -> (bool, Option<String>, Option<TabColor>) {
    // Match patterns (in priority order):
    // 1. "{dirname} - {branch}" (worktree-cli convention)
    // 2. Exact "{dirname}"
    // 3. Exact "{branch}" (tab named after branch)
    // 4. Tab name matches end of dirname (e.g. tab "cccc" matches dirname "worktree-cli-cccc")
    let full_pattern = format!("{} - {}", worktree_name, branch);

    for tab in open_tabs {
        if tab == &full_pattern
            || tab == worktree_name
            || tab == branch
            || worktree_name.ends_with(&format!("-{}", tab))
        {
            return (
                true,
                Some(tab.clone()),
                Some(TabColor::from_name(color_name)),
            );
        }
    }

    (false, None, None)
}
